//! Tests for the smart model loader.
//!
//! Extracted from `smart_loader.rs` for Section 4 compliance.

use super::*;
use std::io::Write;
use std::time::Instant;
use tempfile::NamedTempFile;

fn create_test_model(size: usize) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(&vec![0u8; size]).unwrap();
    file.flush().unwrap();
    file
}

#[tokio::test]
async fn test_register_zero_overhead() {
    let loader = SmartLoader::new(SmartLoaderConfig::default());
    let file = create_test_model(1_000_000);

    loader
        .register("test".to_string(), file.path().to_path_buf(), ModelTier::Light)
        .await
        .unwrap();

    let status = loader.status().await;
    assert_eq!(status.registered_count, 1);
    assert_eq!(status.loaded_count, 0);
    assert_eq!(status.total_loaded_bytes, 0);
}

#[tokio::test]
async fn test_semantic_hint_quick_query() {
    let loader = SmartLoader::new(SmartLoaderConfig::default());

    let light = create_test_model(100_000);
    let quality = create_test_model(200_000);

    loader
        .register("light".to_string(), light.path().to_path_buf(), ModelTier::Light)
        .await.unwrap();
    loader
        .register("quality".to_string(), quality.path().to_path_buf(), ModelTier::Quality)
        .await.unwrap();

    loader.hint(LoadHint::QuickQuery).await;

    let status = loader.status().await;
    assert_eq!(status.predicted_next, Some("light".to_string()));
}

#[tokio::test]
async fn test_cache_hit_fast() {
    let loader = SmartLoader::new(SmartLoaderConfig::default());
    let file = create_test_model(100_000);

    loader
        .register("test".to_string(), file.path().to_path_buf(), ModelTier::Balanced)
        .await.unwrap();

    let start = Instant::now();
    loader.get("test").await.unwrap();
    let cold_time = start.elapsed();

    let start = Instant::now();
    loader.get("test").await.unwrap();
    let warm_time = start.elapsed();

    println!("Cold: {:?}, Warm: {:?}", cold_time, warm_time);

    let metrics = loader.metrics().await;
    assert_eq!(metrics.total_loads, 1);
    assert_eq!(metrics.cache_hits, 1);
}

#[tokio::test]
async fn test_tier_based_hints() {
    let loader = SmartLoader::new(SmartLoaderConfig::default());

    let light = create_test_model(100);
    let balanced = create_test_model(100);
    let quality = create_test_model(100);

    loader.register("l".to_string(), light.path().to_path_buf(), ModelTier::Light).await.unwrap();
    loader.register("b".to_string(), balanced.path().to_path_buf(), ModelTier::Balanced).await.unwrap();
    loader.register("q".to_string(), quality.path().to_path_buf(), ModelTier::Quality).await.unwrap();

    loader.hint(LoadHint::QuickQuery).await;
    assert_eq!(loader.status().await.predicted_next, Some("l".to_string()));

    loader.hint(LoadHint::ComplexTask).await;
    assert_eq!(loader.status().await.predicted_next, Some("q".to_string()));

    loader.hint(LoadHint::PreferModel { tier: ModelTier::Balanced }).await;
    assert_eq!(loader.status().await.predicted_next, Some("b".to_string()));
}
