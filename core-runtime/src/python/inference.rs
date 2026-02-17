// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Python inference parameter and result types

use pyo3::prelude::*;

use crate::engine::InferenceParams as RustParams;
use crate::engine::InferenceResult as RustResult;

/// Inference parameters for controlling generation
///
/// Example:
/// ```python
/// params = InferenceParams(
///     max_tokens=256,
///     temperature=0.7,
///     top_p=0.9,
///     top_k=40
/// )
/// result = session.infer("model", tokens, params)
/// ```
#[pyclass]
#[derive(Clone)]
pub struct InferenceParams {
    /// Maximum tokens to generate
    #[pyo3(get, set)]
    pub max_tokens: u32,

    /// Temperature for sampling (0.0 = deterministic, 2.0 = very random)
    #[pyo3(get, set)]
    pub temperature: f32,

    /// Nucleus sampling threshold (0.0-1.0)
    #[pyo3(get, set)]
    pub top_p: f32,

    /// Top-k sampling (number of tokens to consider)
    #[pyo3(get, set)]
    pub top_k: u32,

    /// Enable streaming output
    #[pyo3(get, set)]
    pub stream: bool,

    /// Timeout in milliseconds (None = no timeout)
    #[pyo3(get, set)]
    pub timeout_ms: Option<u64>,
}

#[pymethods]
impl InferenceParams {
    /// Create inference parameters
    #[new]
    #[pyo3(signature = (max_tokens=256, temperature=0.7, top_p=0.9, top_k=40, stream=false, timeout_ms=None))]
    fn new(
        max_tokens: u32,
        temperature: f32,
        top_p: f32,
        top_k: u32,
        stream: bool,
        timeout_ms: Option<u64>,
    ) -> Self {
        Self {
            max_tokens,
            temperature,
            top_p,
            top_k,
            stream,
            timeout_ms,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "InferenceParams(max_tokens={}, temperature={}, top_p={}, top_k={}, stream={})",
            self.max_tokens, self.temperature, self.top_p, self.top_k, self.stream
        )
    }
}

impl Default for InferenceParams {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            stream: false,
            timeout_ms: None,
        }
    }
}

impl From<&InferenceParams> for RustParams {
    fn from(py: &InferenceParams) -> Self {
        Self {
            max_tokens: py.max_tokens as usize,
            temperature: py.temperature,
            top_p: py.top_p,
            top_k: py.top_k as usize,
            stream: py.stream,
            timeout_ms: py.timeout_ms,
        }
    }
}

/// Result from inference operation
///
/// Contains the generated tokens and completion status.
#[pyclass]
#[derive(Clone)]
pub struct InferenceResult {
    /// Generated token IDs
    #[pyo3(get)]
    pub tokens: Vec<u32>,

    /// Whether generation finished normally (vs truncated/cancelled)
    #[pyo3(get)]
    pub finished: bool,
}

#[pymethods]
impl InferenceResult {
    fn __repr__(&self) -> String {
        format!(
            "InferenceResult(tokens=[...{} tokens], finished={})",
            self.tokens.len(),
            self.finished
        )
    }

    fn __len__(&self) -> usize {
        self.tokens.len()
    }

    /// Get token at index
    fn __getitem__(&self, idx: isize) -> PyResult<u32> {
        let len = self.tokens.len() as isize;
        let actual_idx = if idx < 0 { len + idx } else { idx };

        if actual_idx < 0 || actual_idx >= len {
            Err(pyo3::exceptions::PyIndexError::new_err("index out of range"))
        } else {
            Ok(self.tokens[actual_idx as usize])
        }
    }
}

impl From<RustResult> for InferenceResult {
    fn from(result: RustResult) -> Self {
        Self {
            tokens: result.output_tokens,
            finished: result.finished,
        }
    }
}
