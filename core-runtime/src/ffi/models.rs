// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Model management functions for FFI

use std::ffi::{c_char, CStr, CString};

use super::error::{set_last_error, CoreErrorCode};
use super::runtime::CoreRuntime;
use super::types::CoreModelMetadata;

/// Load a model from path (relative to base_path/models/)
#[no_mangle]
pub unsafe extern "C" fn core_model_load(
    runtime: *mut CoreRuntime,
    model_path: *const c_char,
    out_handle_id: *mut u64,
) -> CoreErrorCode {
    if runtime.is_null() || model_path.is_null() || out_handle_id.is_null() {
        set_last_error("null pointer argument");
        return CoreErrorCode::NullPointer;
    }

    let rt = &*runtime;
    let path_str = match CStr::from_ptr(model_path).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("invalid UTF-8 in model_path");
            return CoreErrorCode::InvalidParams;
        }
    };

    // Validate path
    let model_path = match rt.inner.model_loader.validate_path(path_str) {
        Ok(p) => p,
        Err(e) => return e.into(),
    };

    // Load metadata
    let metadata = match rt.inner.model_loader.load_metadata(&model_path) {
        Ok(m) => m,
        Err(e) => return e.into(),
    };

    // Register model
    let handle = rt.tokio.block_on(async {
        rt.inner
            .model_registry
            .register(metadata, 0)
            .await
    });

    *out_handle_id = handle.id();
    CoreErrorCode::Ok
}

/// Unload a model by handle
#[no_mangle]
pub unsafe extern "C" fn core_model_unload(
    runtime: *mut CoreRuntime,
    handle_id: u64,
) -> CoreErrorCode {
    if runtime.is_null() {
        set_last_error("null pointer argument");
        return CoreErrorCode::NullPointer;
    }

    let rt = &*runtime;
    let handle = crate::models::ModelHandle::new(handle_id);

    let result = rt.tokio.block_on(async {
        rt.inner.model_registry.unregister(handle).await
    });

    match result {
        Some(_) => CoreErrorCode::Ok,
        None => {
            set_last_error("model not found");
            CoreErrorCode::ModelNotFound
        }
    }
}

/// Get model info
#[no_mangle]
pub unsafe extern "C" fn core_model_info(
    runtime: *mut CoreRuntime,
    handle_id: u64,
    out_metadata: *mut CoreModelMetadata,
) -> CoreErrorCode {
    if runtime.is_null() || out_metadata.is_null() {
        set_last_error("null pointer argument");
        return CoreErrorCode::NullPointer;
    }

    let rt = &*runtime;
    let handle = crate::models::ModelHandle::new(handle_id);

    let metadata = rt.tokio.block_on(async {
        rt.inner.model_registry.get_metadata(handle).await
    });

    match metadata {
        Some(m) => {
            let name_cstr = CString::new(m.name).unwrap_or_default();
            (*out_metadata).name = name_cstr.into_raw();
            (*out_metadata).size_bytes = m.size_bytes;
            (*out_metadata).handle_id = handle_id;
            CoreErrorCode::Ok
        }
        None => {
            set_last_error("model not found");
            CoreErrorCode::ModelNotFound
        }
    }
}

/// Free model metadata
#[no_mangle]
pub unsafe extern "C" fn core_free_model_metadata(metadata: *mut CoreModelMetadata) {
    if !metadata.is_null() {
        let m = &mut *metadata;
        if !m.name.is_null() {
            drop(CString::from_raw(m.name as *mut c_char));
            m.name = std::ptr::null();
        }
    }
}

/// List all loaded models
#[no_mangle]
pub unsafe extern "C" fn core_model_list(
    runtime: *mut CoreRuntime,
    out_handles: *mut u64,
    max_count: u32,
    out_count: *mut u32,
) -> CoreErrorCode {
    if runtime.is_null() || out_handles.is_null() || out_count.is_null() {
        set_last_error("null pointer argument");
        return CoreErrorCode::NullPointer;
    }

    let rt = &*runtime;

    let count = rt.tokio.block_on(async {
        rt.inner.model_registry.count().await
    });

    // For now, return count but don't fill handles (registry doesn't expose list)
    *out_count = count.min(max_count as usize) as u32;

    CoreErrorCode::Ok
}

/// Get count of loaded models
#[no_mangle]
pub unsafe extern "C" fn core_model_count(
    runtime: *mut CoreRuntime,
    out_count: *mut u32,
) -> CoreErrorCode {
    if runtime.is_null() || out_count.is_null() {
        set_last_error("null pointer argument");
        return CoreErrorCode::NullPointer;
    }

    let rt = &*runtime;

    let count = rt.tokio.block_on(async {
        rt.inner.model_registry.count().await
    });

    *out_count = count as u32;
    CoreErrorCode::Ok
}
