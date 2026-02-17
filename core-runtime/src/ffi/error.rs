// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Error handling for FFI layer
//!
//! Uses thread-local storage for error messages to provide detailed
//! error information alongside error codes.

use std::cell::RefCell;
use std::ffi::{c_char, CString};

/// Error codes for FFI functions
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreErrorCode {
    Ok = 0,
    NullPointer = -1,
    InvalidConfig = -2,
    AuthFailed = -3,
    SessionExpired = -4,
    SessionNotFound = -5,
    RateLimited = -6,
    ModelNotFound = -7,
    ModelLoadFailed = -8,
    InferenceFailed = -9,
    ContextExceeded = -10,
    InvalidParams = -11,
    QueueFull = -12,
    ShuttingDown = -13,
    Timeout = -14,
    Cancelled = -15,
    Internal = -99,
}

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Store error message in thread-local storage
pub fn set_last_error(msg: impl Into<String>) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = CString::new(msg.into()).ok();
    });
}

/// Clear the last error message
pub fn clear_last_error() {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = None;
    });
}

/// Get pointer to last error message (or null if none)
fn get_last_error_ptr() -> *const c_char {
    LAST_ERROR.with(|e| {
        e.borrow()
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(std::ptr::null())
    })
}

/// Get the last error message (C API)
#[no_mangle]
pub extern "C" fn core_get_last_error() -> *const c_char {
    get_last_error_ptr()
}

/// Clear the last error message (C API)
#[no_mangle]
pub extern "C" fn core_clear_last_error() {
    clear_last_error();
}

impl From<crate::ipc::AuthError> for CoreErrorCode {
    fn from(err: crate::ipc::AuthError) -> Self {
        use crate::ipc::AuthError;
        set_last_error(format!("{}", err));
        match err {
            AuthError::InvalidToken => CoreErrorCode::AuthFailed,
            AuthError::SessionNotFound => CoreErrorCode::SessionNotFound,
            AuthError::SessionExpired => CoreErrorCode::SessionExpired,
            AuthError::NotAuthenticated => CoreErrorCode::AuthFailed,
            AuthError::RateLimited => CoreErrorCode::RateLimited,
        }
    }
}

impl From<crate::engine::InferenceError> for CoreErrorCode {
    fn from(err: crate::engine::InferenceError) -> Self {
        use crate::engine::InferenceError;
        set_last_error(format!("{}", err));
        match err {
            InferenceError::ModelNotLoaded(_) => CoreErrorCode::ModelNotFound,
            InferenceError::InputValidation(_) => CoreErrorCode::InvalidParams,
            InferenceError::Timeout(_) => CoreErrorCode::Timeout,
            InferenceError::MemoryExceeded { .. } => CoreErrorCode::ContextExceeded,
            InferenceError::OutputFiltered { .. } => CoreErrorCode::InferenceFailed,
            InferenceError::ModelError(_) => CoreErrorCode::InferenceFailed,
            InferenceError::RateLimited => CoreErrorCode::RateLimited,
            InferenceError::QueueFull { .. } => CoreErrorCode::QueueFull,
            InferenceError::CapabilityNotSupported(_) => CoreErrorCode::InvalidParams,
            InferenceError::HashMismatch { .. } => CoreErrorCode::ModelLoadFailed,
            InferenceError::InvalidFormat(_) => CoreErrorCode::InvalidParams,
        }
    }
}

impl From<crate::models::LoadError> for CoreErrorCode {
    fn from(err: crate::models::LoadError) -> Self {
        use crate::models::LoadError;
        set_last_error(format!("{}", err));
        match err {
            LoadError::PathNotAllowed(_) => CoreErrorCode::InvalidParams,
            LoadError::NotFound(_) => CoreErrorCode::ModelNotFound,
            LoadError::InvalidFormat(_) => CoreErrorCode::ModelLoadFailed,
            LoadError::Io(_) => CoreErrorCode::ModelLoadFailed,
        }
    }
}

// Handle the InferenceError from inference.rs (used by InferenceEngine::run)
impl From<crate::engine::inference::InferenceError> for CoreErrorCode {
    fn from(err: crate::engine::inference::InferenceError) -> Self {
        use crate::engine::inference::InferenceError;
        set_last_error(format!("{}", err));
        match err {
            InferenceError::ModelNotLoaded(_) => CoreErrorCode::ModelNotFound,
            InferenceError::InvalidParams(_) => CoreErrorCode::InvalidParams,
            InferenceError::ExecutionFailed(_) => CoreErrorCode::InferenceFailed,
            InferenceError::ContextExceeded { .. } => CoreErrorCode::ContextExceeded,
        }
    }
}
