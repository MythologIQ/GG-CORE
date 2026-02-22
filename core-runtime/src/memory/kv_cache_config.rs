//! KV Cache configuration, types, and error definitions.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Acquire a mutex lock, recovering from poison if a thread panicked.
#[inline]
pub(crate) fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("KV cache mutex poisoned, recovering");
        poisoned.into_inner()
    })
}

/// Acquire a read lock, recovering from poison if a thread panicked.
#[inline]
pub(crate) fn read_or_recover<T>(rwlock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    rwlock.read().unwrap_or_else(|poisoned| {
        tracing::warn!("KV cache RwLock poisoned, recovering for read");
        poisoned.into_inner()
    })
}

/// Acquire a write lock, recovering from poison if a thread panicked.
#[inline]
pub(crate) fn write_or_recover<T>(rwlock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    rwlock.write().unwrap_or_else(|poisoned| {
        tracing::warn!("KV cache RwLock poisoned, recovering for write");
        poisoned.into_inner()
    })
}

/// Configuration for the KV Cache Manager.
#[derive(Debug, Clone)]
pub struct KvCacheConfig {
    /// Hidden dimension of the model.
    pub hidden_dim: usize,
    /// Maximum number of pages to allocate.
    pub max_pages: usize,
    /// Maximum sequence length.
    pub max_seq_len: usize,
    /// Number of attention heads.
    pub num_heads: usize,
    /// Head dimension.
    pub head_dim: usize,
    /// Enable Q8 quantization for KV storage.
    pub enable_quantization: bool,
    /// Enable paged attention (vLLM-style).
    pub enable_paged: bool,
    /// Cache eviction policy.
    pub eviction_policy: EvictionPolicy,
    /// Optional sliding window attention configuration.
    pub sliding_window: Option<SlidingWindowConfig>,
}

impl Default for KvCacheConfig {
    fn default() -> Self {
        Self {
            hidden_dim: 4096,
            max_pages: 1024,
            max_seq_len: 4096,
            num_heads: 32,
            head_dim: 128,
            enable_quantization: true,
            enable_paged: true,
            eviction_policy: EvictionPolicy::Lru,
            sliding_window: None,
        }
    }
}

/// Configuration for sliding window attention.
#[derive(Debug, Clone)]
pub struct SlidingWindowConfig {
    /// Maximum window size in tokens. Only this many recent tokens
    /// have KV entries cached.
    pub window_size: usize,
    /// Overlap tokens to preserve at the window boundary.
    /// Avoids re-computation of tokens near the boundary.
    pub overlap_tokens: usize,
}

/// Cache eviction policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// Least Recently Used - evict oldest unused entries.
    Lru,
    /// First In First Out - evict oldest entries.
    Fifo,
    /// Least Frequently Used - evict entries with lowest access count.
    Lfu,
}

/// Statistics for the KV cache.
#[derive(Debug, Default, Clone)]
pub struct KvCacheStats {
    pub total_pages_allocated: u64,
    pub total_pages_freed: u64,
    pub current_pages_in_use: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub evictions: u64,
    pub quantization_errors: u64,
    pub memory_bytes_used: u64,
    pub peak_memory_bytes: u64,
}

impl KvCacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            return 0.0;
        }
        self.cache_hits as f64 / total as f64
    }
}

/// Unique identifier for a cache sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SequenceId(pub u64);

/// Errors for KV cache operations.
#[derive(Debug, thiserror::Error)]
pub enum KvCacheError {
    #[error("Sequence not found: {0}")]
    SequenceNotFound(u64),

    #[error("Position {pos} out of bounds for sequence length {seq_len}")]
    PositionOutOfBounds { pos: usize, seq_len: usize },

    #[error("Page not found")]
    PageNotFound,

    #[error("Memory exhausted - cannot allocate more pages")]
    MemoryExhausted,

    #[error("Quantization error: {0}")]
    QuantizationError(String),
}
