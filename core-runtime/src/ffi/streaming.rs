// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Callback-based streaming for FFI

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::auth::CoreSession;
use super::error::{set_last_error, CoreErrorCode};
use super::inference::params_from_c;
use super::runtime::CoreRuntime;
use super::types::CoreInferenceParams;
use crate::engine::TokenStream;
use crate::models::ModelHandle;

/// Streaming callback signature
/// Return false to cancel streaming
pub type CoreStreamCallback = unsafe extern "C" fn(
    user_data: *mut c_void,
    token: u32,
    is_final: bool,
    error: *const c_char,
) -> bool;

/// Wrapper to invoke C callback from Rust async context
struct CallbackInvoker {
    callback: CoreStreamCallback,
    user_data: *mut c_void,
    cancelled: Arc<AtomicBool>,
}

// SAFETY: user_data pointer is provided by caller who ensures thread safety
unsafe impl Send for CallbackInvoker {}
unsafe impl Sync for CallbackInvoker {}

impl CallbackInvoker {
    fn invoke(&self, token: u32, is_final: bool, error: Option<&str>) -> bool {
        if self.cancelled.load(Ordering::SeqCst) {
            return false;
        }

        let error_cstr = error.and_then(|e| CString::new(e).ok());
        let error_ptr = error_cstr
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(std::ptr::null());

        let should_continue = unsafe {
            (self.callback)(self.user_data, token, is_final, error_ptr)
        };

        if !should_continue {
            self.cancelled.store(true, Ordering::SeqCst);
        }

        should_continue
    }
}

/// Submit streaming inference request (blocks until complete/cancelled)
#[no_mangle]
pub unsafe extern "C" fn core_infer_streaming(
    runtime: *mut CoreRuntime,
    session: *mut CoreSession,
    model_id: *const c_char,
    prompt_tokens: *const u32,
    prompt_token_count: u32,
    params: *const CoreInferenceParams,
    callback: CoreStreamCallback,
    user_data: *mut c_void,
) -> CoreErrorCode {
    if runtime.is_null() || session.is_null() {
        set_last_error("null runtime or session pointer");
        return CoreErrorCode::NullPointer;
    }
    if model_id.is_null() || prompt_tokens.is_null() {
        set_last_error("null argument pointer");
        return CoreErrorCode::NullPointer;
    }

    let rt = &*runtime;
    let sess = &*session;

    // Validate session
    let validate_result = rt.tokio.block_on(async {
        rt.inner.ipc_handler.auth.validate(&sess.token).await
    });
    if let Err(e) = validate_result {
        return e.into();
    }

    let _model_str = match CStr::from_ptr(model_id).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("invalid UTF-8 in model_id");
            return CoreErrorCode::InvalidParams;
        }
    };

    let tokens: Vec<u32> =
        std::slice::from_raw_parts(prompt_tokens, prompt_token_count as usize).to_vec();

    let default_params = CoreInferenceParams::default();
    let c_params = if params.is_null() { &default_params } else { &*params };
    let rust_params = params_from_c(c_params);

    let cancelled = Arc::new(AtomicBool::new(false));
    let invoker = CallbackInvoker {
        callback,
        user_data,
        cancelled: cancelled.clone(),
    };

    let result = rt.tokio.block_on(async {
        stream_inference(&rt.inner, &tokens, &rust_params, &invoker).await
    });

    match result {
        Ok(()) => {
            if cancelled.load(Ordering::SeqCst) {
                CoreErrorCode::Cancelled
            } else {
                CoreErrorCode::Ok
            }
        }
        Err(e) => {
            invoker.invoke(0, true, Some(&e.to_string()));
            e.into()
        }
    }
}

/// Internal streaming inference implementation
async fn stream_inference(
    runtime: &crate::Runtime,
    tokens: &[u32],
    params: &crate::engine::InferenceParams,
    invoker: &CallbackInvoker,
) -> Result<(), crate::engine::inference::InferenceError> {
    // Create token stream
    let (_sender, _stream) = TokenStream::new(32);

    // Run inference (placeholder - actual impl would spawn task and send tokens)
    let result = runtime
        .inference_engine
        .run(ModelHandle::new(0), tokens, params)
        .await?;

    // Stream output tokens
    for (i, &token) in result.output_tokens.iter().enumerate() {
        let is_final = i == result.output_tokens.len() - 1;
        if !invoker.invoke(token, is_final, None) {
            break;
        }
    }

    Ok(())
}

/// Free string allocated by core functions
#[no_mangle]
pub unsafe extern "C" fn core_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}
