// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Python streaming types for token-by-token output

use pyo3::prelude::*;

/// A single streaming result chunk
///
/// Represents one token in the streaming output.
#[pyclass]
#[derive(Clone)]
pub struct StreamingResult {
    /// The generated token ID
    #[pyo3(get)]
    pub token: u32,

    /// Index of this token in the sequence (0-based)
    #[pyo3(get)]
    pub index: usize,

    /// Whether this is the final token
    #[pyo3(get)]
    pub is_final: bool,

    /// Error message if generation failed (None if successful)
    #[pyo3(get)]
    pub error: Option<String>,
}

#[pymethods]
impl StreamingResult {
    fn __repr__(&self) -> String {
        if let Some(ref err) = self.error {
            format!("StreamingResult(error={})", err)
        } else {
            format!(
                "StreamingResult(token={}, index={}, is_final={})",
                self.token, self.index, self.is_final
            )
        }
    }

    /// Check if this result indicates an error
    #[getter]
    fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

/// Iterator for streaming inference results
///
/// Yields StreamingResult objects one at a time.
///
/// Example:
/// ```python
/// for chunk in session.infer_streaming("model", tokens):
///     if chunk.is_error:
///         print(f"Error: {chunk.error}")
///     else:
///         print(f"Token {chunk.index}: {chunk.token}")
/// ```
#[pyclass]
pub struct StreamingIterator {
    tokens: Vec<u32>,
    index: usize,
}

impl StreamingIterator {
    pub fn new(tokens: Vec<u32>) -> Self {
        Self { tokens, index: 0 }
    }
}

#[pymethods]
impl StreamingIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<StreamingResult> {
        if slf.index >= slf.tokens.len() {
            return None;
        }

        let token = slf.tokens[slf.index];
        let index = slf.index;
        let is_final = index == slf.tokens.len() - 1;

        slf.index += 1;

        Some(StreamingResult {
            token,
            index,
            is_final,
            error: None,
        })
    }

    /// Get total number of tokens
    fn __len__(&self) -> usize {
        self.tokens.len()
    }
}

/// Async iterator for streaming inference (future implementation)
///
/// For true async streaming, this would yield tokens as they are generated.
#[pyclass]
pub struct AsyncStreamingIterator {
    tokens: Vec<u32>,
    index: usize,
}

impl AsyncStreamingIterator {
    #[allow(dead_code)]
    pub fn new(tokens: Vec<u32>) -> Self {
        Self { tokens, index: 0 }
    }
}

#[pymethods]
impl AsyncStreamingIterator {
    fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __anext__(mut slf: PyRefMut<'_, Self>) -> Option<StreamingResult> {
        if slf.index >= slf.tokens.len() {
            return None;
        }

        let token = slf.tokens[slf.index];
        let index = slf.index;
        let is_final = index == slf.tokens.len() - 1;

        slf.index += 1;

        Some(StreamingResult {
            token,
            index,
            is_final,
            error: None,
        })
    }
}
