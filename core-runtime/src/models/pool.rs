//! Model pool for instant switching between pre-loaded models.
//!
//! Maintains multiple models in memory to enable seamless tier transitions
//! without load-time latency.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

use super::registry::{ModelHandle, ModelRegistry};

#[derive(Error, Debug)]
pub enum PoolError {
    #[error("Pool capacity exceeded: {current}/{max}")]
    CapacityExceeded { current: usize, max: usize },

    #[error("Model not in pool: {0}")]
    ModelNotFound(String),

    #[error("Model already in pool: {0}")]
    AlreadyLoaded(String),

    #[error("Eviction failed: no evictable models")]
    EvictionFailed,
}

/// Model tier for prioritized eviction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelTier {
    /// CI/Testing - lowest priority, first to evict
    Testing = 0,
    /// Default installation tier
    Default = 1,
    /// Quality/Production tier - highest priority
    Quality = 2,
}

/// Pooled model entry with usage tracking.
#[derive(Debug)]
struct PooledModel {
    handle: ModelHandle,
    /// Stored for debugging/logging
    #[allow(dead_code)]
    model_id: String,
    tier: ModelTier,
    memory_bytes: usize,
    /// Stored for pool age tracking
    #[allow(dead_code)]
    loaded_at: Instant,
    last_used: Instant,
    use_count: u64,
    warmup_complete: bool,
}

impl PooledModel {
    /// Calculate eviction score (lower = evict first).
    fn eviction_score(&self) -> u64 {
        let tier_weight = (self.tier as u64) * 1_000_000;
        let recency_weight = self.last_used.elapsed().as_secs();
        let usage_weight = self.use_count.min(1000);

        // Higher tier + more recent + more used = higher score (keep longer)
        tier_weight + usage_weight - recency_weight.min(999)
    }
}

/// Configuration for the model pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of models to keep in pool
    pub max_models: usize,
    /// Maximum total memory for pooled models (bytes)
    pub max_memory_bytes: usize,
    /// Warmup prompt for new models
    pub warmup_prompt: String,
    /// Enable background preloading
    pub enable_preload: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_models: 3,
            max_memory_bytes: 8 * 1024 * 1024 * 1024, // 8 GB
            warmup_prompt: "Hello".to_string(),
            enable_preload: true,
        }
    }
}

/// Metrics for pool operations.
#[derive(Debug, Default, Clone)]
pub struct PoolMetrics {
    pub pool_hits: u64,
    pub pool_misses: u64,
    pub evictions: u64,
    pub warmups_completed: u64,
    pub avg_switch_latency_ns: u64,
}

/// Model pool for instant tier switching.
pub struct ModelPool {
    config: PoolConfig,
    registry: Arc<ModelRegistry>,
    models: Arc<RwLock<HashMap<String, PooledModel>>>,
    active_model: Arc<RwLock<Option<String>>>,
    metrics: Arc<RwLock<PoolMetrics>>,
}

impl ModelPool {
    pub fn new(config: PoolConfig, registry: Arc<ModelRegistry>) -> Self {
        Self {
            config,
            registry,
            models: Arc::new(RwLock::new(HashMap::new())),
            active_model: Arc::new(RwLock::new(None)),
            metrics: Arc::new(RwLock::new(PoolMetrics::default())),
        }
    }

    /// Add a model to the pool (preload without activating).
    pub async fn preload(
        &self,
        model_id: String,
        handle: ModelHandle,
        tier: ModelTier,
        memory_bytes: usize,
    ) -> Result<(), PoolError> {
        let mut models = self.models.write().await;

        // Check if already loaded
        if models.contains_key(&model_id) {
            return Err(PoolError::AlreadyLoaded(model_id));
        }

        // Check capacity
        if models.len() >= self.config.max_models {
            drop(models);
            self.evict_one().await?;
            models = self.models.write().await;
        }

        // Check memory
        let current_memory: usize = models.values().map(|m| m.memory_bytes).sum();
        if current_memory + memory_bytes > self.config.max_memory_bytes {
            drop(models);
            self.evict_for_memory(memory_bytes).await?;
            models = self.models.write().await;
        }

        let now = Instant::now();
        models.insert(
            model_id.clone(),
            PooledModel {
                handle,
                model_id,
                tier,
                memory_bytes,
                loaded_at: now,
                last_used: now,
                use_count: 0,
                warmup_complete: false,
            },
        );

        Ok(())
    }

    /// Switch to a model in the pool (instant if preloaded).
    pub async fn switch_to(&self, model_id: &str) -> Result<SwitchResult, PoolError> {
        let start = Instant::now();

        let mut models = self.models.write().await;
        let model = models
            .get_mut(model_id)
            .ok_or_else(|| PoolError::ModelNotFound(model_id.to_string()))?;

        model.last_used = Instant::now();
        model.use_count += 1;

        let handle = model.handle;
        let was_warmed = model.warmup_complete;

        drop(models);

        // Update active model
        *self.active_model.write().await = Some(model_id.to_string());

        let switch_latency = start.elapsed();

        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.pool_hits += 1;
        // Running average
        let total = metrics.pool_hits;
        metrics.avg_switch_latency_ns =
            (metrics.avg_switch_latency_ns * (total - 1) + switch_latency.as_nanos() as u64) / total;

        Ok(SwitchResult {
            handle,
            switch_latency,
            was_preloaded: true,
            was_warmed,
        })
    }

    /// Mark a model as warmed up (after running warmup inference).
    pub async fn mark_warmed(&self, model_id: &str) {
        if let Some(model) = self.models.write().await.get_mut(model_id) {
            model.warmup_complete = true;
            self.metrics.write().await.warmups_completed += 1;
        }
    }

    /// Evict lowest-priority model from pool.
    async fn evict_one(&self) -> Result<String, PoolError> {
        let mut models = self.models.write().await;
        let active = self.active_model.read().await.clone();

        // Find model with lowest eviction score (excluding active)
        let evict_id = models
            .iter()
            .filter(|(id, _)| active.as_ref() != Some(*id))
            .min_by_key(|(_, m)| m.eviction_score())
            .map(|(id, _)| id.clone());

        if let Some(id) = evict_id {
            let model = models.remove(&id).unwrap();
            self.registry.unregister(model.handle).await;
            self.metrics.write().await.evictions += 1;
            Ok(id)
        } else {
            Err(PoolError::EvictionFailed)
        }
    }

    /// Evict models until we have enough memory.
    async fn evict_for_memory(&self, needed_bytes: usize) -> Result<(), PoolError> {
        loop {
            let current: usize = self.models.read().await.values().map(|m| m.memory_bytes).sum();
            if current + needed_bytes <= self.config.max_memory_bytes {
                return Ok(());
            }
            self.evict_one().await?;
        }
    }

    /// Get current pool status.
    pub async fn status(&self) -> PoolStatus {
        let models = self.models.read().await;
        let active = self.active_model.read().await.clone();
        let metrics = self.metrics.read().await.clone();

        PoolStatus {
            model_count: models.len(),
            total_memory_bytes: models.values().map(|m| m.memory_bytes).sum(),
            active_model: active,
            loaded_models: models.keys().cloned().collect(),
            metrics,
        }
    }

    /// Check if a model is in the pool.
    pub async fn contains(&self, model_id: &str) -> bool {
        self.models.read().await.contains_key(model_id)
    }

    /// Get the active model ID.
    pub async fn active(&self) -> Option<String> {
        self.active_model.read().await.clone()
    }

    /// Remove a model from the pool.
    pub async fn remove(&self, model_id: &str) -> Option<ModelHandle> {
        let mut models = self.models.write().await;
        if let Some(model) = models.remove(model_id) {
            // Clear active if this was active
            let mut active = self.active_model.write().await;
            if active.as_ref() == Some(&model_id.to_string()) {
                *active = None;
            }
            Some(model.handle)
        } else {
            None
        }
    }
}

/// Result of switching to a pooled model.
#[derive(Debug)]
pub struct SwitchResult {
    pub handle: ModelHandle,
    pub switch_latency: Duration,
    pub was_preloaded: bool,
    pub was_warmed: bool,
}

/// Current pool status.
#[derive(Debug)]
pub struct PoolStatus {
    pub model_count: usize,
    pub total_memory_bytes: usize,
    pub active_model: Option<String>,
    pub loaded_models: Vec<String>,
    pub metrics: PoolMetrics,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pool_preload_and_switch() {
        let registry = Arc::new(ModelRegistry::new());
        let config = PoolConfig::default();
        let pool = ModelPool::new(config, registry.clone());

        let handle = ModelHandle::new(1);
        pool.preload(
            "qwen-0.5b".to_string(),
            handle,
            ModelTier::Testing,
            500_000_000,
        ).await.unwrap();

        assert!(pool.contains("qwen-0.5b").await);

        let result = pool.switch_to("qwen-0.5b").await.unwrap();
        assert_eq!(result.handle, handle);
        assert!(result.was_preloaded);
        assert!(result.switch_latency < Duration::from_millis(1));
    }

    #[tokio::test]
    async fn pool_eviction_by_tier() {
        let registry = Arc::new(ModelRegistry::new());
        let config = PoolConfig {
            max_models: 2,
            ..Default::default()
        };
        let pool = ModelPool::new(config, registry.clone());

        // Preload testing tier
        pool.preload("ci".to_string(), ModelHandle::new(1), ModelTier::Testing, 100).await.unwrap();
        // Preload quality tier
        pool.preload("prod".to_string(), ModelHandle::new(2), ModelTier::Quality, 100).await.unwrap();

        // Third model should evict testing tier
        pool.preload("default".to_string(), ModelHandle::new(3), ModelTier::Default, 100).await.unwrap();

        assert!(!pool.contains("ci").await);
        assert!(pool.contains("prod").await);
        assert!(pool.contains("default").await);
    }

    #[tokio::test]
    async fn pool_switch_latency_under_1ms() {
        let registry = Arc::new(ModelRegistry::new());
        let pool = ModelPool::new(PoolConfig::default(), registry.clone());

        pool.preload("test".to_string(), ModelHandle::new(1), ModelTier::Default, 100).await.unwrap();

        // Multiple switches should all be fast
        for _ in 0..100 {
            let result = pool.switch_to("test").await.unwrap();
            assert!(result.switch_latency < Duration::from_millis(1));
        }
    }

    #[tokio::test]
    async fn pool_warmup_tracking() {
        let registry = Arc::new(ModelRegistry::new());
        let pool = ModelPool::new(PoolConfig::default(), registry.clone());

        pool.preload("test".to_string(), ModelHandle::new(1), ModelTier::Default, 100).await.unwrap();

        let result = pool.switch_to("test").await.unwrap();
        assert!(!result.was_warmed);

        pool.mark_warmed("test").await;

        let result = pool.switch_to("test").await.unwrap();
        assert!(result.was_warmed);
    }
}
