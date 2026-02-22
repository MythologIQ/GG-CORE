//! KV Cache read, attention, eviction, and query operations.

use super::kv_cache_config::{
    read_or_recover, write_or_recover, KvCacheError, KvCacheStats, SequenceId,
};
use super::kv_cache_core::KvCacheManager;
use super::paged::PAGE_TOKENS;

impl KvCacheManager {
    /// Read KV pairs from a sequence at given position.
    pub fn read_kv(
        &self,
        seq_id: SequenceId,
        pos: usize,
        keys_out: &mut [f32],
        values_out: &mut [f32],
    ) -> Result<(), KvCacheError> {
        let mut sequences = write_or_recover(&self.sequences);
        let entry = sequences
            .get_mut(&seq_id)
            .ok_or(KvCacheError::SequenceNotFound(seq_id.0))?;

        if pos >= entry.seq_len {
            return Err(KvCacheError::PositionOutOfBounds {
                pos,
                seq_len: entry.seq_len,
            });
        }
        entry.last_access = std::time::Instant::now();
        entry.access_count += 1;

        if let Some(ref qs) = entry.quant_store {
            if pos < qs.seq_len() {
                qs.read_keys(pos, keys_out);
                qs.read_values(pos, values_out);
                return Ok(());
            }
        }
        drop(sequences);
        self.read_from_page_table(pos, keys_out, values_out)
    }

    fn read_from_page_table(
        &self,
        pos: usize,
        keys_out: &mut [f32],
        values_out: &mut [f32],
    ) -> Result<(), KvCacheError> {
        let page_table = read_or_recover(&self.page_table);
        if let Some(page) = page_table.get(pos) {
            let slot = pos % PAGE_TOKENS;
            keys_out.copy_from_slice(page.read_keys(slot));
            values_out.copy_from_slice(page.read_values(slot));
            Ok(())
        } else {
            Err(KvCacheError::PageNotFound)
        }
    }

    /// Compute attention scores for a query against cached keys.
    pub fn attention_scores(
        &self,
        seq_id: SequenceId,
        query: &[f32],
        scores_out: &mut [f32],
    ) -> Result<(), KvCacheError> {
        let sequences = read_or_recover(&self.sequences);
        let entry = sequences
            .get(&seq_id)
            .ok_or(KvCacheError::SequenceNotFound(seq_id.0))?;
        let seq_len = entry.seq_len;

        if let Some(ref qs) = entry.quant_store {
            if qs.seq_len() >= seq_len {
                qs.attention_scores(query, scores_out);
                return Ok(());
            }
        }
        drop(sequences);
        self.attention_from_pages(seq_len, query, scores_out)
    }

    fn attention_from_pages(
        &self,
        seq_len: usize,
        query: &[f32],
        scores_out: &mut [f32],
    ) -> Result<(), KvCacheError> {
        let page_table = read_or_recover(&self.page_table);
        for pos in 0..seq_len {
            if let Some(page) = page_table.get(pos) {
                let slot = pos % PAGE_TOKENS;
                scores_out[pos] = Self::dot_product(query, page.read_keys(slot));
            }
        }
        Ok(())
    }

    /// Evict KV cache entries beyond the sliding window boundary.
    ///
    /// Given current sequence position, evicts all pages whose token range
    /// falls before `(position - window_size + overlap_tokens)`.
    /// Returns the number of pages evicted.
    pub fn evict_beyond_window(&self, seq_id: SequenceId, current_pos: usize) -> usize {
        let sw = match &self.config.sliding_window {
            Some(sw) => sw.clone(),
            None => return 0,
        };
        let keep = sw.window_size.saturating_add(sw.overlap_tokens);
        let cutoff = current_pos.saturating_sub(keep);
        if cutoff == 0 {
            return 0;
        }
        self.evict_pages_before(seq_id, cutoff)
    }

    fn evict_pages_before(&self, seq_id: SequenceId, cutoff_token: usize) -> usize {
        let mut sequences = write_or_recover(&self.sequences);
        let entry = match sequences.get_mut(&seq_id) {
            Some(e) => e,
            None => return 0,
        };
        let cutoff_page = cutoff_token / PAGE_TOKENS;
        if cutoff_page == 0 {
            return 0;
        }
        let evict_count = cutoff_page.min(entry.page_ids.len());
        let evicted: Vec<_> = entry.page_ids.drain(..evict_count).collect();
        let mut page_table = write_or_recover(&self.page_table);
        page_table.free(&evicted);
        evict_count
    }

    /// Get current statistics.
    pub fn stats(&self) -> KvCacheStats {
        (*self.stats).clone()
    }

    /// Get sequence length.
    pub fn seq_len(&self, seq_id: SequenceId) -> Result<usize, KvCacheError> {
        let sequences = read_or_recover(&self.sequences);
        let entry = sequences
            .get(&seq_id)
            .ok_or(KvCacheError::SequenceNotFound(seq_id.0))?;
        Ok(entry.seq_len)
    }

    /// Check if sequence exists.
    pub fn has_sequence(&self, seq_id: SequenceId) -> bool {
        read_or_recover(&self.sequences).contains_key(&seq_id)
    }

    /// Get number of active sequences.
    pub fn active_sequences(&self) -> usize {
        read_or_recover(&self.sequences).len()
    }

    /// Get memory usage in bytes.
    pub fn memory_usage(&self) -> usize {
        let page_table = read_or_recover(&self.page_table);
        let count = page_table.page_count();
        count * PAGE_TOKENS * self.config.hidden_dim * 2 * std::mem::size_of::<f32>()
    }

    /// Get page count for a sequence (for testing).
    pub fn sequence_page_count(&self, seq_id: SequenceId) -> usize {
        let sequences = read_or_recover(&self.sequences);
        sequences.get(&seq_id).map_or(0, |e| e.page_ids.len())
    }
}
