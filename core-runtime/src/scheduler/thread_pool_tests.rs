//! Tests for the configurable thread pool.

use super::super::thread_pool_types::*;
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[test]
fn test_thread_pool_basic() {
    let pool = ThreadPool::new(ThreadPoolConfig::default());
    assert!(pool.num_threads() > 0);

    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    pool.submit(Box::new(move || {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    }))
    .unwrap();

    thread::sleep(Duration::from_millis(100));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[test]
fn test_priority_tasks() {
    let config = ThreadPoolConfig { enable_priority: true, ..Default::default() };
    let pool = ThreadPool::new(config);

    let results = Arc::new(Mutex::new(Vec::new()));

    for i in 0..3 {
        let results_clone = results.clone();
        pool.submit_with_priority(
            Box::new(move || {
                results_clone.lock().unwrap().push(format!("normal-{}", i));
            }),
            TaskPriority::Normal,
        )
        .unwrap();
    }

    pool.submit_with_priority(Box::new(|| {}), TaskPriority::High).unwrap();

    thread::sleep(Duration::from_millis(100));

    let stats = pool.stats();
    assert!(stats.total_tasks_executed >= 3);
}

#[test]
fn test_config_presets() {
    let inference_config = ThreadPoolConfig::inference_optimized();
    assert!(inference_config.enable_work_stealing);
    assert!(inference_config.enable_priority);

    let batch_config = ThreadPoolConfig::batch_optimized();
    assert!(batch_config.enable_work_stealing);
    assert!(!batch_config.enable_priority);
}

#[test]
fn test_stats_tracking() {
    let pool = ThreadPool::new(ThreadPoolConfig::default());

    for _ in 0..10 {
        pool.submit(Box::new(|| {})).unwrap();
    }

    thread::sleep(Duration::from_millis(200));

    let stats = pool.stats();
    assert!(stats.total_tasks_executed >= 10);
}
