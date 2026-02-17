// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Serializable snapshot of variant statistics.

use serde::{Deserialize, Serialize};

/// Serializable snapshot of variant stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantStatsSnapshot {
    pub requests: u64,
    pub successes: u64,
    pub failures: u64,
    pub avg_latency_ms: f64,
    pub avg_tokens: f64,
    pub success_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_stats_snapshot_serialization() {
        let snapshot = VariantStatsSnapshot {
            requests: 100,
            successes: 95,
            failures: 5,
            avg_latency_ms: 50.5,
            avg_tokens: 75.3,
            success_rate: 0.95,
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: VariantStatsSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(snapshot.requests, deserialized.requests);
        assert_eq!(snapshot.successes, deserialized.successes);
        assert_eq!(snapshot.failures, deserialized.failures);
        assert!((snapshot.avg_latency_ms - deserialized.avg_latency_ms).abs() < 0.001);
        assert!((snapshot.success_rate - deserialized.success_rate).abs() < 0.001);
    }
}
