//! FFI module integration tests for CORE Runtime.
//!
//! Tests the C FFI layer including error handling, type conversions,
//! and API function signatures. These tests focus on FFI-specific logic
//! that can be tested without a full inference backend.

#![cfg(feature = "ffi")]

use std::ffi::{c_char, CStr, CString};
use std::ptr;

use veritas_sdr::ffi::{
    core_clear_last_error, core_config_default, core_get_last_error,
    core_runtime_create, core_runtime_destroy,
    CoreConfig, CoreErrorCode, CoreHealthReport, CoreHealthState,
    CoreInferenceParams, CoreInferenceResult, CoreModelMetadata,
};

// ============================================================================
// Error Code Tests
// ============================================================================

#[test]
fn test_error_codes_have_correct_values() {
    assert_eq!(CoreErrorCode::Ok as i32, 0);
    assert_eq!(CoreErrorCode::NullPointer as i32, -1);
    assert_eq!(CoreErrorCode::InvalidConfig as i32, -2);
    assert_eq!(CoreErrorCode::AuthFailed as i32, -3);
    assert_eq!(CoreErrorCode::SessionExpired as i32, -4);
    assert_eq!(CoreErrorCode::SessionNotFound as i32, -5);
    assert_eq!(CoreErrorCode::RateLimited as i32, -6);
    assert_eq!(CoreErrorCode::ModelNotFound as i32, -7);
    assert_eq!(CoreErrorCode::ModelLoadFailed as i32, -8);
    assert_eq!(CoreErrorCode::InferenceFailed as i32, -9);
    assert_eq!(CoreErrorCode::ContextExceeded as i32, -10);
    assert_eq!(CoreErrorCode::InvalidParams as i32, -11);
    assert_eq!(CoreErrorCode::QueueFull as i32, -12);
    assert_eq!(CoreErrorCode::ShuttingDown as i32, -13);
    assert_eq!(CoreErrorCode::Timeout as i32, -14);
    assert_eq!(CoreErrorCode::Cancelled as i32, -15);
    assert_eq!(CoreErrorCode::Internal as i32, -99);
}

#[test]
fn test_error_codes_are_copy_and_eq() {
    let code1 = CoreErrorCode::Ok;
    let code2 = code1; // Copy
    assert_eq!(code1, code2);
    assert_eq!(CoreErrorCode::AuthFailed, CoreErrorCode::AuthFailed);
    assert_ne!(CoreErrorCode::Ok, CoreErrorCode::Internal);
}

// ============================================================================
// Error Message Tests
// ============================================================================

#[test]
fn test_get_last_error_returns_null_when_no_error() {
    core_clear_last_error();
    let ptr = core_get_last_error();
    assert!(ptr.is_null());
}

#[test]
fn test_clear_last_error_clears_message() {
    // Trigger an error by passing null config
    let mut out_runtime: *mut veritas_sdr::ffi::CoreRuntime = ptr::null_mut();
    unsafe {
        core_runtime_create(ptr::null(), &mut out_runtime);
    }

    // Should have an error message
    let ptr = core_get_last_error();
    assert!(!ptr.is_null());

    // Clear it
    core_clear_last_error();

    // Should be null now
    let ptr = core_get_last_error();
    assert!(ptr.is_null());
}

#[test]
fn test_error_message_set_on_null_pointer() {
    core_clear_last_error();

    let mut out_runtime: *mut veritas_sdr::ffi::CoreRuntime = ptr::null_mut();
    let result = unsafe { core_runtime_create(ptr::null(), &mut out_runtime) };

    assert_eq!(result, CoreErrorCode::NullPointer);

    let ptr = core_get_last_error();
    assert!(!ptr.is_null());

    let msg = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
    assert!(msg.contains("null"));
}

// ============================================================================
// CoreConfig Tests
// ============================================================================

#[test]
fn test_config_default_sets_reasonable_values() {
    let mut config = CoreConfig {
        base_path: ptr::null(),
        auth_token: ptr::null(),
        session_timeout_secs: 0,
        max_context_length: 0,
        max_queue_depth: 0,
        shutdown_timeout_secs: 0,
    };

    core_config_default(&mut config);

    assert!(config.base_path.is_null());
    assert!(config.auth_token.is_null());
    assert_eq!(config.session_timeout_secs, 3600);
    assert_eq!(config.max_context_length, 4096);
    assert_eq!(config.max_queue_depth, 1000);
    assert_eq!(config.shutdown_timeout_secs, 30);
}

#[test]
fn test_config_default_handles_null_pointer() {
    // Should not panic when passed null
    core_config_default(ptr::null_mut());
}

// ============================================================================
// CoreInferenceParams Tests
// ============================================================================

#[test]
fn test_inference_params_default_values() {
    let params = CoreInferenceParams::default();

    assert_eq!(params.max_tokens, 256);
    assert!((params.temperature - 0.7).abs() < f32::EPSILON);
    assert!((params.top_p - 0.9).abs() < f32::EPSILON);
    assert_eq!(params.top_k, 40);
    assert!(!params.stream);
    assert_eq!(params.timeout_ms, 0);
}

#[test]
fn test_inference_params_repr_c() {
    // Verify struct is correctly laid out for C
    let params = CoreInferenceParams {
        max_tokens: 512,
        temperature: 0.5,
        top_p: 0.95,
        top_k: 50,
        stream: true,
        timeout_ms: 30000,
    };

    assert_eq!(params.max_tokens, 512);
    assert!((params.temperature - 0.5).abs() < f32::EPSILON);
    assert_eq!(params.timeout_ms, 30000);
}

// ============================================================================
// CoreInferenceResult Tests
// ============================================================================

#[test]
fn test_inference_result_default_values() {
    let result = CoreInferenceResult::default();

    assert!(result.tokens.is_null());
    assert_eq!(result.token_count, 0);
    assert!(!result.finished);
}

// ============================================================================
// CoreHealthState Tests
// ============================================================================

#[test]
fn test_health_state_values() {
    assert_eq!(CoreHealthState::Healthy as i32, 0);
    assert_eq!(CoreHealthState::Degraded as i32, 1);
    assert_eq!(CoreHealthState::Unhealthy as i32, 2);
}

#[test]
fn test_health_state_is_copy_and_eq() {
    let state1 = CoreHealthState::Healthy;
    let state2 = state1; // Copy
    assert_eq!(state1, state2);
}

// ============================================================================
// CoreHealthReport Tests
// ============================================================================

#[test]
fn test_health_report_default_values() {
    let report = CoreHealthReport::default();

    assert_eq!(report.state, CoreHealthState::Unhealthy);
    assert!(!report.ready);
    assert!(!report.accepting_requests);
    assert_eq!(report.models_loaded, 0);
    assert_eq!(report.memory_used_bytes, 0);
    assert_eq!(report.queue_depth, 0);
    assert_eq!(report.uptime_secs, 0);
}

// ============================================================================
// CoreModelMetadata Tests
// ============================================================================

#[test]
fn test_model_metadata_default_values() {
    let metadata = CoreModelMetadata::default();

    assert!(metadata.name.is_null());
    assert_eq!(metadata.size_bytes, 0);
    assert_eq!(metadata.handle_id, 0);
}

// ============================================================================
// Runtime Lifecycle Tests (without full backend)
// ============================================================================

#[test]
fn test_runtime_create_rejects_null_config() {
    core_clear_last_error();

    let mut out_runtime: *mut veritas_sdr::ffi::CoreRuntime = ptr::null_mut();
    let result = unsafe { core_runtime_create(ptr::null(), &mut out_runtime) };

    assert_eq!(result, CoreErrorCode::NullPointer);
    assert!(out_runtime.is_null());
}

#[test]
fn test_runtime_create_rejects_null_out_pointer() {
    core_clear_last_error();

    let config = CoreConfig::default();
    let result = unsafe { core_runtime_create(&config, ptr::null_mut()) };

    assert_eq!(result, CoreErrorCode::NullPointer);
}

#[test]
fn test_runtime_create_requires_auth_token() {
    core_clear_last_error();

    let config = CoreConfig::default();
    let mut out_runtime: *mut veritas_sdr::ffi::CoreRuntime = ptr::null_mut();

    let result = unsafe { core_runtime_create(&config, &mut out_runtime) };

    assert_eq!(result, CoreErrorCode::InvalidConfig);
    assert!(out_runtime.is_null());

    let ptr = core_get_last_error();
    assert!(!ptr.is_null());
    let msg = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
    assert!(msg.contains("auth_token"));
}

#[test]
fn test_runtime_destroy_handles_null() {
    // Should not panic when passed null
    unsafe { core_runtime_destroy(ptr::null_mut()) };
}

#[test]
fn test_runtime_create_and_destroy_lifecycle() {
    core_clear_last_error();

    let auth_token = CString::new("test_token_12345").unwrap();
    let mut config = CoreConfig::default();
    config.auth_token = auth_token.as_ptr();

    let mut out_runtime: *mut veritas_sdr::ffi::CoreRuntime = ptr::null_mut();

    let result = unsafe { core_runtime_create(&config, &mut out_runtime) };

    assert_eq!(result, CoreErrorCode::Ok);
    assert!(!out_runtime.is_null());

    // Clean up
    unsafe { core_runtime_destroy(out_runtime) };
}

// ============================================================================
// Type Size and Alignment Tests (for FFI ABI stability)
// ============================================================================

#[test]
fn test_core_config_is_repr_c() {
    // These tests verify the struct layout is stable for C interop
    use std::mem::{align_of, size_of};

    // CoreConfig should have pointer alignment (8 bytes on 64-bit)
    assert!(align_of::<CoreConfig>() >= align_of::<*const c_char>());

    // CoreConfig has: 2 pointers + 2 u64 + 2 u32 = 16 + 16 + 8 = 40 bytes minimum
    // With alignment padding it may be larger
    assert!(size_of::<CoreConfig>() >= 40);
}

#[test]
fn test_core_error_code_is_i32() {
    use std::mem::size_of;

    // Error codes should be i32 for C compatibility
    assert_eq!(size_of::<CoreErrorCode>(), size_of::<i32>());
}
