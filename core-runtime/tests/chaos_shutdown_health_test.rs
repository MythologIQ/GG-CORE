// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Chaos & Resilience - Shutdown, Health, Resources, Combined Stress
//!
//! Shutdown drain, health check under failure, resource limits,
//! connection pool stress, and combined stress scenarios.

use std::sync::Arc;
use std::time::Duration;

use gg_core::engine::InferenceParams;
use gg_core::health::{HealthChecker, HealthConfig, HealthState};
use gg_core::ipc::{ConnectionConfig, ConnectionPool};
use gg_core::memory::{ResourceLimits, ResourceLimitsConfig};
use gg_core::scheduler::{Priority, RequestQueue, RequestQueueConfig};
use gg_core::shutdown::{ShutdownCoordinator, ShutdownResult, ShutdownState};

// ============================================================================
// Shutdown Under Stress
// ============================================================================

#[tokio::test]
async fn chaos_shutdown_rejects_new_requests() {
    let shutdown = Arc::new(ShutdownCoordinator::new());
    let sc = Arc::clone(&shutdown);
    let handle = tokio::spawn(async move {
        sc.initiate(Duration::from_millis(100)).await
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(shutdown.track().is_none());
    let _ = handle.await;
}

#[tokio::test]
async fn chaos_shutdown_drains_in_flight() {
    let shutdown = Arc::new(ShutdownCoordinator::new());
    let g1 = shutdown.track().unwrap();
    let g2 = shutdown.track().unwrap();
    assert_eq!(shutdown.in_flight_count(), 2);
    let sc = Arc::clone(&shutdown);
    let handle = tokio::spawn(async move {
        sc.initiate(Duration::from_millis(500)).await
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    drop(g1);
    tokio::time::sleep(Duration::from_millis(20)).await;
    drop(g2);
    assert_eq!(handle.await.unwrap(), ShutdownResult::Complete);
}

#[tokio::test]
async fn chaos_shutdown_timeout_with_stuck_requests() {
    let shutdown = Arc::new(ShutdownCoordinator::new());
    let _guard = shutdown.track().unwrap();
    match shutdown.initiate(Duration::from_millis(50)).await {
        ShutdownResult::Timeout { remaining } => assert_eq!(remaining, 1),
        ShutdownResult::Complete => panic!("Should have timed out"),
    }
}

#[tokio::test]
async fn chaos_shutdown_concurrent_track_during_drain() {
    let shutdown = Arc::new(ShutdownCoordinator::new());
    let sc = Arc::clone(&shutdown);
    tokio::spawn(async move { sc.initiate(Duration::from_millis(200)).await; });
    tokio::time::sleep(Duration::from_millis(10)).await;
    let mut handles = vec![];
    for _ in 0..10 {
        let s = Arc::clone(&shutdown);
        handles.push(tokio::spawn(async move { s.track().is_none() }));
    }
    let results: Vec<bool> = futures::future::join_all(handles)
        .await.into_iter().map(|r| r.unwrap()).collect();
    assert!(results.iter().all(|&r| r));
}

// ============================================================================
// Health Check Under Failure
// ============================================================================

#[test]
fn chaos_health_degraded_no_models() {
    let cfg = HealthConfig { require_model_loaded: true, max_queue_depth: 100 };
    let report = HealthChecker::new(cfg).report(ShutdownState::Running, 0, 0, 0);
    assert_eq!(report.state, HealthState::Degraded);
    assert!(!report.ready);
}

#[test]
fn chaos_health_degraded_queue_full() {
    let cfg = HealthConfig { require_model_loaded: false, max_queue_depth: 100 };
    let report = HealthChecker::new(cfg).report(ShutdownState::Running, 1, 1024, 100);
    assert_eq!(report.state, HealthState::Degraded);
    assert!(!report.ready);
}

#[test]
fn chaos_health_unhealthy_during_shutdown() {
    let report = HealthChecker::default().report(ShutdownState::Draining, 1, 1024, 0);
    assert_eq!(report.state, HealthState::Unhealthy);
    assert!(!report.accepting_requests);
}

#[test]
fn chaos_health_unhealthy_when_stopped() {
    let report = HealthChecker::default().report(ShutdownState::Stopped, 0, 0, 0);
    assert_eq!(report.state, HealthState::Unhealthy);
    assert!(!report.ready);
}

// ============================================================================
// Resource Limits
// ============================================================================

#[test]
fn chaos_resource_exceed_per_call_memory() {
    let limits = ResourceLimits::new(ResourceLimitsConfig {
        max_memory_per_call: 1024, max_total_memory: 4096, max_concurrent: 4,
    });
    assert!(limits.try_acquire(2048).is_err());
}

#[test]
fn chaos_resource_exceed_total_memory() {
    let limits = ResourceLimits::new(ResourceLimitsConfig {
        max_memory_per_call: 2048, max_total_memory: 3000, max_concurrent: 10,
    });
    let _g = limits.try_acquire(2000).unwrap();
    assert!(limits.try_acquire(1500).is_err());
}

#[test]
fn chaos_resource_exceed_concurrent_limit() {
    let limits = ResourceLimits::new(ResourceLimitsConfig {
        max_memory_per_call: 1024 * 1024,
        max_total_memory: 10 * 1024 * 1024,
        max_concurrent: 2,
    });
    let _g1 = limits.try_acquire(100).unwrap();
    let _g2 = limits.try_acquire(100).unwrap();
    assert!(limits.try_acquire(100).is_err());
}

#[test]
fn chaos_resource_release_then_reacquire() {
    let limits = ResourceLimits::new(ResourceLimitsConfig {
        max_memory_per_call: 1024, max_total_memory: 1024, max_concurrent: 1,
    });
    { let _g = limits.try_acquire(512).unwrap(); }
    assert_eq!(limits.current_memory(), 0);
    let _g2 = limits.try_acquire(512).unwrap();
    assert_eq!(limits.current_memory(), 512);
}

#[test]
fn chaos_resource_zero_and_exact_boundary() {
    let limits = ResourceLimits::new(ResourceLimitsConfig {
        max_memory_per_call: 1024, max_total_memory: 1024, max_concurrent: 2,
    });
    assert!(limits.try_acquire(0).is_ok());
    assert!(limits.try_acquire(1024).is_ok());
}

// ============================================================================
// Combined Stress Scenarios
// ============================================================================

#[tokio::test]
async fn chaos_combined_resource_limits_and_queue() {
    let limits = ResourceLimits::new(ResourceLimitsConfig {
        max_memory_per_call: 1024, max_total_memory: 2048, max_concurrent: 2,
    });
    let queue = RequestQueue::new(RequestQueueConfig { max_pending: 5, ..Default::default() });
    let mut guards = vec![];
    let mut enqueued = 0;
    for i in 0..10 {
        if let Ok(g) = limits.try_acquire(512) {
            guards.push(g);
            if queue.enqueue(
                "model".into(), format!("prompt {}", i),
                InferenceParams::default(), Priority::Normal,
            ).await.is_ok() { enqueued += 1; }
        }
    }
    assert!(enqueued > 0 && enqueued <= 5);
}

#[tokio::test]
async fn chaos_combined_shutdown_and_queue() {
    let shutdown = Arc::new(ShutdownCoordinator::new());
    let queue = Arc::new(RequestQueue::new(RequestQueueConfig { max_pending: 100, ..Default::default() }));
    for i in 0..10u32 {
        queue.enqueue("model".into(), format!("prompt {}", i), InferenceParams::default(), Priority::Normal)
            .await.unwrap();
    }
    let guard = shutdown.track().unwrap();
    let sc = Arc::clone(&shutdown);
    let handle = tokio::spawn(async move {
        sc.initiate(Duration::from_millis(200)).await
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(shutdown.track().is_none());
    drop(guard);
    assert_eq!(handle.await.unwrap(), ShutdownResult::Complete);
    assert!(!queue.is_empty().await);
}

#[test]
fn chaos_connection_pool_concurrent_stress() {
    let pool = Arc::new(ConnectionPool::new(ConnectionConfig { max_connections: 10 }));
    let mut handles = vec![];
    for _ in 0..8 {
        let p = Arc::clone(&pool);
        handles.push(std::thread::spawn(move || {
            for _ in 0..100 {
                if let Some(g) = p.try_acquire() {
                    std::thread::yield_now();
                    drop(g);
                }
            }
        }));
    }
    for h in handles { h.join().unwrap(); }
    assert_eq!(pool.active_count(), 0);
}

#[test]
fn chaos_connection_pool_exhaustion() {
    let pool = ConnectionPool::new(ConnectionConfig { max_connections: 3 });
    let _g1 = pool.try_acquire().unwrap();
    let _g2 = pool.try_acquire().unwrap();
    let _g3 = pool.try_acquire().unwrap();
    assert!(pool.try_acquire().is_none());
}

#[test]
fn chaos_connection_pool_zero_max() {
    let pool = ConnectionPool::new(ConnectionConfig { max_connections: 0 });
    assert!(pool.try_acquire().is_none());
}
