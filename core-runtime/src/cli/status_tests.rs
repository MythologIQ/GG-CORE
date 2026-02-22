//! Tests for the status command.

use super::*;
use super::super::status_format::*;

#[test]
fn test_format_uptime() {
    assert_eq!(format_uptime(0), "0m");
    assert_eq!(format_uptime(59), "0m");
    assert_eq!(format_uptime(60), "1m");
    assert_eq!(format_uptime(3600), "1h 0m");
    assert_eq!(format_uptime(3661), "1h 1m");
    assert_eq!(format_uptime(86400), "1d 0h 0m");
    assert_eq!(format_uptime(90061), "1d 1h 1m");
}

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1024), "1.0 KB");
    assert_eq!(format_bytes(1536), "1.5 KB");
    assert_eq!(format_bytes(1048576), "1.0 MB");
    assert_eq!(format_bytes(1073741824), "1.0 GB");
}

#[test]
fn test_truncate() {
    assert_eq!(truncate("short", 10), "short");
    assert_eq!(truncate("this is a very long string", 10), "this is...");
}

#[test]
fn test_health_state_display() {
    assert_eq!(format!("{}", HealthState::Healthy), "healthy");
    assert_eq!(format!("{}", HealthState::Degraded), "degraded");
    assert_eq!(format!("{}", HealthState::Unhealthy), "unhealthy");
}

#[test]
fn test_model_state_display() {
    assert_eq!(format!("{}", ModelState::Loading), "loading");
    assert_eq!(format!("{}", ModelState::Ready), "ready");
    assert_eq!(format!("{}", ModelState::Unloading), "unloading");
    assert_eq!(format!("{}", ModelState::Error), "error");
}

#[test]
fn test_system_status_serialization() {
    let status = SystemStatus {
        health: HealthState::Healthy,
        uptime_secs: 3600,
        version: VersionInfo {
            version: "0.6.5".to_string(),
            commit: "abc123".to_string(),
            build_date: "2026-02-18".to_string(),
            rust_version: "1.75.0".to_string(),
        },
        models: vec![],
        requests: RequestStats {
            total_requests: 1000,
            successful_requests: 990,
            failed_requests: 10,
            requests_per_second: 10.5,
            avg_latency_ms: 50.0,
            p50_latency_ms: 45.0,
            p95_latency_ms: 100.0,
            p99_latency_ms: 150.0,
            tokens_generated: 50000,
            tokens_per_second: 25.0,
        },
        resources: ResourceUtilization {
            memory_rss_bytes: 4 * 1024 * 1024 * 1024,
            kv_cache_bytes: 2 * 1024 * 1024 * 1024,
            arena_bytes: 512 * 1024 * 1024,
            memory_limit_bytes: 8 * 1024 * 1024 * 1024,
            memory_utilization_percent: 50.0,
            cpu_utilization_percent: 75.0,
            active_threads: 8,
        },
        scheduler: SchedulerStatus {
            queue_depth: 5,
            active_batches: 2,
            pending_requests: 10,
            completed_requests: 1000,
            avg_batch_size: 4.5,
        },
        gpus: None,
        recent_events: vec![],
    };

    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("\"health\":\"healthy\""));
    assert!(json.contains("\"uptime_secs\":3600"));
}
