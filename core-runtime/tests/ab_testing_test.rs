//! Integration tests for the A/B testing module.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use veritas_sdr::ab_testing::{
    TrafficConfig, TrafficSplitter, Variant, VariantLabel, VariantMetrics,
};

// === TrafficSplitter variant selection consistency ===

#[test]
fn traffic_splitter_same_session_returns_same_variant() {
    let splitter = TrafficSplitter::new(TrafficConfig::even_split()).unwrap();
    let session = "user-abc-12345";
    let first = splitter.select(Some(session));
    for _ in 0..50 {
        assert_eq!(splitter.select(Some(session)), first);
    }
}

#[test]
fn traffic_splitter_different_sessions_distribute_traffic() {
    let splitter = TrafficSplitter::new(TrafficConfig::even_split()).unwrap();
    let mut control_count = 0;
    for i in 0..500 {
        let session = format!("session-{}", i);
        if splitter.select(Some(&session)) == &VariantLabel::control() {
            control_count += 1;
        }
    }
    assert!(control_count > 150 && control_count < 350);
}

#[test]
fn traffic_splitter_100_percent_to_single_variant() {
    let splitter = TrafficSplitter::new(TrafficConfig::default()).unwrap();
    for i in 0..100 {
        let session = format!("session-{}", i);
        assert_eq!(splitter.select(Some(&session)), &VariantLabel::control());
    }
}

// === TrafficConfig validation (even_split, canary) ===

#[test]
fn traffic_config_even_split_validates() {
    let config = TrafficConfig::even_split();
    assert!(config.validate().is_ok());
    assert_eq!(config.weights.len(), 2);
    assert_eq!(config.weights.get(&VariantLabel::control()), Some(&50));
    assert_eq!(config.weights.get(&VariantLabel::treatment()), Some(&50));
}

#[test]
fn traffic_config_canary_validates() {
    let canary_label = VariantLabel::new("v2-canary");
    let config = TrafficConfig::canary(canary_label.clone());
    assert!(config.validate().is_ok());
    assert_eq!(config.weights.get(&VariantLabel::control()), Some(&90));
    assert_eq!(config.weights.get(&canary_label), Some(&10));
}

#[test]
fn traffic_config_rejects_invalid_sum() {
    let mut weights = BTreeMap::new();
    weights.insert(VariantLabel::control(), 60);
    weights.insert(VariantLabel::treatment(), 50);
    let config = TrafficConfig { weights, sticky_sessions: true };
    assert!(config.validate().is_err());
}

#[test]
fn traffic_config_accepts_zero_weight_variant() {
    let mut weights = BTreeMap::new();
    weights.insert(VariantLabel::control(), 0);
    weights.insert(VariantLabel::treatment(), 100);
    let config = TrafficConfig { weights, sticky_sessions: true };
    assert!(config.validate().is_ok());
    let splitter = TrafficSplitter::new(config).unwrap();
    for i in 0..50 {
        assert_eq!(splitter.select(Some(&format!("s-{}", i))), &VariantLabel::treatment());
    }
}

// === Edge cases: empty config, single variant ===

#[test]
fn traffic_splitter_rejects_empty_config() {
    let config = TrafficConfig { weights: BTreeMap::new(), sticky_sessions: true };
    assert!(TrafficSplitter::new(config).is_err());
}

#[test]
fn traffic_splitter_single_variant_works() {
    let mut weights = BTreeMap::new();
    weights.insert(VariantLabel::new("only-one"), 100);
    let config = TrafficConfig { weights, sticky_sessions: true };
    let splitter = TrafficSplitter::new(config).unwrap();
    let expected = VariantLabel::new("only-one");
    for i in 0..50 {
        assert_eq!(splitter.select(Some(&format!("s-{}", i))), &expected);
    }
}

#[test]
fn traffic_splitter_three_way_split() {
    let mut weights = BTreeMap::new();
    weights.insert(VariantLabel::new("a"), 33);
    weights.insert(VariantLabel::new("b"), 33);
    weights.insert(VariantLabel::new("c"), 34);
    let config = TrafficConfig { weights, sticky_sessions: true };
    let splitter = TrafficSplitter::new(config).unwrap();
    let mut counts = [0usize; 3];
    for i in 0..900 {
        match splitter.select(Some(&format!("session-{}", i))).as_str() {
            "a" => counts[0] += 1,
            "b" => counts[1] += 1,
            "c" => counts[2] += 1,
            v => panic!("Unexpected variant: {}", v),
        }
    }
    for (i, count) in counts.iter().enumerate() {
        assert!(*count > 180 && *count < 405, "Variant {} got {}", ["a","b","c"][i], count);
    }
}

// === VariantMetrics concurrent access ===

#[test]
fn variant_metrics_concurrent_recording() {
    let metrics = Arc::new(VariantMetrics::new());
    let mut handles = vec![];
    for thread_id in 0..8 {
        let m = Arc::clone(&metrics);
        handles.push(thread::spawn(move || {
            let label = if thread_id % 2 == 0 {
                VariantLabel::control()
            } else {
                VariantLabel::treatment()
            };
            for _ in 0..50 {
                m.get_or_create(&label).record_request();
                m.get_or_create(&label).record_success(Duration::from_millis(10), 5);
            }
        }));
    }
    for h in handles { h.join().unwrap(); }
    let snapshots = metrics.all_snapshots();
    assert_eq!(snapshots.get(&VariantLabel::control()).unwrap().requests, 200);
    assert_eq!(snapshots.get(&VariantLabel::treatment()).unwrap().requests, 200);
}

#[test]
fn variant_metrics_isolated_per_variant() {
    let metrics = VariantMetrics::new();
    let control = VariantLabel::control();
    let treatment = VariantLabel::treatment();
    metrics.get_or_create(&control).record_request();
    metrics.get_or_create(&control).record_request();
    metrics.get_or_create(&control).record_success(Duration::from_millis(100), 50);
    metrics.get_or_create(&treatment).record_request();
    metrics.get_or_create(&treatment).record_failure();
    let snapshots = metrics.all_snapshots();
    assert_eq!(snapshots[&control].requests, 2);
    assert_eq!(snapshots[&control].successes, 1);
    assert_eq!(snapshots[&treatment].requests, 1);
    assert_eq!(snapshots[&treatment].failures, 1);
}

// === Integration: traffic splitting with metrics collection ===

#[test]
fn integration_traffic_splitting_with_metrics() {
    let splitter = TrafficSplitter::new(TrafficConfig::even_split()).unwrap();
    let metrics = VariantMetrics::new();
    for i in 0..100 {
        let variant = splitter.select(Some(&format!("user-{}", i)));
        metrics.get_or_create(variant).record_request();
        if i % 5 != 0 {
            metrics.get_or_create(variant).record_success(Duration::from_millis(50), 25);
        } else {
            metrics.get_or_create(variant).record_failure();
        }
    }
    let snapshots = metrics.all_snapshots();
    let total: u64 = snapshots.values().map(|s| s.requests).sum();
    assert_eq!(total, 100);
    assert!(snapshots.contains_key(&VariantLabel::control()));
    assert!(snapshots.contains_key(&VariantLabel::treatment()));
}

#[test]
fn integration_canary_deployment_scenario() {
    let canary = VariantLabel::new("model-v2");
    let splitter = TrafficSplitter::new(TrafficConfig::canary(canary.clone())).unwrap();
    let metrics = VariantMetrics::new();
    for i in 0..1000 {
        let variant = splitter.select(Some(&format!("req-{}", i)));
        metrics.get_or_create(variant).record_request();
        metrics.get_or_create(variant).record_success(Duration::from_millis(30), 20);
    }
    let snapshots = metrics.all_snapshots();
    let control_traffic = snapshots.get(&VariantLabel::control()).map(|s| s.requests).unwrap_or(0);
    let canary_traffic = snapshots.get(&canary).map(|s| s.requests).unwrap_or(0);
    assert!(canary_traffic > 50 && canary_traffic < 200, "Canary got {}", canary_traffic);
    assert!(control_traffic > 800, "Control got {}", control_traffic);
}

// === Variant type tests ===

#[test]
fn variant_creation_and_modification() {
    let variant = Variant::new(VariantLabel::control(), "llama-7b")
        .with_description("Production model")
        .disabled();
    assert_eq!(variant.label.as_str(), "control");
    assert_eq!(variant.model_id, "llama-7b");
    assert!(!variant.enabled);
    assert_eq!(variant.description, Some("Production model".to_string()));
}

#[test]
fn variant_label_ordering() {
    let mut labels = vec![
        VariantLabel::new("z-variant"),
        VariantLabel::new("a-variant"),
        VariantLabel::control(),
    ];
    labels.sort();
    assert_eq!(labels[0].as_str(), "a-variant");
    assert_eq!(labels[1].as_str(), "control");
    assert_eq!(labels[2].as_str(), "z-variant");
}
