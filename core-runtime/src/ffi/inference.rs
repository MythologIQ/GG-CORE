// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Inference API functions for FFI (text-based v1 API)

use std::ffi::{c_char, CStr, CString};

use super::auth::CoreSession;
use super::error::{set_last_error, CoreErrorCode};
use super::runtime::CoreRuntime;
use super::types::{CoreInferenceParams, CoreInferenceResult};
use crate::engine::InferenceParams;
use crate::scheduler::Priority;

/// Submit inference request (blocking, text-based).
/// # Safety
/// All non-null pointers must be valid. `params` may be null for defaults.
#[no_mangle]
pub unsafe extern "C" fn core_infer(
    runtime: *mut CoreRuntime,
    session: *mut CoreSession,
    model_id: *const c_char,
    prompt: *const c_char,
    params: *const CoreInferenceParams,
    out_result: *mut CoreInferenceResult,
) -> CoreErrorCode {
    if runtime.is_null() || session.is_null() {
        set_last_error("null runtime or session pointer");
        return CoreErrorCode::NullPointer;
    }
    if model_id.is_null() || prompt.is_null() || out_result.is_null() {
        set_last_error("null argument pointer");
        return CoreErrorCode::NullPointer;
    }

    let rt = &*runtime;
    let sess = &*session;

    // Validate session
    if let Err(e) = rt.tokio.block_on(async {
        rt.inner.ipc_handler.auth.validate(&sess.token).await
    }) {
        return e.into();
    }

    let model_str = match CStr::from_ptr(model_id).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("invalid UTF-8 in model_id");
            return CoreErrorCode::InvalidParams;
        }
    };

    let prompt_str = match CStr::from_ptr(prompt).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("invalid UTF-8 in prompt");
            return CoreErrorCode::InvalidParams;
        }
    };

    let default_params = CoreInferenceParams::default();
    let c_params = if params.is_null() { &default_params } else { &*params };
    let rust_params = params_from_c(c_params);

    let result = rt.tokio.block_on(async {
        let (_id, rx) = rt.inner.request_queue
            .enqueue_with_response(
                model_str.to_string(),
                prompt_str.to_string(),
                rust_params,
                Priority::Normal,
            )
            .await
            .map_err(|e| e.to_string())?;

        rx.await
            .map_err(|_| "worker dropped channel".to_string())?
            .map_err(|e| e.to_string())
    });

    match result {
        Ok(r) => {
            write_inference_result(&r, &mut *out_result);
            CoreErrorCode::Ok
        }
        Err(e) => {
            set_last_error(&e);
            CoreErrorCode::InferenceFailed
        }
    }
}

/// Submit inference request with timeout (blocking).
/// # Safety
/// Same as `core_infer`.
#[no_mangle]
pub unsafe extern "C" fn core_infer_with_timeout(
    runtime: *mut CoreRuntime,
    session: *mut CoreSession,
    model_id: *const c_char,
    prompt: *const c_char,
    params: *const CoreInferenceParams,
    timeout_ms: u64,
    out_result: *mut CoreInferenceResult,
) -> CoreErrorCode {
    let mut timed_params = if params.is_null() {
        CoreInferenceParams::default()
    } else {
        (*params).clone()
    };
    timed_params.timeout_ms = timeout_ms;

    core_infer(runtime, session, model_id, prompt, &timed_params, out_result)
}

/// Free inference result text. # Safety: `result` must be null or from `core_infer`.
#[no_mangle]
pub unsafe extern "C" fn core_free_result(result: *mut CoreInferenceResult) {
    if !result.is_null() {
        let r = &mut *result;
        if !r.output_text.is_null() {
            drop(CString::from_raw(r.output_text));
            r.output_text = std::ptr::null_mut();
        }
    }
}

/// Convert C params to Rust params
pub(super) fn params_from_c(c: &CoreInferenceParams) -> InferenceParams {
    InferenceParams {
        max_tokens: c.max_tokens as usize,
        temperature: c.temperature,
        top_p: c.top_p,
        top_k: c.top_k as usize,
        stream: c.stream,
        timeout_ms: if c.timeout_ms == 0 { None } else { Some(c.timeout_ms) },
    }
}

/// Write inference result to C struct
fn write_inference_result(
    result: &crate::engine::InferenceResult,
    out: &mut CoreInferenceResult,
) {
    let cstr = CString::new(result.output.clone()).unwrap_or_default();
    out.output_text = cstr.into_raw();
    out.tokens_generated = result.tokens_generated as u32;
    out.finished = result.finished;
}

/// Inference with caller-provided buffer. # Safety: all pointers valid, `out_buf` writable for `buf_len`.
#[no_mangle]
pub unsafe extern "C" fn core_infer_bounded(
    runtime: *mut CoreRuntime,
    session: *mut CoreSession,
    model_id: *const c_char,
    prompt: *const c_char,
    params: *const CoreInferenceParams,
    out_buf: *mut u8,
    buf_len: usize,
    out_len: *mut usize,
) -> CoreErrorCode {
    if runtime.is_null() || session.is_null() {
        set_last_error("null runtime or session pointer");
        return CoreErrorCode::NullPointer;
    }
    if model_id.is_null() || prompt.is_null() || out_buf.is_null() || out_len.is_null() {
        set_last_error("null argument pointer");
        return CoreErrorCode::NullPointer;
    }

    // SAFETY: pointers validated non-null above; caller guarantees validity.
    let rt = &*runtime;
    let sess = &*session;

    if let Err(e) = rt.tokio.block_on(async {
        rt.inner.ipc_handler.auth.validate(&sess.token).await
    }) {
        return e.into();
    }

    // SAFETY: model_id validated non-null; caller guarantees valid C string.
    let model_str = match CStr::from_ptr(model_id).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("invalid UTF-8 in model_id");
            return CoreErrorCode::InvalidParams;
        }
    };

    // SAFETY: prompt validated non-null; caller guarantees valid C string.
    let prompt_str = match CStr::from_ptr(prompt).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("invalid UTF-8 in prompt");
            return CoreErrorCode::InvalidParams;
        }
    };

    let default_params = CoreInferenceParams::default();
    let c_params = if params.is_null() { &default_params } else { &*params };
    let rust_params = params_from_c(c_params);

    let result = rt.tokio.block_on(async {
        let (_id, rx) = rt.inner.request_queue
            .enqueue_with_response(
                model_str.to_string(),
                prompt_str.to_string(),
                rust_params,
                Priority::Normal,
            )
            .await
            .map_err(|e| e.to_string())?;
        rx.await
            .map_err(|_| "worker dropped channel".to_string())?
            .map_err(|e| e.to_string())
    });

    match result {
        Ok(r) => {
            let bytes = r.output.as_bytes();
            if bytes.len() > buf_len {
                set_last_error("output exceeds buffer length");
                return CoreErrorCode::BufferTooSmall;
            }
            // SAFETY: out_buf non-null, buf_len writable, bytes.len() <= buf_len.
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf, bytes.len());
            *out_len = bytes.len();
            CoreErrorCode::Ok
        }
        Err(e) => {
            set_last_error(&e);
            CoreErrorCode::InferenceFailed
        }
    }
}

impl Clone for CoreInferenceParams {
    fn clone(&self) -> Self {
        Self {
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k,
            stream: self.stream,
            timeout_ms: self.timeout_ms,
        }
    }
}
