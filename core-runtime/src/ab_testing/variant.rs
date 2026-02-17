// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Variant labels and definitions for A/B testing.

use serde::{Deserialize, Serialize};

/// A variant label (e.g., "control", "v1.2-canary").
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VariantLabel(String);

impl VariantLabel {
    /// Create a new variant label.
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    /// Get the label as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Control variant label.
    pub fn control() -> Self {
        Self::new("control")
    }

    /// Treatment variant label.
    pub fn treatment() -> Self {
        Self::new("treatment")
    }
}

impl std::fmt::Display for VariantLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for VariantLabel {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// A model variant for A/B testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    /// Variant label.
    pub label: VariantLabel,
    /// Model ID to use for this variant.
    pub model_id: String,
    /// Whether this variant is enabled.
    pub enabled: bool,
    /// Optional description.
    pub description: Option<String>,
}

impl Variant {
    /// Create a new variant.
    pub fn new(label: VariantLabel, model_id: &str) -> Self {
        Self {
            label,
            model_id: model_id.to_string(),
            enabled: true,
            description: None,
        }
    }

    /// Create variant with description.
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Disable this variant.
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_label() {
        let label = VariantLabel::new("v1.2-canary");
        assert_eq!(label.as_str(), "v1.2-canary");
        assert_eq!(format!("{}", label), "v1.2-canary");
    }

    #[test]
    fn test_variant_creation() {
        let variant =
            Variant::new(VariantLabel::control(), "model-v1").with_description("Control group");

        assert!(variant.enabled);
        assert_eq!(variant.model_id, "model-v1");
        assert_eq!(variant.description, Some("Control group".to_string()));
    }

    // --- Additional tests ---

    #[test]
    fn test_variant_label_control() {
        let label = VariantLabel::control();
        assert_eq!(label.as_str(), "control");
    }

    #[test]
    fn test_variant_label_treatment() {
        let label = VariantLabel::treatment();
        assert_eq!(label.as_str(), "treatment");
    }

    #[test]
    fn test_variant_label_from_str() {
        let label: VariantLabel = "custom-variant".into();
        assert_eq!(label.as_str(), "custom-variant");
    }

    #[test]
    fn test_variant_label_equality() {
        let label1 = VariantLabel::new("test");
        let label2 = VariantLabel::new("test");
        let label3 = VariantLabel::new("other");

        assert_eq!(label1, label2);
        assert_ne!(label1, label3);
    }

    #[test]
    fn test_variant_label_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(VariantLabel::new("a"));
        set.insert(VariantLabel::new("a")); // Duplicate
        set.insert(VariantLabel::new("b"));

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_variant_label_empty() {
        let label = VariantLabel::new("");
        assert_eq!(label.as_str(), "");
    }

    #[test]
    fn test_variant_label_special_chars() {
        let label = VariantLabel::new("v1.0-beta_test@prod");
        assert_eq!(label.as_str(), "v1.0-beta_test@prod");
    }

    #[test]
    fn test_variant_new_enabled_by_default() {
        let variant = Variant::new(VariantLabel::control(), "model");
        assert!(variant.enabled);
    }

    #[test]
    fn test_variant_no_description_by_default() {
        let variant = Variant::new(VariantLabel::control(), "model");
        assert!(variant.description.is_none());
    }

    #[test]
    fn test_variant_disabled() {
        let variant = Variant::new(VariantLabel::control(), "model").disabled();
        assert!(!variant.enabled);
    }

    #[test]
    fn test_variant_chaining() {
        let variant = Variant::new(VariantLabel::treatment(), "model-v2")
            .with_description("Treatment group")
            .disabled();

        assert!(!variant.enabled);
        assert_eq!(variant.description, Some("Treatment group".to_string()));
        assert_eq!(variant.model_id, "model-v2");
    }

    #[test]
    fn test_variant_serialization() {
        let variant =
            Variant::new(VariantLabel::control(), "model-v1").with_description("Test variant");

        let json = serde_json::to_string(&variant).unwrap();
        let deserialized: Variant = serde_json::from_str(&json).unwrap();

        assert_eq!(variant.label, deserialized.label);
        assert_eq!(variant.model_id, deserialized.model_id);
        assert_eq!(variant.enabled, deserialized.enabled);
        assert_eq!(variant.description, deserialized.description);
    }

    #[test]
    fn test_variant_label_serialization() {
        let label = VariantLabel::new("test-label");

        let json = serde_json::to_string(&label).unwrap();
        let deserialized: VariantLabel = serde_json::from_str(&json).unwrap();

        assert_eq!(label, deserialized);
    }

    #[test]
    fn test_variant_label_clone() {
        let label1 = VariantLabel::new("test");
        let label2 = label1.clone();

        assert_eq!(label1, label2);
    }

    #[test]
    fn test_variant_clone() {
        let variant1 = Variant::new(VariantLabel::control(), "model").with_description("Desc");
        let variant2 = variant1.clone();

        assert_eq!(variant1.label, variant2.label);
        assert_eq!(variant1.model_id, variant2.model_id);
        assert_eq!(variant1.description, variant2.description);
    }

    #[test]
    fn test_variant_label_debug() {
        let label = VariantLabel::new("debug-test");
        let debug_str = format!("{:?}", label);
        assert!(debug_str.contains("debug-test"));
    }

    #[test]
    fn test_variant_debug() {
        let variant = Variant::new(VariantLabel::control(), "model");
        let debug_str = format!("{:?}", variant);
        assert!(debug_str.contains("control"));
        assert!(debug_str.contains("model"));
    }
}
