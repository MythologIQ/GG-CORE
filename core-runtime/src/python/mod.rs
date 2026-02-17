// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Python bindings for Veritas SDR using PyO3
//!
//! Provides a Pythonic API for the inference runtime with both
//! synchronous and asynchronous interfaces.

mod exceptions;
mod inference;
mod models;
mod runtime;
mod session;
mod streaming;

use pyo3::prelude::*;

/// Veritas SDR Python module
///
/// Example usage:
/// ```python
/// import veritas_sdr
///
/// runtime = veritas_sdr.Runtime(auth_token="secret")
/// with runtime.session() as session:
///     result = session.infer("model-id", [1, 2, 3])
///     print(result.tokens)
/// ```
#[pymodule]
fn veritas_sdr(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Add exception types
    m.add("CoreError", m.py().get_type::<exceptions::CoreError>())?;
    m.add(
        "AuthenticationError",
        m.py().get_type::<exceptions::AuthenticationError>(),
    )?;
    m.add(
        "InferenceError",
        m.py().get_type::<exceptions::InferenceError>(),
    )?;
    m.add("ModelError", m.py().get_type::<exceptions::ModelError>())?;
    m.add(
        "TimeoutError",
        m.py().get_type::<exceptions::TimeoutError>(),
    )?;
    m.add(
        "CancellationError",
        m.py().get_type::<exceptions::CancellationError>(),
    )?;

    // Add classes
    m.add_class::<runtime::Runtime>()?;
    m.add_class::<session::Session>()?;
    m.add_class::<session::AsyncSession>()?;
    m.add_class::<inference::InferenceParams>()?;
    m.add_class::<inference::InferenceResult>()?;
    m.add_class::<streaming::StreamingResult>()?;
    m.add_class::<models::ModelInfo>()?;

    // Module metadata
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}
