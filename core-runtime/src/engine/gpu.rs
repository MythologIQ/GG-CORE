// Copyright 2024-2026 GG-CORE Contributors
// Licensed under the Apache License, Version 2.0

//! GPU Backend Support
//!
//! Provides GPU acceleration for inference using CUDA (NVIDIA) or Metal (Apple Silicon).
//! This module implements the GPU abstraction layer for GG-CORE.

use std::fmt;
use std::sync::Arc;
use thiserror::Error;

/// GPU Backend Types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuBackend {
    /// NVIDIA CUDA backend
    Cuda,
    /// Apple Metal backend (macOS only)
    Metal,
    /// CPU fallback (no GPU)
    Cpu,
}

impl Default for GpuBackend {
    fn default() -> Self {
        Self::Cpu
    }
}

impl fmt::Display for GpuBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpuBackend::Cuda => write!(f, "CUDA"),
            GpuBackend::Metal => write!(f, "Metal"),
            GpuBackend::Cpu => write!(f, "CPU"),
        }
    }
}

/// GPU Device Information
#[derive(Debug, Clone)]
pub struct GpuDevice {
    /// Backend type
    pub backend: GpuBackend,
    /// Device index (for multi-GPU systems)
    pub index: usize,
    /// Device name
    pub name: String,
    /// Total memory in bytes
    pub total_memory: u64,
    /// Available memory in bytes
    pub available_memory: u64,
    /// Compute capability (CUDA only)
    pub compute_capability: Option<(u32, u32)>,
}

impl GpuDevice {
    /// Create a CPU device
    pub fn cpu() -> Self {
        Self {
            backend: GpuBackend::Cpu,
            index: 0,
            name: "CPU".to_string(),
            total_memory: 0,
            available_memory: 0,
            compute_capability: None,
        }
    }

    /// Check if device has enough memory
    pub fn has_memory(&self, required: u64) -> bool {
        self.backend == GpuBackend::Cpu || self.available_memory >= required
    }

    /// Get memory utilization percentage
    pub fn memory_utilization(&self) -> f32 {
        if self.total_memory == 0 {
            return 0.0;
        }
        ((self.total_memory - self.available_memory) as f64 / self.total_memory as f64) as f32
    }
}

/// GPU Configuration
#[derive(Debug, Clone)]
pub struct GpuConfig {
    /// Preferred backend
    pub backend: GpuBackend,
    /// Device index to use
    pub device_index: usize,
    /// Memory fraction to use (0.0 - 1.0)
    pub memory_fraction: f32,
    /// Enable flash attention
    pub flash_attention: bool,
    /// Number of GPU layers to offload
    pub gpu_layers: u32,
    /// Split model across multiple GPUs
    pub multi_gpu: bool,
    /// Main GPU for multi-GPU setups
    pub main_gpu: usize,
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            backend: GpuBackend::Cpu,
            device_index: 0,
            memory_fraction: 0.9,
            flash_attention: true,
            gpu_layers: 0,
            multi_gpu: false,
            main_gpu: 0,
        }
    }
}

impl GpuConfig {
    /// Create a CPU-only configuration
    pub fn cpu() -> Self {
        Self {
            backend: GpuBackend::Cpu,
            gpu_layers: 0,
            ..Default::default()
        }
    }

    /// Create a CUDA configuration with all layers on GPU
    pub fn cuda_all_layers() -> Self {
        Self {
            backend: GpuBackend::Cuda,
            gpu_layers: u32::MAX,
            ..Default::default()
        }
    }

    /// Create a Metal configuration (macOS)
    #[cfg(target_os = "macos")]
    pub fn metal() -> Self {
        Self {
            backend: GpuBackend::Metal,
            gpu_layers: u32::MAX,
            ..Default::default()
        }
    }
}

/// GPU Error Types
#[derive(Debug, Error)]
pub enum GpuError {
    #[error("No GPU devices available")]
    NoDevicesAvailable,

    #[error("CUDA not available: {0}")]
    CudaNotAvailable(String),

    #[error("Metal not available: {0}")]
    MetalNotAvailable(String),

    #[error("Device not found: {0}")]
    DeviceNotFound(usize),

    #[error("Out of GPU memory: required {required} bytes, available {available} bytes")]
    OutOfMemory { required: u64, available: u64 },

    #[error("GPU operation failed: {0}")]
    OperationFailed(String),

    #[error("Memory allocation failed: {0}")]
    AllocationFailed(String),

    #[error("Kernel launch failed: {0}")]
    KernelLaunchFailed(String),
}

/// GPU Memory Handle
pub struct GpuMemory {
    /// Size in bytes
    pub size: u64,
    /// Device the memory is allocated on
    pub device: Arc<GpuDevice>,
    /// Pointer to GPU memory (opaque)
    pub ptr: *mut std::ffi::c_void,
}

impl Drop for GpuMemory {
    fn drop(&mut self) {
        // Actual deallocation would happen here
        // Safety: ptr is valid and points to GPU memory
    }
}

// Safety: GpuMemory can be sent between threads
unsafe impl Send for GpuMemory {}
unsafe impl Sync for GpuMemory {}

#[cfg(test)]
#[path = "gpu_tests.rs"]
mod tests;
