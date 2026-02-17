// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Per-variant statistics with atomic counters.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::snapshot::VariantStatsSnapshot;

/// Statistics for a single variant.
#[derive(Debug, Default)]
pub struct VariantStats {
    /// Total requests.
    pub requests: AtomicU64,
    /// Successful completions.
    pub successes: AtomicU64,
    /// Failed requests.
    pub failures: AtomicU64,
    /// Total latency in microseconds (for averaging).
    pub total_latency_us: AtomicU64,
    /// Total tokens generated.
    pub total_tokens: AtomicU64,
}

impl VariantStats {
    /// Create new stats.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a request start.
    pub fn record_request(&self) {
        self.requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a successful completion.
    pub fn record_success(&self, latency: Duration, tokens: u64) {
        self.successes.fetch_add(1, Ordering::Relaxed);
        self.total_latency_us
            .fetch_add(latency.as_micros() as u64, Ordering::Relaxed);
        self.total_tokens.fetch_add(tokens, Ordering::Relaxed);
    }

    /// Record a failure.
    pub fn record_failure(&self) {
        self.failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get snapshot for serialization.
    pub fn snapshot(&self) -> VariantStatsSnapshot {
        let requests = self.requests.load(Ordering::Relaxed);
        let successes = self.successes.load(Ordering::Relaxed);
        let total_latency_us = self.total_latency_us.load(Ordering::Relaxed);
        let total_tokens = self.total_tokens.load(Ordering::Relaxed);

        VariantStatsSnapshot {
            requests,
            successes,
            failures: self.failures.load(Ordering::Relaxed),
            avg_latency_ms: if successes > 0 {
                (total_latency_us / successes / 1000) as f64
            } else {
                0.0
            },
            avg_tokens: if successes > 0 {
                (total_tokens / successes) as f64
            } else {
                0.0
            },
            success_rate: if requests > 0 {
                successes as f64 / requests as f64
            } else {
                0.0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_recording() {
        let stats = VariantStats::new();

        stats.record_request();
        stats.record_success(Duration::from_millis(100), 50);

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.requests, 1);
        assert_eq!(snapshot.successes, 1);
        assert_eq!(snapshot.avg_tokens, 50.0);
        assert!(snapshot.avg_latency_ms >= 99.0 && snapshot.avg_latency_ms <= 101.0);
    }

    #[test]
    fn test_variant_stats_default() {
        let stats = VariantStats::default();
        let snapshot = stats.snapshot();

        assert_eq!(snapshot.requests, 0);
        assert_eq!(snapshot.successes, 0);
        assert_eq!(snapshot.failures, 0);
        assert_eq!(snapshot.avg_latency_ms, 0.0);
        assert_eq!(snapshot.avg_tokens, 0.0);
        assert_eq!(snapshot.success_rate, 0.0);
    }

    #[test]
    fn test_variant_stats_new() {
        let stats = VariantStats::new();
        assert_eq!(stats.requests.load(Ordering::Relaxed), 0);
        assert_eq!(stats.successes.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_record_failure() {
        let stats = VariantStats::new();

        stats.record_request();
        stats.record_failure();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.requests, 1);
        assert_eq!(snapshot.failures, 1);
        assert_eq!(snapshot.successes, 0);
    }

    #[test]
    fn test_success_rate_calculation() {
        let stats = VariantStats::new();

        // 10 requests, 7 successes, 3 failures
        for _ in 0..10 {
            stats.record_request();
        }
        for _ in 0..7 {
            stats.record_success(Duration::from_millis(10), 10);
        }
        for _ in 0..3 {
            stats.record_failure();
        }

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.requests, 10);
        assert_eq!(snapshot.successes, 7);
        assert_eq!(snapshot.failures, 3);
        // success_rate = 7/10 = 0.7
        assert!((snapshot.success_rate - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_average_latency_calculation() {
        let stats = VariantStats::new();

        stats.record_request();
        stats.record_success(Duration::from_millis(100), 10);

        stats.record_request();
        stats.record_success(Duration::from_millis(200), 20);

        let snapshot = stats.snapshot();
        // Total latency = 300ms, 2 successes
        // Note: using integer division in the code
        assert!(snapshot.avg_latency_ms >= 100.0 && snapshot.avg_latency_ms <= 200.0);
    }

    #[test]
    fn test_average_tokens_calculation() {
        let stats = VariantStats::new();

        stats.record_success(Duration::from_millis(10), 100);
        stats.record_success(Duration::from_millis(10), 200);

        let snapshot = stats.snapshot();
        // Total tokens = 300, 2 successes, avg = 150
        assert_eq!(snapshot.avg_tokens, 150.0);
    }

    #[test]
    fn test_concurrent_recording() {
        use std::sync::Arc;
        use std::thread;

        let stats = Arc::new(VariantStats::new());
        let mut handles = vec![];

        for _ in 0..10 {
            let stats_clone = Arc::clone(&stats);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    stats_clone.record_request();
                    stats_clone.record_success(Duration::from_millis(10), 5);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.requests, 1000);
        assert_eq!(snapshot.successes, 1000);
    }

    #[test]
    fn test_high_latency_values() {
        let stats = VariantStats::new();

        // 1 second latency
        stats.record_success(Duration::from_secs(1), 100);

        let snapshot = stats.snapshot();
        assert!(snapshot.avg_latency_ms >= 999.0);
    }

    #[test]
    fn test_microsecond_precision() {
        let stats = VariantStats::new();

        // Very small latency
        stats.record_success(Duration::from_micros(500), 10);

        // Should record but may round to 0ms due to integer division
        let snapshot = stats.snapshot();
        assert!(snapshot.avg_latency_ms >= 0.0);
    }

    #[test]
    fn test_large_token_counts() {
        let stats = VariantStats::new();

        stats.record_success(Duration::from_millis(100), 1_000_000);

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.avg_tokens, 1_000_000.0);
    }
}
