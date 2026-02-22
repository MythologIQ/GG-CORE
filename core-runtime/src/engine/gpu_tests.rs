//! Tests for GPU backend support.
//!
//! Extracted from `gpu.rs` for Section 4 compliance.

use super::*;
use crate::engine::gpu_manager::GpuManager;

#[test]
fn test_gpu_backend_display() {
    assert_eq!(format!("{}", GpuBackend::Cuda), "CUDA");
    assert_eq!(format!("{}", GpuBackend::Metal), "Metal");
    assert_eq!(format!("{}", GpuBackend::Cpu), "CPU");
}

#[test]
fn test_gpu_device_cpu() {
    let device = GpuDevice::cpu();
    assert_eq!(device.backend, GpuBackend::Cpu);
    assert!(device.has_memory(0));
    assert_eq!(device.memory_utilization(), 0.0);
}

#[test]
fn test_gpu_config_default() {
    let config = GpuConfig::default();
    assert_eq!(config.backend, GpuBackend::Cpu);
    assert_eq!(config.gpu_layers, 0);
}

#[test]
fn test_gpu_config_cpu() {
    let config = GpuConfig::cpu();
    assert_eq!(config.backend, GpuBackend::Cpu);
    assert_eq!(config.gpu_layers, 0);
}

#[test]
fn test_gpu_config_cuda_all_layers() {
    let config = GpuConfig::cuda_all_layers();
    assert_eq!(config.backend, GpuBackend::Cuda);
    assert_eq!(config.gpu_layers, u32::MAX);
}

#[test]
fn test_gpu_manager_cpu_only() {
    let config = GpuConfig::cpu();
    let manager = GpuManager::new(config).unwrap();

    assert!(manager.active_device().is_some());
    assert_eq!(manager.active_device().unwrap().backend, GpuBackend::Cpu);
}
