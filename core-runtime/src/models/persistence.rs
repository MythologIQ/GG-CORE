// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! JSON-based model registry persistence.
//!
//! Saves and loads registry state for restart recovery.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::history::VersionHistory;
use super::manifest::{ModelArchitecture, ModelCapability};
use super::version::ModelVersion;

/// Error type for persistence operations.
#[derive(Debug, Clone)]
pub enum PersistenceError {
    /// Failed to read state file.
    ReadError(String),
    /// Failed to write state file.
    WriteError(String),
    /// Failed to parse state JSON.
    ParseError(String),
    /// State file not found.
    NotFound,
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadError(e) => write!(f, "Failed to read state: {e}"),
            Self::WriteError(e) => write!(f, "Failed to write state: {e}"),
            Self::ParseError(e) => write!(f, "Failed to parse state: {e}"),
            Self::NotFound => write!(f, "State file not found"),
        }
    }
}

impl std::error::Error for PersistenceError {}

/// Persisted model entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedModel {
    /// Model identifier.
    pub model_id: String,
    /// Path to model file.
    pub path: PathBuf,
    /// Model version.
    pub version: ModelVersion,
    /// Model capabilities.
    pub capabilities: Vec<ModelCapability>,
    /// Model architecture.
    pub architecture: ModelArchitecture,
    /// Whether model should auto-load on startup.
    pub auto_load: bool,
    /// Version history.
    pub history: VersionHistory,
}

/// Complete registry state for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryState {
    /// Schema version for forward compatibility.
    pub schema_version: u32,
    /// Timestamp when state was saved.
    pub saved_at: u64,
    /// Persisted models indexed by model_id.
    pub models: HashMap<String, PersistedModel>,
    /// Default model ID (if set).
    pub default_model: Option<String>,
}

impl Default for RegistryState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            saved_at: 0,
            models: HashMap::new(),
            default_model: None,
        }
    }
}

/// Handles saving and loading registry state to JSON.
pub struct RegistryPersistence {
    state_path: PathBuf,
}

impl RegistryPersistence {
    /// Create a new persistence handler.
    pub fn new(state_path: PathBuf) -> Self {
        Self { state_path }
    }

    /// Save registry state to disk.
    pub fn save(&self, state: &RegistryState) -> Result<(), PersistenceError> {
        // Ensure parent directory exists
        if let Some(parent) = self.state_path.parent() {
            fs::create_dir_all(parent).map_err(|e| PersistenceError::WriteError(e.to_string()))?;
        }

        // Write to temp file first, then rename for atomic update
        let temp_path = self.state_path.with_extension("tmp");
        let file =
            File::create(&temp_path).map_err(|e| PersistenceError::WriteError(e.to_string()))?;
        let writer = BufWriter::new(file);

        serde_json::to_writer_pretty(writer, state)
            .map_err(|e| PersistenceError::WriteError(e.to_string()))?;

        fs::rename(&temp_path, &self.state_path)
            .map_err(|e| PersistenceError::WriteError(e.to_string()))?;

        Ok(())
    }

    /// Load registry state from disk.
    ///
    /// # Security
    /// This method avoids TOCTOU (Time-Of-Check-Time-Of-Use) vulnerabilities
    /// by attempting to open the file directly and handling the error,
    /// rather than checking existence first.
    pub fn load(&self) -> Result<RegistryState, PersistenceError> {
        // SECURITY: Do NOT check exists() before opening - that's a TOCTOU race.
        // Instead, try to open directly and handle the error appropriately.
        let file = File::open(&self.state_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PersistenceError::NotFound
            } else {
                PersistenceError::ReadError(e.to_string())
            }
        })?;
        let reader = BufReader::new(file);

        serde_json::from_reader(reader).map_err(|e| PersistenceError::ParseError(e.to_string()))
    }

    /// Load state or return default if not found.
    pub fn load_or_default(&self) -> RegistryState {
        self.load().unwrap_or_default()
    }

    /// Check if state file exists.
    pub fn exists(&self) -> bool {
        self.state_path.exists()
    }

    /// Delete state file.
    ///
    /// # Security
    /// This method avoids TOCTOU (Time-Of-Check-Time-Of-Use) vulnerabilities
    /// by attempting to delete the file directly and handling the error,
    /// rather than checking existence first.
    pub fn delete(&self) -> Result<(), PersistenceError> {
        // SECURITY: Do NOT check exists() before deleting - that's a TOCTOU race.
        // Instead, try to delete directly and handle NotFound gracefully.
        match fs::remove_file(&self.state_path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File doesn't exist - that's fine, it's already "deleted"
                Ok(())
            }
            Err(e) => Err(PersistenceError::WriteError(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("veritas_test_{}", name))
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = temp_test_dir("persistence_save_load");
        let _ = fs::remove_dir_all(&temp_dir);
        let state_path = temp_dir.join("registry_state.json");
        let persistence = RegistryPersistence::new(state_path);

        let mut state = RegistryState::default();
        state.saved_at = timestamp();
        state.default_model = Some("test-model".to_string());
        state.models.insert(
            "test-model".to_string(),
            PersistedModel {
                model_id: "test-model".to_string(),
                path: PathBuf::from("/models/test.gguf"),
                version: ModelVersion::new(1, 0, 0),
                capabilities: vec![ModelCapability::TextGeneration],
                architecture: ModelArchitecture::Gguf,
                auto_load: true,
                history: VersionHistory::new(),
            },
        );

        persistence.save(&state).unwrap();
        assert!(persistence.exists());

        let loaded = persistence.load().unwrap();
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.default_model, Some("test-model".to_string()));
        assert!(loaded.models.contains_key("test-model"));

        persistence.delete().unwrap();
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_not_found() {
        let persistence = RegistryPersistence::new(PathBuf::from("/nonexistent/state.json"));
        assert!(matches!(
            persistence.load(),
            Err(PersistenceError::NotFound)
        ));
    }

    #[test]
    fn test_load_or_default() {
        let persistence = RegistryPersistence::new(PathBuf::from("/nonexistent/state.json"));
        let state = persistence.load_or_default();
        assert_eq!(state.schema_version, 1);
        assert!(state.models.is_empty());
    }

    // --- Additional tests ---

    #[test]
    fn test_registry_state_default() {
        let state = RegistryState::default();
        assert_eq!(state.schema_version, 1);
        assert_eq!(state.saved_at, 0);
        assert!(state.models.is_empty());
        assert!(state.default_model.is_none());
    }

    #[test]
    fn test_persisted_model_serialization() {
        let model = PersistedModel {
            model_id: "test".to_string(),
            path: PathBuf::from("/models/test.gguf"),
            version: ModelVersion::new(1, 2, 3),
            capabilities: vec![ModelCapability::TextGeneration, ModelCapability::Embedding],
            architecture: ModelArchitecture::Gguf,
            auto_load: false,
            history: VersionHistory::new(),
        };

        let json = serde_json::to_string(&model).unwrap();
        let deserialized: PersistedModel = serde_json::from_str(&json).unwrap();

        assert_eq!(model.model_id, deserialized.model_id);
        assert_eq!(model.auto_load, deserialized.auto_load);
        assert_eq!(model.capabilities.len(), deserialized.capabilities.len());
    }

    #[test]
    fn test_registry_state_serialization() {
        let mut state = RegistryState::default();
        state.saved_at = 1234567890;
        state.default_model = Some("default".to_string());

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: RegistryState = serde_json::from_str(&json).unwrap();

        assert_eq!(state.schema_version, deserialized.schema_version);
        assert_eq!(state.saved_at, deserialized.saved_at);
        assert_eq!(state.default_model, deserialized.default_model);
    }

    #[test]
    fn test_persistence_error_display() {
        let err = PersistenceError::ReadError("file not found".to_string());
        assert!(err.to_string().contains("Failed to read"));

        let err = PersistenceError::WriteError("disk full".to_string());
        assert!(err.to_string().contains("Failed to write"));

        let err = PersistenceError::ParseError("invalid json".to_string());
        assert!(err.to_string().contains("Failed to parse"));

        let err = PersistenceError::NotFound;
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_persistence_exists_false() {
        let persistence = RegistryPersistence::new(PathBuf::from("/nonexistent/path.json"));
        assert!(!persistence.exists());
    }

    #[test]
    fn test_delete_nonexistent_file() {
        let persistence = RegistryPersistence::new(PathBuf::from("/nonexistent/path.json"));
        // Should succeed even if file doesn't exist
        assert!(persistence.delete().is_ok());
    }

    #[test]
    fn test_save_creates_parent_directories() {
        let temp_dir = temp_test_dir("persistence_nested");
        let _ = fs::remove_dir_all(&temp_dir);
        let nested_path = temp_dir.join("deep").join("nested").join("state.json");
        let persistence = RegistryPersistence::new(nested_path.clone());

        let state = RegistryState::default();
        persistence.save(&state).unwrap();

        assert!(nested_path.exists());
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_multiple_save_overwrite() {
        let temp_dir = temp_test_dir("persistence_overwrite");
        let _ = fs::remove_dir_all(&temp_dir);
        let state_path = temp_dir.join("state.json");
        let persistence = RegistryPersistence::new(state_path);

        // Save first state
        let mut state1 = RegistryState::default();
        state1.default_model = Some("model1".to_string());
        persistence.save(&state1).unwrap();

        // Save second state (overwrite)
        let mut state2 = RegistryState::default();
        state2.default_model = Some("model2".to_string());
        persistence.save(&state2).unwrap();

        // Load and verify latest
        let loaded = persistence.load().unwrap();
        assert_eq!(loaded.default_model, Some("model2".to_string()));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_multiple_models() {
        let temp_dir = temp_test_dir("persistence_multi_models");
        let _ = fs::remove_dir_all(&temp_dir);
        let state_path = temp_dir.join("state.json");
        let persistence = RegistryPersistence::new(state_path);

        let mut state = RegistryState::default();
        for i in 0..5 {
            state.models.insert(
                format!("model-{}", i),
                PersistedModel {
                    model_id: format!("model-{}", i),
                    path: PathBuf::from(format!("/models/model{}.gguf", i)),
                    version: ModelVersion::new(1, i, 0),
                    capabilities: vec![ModelCapability::TextGeneration],
                    architecture: ModelArchitecture::Gguf,
                    auto_load: i % 2 == 0,
                    history: VersionHistory::new(),
                },
            );
        }

        persistence.save(&state).unwrap();
        let loaded = persistence.load().unwrap();

        assert_eq!(loaded.models.len(), 5);
        for i in 0..5 {
            assert!(loaded.models.contains_key(&format!("model-{}", i)));
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_corrupted_json() {
        let temp_dir = temp_test_dir("persistence_corrupted");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let state_path = temp_dir.join("state.json");

        // Write invalid JSON
        fs::write(&state_path, "{ invalid json }").unwrap();

        let persistence = RegistryPersistence::new(state_path);
        let result = persistence.load();

        assert!(matches!(result, Err(PersistenceError::ParseError(_))));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_all_architectures_serializable() {
        let architectures = vec![
            ModelArchitecture::Gguf,
            ModelArchitecture::Onnx,
            ModelArchitecture::SafeTensors,
        ];

        for arch in architectures {
            let model = PersistedModel {
                model_id: "test".to_string(),
                path: PathBuf::from("/test.bin"),
                version: ModelVersion::new(1, 0, 0),
                capabilities: vec![ModelCapability::TextGeneration],
                architecture: arch,
                auto_load: true,
                history: VersionHistory::new(),
            };

            let json = serde_json::to_string(&model).unwrap();
            let _: PersistedModel = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_all_capabilities_serializable() {
        let capabilities = vec![
            ModelCapability::TextClassification,
            ModelCapability::TextGeneration,
            ModelCapability::Embedding,
            ModelCapability::NamedEntityRecognition,
        ];

        let model = PersistedModel {
            model_id: "test".to_string(),
            path: PathBuf::from("/test.bin"),
            version: ModelVersion::new(1, 0, 0),
            capabilities,
            architecture: ModelArchitecture::Gguf,
            auto_load: true,
            history: VersionHistory::new(),
        };

        let json = serde_json::to_string(&model).unwrap();
        let deserialized: PersistedModel = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.capabilities.len(), 4);
    }
}
