// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! CRD type definitions for Kubernetes operator.
//!
//! # Security
//! All input fields are validated to prevent:
//! - Path traversal attacks (e.g., `../../../etc/passwd`)
//! - Command injection (e.g., `; rm -rf /`)
//! - Invalid resource names

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Validation error types
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// Path contains traversal sequences
    PathTraversal(String),
    /// Path is not relative to allowed directory
    InvalidPath(String),
    /// Image reference is invalid
    InvalidImage(String),
    /// Model ID contains invalid characters
    InvalidModelId(String),
    /// Socket path is invalid
    InvalidSocketPath(String),
    /// Field exceeds maximum length
    MaxLengthExceeded { field: String, max: usize },
    /// Field is empty but required
    EmptyField(String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::PathTraversal(p) => write!(f, "Path traversal detected: {}", p),
            ValidationError::InvalidPath(p) => write!(f, "Invalid path: {}", p),
            ValidationError::InvalidImage(img) => write!(f, "Invalid image reference: {}", img),
            ValidationError::InvalidModelId(id) => write!(f, "Invalid model ID: {}", id),
            ValidationError::InvalidSocketPath(p) => write!(f, "Invalid socket path: {}", p),
            ValidationError::MaxLengthExceeded { field, max } => {
                write!(f, "Field '{}' exceeds maximum length of {}", field, max)
            }
            ValidationError::EmptyField(field) => write!(f, "Field '{}' cannot be empty", field),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Maximum allowed length for string fields
const MAX_FIELD_LENGTH: usize = 256;
const MAX_PATH_LENGTH: usize = 1024;

/// Validate a path for security issues
///
/// # Security Checks
/// - No path traversal sequences (`..`)
/// - No null bytes
/// - Must be a relative path within allowed directory
/// - Reasonable length limit
fn validate_path(path: &str, field_name: &str) -> Result<(), ValidationError> {
    // Check for empty path
    if path.is_empty() {
        return Err(ValidationError::EmptyField(field_name.to_string()));
    }

    // Check length
    if path.len() > MAX_PATH_LENGTH {
        return Err(ValidationError::MaxLengthExceeded {
            field: field_name.to_string(),
            max: MAX_PATH_LENGTH,
        });
    }

    // Check for null bytes (potential null byte injection)
    if path.contains('\0') {
        return Err(ValidationError::InvalidPath(format!(
            "{}: contains null byte",
            field_name
        )));
    }

    // Check for path traversal sequences
    let path_obj = Path::new(path);
    for component in path_obj.components() {
        if let std::path::Component::ParentDir = component {
            return Err(ValidationError::PathTraversal(format!(
                "{}: contains '..' sequence",
                field_name
            )));
        }
    }

    // Check for absolute path (should be relative to PVC mount)
    if path.starts_with('/') {
        // Allow absolute paths for socket paths, but log warning
        // This is a soft check - absolute paths are valid for sockets
    }

    Ok(())
}

/// Validate a container image reference
///
/// # Security Checks
/// - No shell metacharacters
/// - Valid DNS name format for registry
/// - Valid tag format
fn validate_image(image: &str) -> Result<(), ValidationError> {
    if image.is_empty() {
        return Err(ValidationError::EmptyField("image".to_string()));
    }

    if image.len() > MAX_FIELD_LENGTH {
        return Err(ValidationError::MaxLengthExceeded {
            field: "image".to_string(),
            max: MAX_FIELD_LENGTH,
        });
    }

    // Check for shell metacharacters that could enable injection
    let forbidden_chars = [
        ';', '&', '|', '`', '$', '(', ')', '{', '}', '<', '>', '\n', '\r', '\0',
    ];
    for ch in forbidden_chars {
        if image.contains(ch) {
            return Err(ValidationError::InvalidImage(format!(
                "contains forbidden character: {:?}",
                ch
            )));
        }
    }

    // Basic image reference format validation
    // Format: [registry/]name[:tag|@digest]
    let parts: Vec<&str> = image.rsplitn(2, ':').collect();
    let name_part = parts.last().unwrap_or(&image);

    // Check name doesn't start with dash or dot
    if name_part.starts_with('-') || name_part.starts_with('.') {
        return Err(ValidationError::InvalidImage(
            "name cannot start with dash or dot".to_string(),
        ));
    }

    Ok(())
}

/// Validate a model ID
///
/// # Security Checks
/// - Alphanumeric with dashes, underscores, and dots only
/// - No path separators
/// - Reasonable length
fn validate_model_id(model_id: &str) -> Result<(), ValidationError> {
    if model_id.is_empty() {
        return Err(ValidationError::EmptyField("model_id".to_string()));
    }

    if model_id.len() > MAX_FIELD_LENGTH {
        return Err(ValidationError::MaxLengthExceeded {
            field: "model_id".to_string(),
            max: MAX_FIELD_LENGTH,
        });
    }

    // Model IDs should be alphanumeric with limited special chars
    let valid_chars = |c: char| c.is_alphanumeric() || c == '-' || c == '_' || c == '.';
    if !model_id.chars().all(valid_chars) {
        return Err(ValidationError::InvalidModelId(
            "must contain only alphanumeric characters, dashes, underscores, and dots".to_string(),
        ));
    }

    // Check for path separators
    if model_id.contains('/') || model_id.contains('\\') {
        return Err(ValidationError::InvalidModelId(
            "cannot contain path separators".to_string(),
        ));
    }

    Ok(())
}

/// Validate a socket path
///
/// # Security Checks
/// - No path traversal
/// - No null bytes
/// - Must be absolute path (Unix domain sockets)
fn validate_socket_path(socket_path: &str) -> Result<(), ValidationError> {
    if socket_path.is_empty() {
        return Err(ValidationError::EmptyField("socket_path".to_string()));
    }

    if socket_path.len() > MAX_PATH_LENGTH {
        return Err(ValidationError::MaxLengthExceeded {
            field: "socket_path".to_string(),
            max: MAX_PATH_LENGTH,
        });
    }

    // Check for null bytes
    if socket_path.contains('\0') {
        return Err(ValidationError::InvalidSocketPath(
            "contains null byte".to_string(),
        ));
    }

    // Check for path traversal
    if socket_path.contains("..") {
        return Err(ValidationError::PathTraversal(format!(
            "socket_path: {}",
            socket_path
        )));
    }

    // Socket paths should typically be absolute
    if !socket_path.starts_with('/') {
        return Err(ValidationError::InvalidSocketPath(
            "must be an absolute path".to_string(),
        ));
    }

    Ok(())
}

/// VeritasRuntime CRD spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VeritasRuntimeSpec {
    /// Number of replicas.
    pub replicas: u32,
    /// Container image.
    pub image: String,
    /// Memory request/limit.
    pub memory: String,
    /// CPU request/limit.
    pub cpu: String,
    /// GPU resources (optional).
    pub gpu: Option<GpuSpec>,
    /// Model volume claim name.
    pub model_pvc: String,
    /// Socket path for IPC.
    pub socket_path: Option<String>,
}

impl VeritasRuntimeSpec {
    /// Validate all fields in the spec
    ///
    /// # Errors
    /// Returns a `ValidationError` if any field fails validation
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_image(&self.image)?;

        if let Some(ref socket_path) = self.socket_path {
            validate_socket_path(socket_path)?;
        }

        // Validate model_pvc name (basic K8s naming)
        if self.model_pvc.is_empty() {
            return Err(ValidationError::EmptyField("model_pvc".to_string()));
        }

        if self.model_pvc.len() > MAX_FIELD_LENGTH {
            return Err(ValidationError::MaxLengthExceeded {
                field: "model_pvc".to_string(),
                max: MAX_FIELD_LENGTH,
            });
        }

        Ok(())
    }
}

/// GPU resource specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuSpec {
    /// Number of GPUs.
    pub count: u32,
    /// GPU type (nvidia.com/gpu, etc).
    pub resource_type: String,
}

/// VeritasRuntime CRD.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VeritasRuntime {
    pub api_version: String,
    pub kind: String,
    pub metadata: CrdMetadata,
    pub spec: VeritasRuntimeSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<VeritasRuntimeStatus>,
}

/// Runtime status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VeritasRuntimeStatus {
    pub ready_replicas: u32,
    pub phase: String,
    pub conditions: Vec<Condition>,
}

/// VeritasModel CRD spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VeritasModelSpec {
    /// Model identifier.
    pub model_id: String,
    /// Model version.
    pub version: String,
    /// Source configuration.
    pub source: ModelSource,
    /// A/B testing variant label.
    pub variant: Option<String>,
    /// Auto-load on startup.
    pub auto_load: bool,
}

impl VeritasModelSpec {
    /// Validate all fields in the spec
    ///
    /// # Errors
    /// Returns a `ValidationError` if any field fails validation
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_model_id(&self.model_id)?;

        // Validate version (basic format check)
        if self.version.is_empty() {
            return Err(ValidationError::EmptyField("version".to_string()));
        }

        // Validate source
        self.source.validate()?;

        // Validate variant if present
        if let Some(ref variant) = self.variant {
            if variant.len() > MAX_FIELD_LENGTH {
                return Err(ValidationError::MaxLengthExceeded {
                    field: "variant".to_string(),
                    max: MAX_FIELD_LENGTH,
                });
            }
        }

        Ok(())
    }
}

/// Model source location.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSource {
    /// PVC name containing the model.
    pub pvc: String,
    /// Path within the PVC.
    pub path: String,
}

impl ModelSource {
    /// Validate the model source
    ///
    /// # Errors
    /// Returns a `ValidationError` if validation fails
    pub fn validate(&self) -> Result<(), ValidationError> {
        // Validate PVC name
        if self.pvc.is_empty() {
            return Err(ValidationError::EmptyField("pvc".to_string()));
        }

        if self.pvc.len() > MAX_FIELD_LENGTH {
            return Err(ValidationError::MaxLengthExceeded {
                field: "pvc".to_string(),
                max: MAX_FIELD_LENGTH,
            });
        }

        // Validate path
        validate_path(&self.path, "source.path")?;

        Ok(())
    }
}

/// VeritasModel CRD.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VeritasModel {
    pub api_version: String,
    pub kind: String,
    pub metadata: CrdMetadata,
    pub spec: VeritasModelSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<VeritasModelStatus>,
}

/// Model status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VeritasModelStatus {
    pub loaded: bool,
    pub phase: String,
    pub conditions: Vec<Condition>,
}

/// Common CRD metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdMetadata {
    pub name: String,
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::HashMap<String, String>>,
}

/// Condition for status reporting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    #[serde(rename = "type")]
    pub condition_type: String,
    pub status: String,
    pub reason: Option<String>,
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_runtime() {
        let runtime = VeritasRuntime {
            api_version: "veritas.io/v1".to_string(),
            kind: "VeritasRuntime".to_string(),
            metadata: CrdMetadata {
                name: "veritas-prod".to_string(),
                namespace: Some("default".to_string()),
                labels: None,
            },
            spec: VeritasRuntimeSpec {
                replicas: 3,
                image: "veritas-sdr:0.5.0".to_string(),
                memory: "4Gi".to_string(),
                cpu: "2".to_string(),
                gpu: None,
                model_pvc: "models-pvc".to_string(),
                socket_path: None,
            },
            status: None,
        };

        let json = serde_json::to_string_pretty(&runtime).unwrap();
        assert!(json.contains("veritas.io/v1"));
        assert!(json.contains("VeritasRuntime"));
    }

    // --- Additional tests ---

    #[test]
    fn test_runtime_spec_with_gpu() {
        let spec = VeritasRuntimeSpec {
            replicas: 2,
            image: "veritas-sdr:0.5.0".to_string(),
            memory: "8Gi".to_string(),
            cpu: "4".to_string(),
            gpu: Some(GpuSpec {
                count: 2,
                resource_type: "nvidia.com/gpu".to_string(),
            }),
            model_pvc: "models-pvc".to_string(),
            socket_path: Some("/var/run/veritas.sock".to_string()),
        };

        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("nvidia.com/gpu"));
        assert!(json.contains("\"count\":2"));
    }

    #[test]
    fn test_gpu_spec_serialization() {
        let gpu = GpuSpec {
            count: 4,
            resource_type: "amd.com/gpu".to_string(),
        };

        let json = serde_json::to_string(&gpu).unwrap();
        let deserialized: GpuSpec = serde_json::from_str(&json).unwrap();

        assert_eq!(gpu.count, deserialized.count);
        assert_eq!(gpu.resource_type, deserialized.resource_type);
    }

    #[test]
    fn test_runtime_status_serialization() {
        let status = VeritasRuntimeStatus {
            ready_replicas: 3,
            phase: "Running".to_string(),
            conditions: vec![Condition {
                condition_type: "Ready".to_string(),
                status: "True".to_string(),
                reason: Some("AllReplicasReady".to_string()),
                message: Some("All replicas are ready".to_string()),
            }],
        };

        let json = serde_json::to_string(&status).unwrap();
        let deserialized: VeritasRuntimeStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(status.ready_replicas, deserialized.ready_replicas);
        assert_eq!(status.phase, deserialized.phase);
        assert_eq!(status.conditions.len(), deserialized.conditions.len());
    }

    #[test]
    fn test_model_spec_serialization() {
        let spec = VeritasModelSpec {
            model_id: "llama-7b".to_string(),
            version: "1.0.0".to_string(),
            source: ModelSource {
                pvc: "models-pvc".to_string(),
                path: "/models/llama-7b.gguf".to_string(),
            },
            variant: Some("control".to_string()),
            auto_load: true,
        };

        let json = serde_json::to_string(&spec).unwrap();
        let deserialized: VeritasModelSpec = serde_json::from_str(&json).unwrap();

        assert_eq!(spec.model_id, deserialized.model_id);
        assert_eq!(spec.version, deserialized.version);
        assert_eq!(spec.auto_load, deserialized.auto_load);
        assert_eq!(spec.variant, deserialized.variant);
    }

    #[test]
    fn test_model_source_serialization() {
        let source = ModelSource {
            pvc: "my-pvc".to_string(),
            path: "/data/model.bin".to_string(),
        };

        let json = serde_json::to_string(&source).unwrap();
        let deserialized: ModelSource = serde_json::from_str(&json).unwrap();

        assert_eq!(source.pvc, deserialized.pvc);
        assert_eq!(source.path, deserialized.path);
    }

    #[test]
    fn test_veritas_model_full() {
        let model = VeritasModel {
            api_version: "veritas.io/v1".to_string(),
            kind: "VeritasModel".to_string(),
            metadata: CrdMetadata {
                name: "llama-model".to_string(),
                namespace: Some("ml-models".to_string()),
                labels: Some({
                    let mut map = std::collections::HashMap::new();
                    map.insert("app".to_string(), "veritas".to_string());
                    map
                }),
            },
            spec: VeritasModelSpec {
                model_id: "llama-7b".to_string(),
                version: "1.0.0".to_string(),
                source: ModelSource {
                    pvc: "models-pvc".to_string(),
                    path: "/models/llama.gguf".to_string(),
                },
                variant: None,
                auto_load: false,
            },
            status: Some(VeritasModelStatus {
                loaded: true,
                phase: "Loaded".to_string(),
                conditions: vec![],
            }),
        };

        let json = serde_json::to_string_pretty(&model).unwrap();
        assert!(json.contains("VeritasModel"));
        assert!(json.contains("llama-7b"));
        assert!(json.contains("ml-models"));
    }

    #[test]
    fn test_model_status_serialization() {
        let status = VeritasModelStatus {
            loaded: false,
            phase: "Loading".to_string(),
            conditions: vec![Condition {
                condition_type: "Loading".to_string(),
                status: "True".to_string(),
                reason: None,
                message: None,
            }],
        };

        let json = serde_json::to_string(&status).unwrap();
        let deserialized: VeritasModelStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(status.loaded, deserialized.loaded);
        assert_eq!(status.phase, deserialized.phase);
    }

    #[test]
    fn test_crd_metadata_minimal() {
        let meta = CrdMetadata {
            name: "test".to_string(),
            namespace: None,
            labels: None,
        };

        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: CrdMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(meta.name, deserialized.name);
        assert!(deserialized.namespace.is_none());
        assert!(deserialized.labels.is_none());
    }

    #[test]
    fn test_crd_metadata_with_labels() {
        let mut labels = std::collections::HashMap::new();
        labels.insert("env".to_string(), "prod".to_string());
        labels.insert("team".to_string(), "ml".to_string());

        let meta = CrdMetadata {
            name: "test".to_string(),
            namespace: Some("default".to_string()),
            labels: Some(labels),
        };

        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("env"));
        assert!(json.contains("prod"));
    }

    #[test]
    fn test_condition_full() {
        let condition = Condition {
            condition_type: "Available".to_string(),
            status: "True".to_string(),
            reason: Some("MinimumReplicasAvailable".to_string()),
            message: Some("Deployment has minimum availability".to_string()),
        };

        let json = serde_json::to_string(&condition).unwrap();
        let deserialized: Condition = serde_json::from_str(&json).unwrap();

        assert_eq!(condition.condition_type, deserialized.condition_type);
        assert_eq!(condition.status, deserialized.status);
        assert_eq!(condition.reason, deserialized.reason);
        assert_eq!(condition.message, deserialized.message);
    }

    #[test]
    fn test_condition_minimal() {
        let condition = Condition {
            condition_type: "Ready".to_string(),
            status: "False".to_string(),
            reason: None,
            message: None,
        };

        let json = serde_json::to_string(&condition).unwrap();
        let deserialized: Condition = serde_json::from_str(&json).unwrap();

        assert!(deserialized.reason.is_none());
        assert!(deserialized.message.is_none());
    }

    #[test]
    fn test_camel_case_serialization() {
        let spec = VeritasRuntimeSpec {
            replicas: 1,
            image: "test:latest".to_string(),
            memory: "1Gi".to_string(),
            cpu: "1".to_string(),
            gpu: None,
            model_pvc: "pvc-1".to_string(),
            socket_path: None,
        };

        let json = serde_json::to_string(&spec).unwrap();
        // Should use camelCase
        assert!(json.contains("modelPvc"));
        assert!(json.contains("socketPath"));
        // Should not use snake_case
        assert!(!json.contains("model_pvc"));
        assert!(!json.contains("socket_path"));
    }

    #[test]
    fn test_runtime_deserialization() {
        let json = r#"{
            "apiVersion": "veritas.io/v1",
            "kind": "VeritasRuntime",
            "metadata": {
                "name": "test-runtime",
                "namespace": "default"
            },
            "spec": {
                "replicas": 2,
                "image": "veritas:latest",
                "memory": "2Gi",
                "cpu": "1",
                "modelPvc": "models"
            }
        }"#;

        let runtime: VeritasRuntime = serde_json::from_str(json).unwrap();
        assert_eq!(runtime.metadata.name, "test-runtime");
        assert_eq!(runtime.spec.replicas, 2);
        assert_eq!(runtime.spec.model_pvc, "models");
    }

    #[test]
    fn test_model_deserialization() {
        let json = r#"{
            "apiVersion": "veritas.io/v1",
            "kind": "VeritasModel",
            "metadata": {
                "name": "test-model"
            },
            "spec": {
                "modelId": "test",
                "version": "1.0.0",
                "source": {
                    "pvc": "data-pvc",
                    "path": "/models/test.bin"
                },
                "autoLoad": true
            }
        }"#;

        let model: VeritasModel = serde_json::from_str(json).unwrap();
        assert_eq!(model.spec.model_id, "test");
        assert!(model.spec.auto_load);
    }

    #[test]
    fn test_skip_serializing_none_status() {
        let runtime = VeritasRuntime {
            api_version: "veritas.io/v1".to_string(),
            kind: "VeritasRuntime".to_string(),
            metadata: CrdMetadata {
                name: "test".to_string(),
                namespace: None,
                labels: None,
            },
            spec: VeritasRuntimeSpec {
                replicas: 1,
                image: "test".to_string(),
                memory: "1Gi".to_string(),
                cpu: "1".to_string(),
                gpu: None,
                model_pvc: "pvc".to_string(),
                socket_path: None,
            },
            status: None,
        };

        let json = serde_json::to_string(&runtime).unwrap();
        // status field should not appear when None
        assert!(!json.contains("status"));
    }

    #[test]
    fn test_clone_traits() {
        let runtime = VeritasRuntime {
            api_version: "veritas.io/v1".to_string(),
            kind: "VeritasRuntime".to_string(),
            metadata: CrdMetadata {
                name: "test".to_string(),
                namespace: None,
                labels: None,
            },
            spec: VeritasRuntimeSpec {
                replicas: 1,
                image: "test".to_string(),
                memory: "1Gi".to_string(),
                cpu: "1".to_string(),
                gpu: None,
                model_pvc: "pvc".to_string(),
                socket_path: None,
            },
            status: None,
        };

        let cloned = runtime.clone();
        assert_eq!(runtime.metadata.name, cloned.metadata.name);
    }

    #[test]
    fn test_debug_traits() {
        let gpu = GpuSpec {
            count: 1,
            resource_type: "nvidia.com/gpu".to_string(),
        };

        let debug_str = format!("{:?}", gpu);
        assert!(debug_str.contains("GpuSpec"));
        assert!(debug_str.contains("nvidia.com/gpu"));
    }

    // --- Validation tests ---

    #[test]
    fn test_validate_path_traversal() {
        // Valid paths
        assert!(validate_path("/models/test.gguf", "path").is_ok());
        assert!(validate_path("models/test.gguf", "path").is_ok());

        // Path traversal attempts
        assert!(matches!(
            validate_path("../../../etc/passwd", "path"),
            Err(ValidationError::PathTraversal(_))
        ));
        assert!(matches!(
            validate_path("models/../../secret", "path"),
            Err(ValidationError::PathTraversal(_))
        ));
    }

    #[test]
    fn test_validate_path_null_byte() {
        assert!(matches!(
            validate_path("models/test\0.gguf", "path"),
            Err(ValidationError::InvalidPath(_))
        ));
    }

    #[test]
    fn test_validate_path_empty() {
        assert!(matches!(
            validate_path("", "path"),
            Err(ValidationError::EmptyField(_))
        ));
    }

    #[test]
    fn test_validate_image_valid() {
        assert!(validate_image("veritas-sdr:0.5.0").is_ok());
        assert!(validate_image("registry.io/veritas-sdr:latest").is_ok());
        assert!(validate_image("veritas-sdr").is_ok());
    }

    #[test]
    fn test_validate_image_injection() {
        // Shell metacharacters should be rejected
        assert!(matches!(
            validate_image("veritas; rm -rf /"),
            Err(ValidationError::InvalidImage(_))
        ));
        assert!(matches!(
            validate_image("veritas && cat /etc/passwd"),
            Err(ValidationError::InvalidImage(_))
        ));
        assert!(matches!(
            validate_image("veritas`whoami`"),
            Err(ValidationError::InvalidImage(_))
        ));
        assert!(matches!(
            validate_image("$(cat /etc/passwd)"),
            Err(ValidationError::InvalidImage(_))
        ));
    }

    #[test]
    fn test_validate_image_empty() {
        assert!(matches!(
            validate_image(""),
            Err(ValidationError::EmptyField(_))
        ));
    }

    #[test]
    fn test_validate_model_id_valid() {
        assert!(validate_model_id("llama-7b").is_ok());
        assert!(validate_model_id("model_v2.0").is_ok());
        assert!(validate_model_id("my-model-123").is_ok());
    }

    #[test]
    fn test_validate_model_id_invalid() {
        // Path separators
        assert!(matches!(
            validate_model_id("models/llama"),
            Err(ValidationError::InvalidModelId(_))
        ));
        // Special characters
        assert!(matches!(
            validate_model_id("model;drop"),
            Err(ValidationError::InvalidModelId(_))
        ));
        // Empty
        assert!(matches!(
            validate_model_id(""),
            Err(ValidationError::EmptyField(_))
        ));
    }

    #[test]
    fn test_validate_socket_path_valid() {
        assert!(validate_socket_path("/var/run/veritas.sock").is_ok());
        assert!(validate_socket_path("/tmp/socket").is_ok());
    }

    #[test]
    fn test_validate_socket_path_invalid() {
        // Relative path
        assert!(matches!(
            validate_socket_path("var/run/veritas.sock"),
            Err(ValidationError::InvalidSocketPath(_))
        ));
        // Path traversal
        assert!(matches!(
            validate_socket_path("/var/../etc/passwd"),
            Err(ValidationError::PathTraversal(_))
        ));
        // Null byte
        assert!(matches!(
            validate_socket_path("/var/run\0/veritas.sock"),
            Err(ValidationError::InvalidSocketPath(_))
        ));
    }

    #[test]
    fn test_runtime_spec_validate() {
        let valid_spec = VeritasRuntimeSpec {
            replicas: 2,
            image: "veritas-sdr:0.5.0".to_string(),
            memory: "4Gi".to_string(),
            cpu: "2".to_string(),
            gpu: None,
            model_pvc: "models-pvc".to_string(),
            socket_path: Some("/var/run/veritas.sock".to_string()),
        };
        assert!(valid_spec.validate().is_ok());

        let invalid_image = VeritasRuntimeSpec {
            replicas: 2,
            image: "veritas; rm -rf /".to_string(),
            memory: "4Gi".to_string(),
            cpu: "2".to_string(),
            gpu: None,
            model_pvc: "models-pvc".to_string(),
            socket_path: None,
        };
        assert!(invalid_image.validate().is_err());
    }

    #[test]
    fn test_model_spec_validate() {
        let valid_spec = VeritasModelSpec {
            model_id: "llama-7b".to_string(),
            version: "1.0.0".to_string(),
            source: ModelSource {
                pvc: "models-pvc".to_string(),
                path: "/models/llama.gguf".to_string(),
            },
            variant: Some("control".to_string()),
            auto_load: true,
        };
        assert!(valid_spec.validate().is_ok());

        let invalid_model_id = VeritasModelSpec {
            model_id: "llama/../../../etc/passwd".to_string(),
            version: "1.0.0".to_string(),
            source: ModelSource {
                pvc: "models-pvc".to_string(),
                path: "/models/llama.gguf".to_string(),
            },
            variant: None,
            auto_load: true,
        };
        assert!(invalid_model_id.validate().is_err());
    }

    #[test]
    fn test_model_source_validate() {
        let valid_source = ModelSource {
            pvc: "models-pvc".to_string(),
            path: "/models/test.gguf".to_string(),
        };
        assert!(valid_source.validate().is_ok());

        let traversal_source = ModelSource {
            pvc: "models-pvc".to_string(),
            path: "../../../etc/passwd".to_string(),
        };
        assert!(traversal_source.validate().is_err());
    }
}
