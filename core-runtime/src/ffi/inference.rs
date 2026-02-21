// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Inference API functions for FFI

use std::ffi::{c_char, CStr};

use super::auth::CoreSession;
use super::error::{set_last_error, CoreErrorCode};
use super::runtime::CoreRuntime;
use super::types::{CoreInferenceParams, CoreInferenceResult};
use crate::engine::InferenceParams;
use crate::models::ModelHandle;

/// Submit inference request (blocking)
#[no_mangle]
pub unsafe extern "C" fn core_infer(
    runtime: *mut CoreRuntime,
    session: *mut CoreSession,
    model_id: *const c_char,
    prompt_tokens: *const u32,
    prompt_token_count: u32,
    params: *const CoreInferenceParams,
    out_result: *mut CoreInferenceResult,
) -> CoreErrorCode {
    // Validate pointers
    if runtime.is_null() || session.is_null() {
        set_last_error("null runtime or session pointer");
        return CoreErrorCode::NullPointer;
    }
    if model_id.is_null() || prompt_tokens.is_null() || out_result.is_null() {
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

    // Parse model ID
    let _model_str = match CStr::from_ptr(model_id).to_str() {
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

    // Copy input tokens with bounds checking
    let tokens: Vec<u32> = if prompt_token_count == 0 {
        Vec::new()
    } else {
        // SAFETY: prompt_token_count is validated above, caller ensures valid pointer
        unsafe { std::slice::from_raw_parts(prompt_tokens, prompt_token_count as usize).to_vec() }
    };

    // Convert params
    let default_params = CoreInferenceParams::default();
    let c_params = if params.is_null() {
        &default_params
    } else {
        &*params
    };
    let rust_params = params_from_c(c_params);

    // Run inference
    let result = rt.tokio.block_on(async {
        rt.inner
            .inference_engine
            .run(ModelHandle::new(0), &tokens, &rust_params)
            .await
    });

    match result {
        Ok(inference_result) => {
            write_inference_result(&inference_result, &mut *out_result);
            CoreErrorCode::Ok
        }
        Err(e) => e.into(),
    }
}

/// Submit inference request with timeout (blocking)
#[no_mangle]
pub unsafe extern "C" fn core_infer_with_timeout(
    runtime: *mut CoreRuntime,
    session: *mut CoreSession,
    model_id: *const c_char,
    prompt_tokens: *const u32,
    prompt_token_count: u32,
    params: *const CoreInferenceParams,
    timeout_ms: u64,
    out_result: *mut CoreInferenceResult,
) -> CoreErrorCode {
    // Create params with timeout
    let mut timed_params = if params.is_null() {
        CoreInferenceParams::default()
    } else {
        (*params).clone()
    };
    timed_params.timeout_ms = timeout_ms;

    core_infer(
        runtime,
        session,
        model_id,
        prompt_tokens,
        prompt_token_count,
        &timed_params,
        out_result,
    )
}

/// Free tokens from inference result
#[no_mangle]
pub unsafe extern "C" fn core_free_tokens(tokens: *mut u32, count: u32) {
    if !tokens.is_null() && count > 0 {
        let _ = Vec::from_raw_parts(tokens, count as usize, count as usize);
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
        timeout_ms: if c.timeout_ms == 0 {
            None
        } else {
            Some(c.timeout_ms)
        },
    }
}

/// Write inference result to C struct
fn write_inference_result(result: &crate::engine::InferenceResult, out: &mut CoreInferenceResult) {
    let mut output = result.output_tokens.clone().into_boxed_slice();
    out.token_count = output.len() as u32;
    out.tokens = output.as_mut_ptr();
    out.finished = result.finished;
    std::mem::forget(output); // Caller must free via core_free_tokens
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
