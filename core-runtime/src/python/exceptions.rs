// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Python exception types for Veritas SDR errors

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

use crate::engine::error::InferenceError as EngineInferenceError;
use crate::engine::inference::InferenceError as RuntimeInferenceError;
use crate::ipc::AuthError;
use crate::models::LoadError;

// Define exception hierarchy
create_exception!(veritas_sdr, CoreError, PyException);
create_exception!(veritas_sdr, AuthenticationError, CoreError);
create_exception!(veritas_sdr, InferenceError, CoreError);
create_exception!(veritas_sdr, ModelError, CoreError);
create_exception!(veritas_sdr, TimeoutError, CoreError);
create_exception!(veritas_sdr, CancellationError, CoreError);

/// Convert AuthError to Python exception
impl From<AuthError> for PyErr {
    fn from(err: AuthError) -> PyErr {
        AuthenticationError::new_err(err.to_string())
    }
}

/// Convert engine InferenceError to Python exception
impl From<EngineInferenceError> for PyErr {
    fn from(err: EngineInferenceError) -> PyErr {
        match &err {
            EngineInferenceError::Timeout(_) => TimeoutError::new_err(err.to_string()),
            _ => InferenceError::new_err(err.to_string()),
        }
    }
}

/// Convert LoadError to Python exception
impl From<LoadError> for PyErr {
    fn from(err: LoadError) -> PyErr {
        ModelError::new_err(err.to_string())
    }
}

/// Convert runtime InferenceError to Python exception
impl From<RuntimeInferenceError> for PyErr {
    fn from(err: RuntimeInferenceError) -> PyErr {
        InferenceError::new_err(err.to_string())
    }
}

/// Helper to convert generic errors to CoreError
pub fn core_error(msg: impl Into<String>) -> PyErr {
    CoreError::new_err(msg.into())
}
