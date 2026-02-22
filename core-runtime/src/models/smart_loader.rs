//! Smart model loader with semantic hints and adaptive caching.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::{RwLock, Semaphore};

use super::registry::ModelHandle;
pub use super::smart_loader_types::*;

#[derive(Error, Debug)]
pub enum SmartLoaderError {
    #[error("Model not registered: {0}")]
    NotRegistered(String),

    #[error("Load failed: {0}")]
    LoadFailed(String),

    #[error("Already loading")]
    AlreadyLoading,
}

/// Smart model loader with semantic hints.
pub struct SmartLoader {
    /// Configuration for auto-unload timing and prediction
    #[allow(dead_code)]
    config: SmartLoaderConfig,
    models: Arc<RwLock<HashMap<String, ModelEntry>>>,
    active_tier: Arc<RwLock<Option<ModelTier>>>,
    metrics: Arc<RwLock<SmartLoaderMetrics>>,
    load_semaphore: Semaphore,
    load_callback: Option<Arc<LoadCallback>>,
    predicted_next: Arc<RwLock<Option<String>>>,
}

impl SmartLoader {
    pub fn new(config: SmartLoaderConfig) -> Self {
        let max_loads = config.max_concurrent_loads;
        Self {
            config,
            models: Arc::new(RwLock::new(HashMap::new())),
            active_tier: Arc::new(RwLock::new(None)),
            metrics: Arc::new(RwLock::new(SmartLoaderMetrics::default())),
            load_semaphore: Semaphore::new(max_loads),
            load_callback: None,
            predicted_next: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the callback for actual model loading.
    pub fn set_load_callback(&mut self, callback: LoadCallback) {
        self.load_callback = Some(Arc::new(callback));
    }

    /// Register a model (zero overhead).
    pub async fn register(
        &self,
        model_id: String,
        path: PathBuf,
        tier: ModelTier,
    ) -> Result<(), SmartLoaderError> {
        let size_bytes = std::fs::metadata(&path)
            .map_err(|e| SmartLoaderError::LoadFailed(e.to_string()))?
            .len();

        self.models.write().await.insert(
            model_id,
            ModelEntry {
                path,
                tier,
                size_bytes,
                state: LoadState::Registered,
                handle: None,
                last_used: None,
                use_count: 0,
                load_time_ms: None,
            },
        );
        Ok(())
    }

    /// Provide a semantic hint about upcoming usage.
    pub async fn hint(&self, hint: LoadHint) {
        let prediction = match hint {
            LoadHint::QuickQuery => self.find_model_by_tier(ModelTier::Light).await,
            LoadHint::ComplexTask => self.find_model_by_tier(ModelTier::Quality).await,
            LoadHint::BatchIncoming { count } if count > 10 => {
                self.find_model_by_tier(ModelTier::Quality).await
            }
            LoadHint::BatchIncoming { .. } => {
                self.find_model_by_tier(ModelTier::Balanced).await
            }
            LoadHint::UserIdle => self.predict_next().await,
            LoadHint::PreferModel { tier } => self.find_model_by_tier(tier).await,
        };

        if let Some(model_id) = prediction {
            *self.predicted_next.write().await = Some(model_id.clone());
            self.metrics.write().await.predictions_made += 1;
            self.preload_background(&model_id);
        }
    }

    /// Find a registered model by tier.
    async fn find_model_by_tier(&self, tier: ModelTier) -> Option<String> {
        self.models
            .read()
            .await
            .iter()
            .find(|(_, m)| m.tier == tier)
            .map(|(id, _)| id.clone())
    }

    /// Predict most likely next model (most-used that isn't the active tier).
    async fn predict_next(&self) -> Option<String> {
        let models = self.models.read().await;
        let active = self.active_tier.read().await.clone();
        models
            .iter()
            .filter(|(_, m)| Some(m.tier) != active)
            .max_by_key(|(_, m)| m.use_count)
            .map(|(id, _)| id.clone())
    }

    /// Preload a model in the background.
    fn preload_background(&self, model_id: &str) {
        if self.load_semaphore.available_permits() == 0 {
            return;
        }
        super::smart_loader_ops::preload_background(
            self.models.clone(),
            model_id,
            self.load_callback.clone(),
        );
    }

    /// Get a model, loading if necessary.
    pub async fn get(&self, model_id: &str) -> Result<ModelHandle, SmartLoaderError> {
        let start = Instant::now();

        // Check prediction accuracy
        {
            let predicted = self.predicted_next.read().await.clone();
            if predicted.as_ref() == Some(&model_id.to_string()) {
                self.metrics.write().await.predictions_correct += 1;
            }
        }

        // Check if already loaded
        {
            let mut models = self.models.write().await;
            if let Some(entry) = models.get_mut(model_id) {
                if entry.state == LoadState::Ready {
                    if let Some(handle) = entry.handle {
                        entry.last_used = Some(Instant::now());
                        entry.use_count += 1;
                        *self.active_tier.write().await = Some(entry.tier);

                        let load_ms = start.elapsed().as_millis() as f64;
                        let mut metrics = self.metrics.write().await;
                        metrics.cache_hits += 1;
                        let n = metrics.cache_hits as f64;
                        metrics.avg_cache_hit_ms =
                            (metrics.avg_cache_hit_ms * (n - 1.0) + load_ms) / n;

                        return Ok(handle);
                    }
                }
            }
        }

        // Need to load
        self.load_sync(model_id).await?;

        let models = self.models.read().await;
        let entry = models
            .get(model_id)
            .ok_or_else(|| SmartLoaderError::NotRegistered(model_id.to_string()))?;

        let handle = entry
            .handle
            .ok_or_else(|| SmartLoaderError::LoadFailed("Load did not produce handle".into()))?;

        let load_ms = start.elapsed().as_millis() as f64;
        let mut metrics = self.metrics.write().await;
        metrics.total_loads += 1;
        let n = metrics.total_loads as f64;
        metrics.avg_load_ms = (metrics.avg_load_ms * (n - 1.0) + load_ms) / n;

        Ok(handle)
    }

    /// Synchronous load (blocks until complete).
    async fn load_sync(&self, model_id: &str) -> Result<(), SmartLoaderError> {
        super::smart_loader_ops::load_sync(
            &self.models,
            model_id,
            self.load_callback.as_ref(),
            &self.active_tier,
            &self.load_semaphore,
        )
        .await
    }

    /// Unload a model.
    pub async fn unload(&self, model_id: &str) {
        if let Some(entry) = self.models.write().await.get_mut(model_id) {
            entry.state = LoadState::Registered;
            entry.handle = None;
        }
    }

    /// Get metrics.
    pub async fn metrics(&self) -> SmartLoaderMetrics {
        self.metrics.read().await.clone()
    }

    /// Get current status.
    pub async fn status(&self) -> SmartLoaderStatus {
        let models = self.models.read().await;
        let active_tier = self.active_tier.read().await.clone();

        let loaded: Vec<_> = models
            .iter()
            .filter(|(_, m)| m.state == LoadState::Ready)
            .map(|(id, m)| (id.clone(), m.tier))
            .collect();

        let total_loaded_bytes: u64 = models
            .values()
            .filter(|m| m.state == LoadState::Ready)
            .map(|m| m.size_bytes)
            .sum();

        SmartLoaderStatus {
            registered_count: models.len(),
            loaded_count: loaded.len(),
            loaded_models: loaded,
            active_tier,
            total_loaded_bytes,
            predicted_next: self.predicted_next.read().await.clone(),
        }
    }
}

#[cfg(test)]
#[path = "smart_loader_tests.rs"]
mod tests;
