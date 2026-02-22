//! Tests for multi-GPU support.
//!
//! Extracted from `multi_gpu.rs` for Section 4 compliance.

use std::sync::Arc;
use super::*;
use crate::engine::multi_gpu_partition::CrossGpuCommunication;

fn create_test_devices() -> Vec<Arc<GpuDevice>> {
    vec![
        Arc::new(GpuDevice {
            backend: GpuBackend::Cuda,
            index: 0,
            name: "GPU 0".to_string(),
            total_memory: 24_000_000_000,
            available_memory: 20_000_000_000,
            compute_capability: Some((8, 6)),
        }),
        Arc::new(GpuDevice {
            backend: GpuBackend::Cuda,
            index: 1,
            name: "GPU 1".to_string(),
            total_memory: 24_000_000_000,
            available_memory: 22_000_000_000,
            compute_capability: Some((8, 6)),
        }),
        Arc::new(GpuDevice {
            backend: GpuBackend::Cuda,
            index: 2,
            name: "GPU 2".to_string(),
            total_memory: 24_000_000_000,
            available_memory: 18_000_000_000,
            compute_capability: Some((8, 6)),
        }),
    ]
}

#[test]
fn test_multi_gpu_manager_creation() {
    let devices = create_test_devices();
    let config = MultiGpuConfig::default();

    let manager = MultiGpuManager::new(devices, config);
    assert!(manager.is_ok());

    let manager = manager.unwrap();
    assert_eq!(manager.num_gpus(), 3);
}

#[test]
fn test_multi_gpu_insufficient_gpus() {
    let devices = vec![Arc::new(GpuDevice::cpu())];
    let config = MultiGpuConfig::default();

    let result = MultiGpuManager::new(devices, config);
    assert!(matches!(
        result,
        Err(MultiGpuError::InsufficientGpus { .. })
    ));
}

#[test]
fn test_partition_by_layers() {
    let devices = create_test_devices();
    let config = MultiGpuConfig {
        strategy: MultiGpuStrategy::LayerParallelism,
        ..Default::default()
    };

    let mut manager = MultiGpuManager::new(devices, config).unwrap();
    let partitions = manager.partition_model(96, 40_000_000_000).unwrap();

    assert_eq!(partitions.len(), 3);

    let total_layers: usize = partitions.iter().map(|p| p.layers.len()).sum();
    assert_eq!(total_layers, 96);
}

#[test]
fn test_memory_variance() {
    let devices = create_test_devices();
    let config = MultiGpuConfig::default();

    let manager = MultiGpuManager::new(devices, config).unwrap();
    let variance = manager.compute_memory_variance();

    assert!(variance < 0.2);
}

#[test]
fn test_total_memory() {
    let devices = create_test_devices();
    let config = MultiGpuConfig::default();

    let manager = MultiGpuManager::new(devices, config).unwrap();
    let total = manager.total_memory();

    assert_eq!(total, 72_000_000_000);
}

#[test]
fn test_cross_gpu_communication() {
    let comm = CrossGpuCommunication::new(0, 1, true);
    assert!(comm.can_direct_transfer());
    assert_eq!(comm.transfer_method(), "P2P Direct");

    let comm = CrossGpuCommunication::new(0, 1, false);
    assert!(!comm.can_direct_transfer());
    assert_eq!(comm.transfer_method(), "Host Staging");
}

#[test]
fn test_multi_gpu_strategy_default() {
    assert_eq!(MultiGpuStrategy::default(), MultiGpuStrategy::Auto);
}
