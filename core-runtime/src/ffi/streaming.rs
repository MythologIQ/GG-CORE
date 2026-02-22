// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Callback-based streaming for FFI (text-based v1 API)

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::auth::CoreSession;
use super::error::{set_last_error, CoreErrorCode};
use super::inference::params_from_c;
use super::runtime::CoreRuntime;
use super::types::CoreInferenceParams;
use crate::scheduler::Priority;

/// Streaming callback signature
/// Return false to cancel streaming
pub type CoreStreamCallback = unsafe extern "C" fn(
    user_data: *mut c_void,
    text: *const c_char,
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
    fn invoke(&self, text: &str, is_final: bool, error: Option<&str>) -> bool {
        if self.cancelled.load(Ordering::SeqCst) {
            return false;
        }

        let text_cstr = CString::new(text).unwrap_or_default();
        let error_cstr = error.and_then(|e| CString::new(e).ok());
        let error_ptr = error_cstr
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(std::ptr::null());

        // SAFETY: callback and user_data are provided by the FFI caller who guarantees
        // the function pointer is valid and user_data lifetime spans this call.
        let cont = unsafe {
            (self.callback)(self.user_data, text_cstr.as_ptr(), is_final, error_ptr)
        };

        if !cont {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        cont
    }
}

/// Submit streaming inference (blocks until done/cancelled).
/// # Safety
/// All pointers valid. `callback` must be safe to invoke from any thread.
#[no_mangle]
pub unsafe extern "C" fn core_infer_streaming(
    runtime: *mut CoreRuntime,
    session: *mut CoreSession,
    model_id: *const c_char,
    prompt: *const c_char,
    params: *const CoreInferenceParams,
    callback: CoreStreamCallback,
    user_data: *mut c_void,
) -> CoreErrorCode {
    if runtime.is_null() || session.is_null() {
        set_last_error("null runtime or session pointer");
        return CoreErrorCode::NullPointer;
    }
    if model_id.is_null() || prompt.is_null() {
        set_last_error("null argument pointer");
        return CoreErrorCode::NullPointer;
    }

    let rt = &*runtime;
    let sess = &*session;

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

    let cancelled = Arc::new(AtomicBool::new(false));
    let invoker = CallbackInvoker {
        callback,
        user_data,
        cancelled: cancelled.clone(),
    };

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
            invoker.invoke(&r.output, true, None);
            if cancelled.load(Ordering::SeqCst) {
                CoreErrorCode::Cancelled
            } else {
                CoreErrorCode::Ok
            }
        }
        Err(e) => {
            invoker.invoke("", true, Some(&e));
            set_last_error(&e);
            CoreErrorCode::InferenceFailed
        }
    }
}

/// Free string allocated by core functions. # Safety: `s` must be null or from core APIs.
#[no_mangle]
pub unsafe extern "C" fn core_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}
