// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! C-compatible type definitions for FFI

use std::ffi::c_char;

/// Runtime configuration (C-compatible struct)
#[repr(C)]
pub struct CoreConfig {
    /// Base path for models directory (NULL = current directory)
    pub base_path: *const c_char,
    /// Authentication token (required, non-NULL)
    pub auth_token: *const c_char,
    /// Session timeout in seconds (default: 3600)
    pub session_timeout_secs: u64,
    /// Maximum context length (default: 4096)
    pub max_context_length: u32,
    /// Maximum queue depth (default: 1000)
    pub max_queue_depth: u32,
    /// Shutdown timeout in seconds (default: 30)
    pub shutdown_timeout_secs: u64,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            base_path: std::ptr::null(),
            auth_token: std::ptr::null(),
            session_timeout_secs: 3600,
            max_context_length: 4096,
            max_queue_depth: 1000,
            shutdown_timeout_secs: 30,
        }
    }
}

/// Inference parameters (matches InferenceParams)
#[repr(C)]
pub struct CoreInferenceParams {
    /// Maximum tokens to generate (default: 256)
    pub max_tokens: u32,
    /// Temperature for sampling (default: 0.7)
    pub temperature: f32,
    /// Top-p (nucleus) sampling (default: 0.9)
    pub top_p: f32,
    /// Top-k sampling (default: 40)
    pub top_k: u32,
    /// Enable streaming output (default: false)
    pub stream: bool,
    /// Timeout in milliseconds (0 = no timeout)
    pub timeout_ms: u64,
}

impl Default for CoreInferenceParams {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            stream: false,
            timeout_ms: 0,
        }
    }
}

/// Inference result (for non-streaming)
#[repr(C)]
pub struct CoreInferenceResult {
    /// Output tokens (caller must free with core_free_tokens)
    pub tokens: *mut u32,
    /// Number of output tokens
    pub token_count: u32,
    /// Whether generation finished normally
    pub finished: bool,
}

impl Default for CoreInferenceResult {
    fn default() -> Self {
        Self {
            tokens: std::ptr::null_mut(),
            token_count: 0,
            finished: false,
        }
    }
}

/// Health state enumeration
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreHealthState {
    Healthy = 0,
    Degraded = 1,
    Unhealthy = 2,
}

/// Health check report
#[repr(C)]
pub struct CoreHealthReport {
    /// Overall health state
    pub state: CoreHealthState,
    /// Ready to accept requests
    pub ready: bool,
    /// Currently accepting requests
    pub accepting_requests: bool,
    /// Number of models loaded
    pub models_loaded: u32,
    /// Memory used in bytes
    pub memory_used_bytes: u64,
    /// Current queue depth
    pub queue_depth: u32,
    /// Uptime in seconds
    pub uptime_secs: u64,
}

impl Default for CoreHealthReport {
    fn default() -> Self {
        Self {
            state: CoreHealthState::Unhealthy,
            ready: false,
            accepting_requests: false,
            models_loaded: 0,
            memory_used_bytes: 0,
            queue_depth: 0,
            uptime_secs: 0,
        }
    }
}

/// Model metadata
#[repr(C)]
pub struct CoreModelMetadata {
    /// Model name (borrowed, valid until model unloaded)
    pub name: *const c_char,
    /// Model size in bytes
    pub size_bytes: u64,
    /// Model handle ID
    pub handle_id: u64,
}

impl Default for CoreModelMetadata {
    fn default() -> Self {
        Self {
            name: std::ptr::null(),
            size_bytes: 0,
            handle_id: 0,
        }
    }
}
