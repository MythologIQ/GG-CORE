//! Model registry for tracking loaded models.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

use super::loader::ModelMetadata;

/// Unique handle to a loaded model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModelHandle(u64);

impl ModelHandle {
    /// Create a new handle with the given ID (primarily for testing).
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn id(&self) -> u64 {
        self.0
    }
}

/// Model state enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadedModelState {
    Loading,
    Ready,
    Unloading,
    Error,
}

impl LoadedModelState {
    pub fn as_str(&self) -> &'static str {
        match self {
            LoadedModelState::Loading => "loading",
            LoadedModelState::Ready => "ready",
            LoadedModelState::Unloading => "unloading",
            LoadedModelState::Error => "error",
        }
    }
}

/// Information about a loaded model for diagnostics.
#[derive(Debug, Clone)]
pub struct LoadedModelInfo {
    pub handle_id: u64,
    pub name: String,
    pub format: String,
    pub size_bytes: u64,
    pub memory_bytes: u64,
    pub state: LoadedModelState,
    pub request_count: u64,
    pub total_latency_ms: f64,
    pub loaded_at: SystemTime,
}

struct LoadedModel {
    metadata: ModelMetadata,
    memory_bytes: usize,
    format: String,
    state: LoadedModelState,
    request_count: AtomicU64,
    total_latency_ms: std::sync::atomic::AtomicU64,
    loaded_at: SystemTime,
}

/// Thread-safe registry of loaded models.
pub struct ModelRegistry {
    models: Arc<RwLock<HashMap<ModelHandle, LoadedModel>>>,
    next_id: AtomicU64,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            models: Arc::new(RwLock::new(HashMap::new())),
            next_id: AtomicU64::new(1),
        }
    }

    /// Register a new model and return its handle.
    pub async fn register(&self, metadata: ModelMetadata, memory_bytes: usize) -> ModelHandle {
        self.register_with_format(metadata, memory_bytes, "unknown".to_string()).await
    }

    /// Register a new model with format info and return its handle.
    pub async fn register_with_format(
        &self,
        metadata: ModelMetadata,
        memory_bytes: usize,
        format: String,
    ) -> ModelHandle {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let handle = ModelHandle(id);

        let model = LoadedModel {
            metadata,
            memory_bytes,
            format,
            state: LoadedModelState::Ready,
            request_count: AtomicU64::new(0),
            total_latency_ms: AtomicU64::new(0),
            loaded_at: SystemTime::now(),
        };
        self.models.write().await.insert(handle, model);

        handle
    }

    /// Check if a model handle is valid.
    pub async fn contains(&self, handle: ModelHandle) -> bool {
        self.models.read().await.contains_key(&handle)
    }

    /// Get metadata for a loaded model.
    pub async fn get_metadata(&self, handle: ModelHandle) -> Option<ModelMetadata> {
        self.models.read().await.get(&handle).map(|m| m.metadata.clone())
    }

    /// Remove a model from the registry.
    pub async fn unregister(&self, handle: ModelHandle) -> Option<usize> {
        self.models.write().await.remove(&handle).map(|m| m.memory_bytes)
    }

    /// Total memory used by all registered models.
    pub async fn total_memory(&self) -> usize {
        self.models.read().await.values().map(|m| m.memory_bytes).sum()
    }

    /// Number of loaded models.
    pub async fn count(&self) -> usize {
        self.models.read().await.len()
    }

    /// List all loaded models with their current status.
    pub async fn list_models(&self) -> Vec<LoadedModelInfo> {
        let models = self.models.read().await;
        models
            .iter()
            .map(|(handle, model)| LoadedModelInfo {
                handle_id: handle.id(),
                name: model.metadata.name.clone(),
                format: model.format.clone(),
                size_bytes: model.metadata.size_bytes,
                memory_bytes: model.memory_bytes as u64,
                state: model.state,
                request_count: model.request_count.load(Ordering::Relaxed),
                total_latency_ms: f64::from_bits(model.total_latency_ms.load(Ordering::Relaxed)),
                loaded_at: model.loaded_at,
            })
            .collect()
    }

    /// Record a completed request for a model.
    pub async fn record_request(&self, handle: ModelHandle, latency_ms: f64) {
        if let Some(model) = self.models.read().await.get(&handle) {
            model.request_count.fetch_add(1, Ordering::Relaxed);
            // Atomic f64 addition via CAS loop
            loop {
                let old_bits = model.total_latency_ms.load(Ordering::Relaxed);
                let old_value = f64::from_bits(old_bits);
                let new_value = old_value + latency_ms;
                let new_bits = new_value.to_bits();
                if model
                    .total_latency_ms
                    .compare_exchange(old_bits, new_bits, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    break;
                }
            }
        }
    }

    /// Update model state.
    pub async fn set_state(&self, handle: ModelHandle, state: LoadedModelState) {
        if let Some(model) = self.models.write().await.get_mut(&handle) {
            model.state = state;
        }
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}
