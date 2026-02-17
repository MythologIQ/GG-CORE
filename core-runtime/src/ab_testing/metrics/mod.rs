// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Per-variant metrics collection for A/B testing.

mod snapshot;
mod stats;

use std::collections::HashMap;

use dashmap::DashMap;

pub use snapshot::VariantStatsSnapshot;
pub use stats::VariantStats;

use crate::ab_testing::variant::VariantLabel;

/// Metrics collector for all variants.
///
/// Uses DashMap for thread-safe concurrent access without requiring mutable references.
/// This is critical for A/B testing where multiple threads may record metrics simultaneously.
pub struct VariantMetrics {
    variants: DashMap<VariantLabel, VariantStats>,
}

impl VariantMetrics {
    /// Create new metrics collector.
    pub fn new() -> Self {
        Self {
            variants: DashMap::new(),
        }
    }

    /// Get or create stats for a variant.
    ///
    /// Returns a reference to the stats that can be used for recording.
    /// The returned reference is valid until the DashMap is modified.
    pub fn get_or_create(
        &self,
        label: &VariantLabel,
    ) -> dashmap::mapref::one::RefMut<VariantLabel, VariantStats> {
        self.variants.entry(label.clone()).or_default()
    }

    /// Get stats for a variant (if exists).
    pub fn get(
        &self,
        label: &VariantLabel,
    ) -> Option<dashmap::mapref::one::Ref<VariantLabel, VariantStats>> {
        self.variants.get(label)
    }

    /// Get all variant snapshots.
    pub fn all_snapshots(&self) -> HashMap<VariantLabel, VariantStatsSnapshot> {
        self.variants
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().snapshot()))
            .collect()
    }

    /// Clear all metrics.
    pub fn clear(&self) {
        self.variants.clear();
    }
}

impl Default for VariantMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_variant_metrics() {
        let metrics = VariantMetrics::new();

        let control = VariantLabel::control();
        metrics.get_or_create(&control).record_request();
        metrics
            .get_or_create(&control)
            .record_success(Duration::from_millis(50), 100);

        let snapshots = metrics.all_snapshots();
        assert!(snapshots.contains_key(&control));
        assert_eq!(snapshots[&control].requests, 1);
    }

    #[test]
    fn test_variant_metrics_default() {
        let metrics = VariantMetrics::default();
        assert!(metrics.all_snapshots().is_empty());
    }

    #[test]
    fn test_variant_metrics_get_nonexistent() {
        let metrics = VariantMetrics::new();
        assert!(metrics.get(&VariantLabel::control()).is_none());
    }

    #[test]
    fn test_variant_metrics_get_or_create() {
        let metrics = VariantMetrics::new();

        // First call creates
        metrics.get_or_create(&VariantLabel::control());
        assert!(metrics.get(&VariantLabel::control()).is_some());

        // Second call returns existing
        let stats = metrics.get_or_create(&VariantLabel::control());
        stats.record_request();

        assert_eq!(
            metrics
                .get(&VariantLabel::control())
                .unwrap()
                .requests
                .load(Ordering::Relaxed),
            1
        );
    }

    #[test]
    fn test_variant_metrics_clear() {
        let metrics = VariantMetrics::new();

        metrics.get_or_create(&VariantLabel::control());
        metrics.get_or_create(&VariantLabel::treatment());

        assert_eq!(metrics.all_snapshots().len(), 2);

        metrics.clear();
        assert!(metrics.all_snapshots().is_empty());
    }

    #[test]
    fn test_variant_metrics_multiple_variants() {
        let metrics = VariantMetrics::new();

        let control = VariantLabel::control();
        let treatment = VariantLabel::treatment();
        let canary = VariantLabel::new("canary");

        metrics.get_or_create(&control).record_request();
        metrics.get_or_create(&treatment).record_request();
        metrics.get_or_create(&treatment).record_request();
        metrics.get_or_create(&canary).record_request();
        metrics.get_or_create(&canary).record_request();
        metrics.get_or_create(&canary).record_request();

        let snapshots = metrics.all_snapshots();
        assert_eq!(snapshots[&control].requests, 1);
        assert_eq!(snapshots[&treatment].requests, 2);
        assert_eq!(snapshots[&canary].requests, 3);
    }
}
