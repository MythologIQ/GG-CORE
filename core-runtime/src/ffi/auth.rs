// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Authentication functions for FFI

use std::ffi::{c_char, CStr, CString};
use std::sync::Arc;

use super::error::{set_last_error, CoreErrorCode};
use super::runtime::CoreRuntime;
use crate::ipc::SessionToken;
use crate::Runtime;

/// Session handle with reference counting
pub struct CoreSession {
    pub(crate) token: SessionToken,
    pub(crate) runtime: Arc<Runtime>,
    session_id_cstr: CString,
}

/// Authenticate with token, returns session handle.
/// # Safety
/// All pointers must be valid. `token` must be a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn core_authenticate(
    runtime: *mut CoreRuntime,
    token: *const c_char,
    out_session: *mut *mut CoreSession,
) -> CoreErrorCode {
    if runtime.is_null() || token.is_null() || out_session.is_null() {
        set_last_error("null pointer argument");
        return CoreErrorCode::NullPointer;
    }

    let rt = &*runtime;
    let token_str = match CStr::from_ptr(token).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("invalid UTF-8 in token");
            return CoreErrorCode::InvalidParams;
        }
    };

    let result = rt.tokio.block_on(async {
        rt.inner.ipc_handler.auth.authenticate(token_str).await
    });

    match result {
        Ok(session_token) => {
            let session_id = session_token.as_str().to_string();
            let session_id_cstr = match CString::new(session_id) {
                Ok(s) => s,
                Err(_) => {
                    set_last_error("session ID contains null byte");
                    return CoreErrorCode::Internal;
                }
            };

            let session = Box::new(CoreSession {
                token: session_token,
                runtime: Arc::clone(&rt.inner),
                session_id_cstr,
            });
            *out_session = Box::into_raw(session);
            CoreErrorCode::Ok
        }
        Err(e) => e.into(),
    }
}

/// Validate existing session. # Safety: pointers must be valid.
#[no_mangle]
pub unsafe extern "C" fn core_session_validate(
    runtime: *mut CoreRuntime,
    session: *mut CoreSession,
) -> CoreErrorCode {
    if runtime.is_null() || session.is_null() {
        set_last_error("null pointer argument");
        return CoreErrorCode::NullPointer;
    }

    let rt = &*runtime;
    let sess = &*session;

    let result = rt.tokio.block_on(async {
        rt.inner.ipc_handler.auth.validate(&sess.token).await
    });

    match result {
        Ok(()) => CoreErrorCode::Ok,
        Err(e) => e.into(),
    }
}

/// Release session handle. # Safety: `session` must be null or from `core_authenticate`.
#[no_mangle]
pub unsafe extern "C" fn core_session_release(session: *mut CoreSession) {
    if !session.is_null() {
        drop(Box::from_raw(session));
    }
}

/// Get session ID string (valid until session released). # Safety: session must be valid.
#[no_mangle]
pub unsafe extern "C" fn core_session_id(session: *const CoreSession) -> *const c_char {
    if session.is_null() {
        return std::ptr::null();
    }
    (*session).session_id_cstr.as_ptr()
}
