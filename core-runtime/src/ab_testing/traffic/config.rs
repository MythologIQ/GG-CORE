// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Traffic configuration and error types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ab_testing::variant::VariantLabel;

/// Traffic allocation configuration.
///
/// Uses BTreeMap for deterministic ordering of variant weights,
/// ensuring consistent traffic distribution across restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficConfig {
    /// Weight per variant (0-100, must sum to 100).
    /// BTreeMap ensures deterministic iteration order.
    pub weights: BTreeMap<VariantLabel, u8>,
    /// Use sticky sessions (same session_id -> same variant).
    pub sticky_sessions: bool,
}

impl Default for TrafficConfig {
    fn default() -> Self {
        let mut weights = BTreeMap::new();
        weights.insert(VariantLabel::control(), 100);
        Self {
            weights,
            sticky_sessions: true,
        }
    }
}

impl TrafficConfig {
    /// Create a 50/50 split between control and treatment.
    pub fn even_split() -> Self {
        let mut weights = BTreeMap::new();
        weights.insert(VariantLabel::control(), 50);
        weights.insert(VariantLabel::treatment(), 50);
        Self {
            weights,
            sticky_sessions: true,
        }
    }

    /// Create a canary deployment config (90/10 split).
    pub fn canary(canary_label: VariantLabel) -> Self {
        let mut weights = BTreeMap::new();
        weights.insert(VariantLabel::control(), 90);
        weights.insert(canary_label, 10);
        Self {
            weights,
            sticky_sessions: true,
        }
    }

    /// Validate that weights sum to 100.
    pub fn validate(&self) -> Result<(), TrafficError> {
        let total: u16 = self.weights.values().map(|&w| w as u16).sum();
        if total != 100 {
            return Err(TrafficError::InvalidWeights(total));
        }
        Ok(())
    }
}

/// Traffic splitting errors.
#[derive(Debug, Clone)]
pub enum TrafficError {
    /// Weights don't sum to 100.
    InvalidWeights(u16),
    /// No variants configured.
    NoVariants,
}

impl std::fmt::Display for TrafficError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWeights(sum) => write!(f, "Weights sum to {sum}, expected 100"),
            Self::NoVariants => write!(f, "No variants configured"),
        }
    }
}

impl std::error::Error for TrafficError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_even_split_validation() {
        let config = TrafficConfig::even_split();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_weights() {
        let mut weights = BTreeMap::new();
        weights.insert(VariantLabel::control(), 60);
        weights.insert(VariantLabel::treatment(), 60);
        let config = TrafficConfig {
            weights,
            sticky_sessions: true,
        };
        assert!(matches!(
            config.validate(),
            Err(TrafficError::InvalidWeights(120))
        ));
    }

    #[test]
    fn test_traffic_config_default() {
        let config = TrafficConfig::default();
        assert!(config.sticky_sessions);
        assert_eq!(config.weights.len(), 1);
        assert_eq!(config.weights.get(&VariantLabel::control()), Some(&100));
    }

    #[test]
    fn test_traffic_config_canary() {
        let canary_label = VariantLabel::new("canary-v1");
        let config = TrafficConfig::canary(canary_label.clone());

        assert!(config.validate().is_ok());
        assert_eq!(config.weights.get(&VariantLabel::control()), Some(&90));
        assert_eq!(config.weights.get(&canary_label), Some(&10));
    }

    #[test]
    fn test_traffic_error_display() {
        let err = TrafficError::InvalidWeights(120);
        assert!(err.to_string().contains("120"));
        assert!(err.to_string().contains("100"));

        let err = TrafficError::NoVariants;
        assert!(err.to_string().contains("No variants"));
    }

    #[test]
    fn test_validate_zero_weights() {
        let mut weights = BTreeMap::new();
        weights.insert(VariantLabel::control(), 0);
        weights.insert(VariantLabel::treatment(), 100);
        let config = TrafficConfig {
            weights,
            sticky_sessions: true,
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_traffic_config_serialization() {
        let config = TrafficConfig::even_split();

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TrafficConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.sticky_sessions, deserialized.sticky_sessions);
        assert_eq!(config.weights.len(), deserialized.weights.len());
    }

    #[test]
    fn test_traffic_error_is_error() {
        let err: Box<dyn std::error::Error> = Box::new(TrafficError::NoVariants);
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn test_three_variant_config() {
        let mut weights = BTreeMap::new();
        weights.insert(VariantLabel::new("a"), 33);
        weights.insert(VariantLabel::new("b"), 33);
        weights.insert(VariantLabel::new("c"), 34);
        let config = TrafficConfig {
            weights,
            sticky_sessions: true,
        };

        assert!(config.validate().is_ok());
    }
}
