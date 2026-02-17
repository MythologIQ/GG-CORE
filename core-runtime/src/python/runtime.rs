// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Python Runtime class wrapping the Rust runtime

use std::sync::Arc;

use pyo3::prelude::*;
use tokio::runtime::Runtime as TokioRuntime;

use super::exceptions::{core_error, AuthenticationError};
use super::session::{AsyncSession, Session};
use crate::ipc::SessionToken;
use crate::Runtime as CoreRuntime;

/// Main runtime entry point for Veritas SDR
///
/// Creates and manages the inference runtime with authentication.
///
/// Example:
/// ```python
/// runtime = Runtime(auth_token="your-secret-token", base_path="/models")
/// ```
#[pyclass]
pub struct Runtime {
    inner: Arc<CoreRuntime>,
    tokio: Arc<TokioRuntime>,
    auth_token: String,
}

#[pymethods]
impl Runtime {
    /// Create a new runtime instance
    ///
    /// Args:
    ///     auth_token: Authentication token for session validation
    ///     base_path: Optional path to models directory
    ///     max_context_length: Maximum context window (default: 4096)
    ///     max_queue_depth: Maximum request queue depth (default: 1000)
    #[new]
    #[pyo3(signature = (auth_token, base_path=None, max_context_length=4096, max_queue_depth=1000))]
    fn new(
        auth_token: &str,
        base_path: Option<&str>,
        max_context_length: u32,
        max_queue_depth: u32,
    ) -> PyResult<Self> {
        // Create Tokio runtime
        let tokio = TokioRuntime::new().map_err(|e| core_error(format!("tokio init: {}", e)))?;

        // Build config starting from defaults
        let mut config = crate::RuntimeConfig::default();
        config.auth_token = auth_token.to_string();
        config.max_context_length = max_context_length as usize;
        config.request_queue.max_pending = max_queue_depth as usize;

        if let Some(path) = base_path {
            config.base_path = std::path::PathBuf::from(path);
        }

        // Initialize runtime (sync - no async new)
        let inner = CoreRuntime::new(config);

        Ok(Self {
            inner: Arc::new(inner),
            tokio: Arc::new(tokio),
            auth_token: auth_token.to_string(),
        })
    }

    /// Create a synchronous session
    ///
    /// Returns a session context manager for inference operations.
    ///
    /// Example:
    /// ```python
    /// with runtime.session() as session:
    ///     result = session.infer("model", tokens)
    /// ```
    fn session(&self) -> PyResult<Session> {
        let token = self.authenticate()?;
        Ok(Session::new(self.inner.clone(), self.tokio.clone(), token))
    }

    /// Create an async session
    ///
    /// Returns an async session for use with asyncio.
    ///
    /// Example:
    /// ```python
    /// async with await runtime.session_async() as session:
    ///     result = await session.infer("model", tokens)
    /// ```
    fn session_async(&self) -> PyResult<AsyncSession> {
        let token = self.authenticate()?;
        Ok(AsyncSession::new(self.inner.clone(), token))
    }

    /// Get number of loaded models
    fn model_count(&self) -> PyResult<usize> {
        Ok(self.tokio.block_on(async { self.inner.model_registry.count().await }))
    }

    /// Check if runtime is healthy
    fn is_healthy(&self) -> bool {
        self.tokio.block_on(async {
            let state = self.inner.shutdown.state().await;
            matches!(state, crate::shutdown::ShutdownState::Running)
        })
    }

    /// Get runtime version
    #[staticmethod]
    fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

impl Runtime {
    /// Internal: authenticate and get session token
    fn authenticate(&self) -> PyResult<SessionToken> {
        self.tokio
            .block_on(async {
                self.inner
                    .ipc_handler
                    .auth
                    .authenticate(&self.auth_token)
                    .await
            })
            .map_err(|e| AuthenticationError::new_err(e.to_string()))
    }

    /// Get reference to inner runtime (for session use)
    #[allow(dead_code)]
    pub(super) fn inner(&self) -> &Arc<CoreRuntime> {
        &self.inner
    }

    /// Get reference to tokio runtime (for session use)
    #[allow(dead_code)]
    pub(super) fn tokio(&self) -> &Arc<TokioRuntime> {
        &self.tokio
    }
}
