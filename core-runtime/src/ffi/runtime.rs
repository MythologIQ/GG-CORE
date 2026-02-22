// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Runtime lifecycle functions for FFI

use std::ffi::CStr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::Runtime as TokioRuntime;

use super::error::{set_last_error, CoreErrorCode};
use super::types::CoreConfig;
use crate::{Runtime, RuntimeConfig};

/// Opaque handle wrapping Rust runtime
pub struct CoreRuntime {
    pub(crate) inner: Arc<Runtime>,
    pub(crate) tokio: TokioRuntime,
}

/// Get default configuration values
#[no_mangle]
pub extern "C" fn core_config_default(config: *mut CoreConfig) {
    if config.is_null() {
        return;
    }
    // SAFETY: config validated non-null above; caller guarantees valid writable pointer.
    unsafe {
        *config = CoreConfig::default();
    }
}

/// Create runtime with configuration.
/// # Safety
/// `config` and `out_runtime` must be valid non-null pointers.
#[no_mangle]
pub unsafe extern "C" fn core_runtime_create(
    config: *const CoreConfig,
    out_runtime: *mut *mut CoreRuntime,
) -> CoreErrorCode {
    if config.is_null() || out_runtime.is_null() {
        set_last_error("null pointer argument");
        return CoreErrorCode::NullPointer;
    }

    let c_config = &*config;

    // Validate auth token is provided
    if c_config.auth_token.is_null() {
        set_last_error("auth_token is required");
        return CoreErrorCode::InvalidConfig;
    }

    let rust_config = match config_from_c(c_config) {
        Ok(c) => c,
        Err(e) => {
            set_last_error(e);
            return CoreErrorCode::InvalidConfig;
        }
    };

    // Create tokio runtime for blocking on async
    let tokio = match TokioRuntime::new() {
        Ok(rt) => rt,
        Err(e) => {
            set_last_error(format!("tokio runtime: {}", e));
            return CoreErrorCode::Internal;
        }
    };

    let inner = Arc::new(Runtime::new(rust_config));
    let handle = Box::new(CoreRuntime { inner, tokio });
    *out_runtime = Box::into_raw(handle);

    CoreErrorCode::Ok
}

/// Destroy runtime (blocks until graceful shutdown).
/// # Safety
/// `runtime` must be null or from `core_runtime_create`. Must not be called concurrently.
#[no_mangle]
pub unsafe extern "C" fn core_runtime_destroy(runtime: *mut CoreRuntime) {
    if runtime.is_null() {
        return;
    }

    let rt = Box::from_raw(runtime);

    // Graceful shutdown
    rt.tokio.block_on(async {
        rt.inner.shutdown.initiate(rt.inner.config.shutdown_timeout).await;
    });

    // rt dropped here, releasing Arc
}

/// Convert C config to Rust config
fn config_from_c(c: &CoreConfig) -> Result<RuntimeConfig, String> {
    let auth_token = if c.auth_token.is_null() {
        return Err("auth_token is required".into());
    } else {
        // SAFETY: auth_token validated non-null in the branch above.
        unsafe { CStr::from_ptr(c.auth_token) }
            .to_str()
            .map_err(|_| "invalid UTF-8 in auth_token")?
            .to_string()
    };

    let base_path = if c.base_path.is_null() {
        PathBuf::from(".")
    } else {
        // SAFETY: base_path validated non-null in the branch above.
        let path_str = unsafe { CStr::from_ptr(c.base_path) }
            .to_str()
            .map_err(|_| "invalid UTF-8 in base_path")?;
        PathBuf::from(path_str)
    };

    Ok(RuntimeConfig {
        base_path,
        auth_token,
        session_timeout: Duration::from_secs(c.session_timeout_secs),
        max_context_length: c.max_context_length as usize,
        request_queue: crate::scheduler::RequestQueueConfig {
            max_pending: c.max_queue_depth as usize,
            max_context_tokens: c.max_context_length as usize,
        },
        shutdown_timeout: Duration::from_secs(c.shutdown_timeout_secs),
        ..Default::default()
    })
}
