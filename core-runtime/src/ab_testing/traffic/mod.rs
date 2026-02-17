// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Traffic splitting for A/B testing.
//!
//! Provides deterministic or random traffic allocation across variants.

mod bucket;
mod config;

pub use config::{TrafficConfig, TrafficError};

use crate::ab_testing::variant::VariantLabel;

/// Traffic splitter for routing requests to variants.
pub struct TrafficSplitter {
    config: TrafficConfig,
    /// Sorted variants with cumulative weights for selection.
    cumulative: Vec<(VariantLabel, u8)>,
}

impl TrafficSplitter {
    /// Create a new traffic splitter.
    pub fn new(config: TrafficConfig) -> Result<Self, TrafficError> {
        config.validate()?;
        if config.weights.is_empty() {
            return Err(TrafficError::NoVariants);
        }

        // Build cumulative weights for efficient selection
        let mut cumulative = Vec::with_capacity(config.weights.len());
        let mut running_total = 0u8;
        for (label, weight) in &config.weights {
            running_total += weight;
            cumulative.push((label.clone(), running_total));
        }

        Ok(Self { config, cumulative })
    }

    /// Select a variant for a request.
    pub fn select(&self, session_id: Option<&str>) -> &VariantLabel {
        let bucket = if self.config.sticky_sessions {
            if let Some(sid) = session_id {
                bucket::hash_to_bucket(sid)
            } else {
                bucket::random_bucket()
            }
        } else {
            bucket::random_bucket()
        };

        bucket::bucket_to_variant(bucket, &self.cumulative)
    }

    /// Get current configuration.
    pub fn config(&self) -> &TrafficConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn test_sticky_sessions() {
        let splitter = TrafficSplitter::new(TrafficConfig::even_split()).unwrap();

        // Same session ID should return same variant
        let v1 = splitter.select(Some("session-123"));
        let v2 = splitter.select(Some("session-123"));
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_distribution() {
        let splitter = TrafficSplitter::new(TrafficConfig::even_split()).unwrap();

        let mut control_count = 0;
        for i in 0..1000 {
            let sid = format!("session-{i}");
            if splitter.select(Some(&sid)) == &VariantLabel::control() {
                control_count += 1;
            }
        }

        // Should be roughly 50% (allow wide margin for hash distribution)
        assert!(control_count > 300 && control_count < 700);
    }

    #[test]
    fn test_splitter_no_variants() {
        // Note: Empty weights fail validate() first (sum 0 != 100),
        // so we get InvalidWeights(0) instead of NoVariants
        let config = TrafficConfig {
            weights: BTreeMap::new(),
            sticky_sessions: true,
        };
        let result = TrafficSplitter::new(config);
        assert!(matches!(result, Err(TrafficError::InvalidWeights(0))));
    }

    #[test]
    fn test_splitter_invalid_weights() {
        let mut weights = BTreeMap::new();
        weights.insert(VariantLabel::control(), 50);
        // Doesn't sum to 100
        let config = TrafficConfig {
            weights,
            sticky_sessions: true,
        };
        let result = TrafficSplitter::new(config);
        assert!(matches!(result, Err(TrafficError::InvalidWeights(50))));
    }

    #[test]
    fn test_splitter_config_accessor() {
        let config = TrafficConfig::even_split();
        let splitter = TrafficSplitter::new(config.clone()).unwrap();
        assert!(splitter.config().sticky_sessions);
    }

    #[test]
    fn test_splitter_deterministic_with_same_session() {
        let splitter = TrafficSplitter::new(TrafficConfig::even_split()).unwrap();

        // Same session should always get same variant
        let session = "user-abc-123";
        let variant = splitter.select(Some(session));

        for _ in 0..100 {
            assert_eq!(splitter.select(Some(session)), variant);
        }
    }

    #[test]
    fn test_splitter_random_without_session() {
        let mut config = TrafficConfig::even_split();
        config.sticky_sessions = false;
        let splitter = TrafficSplitter::new(config).unwrap();

        // Without session, selections may vary
        // Just verify it doesn't panic and returns a valid variant
        for _ in 0..100 {
            let variant = splitter.select(None);
            assert!(
                variant == &VariantLabel::control()
                    || variant == &VariantLabel::treatment()
            );
        }
    }

    #[test]
    fn test_splitter_100_percent_single_variant() {
        let config = TrafficConfig::default(); // 100% control
        let splitter = TrafficSplitter::new(config).unwrap();

        for i in 0..100 {
            let sid = format!("session-{}", i);
            assert_eq!(splitter.select(Some(&sid)), &VariantLabel::control());
        }
    }

    #[test]
    fn test_splitter_asymmetric_weights() {
        let mut weights = BTreeMap::new();
        weights.insert(VariantLabel::control(), 90);
        weights.insert(VariantLabel::treatment(), 10);
        let config = TrafficConfig {
            weights,
            sticky_sessions: true,
        };
        let splitter = TrafficSplitter::new(config).unwrap();

        let mut control_count = 0;
        for i in 0..1000 {
            let sid = format!("session-{}", i);
            if splitter.select(Some(&sid)) == &VariantLabel::control() {
                control_count += 1;
            }
        }

        // Should be roughly 90% (allow margin for hash distribution)
        assert!(
            control_count > 800,
            "Expected ~90% control, got {}",
            control_count
        );
    }

    #[test]
    fn test_splitter_empty_session_id() {
        let splitter = TrafficSplitter::new(TrafficConfig::even_split()).unwrap();

        // Empty string should still work
        let v1 = splitter.select(Some(""));
        let v2 = splitter.select(Some(""));
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_splitter_with_zero_weight_variant() {
        let mut weights = BTreeMap::new();
        weights.insert(VariantLabel::control(), 0);
        weights.insert(VariantLabel::treatment(), 100);
        let config = TrafficConfig {
            weights,
            sticky_sessions: true,
        };
        let splitter = TrafficSplitter::new(config).unwrap();

        // All selections should go to treatment
        for i in 0..100 {
            let sid = format!("session-{}", i);
            assert_eq!(splitter.select(Some(&sid)), &VariantLabel::treatment());
        }
    }

    #[test]
    fn test_sticky_sessions_disabled() {
        let mut config = TrafficConfig::even_split();
        config.sticky_sessions = false;
        let splitter = TrafficSplitter::new(config).unwrap();

        // With sticky sessions disabled, session ID is ignored
        // Just verify it doesn't panic
        let _ = splitter.select(Some("session-1"));
        let _ = splitter.select(Some("session-2"));
        let _ = splitter.select(None);
    }
}
