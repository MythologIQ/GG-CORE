// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Health and metrics functions for FFI

use std::ffi::{c_char, CString};

use super::error::{set_last_error, CoreErrorCode};
use super::runtime::CoreRuntime;
use super::types::{CoreHealthReport, CoreHealthState};
use crate::health::HealthState;

/// Health check (no authentication required). # Safety: pointers must be valid.
#[no_mangle]
pub unsafe extern "C" fn core_health_check(
    runtime: *mut CoreRuntime,
    out_report: *mut CoreHealthReport,
) -> CoreErrorCode {
    if runtime.is_null() || out_report.is_null() {
        set_last_error("null pointer argument");
        return CoreErrorCode::NullPointer;
    }

    let rt = &*runtime;

    // Gather health data
    let shutdown_state = rt.tokio.block_on(async { rt.inner.shutdown.state().await });
    let models = rt.tokio.block_on(async { rt.inner.model_registry.count().await });
    let queue = rt.tokio.block_on(async { rt.inner.request_queue.len().await });
    let memory = rt.inner.memory_pool.available();

    let report = rt.inner.health.report(shutdown_state, models, memory, queue);

    (*out_report).state = match report.state {
        HealthState::Healthy => CoreHealthState::Healthy,
        HealthState::Degraded => CoreHealthState::Degraded,
        HealthState::Unhealthy => CoreHealthState::Unhealthy,
    };
    (*out_report).ready = report.ready;
    (*out_report).accepting_requests = report.accepting_requests;
    (*out_report).models_loaded = report.models_loaded as u32;
    (*out_report).memory_used_bytes = report.memory_used_bytes as u64;
    (*out_report).queue_depth = report.queue_depth as u32;
    (*out_report).uptime_secs = report.uptime_secs;

    CoreErrorCode::Ok
}

/// Liveness check. # Safety: `runtime` must be null or valid.
#[no_mangle]
pub unsafe extern "C" fn core_is_alive(runtime: *mut CoreRuntime) -> bool {
    if runtime.is_null() {
        return false;
    }

    let rt = &*runtime;
    rt.inner.health.is_alive()
}

/// Readiness check. # Safety: `runtime` must be null or valid.
#[no_mangle]
pub unsafe extern "C" fn core_is_ready(runtime: *mut CoreRuntime) -> bool {
    if runtime.is_null() {
        return false;
    }

    let rt = &*runtime;
    let shutdown_state = rt.tokio.block_on(async { rt.inner.shutdown.state().await });
    let models = rt.tokio.block_on(async { rt.inner.model_registry.count().await });
    let queue = rt.tokio.block_on(async { rt.inner.request_queue.len().await });

    rt.inner.health.is_ready(shutdown_state, models, queue)
}

/// Get metrics JSON (free with `core_free_string`). # Safety: pointers must be valid.
#[no_mangle]
pub unsafe extern "C" fn core_get_metrics_json(
    runtime: *mut CoreRuntime,
    out_json: *mut *mut c_char,
) -> CoreErrorCode {
    if runtime.is_null() || out_json.is_null() {
        set_last_error("null pointer argument");
        return CoreErrorCode::NullPointer;
    }

    let rt = &*runtime;

    let snapshot = rt.inner.metrics_store.snapshot();
    let json = match serde_json::to_string(&snapshot) {
        Ok(j) => j,
        Err(e) => {
            set_last_error(format!("JSON serialization failed: {}", e));
            return CoreErrorCode::Internal;
        }
    };

    match CString::new(json) {
        Ok(cstr) => {
            *out_json = cstr.into_raw();
            CoreErrorCode::Ok
        }
        Err(_) => {
            set_last_error("metrics contain null byte");
            CoreErrorCode::Internal
        }
    }
}
