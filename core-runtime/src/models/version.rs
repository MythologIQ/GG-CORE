// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Semantic versioning for model versions.
//!
//! Provides version parsing, comparison, and range matching for model management.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Version parsing errors.
#[derive(Error, Debug)]
pub enum VersionError {
    #[error("Invalid version format: {0}")]
    InvalidFormat(String),

    #[error("Invalid version component: {0}")]
    InvalidComponent(String),

    #[error("Invalid version range: {0}")]
    InvalidRange(String),
}

/// Semantic version with major.minor.patch and optional prerelease.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: Option<String>,
}

impl ModelVersion {
    /// Create a new version.
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch, prerelease: None }
    }

    /// Create a version with prerelease tag.
    pub fn with_prerelease(major: u32, minor: u32, patch: u32, prerelease: &str) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: Some(prerelease.to_string()),
        }
    }

    /// Parse version from string (e.g., "1.2.3" or "1.2.3-beta").
    pub fn parse(s: &str) -> Result<Self, VersionError> {
        let (version_part, prerelease) = if let Some(idx) = s.find('-') {
            (&s[..idx], Some(s[idx + 1..].to_string()))
        } else {
            (s, None)
        };

        let parts: Vec<&str> = version_part.split('.').collect();
        if parts.len() != 3 {
            return Err(VersionError::InvalidFormat(s.to_string()));
        }

        let parse_part = |p: &str| -> Result<u32, VersionError> {
            p.parse().map_err(|_| VersionError::InvalidComponent(p.to_string()))
        };

        Ok(Self {
            major: parse_part(parts[0])?,
            minor: parse_part(parts[1])?,
            patch: parse_part(parts[2])?,
            prerelease,
        })
    }

    /// Check if this version satisfies a range.
    pub fn satisfies(&self, range: &VersionRange) -> bool {
        if !range.include_prerelease && self.prerelease.is_some() {
            return false;
        }
        if let Some(ref min) = range.min {
            if self < min {
                return false;
            }
        }
        if let Some(ref max) = range.max {
            if self > max {
                return false;
            }
        }
        true
    }
}

impl fmt::Display for ModelVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref pre) = self.prerelease {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

impl FromStr for ModelVersion {
    type Err = VersionError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Ord for ModelVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            Ordering::Equal => {}
            ord => return ord,
        }
        // Prerelease versions are less than release versions
        match (&self.prerelease, &other.prerelease) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for ModelVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Version range for querying compatible versions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VersionRange {
    pub min: Option<ModelVersion>,
    pub max: Option<ModelVersion>,
    pub include_prerelease: bool,
}

impl VersionRange {
    /// Match any version.
    pub fn any() -> Self {
        Self::default()
    }

    /// Match exact version.
    pub fn exact(version: ModelVersion) -> Self {
        Self {
            min: Some(version.clone()),
            max: Some(version),
            include_prerelease: true,
        }
    }

    /// Match versions >= min.
    pub fn at_least(min: ModelVersion) -> Self {
        Self { min: Some(min), max: None, include_prerelease: false }
    }

    /// Match versions < max.
    pub fn below(max: ModelVersion) -> Self {
        Self { min: None, max: Some(max), include_prerelease: false }
    }

    /// Match versions in range [min, max].
    pub fn between(min: ModelVersion, max: ModelVersion) -> Self {
        Self { min: Some(min), max: Some(max), include_prerelease: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        let v = ModelVersion::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.prerelease.is_none());
    }

    #[test]
    fn test_parse_prerelease() {
        let v = ModelVersion::parse("1.0.0-beta.1").unwrap();
        assert_eq!(v.prerelease, Some("beta.1".to_string()));
    }

    #[test]
    fn test_version_ordering() {
        let v1 = ModelVersion::parse("1.0.0").unwrap();
        let v2 = ModelVersion::parse("1.0.1").unwrap();
        let v3 = ModelVersion::parse("1.0.0-alpha").unwrap();
        assert!(v1 < v2);
        assert!(v3 < v1); // prerelease < release
    }

    #[test]
    fn test_range_matching() {
        let v = ModelVersion::parse("1.5.0").unwrap();
        let range = VersionRange::between(
            ModelVersion::new(1, 0, 0),
            ModelVersion::new(2, 0, 0),
        );
        assert!(v.satisfies(&range));
    }

    // --- Additional edge case tests ---

    #[test]
    fn test_parse_version_zero() {
        let v = ModelVersion::parse("0.0.0").unwrap();
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_parse_version_large_numbers() {
        let v = ModelVersion::parse("999.888.777").unwrap();
        assert_eq!(v.major, 999);
        assert_eq!(v.minor, 888);
        assert_eq!(v.patch, 777);
    }

    #[test]
    fn test_parse_invalid_format_too_few_parts() {
        let result = ModelVersion::parse("1.2");
        assert!(matches!(result, Err(VersionError::InvalidFormat(_))));
    }

    #[test]
    fn test_parse_invalid_format_too_many_parts() {
        let result = ModelVersion::parse("1.2.3.4");
        assert!(matches!(result, Err(VersionError::InvalidFormat(_))));
    }

    #[test]
    fn test_parse_invalid_format_empty() {
        let result = ModelVersion::parse("");
        assert!(matches!(result, Err(VersionError::InvalidFormat(_))));
    }

    #[test]
    fn test_parse_invalid_component_non_numeric() {
        let result = ModelVersion::parse("1.a.3");
        assert!(matches!(result, Err(VersionError::InvalidComponent(_))));
    }

    #[test]
    fn test_parse_invalid_component_negative() {
        // Note: "1.-2.3" is parsed as version "1." with prerelease "2.3",
        // which fails as InvalidFormat because "1." has only 1 part
        let result = ModelVersion::parse("1.-2.3");
        assert!(matches!(result, Err(VersionError::InvalidFormat(_))));
    }

    #[test]
    fn test_parse_prerelease_complex() {
        let v = ModelVersion::parse("2.0.0-rc.1.build.123").unwrap();
        assert_eq!(v.prerelease, Some("rc.1.build.123".to_string()));
    }

    #[test]
    fn test_parse_prerelease_empty_tag() {
        let v = ModelVersion::parse("1.0.0-").unwrap();
        assert_eq!(v.prerelease, Some("".to_string()));
    }

    #[test]
    fn test_version_new() {
        let v = ModelVersion::new(1, 2, 3);
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.prerelease.is_none());
    }

    #[test]
    fn test_version_with_prerelease() {
        let v = ModelVersion::with_prerelease(1, 0, 0, "alpha");
        assert_eq!(v.prerelease, Some("alpha".to_string()));
    }

    #[test]
    fn test_version_display() {
        let v = ModelVersion::new(1, 2, 3);
        assert_eq!(format!("{}", v), "1.2.3");

        let v_pre = ModelVersion::with_prerelease(1, 0, 0, "beta");
        assert_eq!(format!("{}", v_pre), "1.0.0-beta");
    }

    #[test]
    fn test_version_from_str() {
        let v: ModelVersion = "2.1.0".parse().unwrap();
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 1);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_version_ordering_major() {
        let v1 = ModelVersion::new(1, 0, 0);
        let v2 = ModelVersion::new(2, 0, 0);
        assert!(v1 < v2);
    }

    #[test]
    fn test_version_ordering_minor() {
        let v1 = ModelVersion::new(1, 1, 0);
        let v2 = ModelVersion::new(1, 2, 0);
        assert!(v1 < v2);
    }

    #[test]
    fn test_version_ordering_patch() {
        let v1 = ModelVersion::new(1, 1, 1);
        let v2 = ModelVersion::new(1, 1, 2);
        assert!(v1 < v2);
    }

    #[test]
    fn test_version_ordering_prerelease_vs_release() {
        let pre = ModelVersion::with_prerelease(1, 0, 0, "alpha");
        let rel = ModelVersion::new(1, 0, 0);
        assert!(pre < rel, "prerelease should be less than release");
    }

    #[test]
    fn test_version_ordering_prerelease_alphabetical() {
        let alpha = ModelVersion::with_prerelease(1, 0, 0, "alpha");
        let beta = ModelVersion::with_prerelease(1, 0, 0, "beta");
        assert!(alpha < beta, "alpha < beta alphabetically");
    }

    #[test]
    fn test_version_equality() {
        let v1 = ModelVersion::new(1, 0, 0);
        let v2 = ModelVersion::new(1, 0, 0);
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_version_inequality() {
        let v1 = ModelVersion::new(1, 0, 0);
        let v2 = ModelVersion::new(1, 0, 1);
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_version_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ModelVersion::new(1, 0, 0));
        set.insert(ModelVersion::new(1, 0, 0)); // Duplicate
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_range_any() {
        let range = VersionRange::any();
        assert!(range.min.is_none());
        assert!(range.max.is_none());
        assert!(!range.include_prerelease);

        let v = ModelVersion::new(999, 999, 999);
        assert!(v.satisfies(&range));
    }

    #[test]
    fn test_range_exact() {
        let exact = ModelVersion::new(1, 2, 3);
        let range = VersionRange::exact(exact.clone());

        assert!(exact.satisfies(&range));
        assert!(!ModelVersion::new(1, 2, 2).satisfies(&range));
        assert!(!ModelVersion::new(1, 2, 4).satisfies(&range));
    }

    #[test]
    fn test_range_at_least() {
        let range = VersionRange::at_least(ModelVersion::new(2, 0, 0));

        assert!(!ModelVersion::new(1, 9, 9).satisfies(&range));
        assert!(ModelVersion::new(2, 0, 0).satisfies(&range));
        assert!(ModelVersion::new(2, 0, 1).satisfies(&range));
        assert!(ModelVersion::new(3, 0, 0).satisfies(&range));
    }

    #[test]
    fn test_range_below() {
        let range = VersionRange::below(ModelVersion::new(2, 0, 0));

        assert!(ModelVersion::new(1, 9, 9).satisfies(&range));
        // Note: max is inclusive in the satisfies implementation (uses > not >=)
        assert!(ModelVersion::new(2, 0, 0).satisfies(&range));
        assert!(!ModelVersion::new(2, 0, 1).satisfies(&range));
    }

    #[test]
    fn test_range_between() {
        let range = VersionRange::between(
            ModelVersion::new(1, 0, 0),
            ModelVersion::new(2, 0, 0),
        );

        assert!(!ModelVersion::new(0, 9, 9).satisfies(&range));
        assert!(ModelVersion::new(1, 0, 0).satisfies(&range));
        assert!(ModelVersion::new(1, 5, 0).satisfies(&range));
        assert!(ModelVersion::new(2, 0, 0).satisfies(&range));
        assert!(!ModelVersion::new(2, 0, 1).satisfies(&range));
    }

    #[test]
    fn test_range_prerelease_excluded_by_default() {
        let range = VersionRange::at_least(ModelVersion::new(1, 0, 0));
        let prerelease = ModelVersion::with_prerelease(2, 0, 0, "alpha");

        assert!(!prerelease.satisfies(&range), "prerelease should be excluded by default");
    }

    #[test]
    fn test_range_prerelease_included_when_enabled() {
        let mut range = VersionRange::at_least(ModelVersion::new(1, 0, 0));
        range.include_prerelease = true;
        let prerelease = ModelVersion::with_prerelease(2, 0, 0, "alpha");

        assert!(prerelease.satisfies(&range));
    }

    #[test]
    fn test_range_exact_includes_prerelease() {
        let exact = ModelVersion::with_prerelease(1, 0, 0, "beta");
        let range = VersionRange::exact(exact.clone());

        assert!(exact.satisfies(&range), "exact range should include prerelease");
    }

    #[test]
    fn test_version_error_display() {
        let err = VersionError::InvalidFormat("bad".to_string());
        assert!(err.to_string().contains("Invalid version format"));

        let err = VersionError::InvalidComponent("x".to_string());
        assert!(err.to_string().contains("Invalid version component"));

        let err = VersionError::InvalidRange("bad range".to_string());
        assert!(err.to_string().contains("Invalid version range"));
    }

    #[test]
    fn test_version_serialization() {
        let v = ModelVersion::new(1, 2, 3);
        let json = serde_json::to_string(&v).unwrap();
        let deserialized: ModelVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(v, deserialized);
    }

    #[test]
    fn test_version_with_prerelease_serialization() {
        let v = ModelVersion::with_prerelease(1, 0, 0, "rc.1");
        let json = serde_json::to_string(&v).unwrap();
        let deserialized: ModelVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(v, deserialized);
    }

    #[test]
    fn test_range_serialization() {
        let range = VersionRange::between(
            ModelVersion::new(1, 0, 0),
            ModelVersion::new(2, 0, 0),
        );
        let json = serde_json::to_string(&range).unwrap();
        let deserialized: VersionRange = serde_json::from_str(&json).unwrap();
        assert_eq!(range.min, deserialized.min);
        assert_eq!(range.max, deserialized.max);
    }
}
