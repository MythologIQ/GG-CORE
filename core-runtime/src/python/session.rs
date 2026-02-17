// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Python Session classes for inference operations

use std::sync::Arc;

use pyo3::prelude::*;
use tokio::runtime::Runtime as TokioRuntime;

use super::exceptions::AuthenticationError;
use super::inference::{InferenceParams, InferenceResult};
use super::streaming::StreamingIterator;
use crate::engine::InferenceParams as RustParams;
use crate::ipc::SessionToken;
use crate::models::ModelHandle;
use crate::Runtime as CoreRuntime;

/// Synchronous session for inference operations
///
/// Use as a context manager:
/// ```python
/// with runtime.session() as session:
///     result = session.infer("model", [1, 2, 3])
/// ```
#[pyclass]
pub struct Session {
    runtime: Arc<CoreRuntime>,
    tokio: Arc<TokioRuntime>,
    token: SessionToken,
    valid: bool,
}

impl Session {
    pub(super) fn new(
        runtime: Arc<CoreRuntime>,
        tokio: Arc<TokioRuntime>,
        token: SessionToken,
    ) -> Self {
        Self {
            runtime,
            tokio,
            token,
            valid: true,
        }
    }

    fn check_valid(&self) -> PyResult<()> {
        if !self.valid {
            return Err(AuthenticationError::new_err("session has been closed"));
        }
        self.tokio.block_on(async {
            self.runtime.ipc_handler.auth.validate(&self.token).await
        }).map_err(|e| AuthenticationError::new_err(e.to_string()))
    }
}

#[pymethods]
impl Session {
    /// Run inference on a model
    ///
    /// Args:
    ///     model_id: Model identifier or handle ID
    ///     tokens: Input token IDs
    ///     params: Optional inference parameters
    ///
    /// Returns:
    ///     InferenceResult with output tokens
    #[pyo3(signature = (model_id, tokens, params=None))]
    fn infer(
        &self,
        model_id: u64,
        tokens: Vec<u32>,
        params: Option<&InferenceParams>,
    ) -> PyResult<InferenceResult> {
        self.check_valid()?;

        let rust_params = params
            .map(RustParams::from)
            .unwrap_or_default();

        let result = self.tokio.block_on(async {
            self.runtime
                .inference_engine
                .run(ModelHandle::new(model_id), &tokens, &rust_params)
                .await
        })?;

        Ok(InferenceResult::from(result))
    }

    /// Run streaming inference
    ///
    /// Returns an iterator that yields tokens as they are generated.
    ///
    /// Example:
    /// ```python
    /// for chunk in session.infer_streaming("model", tokens):
    ///     print(chunk.token)
    /// ```
    #[pyo3(signature = (model_id, tokens, params=None))]
    fn infer_streaming(
        &self,
        model_id: u64,
        tokens: Vec<u32>,
        params: Option<&InferenceParams>,
    ) -> PyResult<StreamingIterator> {
        self.check_valid()?;

        let rust_params = params
            .map(RustParams::from)
            .unwrap_or_default();

        // Run inference and collect results for iteration
        let result = self.tokio.block_on(async {
            self.runtime
                .inference_engine
                .run(ModelHandle::new(model_id), &tokens, &rust_params)
                .await
        })?;

        Ok(StreamingIterator::new(result.output_tokens))
    }

    /// Context manager enter
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    /// Context manager exit
    fn __exit__(
        &mut self,
        _exc_type: Option<PyObject>,
        _exc_val: Option<PyObject>,
        _exc_tb: Option<PyObject>,
    ) -> bool {
        self.valid = false;
        false // Don't suppress exceptions
    }
}

/// Async session for asyncio-based inference
///
/// Use with async context manager:
/// ```python
/// async with await runtime.session_async() as session:
///     result = await session.infer("model", tokens)
/// ```
#[pyclass]
pub struct AsyncSession {
    runtime: Arc<CoreRuntime>,
    token: SessionToken,
    valid: bool,
}

impl AsyncSession {
    pub(super) fn new(runtime: Arc<CoreRuntime>, token: SessionToken) -> Self {
        Self {
            runtime,
            token,
            valid: true,
        }
    }
}

#[pymethods]
impl AsyncSession {
    /// Async inference
    #[pyo3(signature = (model_id, tokens, params=None))]
    fn infer<'py>(
        &self,
        py: Python<'py>,
        model_id: u64,
        tokens: Vec<u32>,
        params: Option<InferenceParams>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !self.valid {
            return Err(AuthenticationError::new_err("session closed"));
        }

        let runtime = self.runtime.clone();
        let token = self.token.clone();
        let rust_params = params
            .as_ref()
            .map(RustParams::from)
            .unwrap_or_default();

        pyo3_asyncio_0_21::tokio::future_into_py(py, async move {
            // Validate token
            runtime.ipc_handler.auth.validate(&token).await
                .map_err(|e| AuthenticationError::new_err(e.to_string()))?;

            let result = runtime
                .inference_engine
                .run(ModelHandle::new(model_id), &tokens, &rust_params)
                .await?;

            Ok(InferenceResult::from(result))
        })
    }

    /// Async context manager enter
    fn __aenter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    /// Async context manager exit
    fn __aexit__(
        &mut self,
        _exc_type: Option<PyObject>,
        _exc_val: Option<PyObject>,
        _exc_tb: Option<PyObject>,
    ) -> bool {
        self.valid = false;
        false
    }
}
