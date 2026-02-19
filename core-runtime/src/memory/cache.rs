//! Context caching for inference state.
//!
//! Uses DashMap for lock-free concurrent access.

use std::time::{Duration, Instant};
use dashmap::DashMap;

/// Configuration for context cache.
#[derive(Debug, Clone)]
pub struct ContextCacheConfig {
    pub max_entries: usize,
    pub ttl: Duration,
}

impl Default for ContextCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 128,
            ttl: Duration::from_secs(300), // 5 minutes
        }
    }
}

struct CacheEntry {
    data: Vec<u8>,
    created_at: Instant,
}

/// Lock-free cache for inference context data.
/// Uses DashMap for concurrent access without global locks.
pub struct ContextCache {
    entries: DashMap<String, CacheEntry>,
    config: ContextCacheConfig,
}

impl ContextCache {
    pub fn new(config: ContextCacheConfig) -> Self {
        Self {
            entries: DashMap::with_capacity(config.max_entries),
            config,
        }
    }

    /// Store context data with the given key.
    pub async fn store(&self, key: String, data: Vec<u8>) {
        self.store_sync(key, data);
    }

    /// Synchronous store for non-async contexts.
    pub fn store_sync(&self, key: String, data: Vec<u8>) {
        if self.entries.len() >= self.config.max_entries {
            self.evict_oldest();
        }
        self.entries.insert(key, CacheEntry { data, created_at: Instant::now() });
    }

    /// Retrieve cached context data if valid.
    pub async fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.get_sync(key)
    }

    /// Synchronous get for non-async contexts.
    pub fn get_sync(&self, key: &str) -> Option<Vec<u8>> {
        let entry = self.entries.get(key)?;
        if entry.created_at.elapsed() > self.config.ttl {
            return None;
        }
        Some(entry.data.clone())
    }

    /// Remove expired entries.
    pub async fn cleanup(&self) {
        self.cleanup_sync();
    }

    /// Synchronous cleanup.
    pub fn cleanup_sync(&self) {
        self.entries.retain(|_, entry| entry.created_at.elapsed() <= self.config.ttl);
    }

    fn evict_oldest(&self) {
        let oldest = self.entries.iter()
            .min_by_key(|e| e.created_at)
            .map(|e| e.key().clone());
        if let Some(key) = oldest {
            self.entries.remove(&key);
        }
    }

    pub async fn len(&self) -> usize {
        self.entries.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// KV-Cache for transformer inference optimization

/// KV-cache entry for transformer inference.
/// Pre-allocates capacity to avoid reallocation during generation.
#[derive(Clone)]
pub struct KvCacheEntry {
    pub keys: Vec<f32>,
    pub values: Vec<f32>,
    pub seq_len: usize,
    capacity: usize,
}

impl KvCacheEntry {
    /// Create a new KV-cache entry with pre-allocated capacity.
    pub fn new(hidden_size: usize, max_seq_len: usize) -> Self {
        let capacity = hidden_size * max_seq_len;
        Self {
            keys: Vec::with_capacity(capacity),
            values: Vec::with_capacity(capacity),
            seq_len: 0,
            capacity,
        }
    }

    /// Append new KV entries without reallocation (if within capacity).
    pub fn append(&mut self, new_keys: &[f32], new_values: &[f32]) {
        self.keys.extend_from_slice(new_keys);
        self.values.extend_from_slice(new_values);
        self.seq_len += 1;
    }

    /// Memory usage in bytes.
    pub fn memory_bytes(&self) -> usize {
        (self.keys.capacity() + self.values.capacity()) * std::mem::size_of::<f32>()
    }

    /// Check if at capacity.
    pub fn is_full(&self) -> bool {
        self.keys.len() >= self.capacity
    }

    /// Get pre-allocated capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// Lock-free KV cache using DashMap.
/// Optimized for multi-turn generation where KV state is reused.
pub struct KvCache {
    entries: DashMap<String, KvCacheEntry>,
    hidden_size: usize,
    max_seq_len: usize,
    max_entries: usize,
}

impl KvCache {
    /// Create a new KV-cache with specified dimensions.
    pub fn new(hidden_size: usize, max_seq_len: usize, max_entries: usize) -> Self {
        Self {
            entries: DashMap::with_capacity(max_entries),
            hidden_size,
            max_seq_len,
            max_entries,
        }
    }

    /// Get or create a KV-cache entry for a session.
    pub async fn get_or_create(&self, session_id: &str) -> KvCacheEntry {
        self.get_or_create_sync(session_id)
    }

    /// Synchronous get or create.
    pub fn get_or_create_sync(&self, session_id: &str) -> KvCacheEntry {
        if let Some(entry) = self.entries.get(session_id) {
            return entry.clone();
        }
        KvCacheEntry::new(self.hidden_size, self.max_seq_len)
    }

    /// Update KV-cache for a session.
    pub async fn update(&self, session_id: String, entry: KvCacheEntry) {
        self.update_sync(session_id, entry);
    }

    /// Synchronous update.
    pub fn update_sync(&self, session_id: String, entry: KvCacheEntry) {
        if self.entries.len() >= self.max_entries && !self.entries.contains_key(&session_id) {
            self.evict_one();
        }
        self.entries.insert(session_id, entry);
    }

    /// Evict one entry (random selection).
    fn evict_one(&self) {
        // DashMap's iter().next() can be slow for large maps.
        // Instead, try to remove one entry directly by probing shards.
        let key_to_remove = self.entries.iter().next().map(|entry| entry.key().clone());
        if let Some(key) = key_to_remove {
            self.entries.remove(&key);
        }
    }

    /// Remove a specific session's cache.
    pub async fn remove(&self, session_id: &str) {
        self.entries.remove(session_id);
    }

    /// Current number of cached sessions.
    pub async fn len(&self) -> usize {
        self.len_sync()
    }

    /// Synchronous len.
    pub fn len_sync(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty.
    pub async fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Synchronous is_empty.
    pub fn is_empty_sync(&self) -> bool {
        self.entries.is_empty()
    }
}
