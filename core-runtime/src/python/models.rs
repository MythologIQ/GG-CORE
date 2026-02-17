// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Python model management types

use pyo3::prelude::*;

/// Information about a loaded model
#[pyclass]
#[derive(Clone)]
pub struct ModelInfo {
    /// Model name or identifier
    #[pyo3(get)]
    pub name: String,

    /// Model size in bytes
    #[pyo3(get)]
    pub size_bytes: u64,
}

#[pymethods]
impl ModelInfo {
    #[new]
    fn new(name: String, size_bytes: u64) -> Self {
        Self { name, size_bytes }
    }

    fn __repr__(&self) -> String {
        let size_mb = self.size_bytes as f64 / (1024.0 * 1024.0);
        format!("ModelInfo(name='{}', size={:.1}MB)", self.name, size_mb)
    }

    /// Get human-readable size string
    #[getter]
    fn size_human(&self) -> String {
        let bytes = self.size_bytes as f64;
        if bytes >= 1_073_741_824.0 {
            format!("{:.2} GB", bytes / 1_073_741_824.0)
        } else if bytes >= 1_048_576.0 {
            format!("{:.2} MB", bytes / 1_048_576.0)
        } else if bytes >= 1024.0 {
            format!("{:.2} KB", bytes / 1024.0)
        } else {
            format!("{} bytes", self.size_bytes)
        }
    }
}
