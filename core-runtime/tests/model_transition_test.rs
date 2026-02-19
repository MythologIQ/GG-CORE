//! Model transition tests for tiered model switching.
//!
//! Tests seamless transitions between CI, Default, and Quality model tiers.

use std::sync::Arc;
use std::time::{Duration, Instant};
use veritas_sdr::models::pool::{ModelPool, ModelTier, PoolConfig};
use veritas_sdr::models::registry::{ModelHandle, ModelRegistry};

/// Simulated model sizes for the three tiers.
const CI_MODEL_SIZE: usize = 491_000_000;        // ~491 MB (Qwen 0.5B)
const DEFAULT_MODEL_SIZE: usize = 1_100_000_000; // ~1.1 GB (Qwen 1.5B)
const QUALITY_MODEL_SIZE: usize = 2_200_000_000; // ~2.2 GB (Phi-3 Mini)

/// Test that all three tiers can be preloaded into the pool.
#[tokio::test]
async fn test_preload_all_tiers() {
    let registry = Arc::new(ModelRegistry::new());
    let config = PoolConfig {
        max_models: 3,
        max_memory_bytes: 4_000_000_000, // 4 GB
        ..Default::default()
    };
    let pool = ModelPool::new(config, registry.clone());

    // Preload all three tiers
    pool.preload(
        "qwen-0.5b".to_string(),
        ModelHandle::new(1),
        ModelTier::Testing,
        CI_MODEL_SIZE,
    ).await.expect("Failed to preload CI model");

    pool.preload(
        "qwen-1.5b".to_string(),
        ModelHandle::new(2),
        ModelTier::Default,
        DEFAULT_MODEL_SIZE,
    ).await.expect("Failed to preload default model");

    pool.preload(
        "phi-3-mini".to_string(),
        ModelHandle::new(3),
        ModelTier::Quality,
        QUALITY_MODEL_SIZE,
    ).await.expect("Failed to preload quality model");

    let status = pool.status().await;
    assert_eq!(status.model_count, 3);
    assert!(status.loaded_models.contains(&"qwen-0.5b".to_string()));
    assert!(status.loaded_models.contains(&"qwen-1.5b".to_string()));
    assert!(status.loaded_models.contains(&"phi-3-mini".to_string()));
}

/// Test instant switching between preloaded tiers.
#[tokio::test]
async fn test_instant_tier_switching() {
    let registry = Arc::new(ModelRegistry::new());
    let config = PoolConfig {
        max_models: 3,
        max_memory_bytes: 4_000_000_000,
        ..Default::default()
    };
    let pool = ModelPool::new(config, registry.clone());

    // Preload all tiers
    pool.preload("qwen-0.5b".to_string(), ModelHandle::new(1), ModelTier::Testing, CI_MODEL_SIZE).await.unwrap();
    pool.preload("qwen-1.5b".to_string(), ModelHandle::new(2), ModelTier::Default, DEFAULT_MODEL_SIZE).await.unwrap();
    pool.preload("phi-3-mini".to_string(), ModelHandle::new(3), ModelTier::Quality, QUALITY_MODEL_SIZE).await.unwrap();

    let mut switch_times = Vec::new();

    // Switch between all tiers multiple times
    let transitions = [
        "qwen-0.5b",  // Start with CI
        "qwen-1.5b",  // Up to Default
        "phi-3-mini", // Up to Quality
        "qwen-1.5b",  // Down to Default
        "qwen-0.5b",  // Down to CI
        "phi-3-mini", // Jump to Quality
    ];

    for model_id in transitions {
        let result = pool.switch_to(model_id).await.unwrap();
        switch_times.push(result.switch_latency);
        assert!(result.was_preloaded, "Model should be preloaded");
    }

    // All switches should be under 1ms (sub-millisecond)
    for (i, latency) in switch_times.iter().enumerate() {
        assert!(
            *latency < Duration::from_millis(1),
            "Switch {} took {:?}, expected < 1ms",
            i,
            latency
        );
    }

    // Average should be well under 100us
    let avg_ns: u64 = switch_times.iter().map(|d| d.as_nanos() as u64).sum::<u64>()
        / switch_times.len() as u64;
    println!("Average switch latency: {}ns", avg_ns);
    assert!(avg_ns < 100_000, "Average switch should be < 100us, was {}ns", avg_ns);
}

/// Test that warmup tracking affects subsequent switches.
#[tokio::test]
async fn test_warmup_affects_switch_info() {
    let registry = Arc::new(ModelRegistry::new());
    let pool = ModelPool::new(PoolConfig::default(), registry.clone());

    pool.preload("test".to_string(), ModelHandle::new(1), ModelTier::Default, 100).await.unwrap();

    // First switch - not warmed
    let result1 = pool.switch_to("test").await.unwrap();
    assert!(!result1.was_warmed);

    // Mark as warmed (simulating warmup inference completion)
    pool.mark_warmed("test").await;

    // Second switch - should show as warmed
    let result2 = pool.switch_to("test").await.unwrap();
    assert!(result2.was_warmed);
}

/// Test eviction priorities based on tier.
#[tokio::test]
async fn test_tier_based_eviction() {
    let registry = Arc::new(ModelRegistry::new());
    let config = PoolConfig {
        max_models: 2,
        max_memory_bytes: 4_000_000_000,
        ..Default::default()
    };
    let pool = ModelPool::new(config, registry.clone());

    // Load CI and Quality models
    pool.preload("ci".to_string(), ModelHandle::new(1), ModelTier::Testing, 100).await.unwrap();
    pool.preload("quality".to_string(), ModelHandle::new(2), ModelTier::Quality, 100).await.unwrap();

    // Adding a third should evict CI (lowest tier)
    pool.preload("default".to_string(), ModelHandle::new(3), ModelTier::Default, 100).await.unwrap();

    assert!(!pool.contains("ci").await, "CI model should be evicted");
    assert!(pool.contains("quality").await, "Quality model should remain");
    assert!(pool.contains("default").await, "Default model should be added");
}

/// Test active model protection from eviction.
#[tokio::test]
async fn test_active_model_protected() {
    let registry = Arc::new(ModelRegistry::new());
    let config = PoolConfig {
        max_models: 2,
        max_memory_bytes: 4_000_000_000,
        ..Default::default()
    };
    let pool = ModelPool::new(config, registry.clone());

    // Load two models
    pool.preload("ci".to_string(), ModelHandle::new(1), ModelTier::Testing, 100).await.unwrap();
    pool.preload("quality".to_string(), ModelHandle::new(2), ModelTier::Quality, 100).await.unwrap();

    // Activate CI (lowest tier)
    pool.switch_to("ci").await.unwrap();

    // Adding a third should evict quality (not active CI even though lower tier)
    pool.preload("default".to_string(), ModelHandle::new(3), ModelTier::Default, 100).await.unwrap();

    assert!(pool.contains("ci").await, "Active CI should be protected");
    assert!(!pool.contains("quality").await, "Inactive quality should be evicted");
}

/// Test metrics tracking.
#[tokio::test]
async fn test_pool_metrics() {
    let registry = Arc::new(ModelRegistry::new());
    let pool = ModelPool::new(PoolConfig::default(), registry.clone());

    pool.preload("test".to_string(), ModelHandle::new(1), ModelTier::Default, 100).await.unwrap();

    // Multiple switches
    for _ in 0..10 {
        pool.switch_to("test").await.unwrap();
    }

    let status = pool.status().await;
    assert_eq!(status.metrics.pool_hits, 10);
    assert!(status.metrics.avg_switch_latency_ns > 0);
    println!("Pool metrics: {:?}", status.metrics);
}

/// Benchmark: Measure transition latency distribution.
#[tokio::test]
async fn benchmark_transition_latency() {
    let registry = Arc::new(ModelRegistry::new());
    let config = PoolConfig {
        max_models: 3,
        max_memory_bytes: 4_000_000_000,
        ..Default::default()
    };
    let pool = ModelPool::new(config, registry.clone());

    // Preload all tiers
    pool.preload("ci".to_string(), ModelHandle::new(1), ModelTier::Testing, CI_MODEL_SIZE).await.unwrap();
    pool.preload("default".to_string(), ModelHandle::new(2), ModelTier::Default, DEFAULT_MODEL_SIZE).await.unwrap();
    pool.preload("quality".to_string(), ModelHandle::new(3), ModelTier::Quality, QUALITY_MODEL_SIZE).await.unwrap();

    let iterations = 1000;
    let mut latencies: Vec<Duration> = Vec::with_capacity(iterations * 3);

    let models = ["ci", "default", "quality"];

    let start = Instant::now();
    for i in 0..iterations {
        let model = models[i % 3];
        let result = pool.switch_to(model).await.unwrap();
        latencies.push(result.switch_latency);
    }
    let total_time = start.elapsed();

    // Calculate percentiles
    latencies.sort();
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[latencies.len() * 95 / 100];
    let p99 = latencies[latencies.len() * 99 / 100];
    let max = latencies.last().unwrap();

    println!("=== Transition Latency Benchmark ===");
    println!("Iterations: {}", iterations);
    println!("Total time: {:?}", total_time);
    println!("P50: {:?}", p50);
    println!("P95: {:?}", p95);
    println!("P99: {:?}", p99);
    println!("Max: {:?}", max);
    println!("Throughput: {:.0} switches/sec", iterations as f64 / total_time.as_secs_f64());

    // Assertions
    assert!(p50 < Duration::from_micros(50), "P50 should be < 50us");
    assert!(p99 < Duration::from_millis(1), "P99 should be < 1ms");
}

/// Test pool status reporting.
#[tokio::test]
async fn test_pool_status() {
    let registry = Arc::new(ModelRegistry::new());
    let config = PoolConfig {
        max_models: 3,
        max_memory_bytes: 4_000_000_000,
        ..Default::default()
    };
    let pool = ModelPool::new(config, registry.clone());

    pool.preload("ci".to_string(), ModelHandle::new(1), ModelTier::Testing, CI_MODEL_SIZE).await.unwrap();
    pool.preload("default".to_string(), ModelHandle::new(2), ModelTier::Default, DEFAULT_MODEL_SIZE).await.unwrap();

    pool.switch_to("default").await.unwrap();

    let status = pool.status().await;

    assert_eq!(status.model_count, 2);
    assert_eq!(status.total_memory_bytes, CI_MODEL_SIZE + DEFAULT_MODEL_SIZE);
    assert_eq!(status.active_model, Some("default".to_string()));
    assert_eq!(status.loaded_models.len(), 2);
}
