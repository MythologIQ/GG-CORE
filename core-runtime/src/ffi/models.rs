// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Model management functions for FFI (lifecycle-aware v1 API)

use std::ffi::{c_char, CStr, CString};

use super::error::{set_last_error, CoreErrorCode};
use super::runtime::CoreRuntime;
use super::types::CoreModelMetadata;
use crate::engine::gguf;

/// Load a model via ModelLifecycle. # Safety: all pointers must be valid.
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

    let validated = match rt.inner.model_loader.validate_path(path_str) {
        Ok(p) => p,
        Err(e) => return e.into(),
    };

    let metadata = match rt.inner.model_loader.load_metadata(&validated) {
        Ok(m) => m,
        Err(e) => return e.into(),
    };

    let model_id = metadata.name.clone();

    // Load GGUF model (or stub if feature disabled)
    let model = match gguf::load_gguf_model(
        validated.as_path(),
        &model_id,
        &gguf::GgufConfig::default(),
    ) {
        Ok(m) => m,
        Err(e) => {
            set_last_error(format!("model load: {}", e));
            return CoreErrorCode::ModelLoadFailed;
        }
    };

    // Atomic load via lifecycle coordinator
    let result = rt.tokio.block_on(async {
        rt.inner.model_lifecycle.load(model_id, metadata, model).await
    });

    match result {
        Ok(handle) => {
            *out_handle_id = handle.id();
            CoreErrorCode::Ok
        }
        Err(e) => {
            set_last_error(format!("{}", e));
            CoreErrorCode::ModelLoadFailed
        }
    }
}

/// Unload a model via ModelLifecycle. # Safety: `runtime` must be valid.
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

    // Resolve handle -> model_id via lifecycle index
    let model_id = rt.tokio.block_on(async {
        rt.inner.model_lifecycle.get_model_id(handle_id).await
    });

    let model_id = match model_id {
        Some(id) => id,
        None => {
            set_last_error("model not found");
            return CoreErrorCode::ModelNotFound;
        }
    };

    let result = rt.tokio.block_on(async {
        rt.inner.model_lifecycle.unload(&model_id).await
    });

    match result {
        Ok(_) => CoreErrorCode::Ok,
        Err(e) => {
            set_last_error(format!("{}", e));
            CoreErrorCode::ModelNotFound
        }
    }
}

/// Get model info. # Safety: all pointers must be valid.
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

/// Free model metadata. # Safety: `metadata` must be null or from `core_model_info`.
#[no_mangle]
pub unsafe extern "C" fn core_free_model_metadata(
    metadata: *mut CoreModelMetadata,
) {
    if !metadata.is_null() {
        let m = &mut *metadata;
        if !m.name.is_null() {
            drop(CString::from_raw(m.name as *mut c_char));
            m.name = std::ptr::null();
        }
    }
}

/// List loaded models. # Safety: `out_handles` must have room for `max_count` u64 values.
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

    let models = rt.tokio.block_on(async {
        rt.inner.model_registry.list_models().await
    });

    let write_count = models.len().min(max_count as usize);
    for (i, info) in models.iter().take(write_count).enumerate() {
        *out_handles.add(i) = info.handle_id;
    }
    *out_count = write_count as u32;

    CoreErrorCode::Ok
}

/// Get count of loaded models. # Safety: all pointers must be valid.
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
