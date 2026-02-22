//! KV Cache Manager struct definition and write operations.
//!
//! # Panic Safety
//! This module uses poison-recovering lock guards to maintain cache availability
//! even if a thread panics while holding a lock.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use super::kv_cache_config::{
    lock_or_recover, write_or_recover, KvCacheConfig, KvCacheError, KvCacheStats, SequenceId,
};
use super::kv_quant::Q8KvStore;
use super::paged::{PageId, PageTable, PAGE_TOKENS};

/// Entry tracking for a cached sequence.
#[derive(Debug)]
pub(super) struct SequenceEntry {
    #[allow(dead_code)]
    pub(super) id: SequenceId,
    pub(super) page_ids: Vec<PageId>,
    pub(super) seq_len: usize,
    pub(super) last_access: Instant,
    pub(super) access_count: u64,
    pub(super) quant_store: Option<Q8KvStore>,
}

/// Integrated KV Cache Manager.
///
/// Combines paged attention with optional Q8 quantization for
/// efficient memory management during inference.
pub struct KvCacheManager {
    pub(super) config: KvCacheConfig,
    pub(super) page_table: RwLock<PageTable>,
    pub(super) sequences: RwLock<HashMap<SequenceId, SequenceEntry>>,
    pub(super) access_order: Mutex<VecDeque<SequenceId>>,
    pub(super) stats: Arc<KvCacheStats>,
    pub(super) next_seq_id: AtomicU64,
}

impl KvCacheManager {
    /// Create a new KV Cache Manager.
    pub fn new(config: KvCacheConfig) -> Self {
        let page_table = RwLock::new(PageTable::new(config.hidden_dim, config.max_pages));
        Self {
            config,
            page_table,
            sequences: RwLock::new(HashMap::new()),
            access_order: Mutex::new(VecDeque::new()),
            stats: Arc::new(KvCacheStats::default()),
            next_seq_id: AtomicU64::new(1),
        }
    }

    /// Allocate a new sequence in the cache.
    pub fn allocate_sequence(&self) -> SequenceId {
        let id = SequenceId(self.next_seq_id.fetch_add(1, Ordering::SeqCst));
        let quant_store = if self.config.enable_quantization {
            Some(Q8KvStore::new(self.config.hidden_dim, self.config.max_seq_len))
        } else {
            None
        };
        let entry = SequenceEntry {
            id,
            page_ids: Vec::new(),
            seq_len: 0,
            last_access: Instant::now(),
            access_count: 0,
            quant_store,
        };
        write_or_recover(&self.sequences).insert(id, entry);
        lock_or_recover(&self.access_order).push_back(id);
        id
    }

    /// Append KV pairs to a sequence.
    pub fn append_kv(
        &self,
        seq_id: SequenceId,
        keys: &[f32],
        values: &[f32],
    ) -> Result<(), KvCacheError> {
        let mut sequences = write_or_recover(&self.sequences);
        let entry = sequences
            .get_mut(&seq_id)
            .ok_or(KvCacheError::SequenceNotFound(seq_id.0))?;

        entry.last_access = Instant::now();
        entry.access_count += 1;
        let seq_pos = entry.seq_len;
        let slot = seq_pos % PAGE_TOKENS;

        if slot == 0 || entry.page_ids.is_empty() {
            self.allocate_page_for(entry, seq_pos)?;
        }

        self.write_to_page(seq_pos, slot, keys, values);
        Self::write_to_quant_store(entry, keys, values);
        entry.seq_len += 1;
        Ok(())
    }

    pub(super) fn allocate_page_for(
        &self,
        entry: &mut SequenceEntry,
        seq_pos: usize,
    ) -> Result<(), KvCacheError> {
        let mut page_table = write_or_recover(&self.page_table);
        let page_id = match page_table.allocate(seq_pos) {
            Some(id) => id,
            None => {
                drop(page_table);
                self.evict_lru()?;
                write_or_recover(&self.page_table)
                    .allocate(seq_pos)
                    .ok_or(KvCacheError::MemoryExhausted)?
            }
        };
        entry.page_ids.push(page_id);
        Ok(())
    }

    fn write_to_page(&self, seq_pos: usize, slot: usize, keys: &[f32], values: &[f32]) {
        let mut page_table = write_or_recover(&self.page_table);
        if let Some(page) = page_table.get_mut(seq_pos) {
            page.write(slot, keys, values);
        }
    }

    fn write_to_quant_store(entry: &mut SequenceEntry, keys: &[f32], values: &[f32]) {
        if let Some(ref mut qs) = entry.quant_store {
            if !qs.append(keys, values) {
                qs.reset();
                qs.append(keys, values);
            }
        }
    }

    /// Free a sequence and its pages.
    pub fn free_sequence(&self, seq_id: SequenceId) -> Result<(), KvCacheError> {
        let mut sequences = write_or_recover(&self.sequences);
        let entry = sequences
            .remove(&seq_id)
            .ok_or(KvCacheError::SequenceNotFound(seq_id.0))?;
        let mut page_table = write_or_recover(&self.page_table);
        page_table.free(&entry.page_ids);
        if let Ok(mut order) = self.access_order.lock() {
            order.retain(|&id| id != seq_id);
        }
        Ok(())
    }

    pub(super) fn evict_lru(&self) -> Result<(), KvCacheError> {
        let victim_id = lock_or_recover(&self.access_order).pop_front();
        if let Some(id) = victim_id {
            self.free_sequence(id)?;
        }
        Ok(())
    }

    pub(super) fn dot_product(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    /// Reset all cache state.
    pub fn reset(&self) {
        write_or_recover(&self.sequences).clear();
        lock_or_recover(&self.access_order).clear();
    }
}
