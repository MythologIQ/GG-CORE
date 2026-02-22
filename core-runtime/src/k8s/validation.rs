// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Validation functions for K8s CRD fields.
//!
//! Prevents path traversal, command injection, and invalid resource names.

use std::path::Path;

/// Maximum allowed length for string fields.
pub const MAX_FIELD_LENGTH: usize = 256;

/// Maximum allowed length for path fields.
const MAX_PATH_LENGTH: usize = 1024;

/// Validation error types.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// Path contains traversal sequences.
    PathTraversal(String),
    /// Path is not relative to allowed directory.
    InvalidPath(String),
    /// Image reference is invalid.
    InvalidImage(String),
    /// Model ID contains invalid characters.
    InvalidModelId(String),
    /// Socket path is invalid.
    InvalidSocketPath(String),
    /// Field exceeds maximum length.
    MaxLengthExceeded { field: String, max: usize },
    /// Field is empty but required.
    EmptyField(String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathTraversal(p) => write!(f, "Path traversal detected: {}", p),
            Self::InvalidPath(p) => write!(f, "Invalid path: {}", p),
            Self::InvalidImage(img) => write!(f, "Invalid image reference: {}", img),
            Self::InvalidModelId(id) => write!(f, "Invalid model ID: {}", id),
            Self::InvalidSocketPath(p) => write!(f, "Invalid socket path: {}", p),
            Self::MaxLengthExceeded { field, max } => {
                write!(f, "Field '{}' exceeds maximum length of {}", field, max)
            }
            Self::EmptyField(field) => write!(f, "Field '{}' cannot be empty", field),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validate a path for security issues.
///
/// Checks for path traversal, null bytes, and length limits.
pub fn validate_path(path: &str, field_name: &str) -> Result<(), ValidationError> {
    if path.is_empty() {
        return Err(ValidationError::EmptyField(field_name.to_string()));
    }

    if path.len() > MAX_PATH_LENGTH {
        return Err(ValidationError::MaxLengthExceeded {
            field: field_name.to_string(),
            max: MAX_PATH_LENGTH,
        });
    }

    if path.contains('\0') {
        return Err(ValidationError::InvalidPath(format!(
            "{}: contains null byte",
            field_name
        )));
    }

    let path_obj = Path::new(path);
    for component in path_obj.components() {
        if let std::path::Component::ParentDir = component {
            return Err(ValidationError::PathTraversal(format!(
                "{}: contains '..' sequence",
                field_name
            )));
        }
    }

    Ok(())
}

/// Validate a container image reference.
///
/// Rejects shell metacharacters and invalid name formats.
pub fn validate_image(image: &str) -> Result<(), ValidationError> {
    if image.is_empty() {
        return Err(ValidationError::EmptyField("image".to_string()));
    }

    if image.len() > MAX_FIELD_LENGTH {
        return Err(ValidationError::MaxLengthExceeded {
            field: "image".to_string(),
            max: MAX_FIELD_LENGTH,
        });
    }

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

    let parts: Vec<&str> = image.rsplitn(2, ':').collect();
    let name_part = parts.last().unwrap_or(&image);

    if name_part.starts_with('-') || name_part.starts_with('.') {
        return Err(ValidationError::InvalidImage(
            "name cannot start with dash or dot".to_string(),
        ));
    }

    Ok(())
}

/// Validate a model ID.
///
/// Only alphanumeric, dashes, underscores, and dots allowed.
pub fn validate_model_id(model_id: &str) -> Result<(), ValidationError> {
    if model_id.is_empty() {
        return Err(ValidationError::EmptyField("model_id".to_string()));
    }

    if model_id.len() > MAX_FIELD_LENGTH {
        return Err(ValidationError::MaxLengthExceeded {
            field: "model_id".to_string(),
            max: MAX_FIELD_LENGTH,
        });
    }

    let valid_chars = |c: char| c.is_alphanumeric() || c == '-' || c == '_' || c == '.';
    if !model_id.chars().all(valid_chars) {
        return Err(ValidationError::InvalidModelId(
            "must contain only alphanumeric characters, dashes, underscores, and dots".to_string(),
        ));
    }

    if model_id.contains('/') || model_id.contains('\\') {
        return Err(ValidationError::InvalidModelId(
            "cannot contain path separators".to_string(),
        ));
    }

    Ok(())
}

/// Validate a socket path.
///
/// Must be absolute, no traversal or null bytes.
pub fn validate_socket_path(socket_path: &str) -> Result<(), ValidationError> {
    if socket_path.is_empty() {
        return Err(ValidationError::EmptyField("socket_path".to_string()));
    }

    if socket_path.len() > MAX_PATH_LENGTH {
        return Err(ValidationError::MaxLengthExceeded {
            field: "socket_path".to_string(),
            max: MAX_PATH_LENGTH,
        });
    }

    if socket_path.contains('\0') {
        return Err(ValidationError::InvalidSocketPath(
            "contains null byte".to_string(),
        ));
    }

    if socket_path.contains("..") {
        return Err(ValidationError::PathTraversal(format!(
            "socket_path: {}",
            socket_path
        )));
    }

    if !socket_path.starts_with('/') {
        return Err(ValidationError::InvalidSocketPath(
            "must be an absolute path".to_string(),
        ));
    }

    Ok(())
}
