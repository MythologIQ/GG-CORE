// Copyright 2024-2026 GG-CORE Contributors
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

        let should_continue =
            unsafe { (self.callback)(self.user_data, token, is_final, error_ptr) };

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
    let validate_result = rt
        .tokio
        .block_on(async { rt.inner.ipc_handler.auth.validate(&sess.token).await });
    if let Err(e) = validate_result {
        return e.into();
    }

    let model_str = match CStr::from_ptr(model_id).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("invalid UTF-8 in model_id");
            return CoreErrorCode::InvalidParams;
        }
    };

    // SECURITY: Validate token count to prevent memory safety issues
    // Maximum reasonable token count (1M tokens = ~4MB of u32)
    const MAX_TOKEN_COUNT: u32 = 1_000_000;
    if prompt_token_count > MAX_TOKEN_COUNT {
        set_last_error("prompt_token_count exceeds maximum allowed");
        return CoreErrorCode::InvalidParams;
    }

    // SAFETY: We've validated that prompt_token_count is within bounds
    // and the caller ensures prompt_tokens points to valid memory
    let tokens: Vec<u32> = if prompt_token_count == 0 {
        Vec::new()
    } else {
        // SAFETY: prompt_token_count is validated above, caller ensures valid pointer
        unsafe { std::slice::from_raw_parts(prompt_tokens, prompt_token_count as usize).to_vec() }
    };

    let default_params = CoreInferenceParams::default();
    let c_params = if params.is_null() {
        &default_params
    } else {
        &*params
    };
    let rust_params = params_from_c(c_params);

    let cancelled = Arc::new(AtomicBool::new(false));
    let invoker = CallbackInvoker {
        callback,
        user_data,
        cancelled: cancelled.clone(),
    };

    let result = rt.tokio.block_on(async {
        stream_inference(&rt.inner, model_str, &tokens, &rust_params, &invoker).await
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
    model_id: &str,
    tokens: &[u32],
    params: &crate::engine::InferenceParams,
    invoker: &CallbackInvoker,
) -> Result<(), crate::engine::inference::InferenceError> {
    // FAIL-FAST: v0.6.5 protocol is text-based
    // Token-based FFI requires tokenizer to decode tokens to text.
    // This path is deprecated - FFI consumers should migrate to text prompts.
    if !tokens.is_empty() {
        return Err(crate::engine::inference::InferenceError::InvalidParams(
            "Token-based FFI streaming deprecated in v0.6.5. Use text prompts.".into(),
        ));
    }

    // Create token stream for future streaming implementation
    let (_sender, _stream) = TokenStream::new(32);

    // Run inference using text-based API with proper model lookup
    let result = runtime.inference_engine.run(model_id, "", params).await?;

    // Send completion callback (streaming would tokenize output)
    invoker.invoke(0, true, None);

    // Return success - tokens_generated is in the result
    let _ = result.tokens_generated;
    Ok(())
}

/// Free string allocated by core functions
#[no_mangle]
pub unsafe extern "C" fn core_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}
