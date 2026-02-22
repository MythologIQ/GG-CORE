// Copyright 2024-2026 GG-CORE Contributors
// Licensed under the Apache License, Version 2.0

//! GPU Manager - Handles device detection and memory management.
//!
//! Extracted from `gpu.rs` for Section 4 compliance.

use std::sync::Arc;

use super::gpu::{GpuBackend, GpuConfig, GpuDevice, GpuError, GpuMemory};

/// GPU Manager - Handles device detection and memory management
pub struct GpuManager {
    /// Available devices
    devices: Vec<GpuDevice>,
    /// Current configuration
    config: GpuConfig,
    /// Active device
    active_device: Option<Arc<GpuDevice>>,
}

impl GpuManager {
    /// Create a new GPU manager
    pub fn new(config: GpuConfig) -> Result<Self, GpuError> {
        let mut manager = Self {
            devices: Vec::new(),
            config,
            active_device: None,
        };

        manager.detect_devices()?;
        manager.select_device()?;

        Ok(manager)
    }

    /// Detect available GPU devices
    pub fn detect_devices(&mut self) -> Result<(), GpuError> {
        self.devices.clear();
        self.devices.push(GpuDevice::cpu());

        #[cfg(feature = "cuda")]
        {
            if let Ok(cuda_devices) = self.detect_cuda_devices() {
                self.devices.extend(cuda_devices);
            }
        }

        #[cfg(all(feature = "metal", target_os = "macos"))]
        {
            if let Ok(metal_devices) = self.detect_metal_devices() {
                self.devices.extend(metal_devices);
            }
        }

        if self.devices.len() == 1 && self.config.backend != GpuBackend::Cpu {
            return Err(GpuError::NoDevicesAvailable);
        }

        Ok(())
    }

    /// Select the active device based on configuration
    pub fn select_device(&mut self) -> Result<(), GpuError> {
        let device = self
            .devices
            .iter()
            .find(|d| d.backend == self.config.backend && d.index == self.config.device_index)
            .cloned();

        match device {
            Some(d) => {
                self.active_device = Some(Arc::new(d));
                Ok(())
            }
            None => {
                if self.config.backend != GpuBackend::Cpu {
                    self.active_device = Some(Arc::new(GpuDevice::cpu()));
                    Ok(())
                } else {
                    Err(GpuError::DeviceNotFound(self.config.device_index))
                }
            }
        }
    }

    /// Get the active device
    pub fn active_device(&self) -> Option<&GpuDevice> {
        self.active_device.as_deref()
    }

    /// Get all available devices
    pub fn available_devices(&self) -> &[GpuDevice] {
        &self.devices
    }

    /// Check if GPU is available
    pub fn is_gpu_available(&self) -> bool {
        self.devices.iter().any(|d| d.backend != GpuBackend::Cpu)
    }

    /// Get available GPU backends
    pub fn available_backends(&self) -> Vec<GpuBackend> {
        self.devices
            .iter()
            .map(|d| d.backend)
            .filter(|b| *b != GpuBackend::Cpu)
            .collect()
    }

    /// Allocate GPU memory
    pub fn allocate_memory(&self, size: u64) -> Result<GpuMemory, GpuError> {
        let device = self
            .active_device
            .as_ref()
            .ok_or(GpuError::NoDevicesAvailable)?;

        if !device.has_memory(size) {
            return Err(GpuError::OutOfMemory {
                required: size,
                available: device.available_memory,
            });
        }

        Ok(GpuMemory {
            size,
            device: device.clone(),
            ptr: std::ptr::null_mut(),
        })
    }

    /// Detect CUDA devices using cudarc
    #[cfg(feature = "cuda")]
    fn detect_cuda_devices(&self) -> Result<Vec<GpuDevice>, GpuError> {
        use crate::engine::cuda::CudaBackend;

        match CudaBackend::new() {
            Ok(cuda_backend) => {
                let devices: Vec<GpuDevice> = cuda_backend
                    .devices()
                    .iter()
                    .map(|info| info.device.clone())
                    .collect();
                Ok(devices)
            }
            Err(_) => Ok(Vec::new()),
        }
    }

    /// Detect Metal devices using metal crate
    #[cfg(all(feature = "metal", target_os = "macos"))]
    fn detect_metal_devices(&self) -> Result<Vec<GpuDevice>, GpuError> {
        use crate::engine::metal::MetalBackend;

        match MetalBackend::new() {
            Ok(metal_backend) => {
                let devices: Vec<GpuDevice> = metal_backend
                    .devices()
                    .iter()
                    .map(|info| info.device.clone())
                    .collect();
                Ok(devices)
            }
            Err(_) => Ok(Vec::new()),
        }
    }
}
