// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Model search and query API.
//!
//! Provides a builder-pattern query interface for searching the model registry.

use serde::{Deserialize, Serialize};

use super::manifest::{ModelArchitecture, ModelCapability};
// Note: ModelHandle is runtime-specific and not serializable.
// Search results use model_id strings for persistence.
use super::version::{ModelVersion, VersionRange};

/// Query for searching models in the registry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelQuery {
    /// Filter by capability.
    pub capability: Option<ModelCapability>,
    /// Filter by architecture.
    pub architecture: Option<ModelArchitecture>,
    /// Filter by version range.
    pub version_range: Option<VersionRange>,
    /// Filter by name pattern (substring match).
    pub name_pattern: Option<String>,
    /// Maximum results to return.
    pub limit: Option<usize>,
    /// Skip first N results.
    pub offset: Option<usize>,
}

impl ModelQuery {
    /// Create a new empty query (matches all).
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a query builder.
    pub fn builder() -> ModelQueryBuilder {
        ModelQueryBuilder::new()
    }

    /// Check if a model matches this query.
    pub fn matches(
        &self,
        name: &str,
        version: &ModelVersion,
        capabilities: &[ModelCapability],
        architecture: &ModelArchitecture,
    ) -> bool {
        if let Some(ref cap) = self.capability {
            if !capabilities.contains(cap) {
                return false;
            }
        }
        if let Some(ref arch) = self.architecture {
            if architecture != arch {
                return false;
            }
        }
        if let Some(ref range) = self.version_range {
            if !version.satisfies(range) {
                return false;
            }
        }
        if let Some(ref pattern) = self.name_pattern {
            if !name.to_lowercase().contains(&pattern.to_lowercase()) {
                return false;
            }
        }
        true
    }
}

/// Builder for constructing model queries.
#[derive(Debug, Default)]
pub struct ModelQueryBuilder {
    query: ModelQuery,
}

impl ModelQueryBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by capability.
    pub fn capability(mut self, cap: ModelCapability) -> Self {
        self.query.capability = Some(cap);
        self
    }

    /// Filter by architecture.
    pub fn architecture(mut self, arch: ModelArchitecture) -> Self {
        self.query.architecture = Some(arch);
        self
    }

    /// Filter by version range.
    pub fn version_range(mut self, range: VersionRange) -> Self {
        self.query.version_range = Some(range);
        self
    }

    /// Filter by name pattern.
    pub fn name_contains(mut self, pattern: &str) -> Self {
        self.query.name_pattern = Some(pattern.to_string());
        self
    }

    /// Limit results.
    pub fn limit(mut self, n: usize) -> Self {
        self.query.limit = Some(n);
        self
    }

    /// Skip first N results.
    pub fn offset(mut self, n: usize) -> Self {
        self.query.offset = Some(n);
        self
    }

    /// Build the query.
    pub fn build(self) -> ModelQuery {
        self.query
    }
}

/// Search result containing matched model info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSearchResult {
    /// Model identifier.
    pub model_id: String,
    /// Model version.
    pub version: ModelVersion,
    /// Model capabilities.
    pub capabilities: Vec<ModelCapability>,
    /// Model architecture.
    pub architecture: ModelArchitecture,
    /// Model size in bytes.
    pub size_bytes: u64,
    /// Whether model is currently loaded.
    pub is_loaded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_builder() {
        let query = ModelQuery::builder()
            .capability(ModelCapability::TextGeneration)
            .architecture(ModelArchitecture::Gguf)
            .limit(10)
            .build();

        assert_eq!(query.capability, Some(ModelCapability::TextGeneration));
        assert_eq!(query.architecture, Some(ModelArchitecture::Gguf));
        assert_eq!(query.limit, Some(10));
    }

    #[test]
    fn test_query_matching() {
        let query = ModelQuery::builder()
            .capability(ModelCapability::TextGeneration)
            .name_contains("phi")
            .build();

        let version = ModelVersion::new(1, 0, 0);
        let caps = vec![ModelCapability::TextGeneration];
        let arch = ModelArchitecture::Gguf;

        assert!(query.matches("phi3-mini", &version, &caps, &arch));
        assert!(!query.matches("llama-7b", &version, &caps, &arch));
    }

    // --- Additional tests ---

    #[test]
    fn test_query_new_default() {
        let query = ModelQuery::new();
        assert!(query.capability.is_none());
        assert!(query.architecture.is_none());
        assert!(query.version_range.is_none());
        assert!(query.name_pattern.is_none());
        assert!(query.limit.is_none());
        assert!(query.offset.is_none());
    }

    #[test]
    fn test_empty_query_matches_all() {
        let query = ModelQuery::new();
        let version = ModelVersion::new(1, 0, 0);
        let caps = vec![ModelCapability::TextGeneration];
        let arch = ModelArchitecture::Gguf;

        assert!(query.matches("any-model", &version, &caps, &arch));
        assert!(query.matches("another-model", &version, &caps, &arch));
    }

    #[test]
    fn test_query_builder_offset() {
        let query = ModelQuery::builder()
            .offset(5)
            .build();
        assert_eq!(query.offset, Some(5));
    }

    #[test]
    fn test_query_builder_name_contains() {
        let query = ModelQuery::builder()
            .name_contains("llama")
            .build();
        assert_eq!(query.name_pattern, Some("llama".to_string()));
    }

    #[test]
    fn test_query_builder_version_range() {
        let range = VersionRange::at_least(ModelVersion::new(2, 0, 0));
        let query = ModelQuery::builder()
            .version_range(range.clone())
            .build();
        assert!(query.version_range.is_some());
    }

    #[test]
    fn test_query_matches_capability_mismatch() {
        let query = ModelQuery::builder()
            .capability(ModelCapability::Embedding)
            .build();

        let version = ModelVersion::new(1, 0, 0);
        let caps = vec![ModelCapability::TextGeneration]; // No Embedding
        let arch = ModelArchitecture::Gguf;

        assert!(!query.matches("model", &version, &caps, &arch));
    }

    #[test]
    fn test_query_matches_capability_present() {
        let query = ModelQuery::builder()
            .capability(ModelCapability::Embedding)
            .build();

        let version = ModelVersion::new(1, 0, 0);
        let caps = vec![ModelCapability::TextGeneration, ModelCapability::Embedding];
        let arch = ModelArchitecture::Gguf;

        assert!(query.matches("model", &version, &caps, &arch));
    }

    #[test]
    fn test_query_matches_architecture_mismatch() {
        let query = ModelQuery::builder()
            .architecture(ModelArchitecture::Onnx)
            .build();

        let version = ModelVersion::new(1, 0, 0);
        let caps = vec![ModelCapability::TextGeneration];
        let arch = ModelArchitecture::Gguf; // Mismatch

        assert!(!query.matches("model", &version, &caps, &arch));
    }

    #[test]
    fn test_query_matches_version_range() {
        let range = VersionRange::at_least(ModelVersion::new(2, 0, 0));
        let query = ModelQuery::builder()
            .version_range(range)
            .build();

        let old_version = ModelVersion::new(1, 0, 0);
        let new_version = ModelVersion::new(2, 5, 0);
        let caps = vec![ModelCapability::TextGeneration];
        let arch = ModelArchitecture::Gguf;

        assert!(!query.matches("model", &old_version, &caps, &arch));
        assert!(query.matches("model", &new_version, &caps, &arch));
    }

    #[test]
    fn test_query_matches_name_case_insensitive() {
        let query = ModelQuery::builder()
            .name_contains("PHI")
            .build();

        let version = ModelVersion::new(1, 0, 0);
        let caps = vec![ModelCapability::TextGeneration];
        let arch = ModelArchitecture::Gguf;

        assert!(query.matches("phi3-mini", &version, &caps, &arch));
        assert!(query.matches("PHI-Large", &version, &caps, &arch));
        assert!(query.matches("mixed-PhI-case", &version, &caps, &arch));
    }

    #[test]
    fn test_query_matches_name_partial() {
        let query = ModelQuery::builder()
            .name_contains("ama")
            .build();

        let version = ModelVersion::new(1, 0, 0);
        let caps = vec![ModelCapability::TextGeneration];
        let arch = ModelArchitecture::Gguf;

        assert!(query.matches("llama-7b", &version, &caps, &arch));
        assert!(!query.matches("phi3-mini", &version, &caps, &arch));
    }

    #[test]
    fn test_query_matches_combined_filters() {
        let query = ModelQuery::builder()
            .capability(ModelCapability::TextGeneration)
            .architecture(ModelArchitecture::Gguf)
            .name_contains("phi")
            .build();

        let version = ModelVersion::new(1, 0, 0);
        let caps = vec![ModelCapability::TextGeneration];
        let gguf_arch = ModelArchitecture::Gguf;
        let onnx_arch = ModelArchitecture::Onnx;

        // All match
        assert!(query.matches("phi3-mini", &version, &caps, &gguf_arch));
        // Name mismatch
        assert!(!query.matches("llama-7b", &version, &caps, &gguf_arch));
        // Arch mismatch
        assert!(!query.matches("phi3-onnx", &version, &caps, &onnx_arch));
    }

    #[test]
    fn test_query_serialization() {
        let query = ModelQuery::builder()
            .capability(ModelCapability::TextGeneration)
            .limit(10)
            .build();

        let json = serde_json::to_string(&query).unwrap();
        let deserialized: ModelQuery = serde_json::from_str(&json).unwrap();

        assert_eq!(query.capability, deserialized.capability);
        assert_eq!(query.limit, deserialized.limit);
    }

    #[test]
    fn test_model_search_result_serialization() {
        let result = ModelSearchResult {
            model_id: "test-model".to_string(),
            version: ModelVersion::new(1, 0, 0),
            capabilities: vec![ModelCapability::TextGeneration],
            architecture: ModelArchitecture::Gguf,
            size_bytes: 1024 * 1024 * 100,
            is_loaded: true,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: ModelSearchResult = serde_json::from_str(&json).unwrap();

        assert_eq!(result.model_id, deserialized.model_id);
        assert_eq!(result.is_loaded, deserialized.is_loaded);
        assert_eq!(result.size_bytes, deserialized.size_bytes);
    }

    #[test]
    fn test_model_query_builder_chaining() {
        let query = ModelQueryBuilder::new()
            .capability(ModelCapability::Embedding)
            .architecture(ModelArchitecture::SafeTensors)
            .name_contains("bert")
            .version_range(VersionRange::any())
            .limit(5)
            .offset(10)
            .build();

        assert_eq!(query.capability, Some(ModelCapability::Embedding));
        assert_eq!(query.architecture, Some(ModelArchitecture::SafeTensors));
        assert_eq!(query.name_pattern, Some("bert".to_string()));
        assert_eq!(query.limit, Some(5));
        assert_eq!(query.offset, Some(10));
    }

    #[test]
    fn test_empty_capabilities_list() {
        let query = ModelQuery::builder()
            .capability(ModelCapability::TextGeneration)
            .build();

        let version = ModelVersion::new(1, 0, 0);
        let caps: Vec<ModelCapability> = vec![]; // Empty
        let arch = ModelArchitecture::Gguf;

        assert!(!query.matches("model", &version, &caps, &arch));
    }
}
