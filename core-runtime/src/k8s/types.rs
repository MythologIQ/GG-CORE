// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! CRD type definitions for Kubernetes operator.
//!
//! # Security
//! All input fields are validated to prevent:
//! - Path traversal attacks (e.g., `../../../etc/passwd`)
//! - Command injection (e.g., `; rm -rf /`)
//! - Invalid resource names

use serde::{Deserialize, Serialize};

pub use super::validation::{
    validate_image, validate_model_id, validate_path, validate_socket_path, ValidationError,
    MAX_FIELD_LENGTH,
};

/// GgCoreRuntime CRD spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GgCoreRuntimeSpec {
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

impl GgCoreRuntimeSpec {
    /// Validate all fields in the spec
    ///
    /// # Errors
    /// Returns a `ValidationError` if any field fails validation
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_image(&self.image)?;

        if let Some(ref socket_path) = self.socket_path {
            validate_socket_path(socket_path)?;
        }

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

/// GgCoreRuntime CRD.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GgCoreRuntime {
    pub api_version: String,
    pub kind: String,
    pub metadata: CrdMetadata,
    pub spec: GgCoreRuntimeSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<GgCoreRuntimeStatus>,
}

/// Runtime status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GgCoreRuntimeStatus {
    pub ready_replicas: u32,
    pub phase: String,
    pub conditions: Vec<Condition>,
}

/// GgCoreModel CRD spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GgCoreModelSpec {
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

impl GgCoreModelSpec {
    /// Validate all fields in the spec
    ///
    /// # Errors
    /// Returns a `ValidationError` if any field fails validation
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_model_id(&self.model_id)?;

        if self.version.is_empty() {
            return Err(ValidationError::EmptyField("version".to_string()));
        }

        self.source.validate()?;

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
        if self.pvc.is_empty() {
            return Err(ValidationError::EmptyField("pvc".to_string()));
        }

        if self.pvc.len() > MAX_FIELD_LENGTH {
            return Err(ValidationError::MaxLengthExceeded {
                field: "pvc".to_string(),
                max: MAX_FIELD_LENGTH,
            });
        }

        validate_path(&self.path, "source.path")?;
        Ok(())
    }
}

/// GgCoreModel CRD.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GgCoreModel {
    pub api_version: String,
    pub kind: String,
    pub metadata: CrdMetadata,
    pub spec: GgCoreModelSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<GgCoreModelStatus>,
}

/// Model status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GgCoreModelStatus {
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
#[path = "types_tests.rs"]
mod tests;
