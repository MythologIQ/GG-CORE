//! Integrated KV Cache Manager with Paged Attention and Quantization.
//!
//! Combines paged memory allocation with Q8 quantization for efficient
//! KV-cache storage during inference. Provides 4x memory reduction
//! and efficient memory management through page-based allocation.
//!
//! Split into submodules for Section 4 compliance:
//! - `kv_cache_config` — Configuration, types, and error definitions
//! - `kv_cache_core` — KvCacheManager implementation

pub use super::kv_cache_config::{
    EvictionPolicy, KvCacheConfig, KvCacheError, KvCacheStats, SequenceId, SlidingWindowConfig,
};
pub use super::kv_cache_core::KvCacheManager;

#[cfg(test)]
#[path = "kv_cache_tests.rs"]
mod tests;
