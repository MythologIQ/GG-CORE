//! Types and configuration for the smart model loader.
//!
//! Extracted from `smart_loader.rs` for Section 4 compliance.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::registry::ModelHandle;

/// Semantic hints for adaptive loading decisions.
#[derive(Debug, Clone, Copy)]
pub enum LoadHint {
    /// Quick single query - prefer lightweight model
    QuickQuery,
    /// Complex task - prefer quality model
    ComplexTask,
    /// Batch incoming - preload appropriate model
    BatchIncoming { count: usize },
    /// User going idle - good time to preload
    UserIdle,
    /// Explicit model preference
    PreferModel { tier: ModelTier },
}

/// Model tier classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelTier {
    /// Lightweight for quick responses (~500MB)
    Light,
    /// Balanced for general use (~1.5GB)
    Balanced,
    /// Quality for complex tasks (~2.5GB)
    Quality,
}

/// Model load state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadState {
    /// Registered but not loaded
    Registered,
    /// Currently loading
    Loading,
    /// Loaded and ready
    Ready,
    /// Load failed
    Failed,
}

/// Registered model metadata.
pub(super) struct ModelEntry {
    pub path: PathBuf,
    pub tier: ModelTier,
    pub size_bytes: u64,
    pub state: LoadState,
    pub handle: Option<ModelHandle>,
    pub last_used: Option<Instant>,
    pub use_count: u64,
    pub load_time_ms: Option<u64>,
}

/// Smart loader configuration.
#[derive(Debug, Clone)]
pub struct SmartLoaderConfig {
    /// Auto-unload after this duration of inactivity
    pub auto_unload_after: Duration,
    /// Max concurrent loads
    pub max_concurrent_loads: usize,
    /// Enable predictive loading
    pub enable_prediction: bool,
}

impl Default for SmartLoaderConfig {
    fn default() -> Self {
        Self {
            auto_unload_after: Duration::from_secs(60),
            max_concurrent_loads: 1,
            enable_prediction: true,
        }
    }
}

/// Smart loader metrics.
#[derive(Debug, Default, Clone)]
pub struct SmartLoaderMetrics {
    pub total_loads: u64,
    pub cache_hits: u64,
    pub predictions_made: u64,
    pub predictions_correct: u64,
    pub avg_load_ms: f64,
    pub avg_cache_hit_ms: f64,
}

/// Current loader status.
#[derive(Debug)]
pub struct SmartLoaderStatus {
    pub registered_count: usize,
    pub loaded_count: usize,
    pub loaded_models: Vec<(String, ModelTier)>,
    pub active_tier: Option<ModelTier>,
    pub total_loaded_bytes: u64,
    pub predicted_next: Option<String>,
}

/// Callback for model loading (to integrate with actual GGUF loader).
pub type LoadCallback = Box<dyn Fn(&PathBuf) -> Result<ModelHandle, String> + Send + Sync>;
