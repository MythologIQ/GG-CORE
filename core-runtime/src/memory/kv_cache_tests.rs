//! Tests for KV Cache Manager.

use super::*;
use crate::memory::paged::PAGE_TOKENS;

#[test]
fn test_kv_cache_basic() {
    let config = KvCacheConfig {
        hidden_dim: 128,
        max_pages: 16,
        max_seq_len: 256,
        ..Default::default()
    };
    let manager = KvCacheManager::new(config);
    let seq_id = manager.allocate_sequence();

    let keys = vec![1.0f32; 128];
    let values = vec![2.0f32; 128];
    manager.append_kv(seq_id, &keys, &values).unwrap();
    assert_eq!(manager.seq_len(seq_id).unwrap(), 1);

    let mut k_out = vec![0.0f32; 128];
    let mut v_out = vec![0.0f32; 128];
    manager.read_kv(seq_id, 0, &mut k_out, &mut v_out).unwrap();

    assert!(k_out.iter().all(|&x| (x - 1.0).abs() < 0.01));
    assert!(v_out.iter().all(|&x| (x - 2.0).abs() < 0.01));
}

#[test]
fn test_kv_cache_eviction() {
    let config = KvCacheConfig {
        hidden_dim: 128,
        max_pages: 2,
        max_seq_len: 64,
        eviction_policy: EvictionPolicy::Lru,
        ..Default::default()
    };
    let manager = KvCacheManager::new(config);
    let seq1 = manager.allocate_sequence();
    let seq2 = manager.allocate_sequence();

    let keys = vec![1.0f32; 128];
    let values = vec![2.0f32; 128];
    for _ in 0..16 {
        manager.append_kv(seq1, &keys, &values).unwrap();
    }
    assert!(manager.has_sequence(seq2));
}

#[test]
fn test_attention_scores() {
    let config = KvCacheConfig {
        hidden_dim: 64,
        max_pages: 16,
        max_seq_len: 256,
        enable_quantization: true,
        ..Default::default()
    };
    let manager = KvCacheManager::new(config);
    let seq_id = manager.allocate_sequence();

    for i in 0..10 {
        let keys: Vec<f32> = (0..64).map(|j| (i * 64 + j) as f32).collect();
        let values: Vec<f32> = (0..64).map(|j| (i * 64 + j + 1) as f32).collect();
        manager.append_kv(seq_id, &keys, &values).unwrap();
    }

    let query = vec![1.0f32; 64];
    let mut scores = vec![0.0f32; 10];
    manager
        .attention_scores(seq_id, &query, &mut scores)
        .unwrap();
    assert!(scores.iter().any(|&s| s != 0.0));
}

#[test]
fn test_sliding_window_eviction() {
    let config = KvCacheConfig {
        hidden_dim: 128,
        max_pages: 64,
        max_seq_len: 1024,
        enable_quantization: false,
        sliding_window: Some(SlidingWindowConfig {
            window_size: 32,
            overlap_tokens: 0,
        }),
        ..Default::default()
    };
    let manager = KvCacheManager::new(config);
    let seq_id = manager.allocate_sequence();

    let keys = vec![1.0f32; 128];
    let values = vec![2.0f32; 128];
    // Append 64 tokens (4 pages worth at PAGE_TOKENS=16)
    for _ in 0..64 {
        manager.append_kv(seq_id, &keys, &values).unwrap();
    }
    let pages_before = manager.sequence_page_count(seq_id);

    // Evict beyond window: position 64, window 32 => cutoff token 32
    // cutoff page = 32/16 = 2, so 2 pages should be evicted
    let evicted = manager.evict_beyond_window(seq_id, 64);
    assert_eq!(evicted, 2);

    let pages_after = manager.sequence_page_count(seq_id);
    assert_eq!(pages_after, pages_before - 2);
}

#[test]
fn test_sliding_window_overlap_preserved() {
    let config = KvCacheConfig {
        hidden_dim: 128,
        max_pages: 64,
        max_seq_len: 2048,
        enable_quantization: false,
        sliding_window: Some(SlidingWindowConfig {
            window_size: 32,
            overlap_tokens: 16,
        }),
        ..Default::default()
    };
    let manager = KvCacheManager::new(config);
    let seq_id = manager.allocate_sequence();

    let keys = vec![1.0f32; 128];
    let values = vec![2.0f32; 128];
    for _ in 0..80 {
        manager.append_kv(seq_id, &keys, &values).unwrap();
    }
    let pages_before = manager.sequence_page_count(seq_id);

    // position=80, window=32, overlap=16 => keep=48, cutoff=32
    // cutoff_page = 32/16 = 2
    let evicted = manager.evict_beyond_window(seq_id, 80);
    assert_eq!(evicted, 2);
    assert_eq!(manager.sequence_page_count(seq_id), pages_before - 2);

    // With smaller position where overlap saves all pages:
    // position=40, keep=48, cutoff=0 => no eviction
    let evicted2 = manager.evict_beyond_window(seq_id, 40);
    assert_eq!(evicted2, 0);
}

#[test]
fn test_sliding_window_memory_bounded() {
    // 10K tokens with 2K window should stay under page budget
    let window = 2048;
    let max_pages = (window / PAGE_TOKENS) + 4; // small headroom
    let config = KvCacheConfig {
        hidden_dim: 64,
        max_pages,
        max_seq_len: 12000,
        enable_quantization: false,
        sliding_window: Some(SlidingWindowConfig {
            window_size: window,
            overlap_tokens: 0,
        }),
        ..Default::default()
    };
    let manager = KvCacheManager::new(config);
    let seq_id = manager.allocate_sequence();

    let keys = vec![1.0f32; 64];
    let values = vec![2.0f32; 64];

    // Simulate appending 10K tokens, evicting periodically
    for token in 0..10_000 {
        manager.append_kv(seq_id, &keys, &values).unwrap();
        if token > 0 && token % PAGE_TOKENS == 0 {
            manager.evict_beyond_window(seq_id, token + 1);
        }
    }
    // After final eviction, pages should be within budget
    manager.evict_beyond_window(seq_id, 10_000);
    let pages = manager.sequence_page_count(seq_id);
    let budget = (window / PAGE_TOKENS) + 2;
    assert!(pages <= budget, "pages {pages} exceeds budget {budget}");
}

#[test]
fn test_sliding_window_noop_without_config() {
    let config = KvCacheConfig {
        hidden_dim: 128,
        max_pages: 16,
        max_seq_len: 256,
        sliding_window: None,
        ..Default::default()
    };
    let manager = KvCacheManager::new(config);
    let seq_id = manager.allocate_sequence();

    let keys = vec![1.0f32; 128];
    let values = vec![2.0f32; 128];
    for _ in 0..32 {
        manager.append_kv(seq_id, &keys, &values).unwrap();
    }
    let pages_before = manager.sequence_page_count(seq_id);
    let evicted = manager.evict_beyond_window(seq_id, 32);
    assert_eq!(evicted, 0);
    assert_eq!(manager.sequence_page_count(seq_id), pages_before);
}
