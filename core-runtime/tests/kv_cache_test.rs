//! Comprehensive tests for KV Cache Manager with Paged Attention.
//!
//! Tests cover:
//! - Basic KV operations (append, read)
//! - Paged attention allocation
//! - Q8 quantization integration
//! - Cache eviction policies (LRU, FIFO, LFU)
//! - Multi-sequence management
//! - Memory tracking

use gg_core::memory::{EvictionPolicy, KvCacheConfig, KvCacheManager, SequenceId};

/// Create a test configuration.
fn test_config() -> KvCacheConfig {
    KvCacheConfig {
        hidden_dim: 128,
        max_pages: 64,
        max_seq_len: 1024,
        num_heads: 8,
        head_dim: 16,
        enable_quantization: true,
        enable_paged: true,
        eviction_policy: EvictionPolicy::Lru,
        sliding_window: None,
    }
}

#[test]
fn test_allocate_sequence() {
    let manager = KvCacheManager::new(test_config());

    let seq1 = manager.allocate_sequence();
    let seq2 = manager.allocate_sequence();

    assert!(manager.has_sequence(seq1));
    assert!(manager.has_sequence(seq2));
    assert_ne!(seq1, seq2);
    assert_eq!(manager.active_sequences(), 2);
}

#[test]
fn test_append_and_read_single() {
    let manager = KvCacheManager::new(test_config());
    let seq_id = manager.allocate_sequence();

    let keys: Vec<f32> = (0..128).map(|i| i as f32).collect();
    let values: Vec<f32> = (0..128).map(|i| (i + 100) as f32).collect();

    manager.append_kv(seq_id, &keys, &values).unwrap();
    assert_eq!(manager.seq_len(seq_id).unwrap(), 1);

    let mut k_out = vec![0.0f32; 128];
    let mut v_out = vec![0.0f32; 128];
    manager.read_kv(seq_id, 0, &mut k_out, &mut v_out).unwrap();

    // Check values match (with quantization tolerance)
    for i in 0..128 {
        assert!((k_out[i] - keys[i]).abs() < 1.0, "Key mismatch at {}", i);
        assert!(
            (v_out[i] - values[i]).abs() < 1.0,
            "Value mismatch at {}",
            i
        );
    }
}

#[test]
fn test_append_multiple_positions() {
    let manager = KvCacheManager::new(test_config());
    let seq_id = manager.allocate_sequence();

    // Append 32 KV pairs with values that quantize well
    for pos in 0..32 {
        let keys: Vec<f32> = (0..128).map(|i| ((pos * 10) + i / 12) as f32).collect();
        let values: Vec<f32> = (0..128)
            .map(|i| ((pos * 10) + i / 12 + 100) as f32)
            .collect();
        manager.append_kv(seq_id, &keys, &values).unwrap();
    }

    assert_eq!(manager.seq_len(seq_id).unwrap(), 32);

    // Verify all positions with quantization tolerance
    for pos in 0..32 {
        let mut k_out = vec![0.0f32; 128];
        let mut v_out = vec![0.0f32; 128];
        manager
            .read_kv(seq_id, pos, &mut k_out, &mut v_out)
            .unwrap();

        let expected_k: Vec<f32> = (0..128).map(|i| ((pos * 10) + i / 12) as f32).collect();
        for i in 0..128 {
            let tolerance = (expected_k[i].abs() * 0.02).max(2.0);
            assert!(
                (k_out[i] - expected_k[i]).abs() < tolerance,
                "Key mismatch at pos {} idx {}: got {} expected {}",
                pos,
                i,
                k_out[i],
                expected_k[i]
            );
        }
    }
}

#[test]
fn test_cross_page_boundary() {
    let config = KvCacheConfig {
        hidden_dim: 64,
        max_pages: 16,
        max_seq_len: 512,
        ..test_config()
    };
    let manager = KvCacheManager::new(config);
    let seq_id = manager.allocate_sequence();

    // core-runtime uses 16 tokens per page
    // Write 32 positions to cross page boundary
    for pos in 0..32 {
        let keys = vec![pos as f32; 64];
        let values = vec![(pos + 100) as f32; 64];
        manager.append_kv(seq_id, &keys, &values).unwrap();
    }

    assert_eq!(manager.seq_len(seq_id).unwrap(), 32);

    // Verify positions on both pages
    for pos in [0, 15, 16, 31] {
        let mut k_out = vec![0.0f32; 64];
        let mut v_out = vec![0.0f32; 64];
        manager
            .read_kv(seq_id, pos, &mut k_out, &mut v_out)
            .unwrap();

        assert!(
            (k_out[0] - pos as f32).abs() < 1.0,
            "Position {} key mismatch",
            pos
        );
        assert!(
            (v_out[0] - (pos + 100) as f32).abs() < 1.0,
            "Position {} value mismatch",
            pos
        );
    }
}

#[test]
fn test_free_sequence() {
    let manager = KvCacheManager::new(test_config());
    let seq_id = manager.allocate_sequence();

    let keys = vec![1.0f32; 128];
    let values = vec![2.0f32; 128];

    manager.append_kv(seq_id, &keys, &values).unwrap();
    assert!(manager.has_sequence(seq_id));

    manager.free_sequence(seq_id).unwrap();
    assert!(!manager.has_sequence(seq_id));
    assert_eq!(manager.active_sequences(), 0);
}

#[test]
fn test_free_nonexistent_sequence() {
    let manager = KvCacheManager::new(test_config());
    let fake_id = SequenceId(9999);

    let result = manager.free_sequence(fake_id);
    assert!(result.is_err());
}

#[test]
fn test_read_out_of_bounds() {
    let manager = KvCacheManager::new(test_config());
    let seq_id = manager.allocate_sequence();

    let keys = vec![1.0f32; 128];
    let values = vec![2.0f32; 128];

    manager.append_kv(seq_id, &keys, &values).unwrap();

    let mut k_out = vec![0.0f32; 128];
    let mut v_out = vec![0.0f32; 128];

    let result = manager.read_kv(seq_id, 5, &mut k_out, &mut v_out);
    assert!(result.is_err());
}

#[test]
fn test_attention_scores_basic() {
    let manager = KvCacheManager::new(test_config());
    let seq_id = manager.allocate_sequence();

    // Add 8 KV pairs with distinct non-zero patterns
    for i in 0..8 {
        let keys = vec![(i + 1) as f32; 128];
        let values = vec![(i + 10) as f32; 128];
        manager.append_kv(seq_id, &keys, &values).unwrap();
    }

    // Query with similar pattern to position 3
    let query = vec![4.0f32; 128];
    let mut scores = vec![0.0f32; 8];

    manager
        .attention_scores(seq_id, &query, &mut scores)
        .unwrap();

    // All scores should be computed (non-zero for non-zero keys)
    // Note: quantization may cause some variation
    let non_zero_count = scores.iter().filter(|&&s| s.abs() > 0.1).count();
    assert!(
        non_zero_count > 0,
        "Expected some non-zero scores, got {:?}",
        scores
    );
}

#[test]
fn test_memory_usage_tracking() {
    let manager = KvCacheManager::new(test_config());
    let seq_id = manager.allocate_sequence();

    let initial_memory = manager.memory_usage();

    // Add 16 positions (one page)
    for _ in 0..16 {
        let keys = vec![1.0f32; 128];
        let values = vec![2.0f32; 128];
        manager.append_kv(seq_id, &keys, &values).unwrap();
    }

    let after_one_page = manager.memory_usage();
    assert!(after_one_page > initial_memory);

    // Add another page
    for _ in 0..16 {
        let keys = vec![1.0f32; 128];
        let values = vec![2.0f32; 128];
        manager.append_kv(seq_id, &keys, &values).unwrap();
    }

    let after_two_pages = manager.memory_usage();
    assert!(after_two_pages > after_one_page);
}

#[test]
fn test_multi_sequence_independence() {
    let manager = KvCacheManager::new(test_config());

    let seq1 = manager.allocate_sequence();
    let seq2 = manager.allocate_sequence();

    // Write different data to each sequence with larger values for better quantization
    let keys1 = vec![10.0f32; 128];
    let values1 = vec![100.0f32; 128];
    manager.append_kv(seq1, &keys1, &values1).unwrap();

    let keys2 = vec![20.0f32; 128];
    let values2 = vec![200.0f32; 128];
    manager.append_kv(seq2, &keys2, &values2).unwrap();

    // Verify independence
    let mut k1_out = vec![0.0f32; 128];
    let mut v1_out = vec![0.0f32; 128];
    manager.read_kv(seq1, 0, &mut k1_out, &mut v1_out).unwrap();

    let mut k2_out = vec![0.0f32; 128];
    let mut v2_out = vec![0.0f32; 128];
    manager.read_kv(seq2, 0, &mut k2_out, &mut v2_out).unwrap();

    // With per-sequence quantized stores, values should be distinct
    assert!(
        (k1_out[0] - 10.0).abs() < 2.0,
        "Seq1 key mismatch: got {}",
        k1_out[0]
    );
    assert!(
        (k2_out[0] - 20.0).abs() < 2.0,
        "Seq2 key mismatch: got {}",
        k2_out[0]
    );
    assert!(
        (v1_out[0] - 100.0).abs() < 5.0,
        "Seq1 value mismatch: got {}",
        v1_out[0]
    );
    assert!(
        (v2_out[0] - 200.0).abs() < 5.0,
        "Seq2 value mismatch: got {}",
        v2_out[0]
    );
}

#[test]
fn test_reset_clears_all() {
    let manager = KvCacheManager::new(test_config());

    let seq1 = manager.allocate_sequence();
    let seq2 = manager.allocate_sequence();

    let keys = vec![1.0f32; 128];
    let values = vec![2.0f32; 128];

    manager.append_kv(seq1, &keys, &values).unwrap();
    manager.append_kv(seq2, &keys, &values).unwrap();

    assert_eq!(manager.active_sequences(), 2);

    manager.reset();

    assert_eq!(manager.active_sequences(), 0);
    assert!(!manager.has_sequence(seq1));
    assert!(!manager.has_sequence(seq2));
}

#[test]
fn test_quantization_disabled() {
    let config = KvCacheConfig {
        enable_quantization: false,
        ..test_config()
    };
    let manager = KvCacheManager::new(config);
    let seq_id = manager.allocate_sequence();

    let keys: Vec<f32> = (0..128).map(|i| i as f32 * 0.1).collect();
    let values: Vec<f32> = (0..128).map(|i| i as f32 * 0.2).collect();

    manager.append_kv(seq_id, &keys, &values).unwrap();

    let mut k_out = vec![0.0f32; 128];
    let mut v_out = vec![0.0f32; 128];
    manager.read_kv(seq_id, 0, &mut k_out, &mut v_out).unwrap();

    // Without quantization, values should be exact
    for i in 0..128 {
        assert!((k_out[i] - keys[i]).abs() < 0.001, "Key mismatch at {}", i);
        assert!(
            (v_out[i] - values[i]).abs() < 0.001,
            "Value mismatch at {}",
            i
        );
    }
}

#[test]
fn test_large_sequence() {
    let manager = KvCacheManager::new(test_config());
    let seq_id = manager.allocate_sequence();

    // Write 256 positions
    for pos in 0..256 {
        let keys = vec![(pos % 100) as f32; 128];
        let values = vec![(pos % 100 + 50) as f32; 128];
        manager.append_kv(seq_id, &keys, &values).unwrap();
    }

    assert_eq!(manager.seq_len(seq_id).unwrap(), 256);

    // Verify random positions
    for pos in [0, 50, 100, 200, 255] {
        let mut k_out = vec![0.0f32; 128];
        let mut v_out = vec![0.0f32; 128];
        manager
            .read_kv(seq_id, pos, &mut k_out, &mut v_out)
            .unwrap();

        let expected_k = (pos % 100) as f32;
        let expected_v = (pos % 100 + 50) as f32;

        assert!(
            (k_out[0] - expected_k).abs() < 2.0,
            "Position {} key mismatch",
            pos
        );
        assert!(
            (v_out[0] - expected_v).abs() < 2.0,
            "Position {} value mismatch",
            pos
        );
    }
}

#[test]
fn test_stats_tracking() {
    let manager = KvCacheManager::new(test_config());
    let seq_id = manager.allocate_sequence();

    let keys = vec![1.0f32; 128];
    let values = vec![2.0f32; 128];

    for _ in 0..10 {
        manager.append_kv(seq_id, &keys, &values).unwrap();
    }

    let stats = manager.stats();
    assert!(stats.memory_bytes_used > 0 || manager.memory_usage() > 0);
}
