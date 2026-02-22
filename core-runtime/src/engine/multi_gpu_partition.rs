// Copyright 2024-2026 GG-CORE Contributors
// Licensed under the Apache License, Version 2.0

//! Multi-GPU partitioning strategies.
//!
//! Extracted from `multi_gpu.rs` for Section 4 compliance (files <= 250 lines).

use std::sync::Arc;

use super::gpu::{GpuBackend, GpuDevice};
use super::multi_gpu::{GpuPartition, MultiGpuConfig, MultiGpuError, MultiGpuStrategy};

/// Partition a model by layers across GPUs.
pub fn partition_by_layers(
    devices: &[Arc<GpuDevice>],
    num_layers: usize,
    model_size: u64,
) -> Result<Vec<GpuPartition>, MultiGpuError> {
    let num_gpus = devices.len();
    let layers_per_gpu = num_layers / num_gpus;
    let extra_layers = num_layers % num_gpus;

    let mut partitions = Vec::with_capacity(num_gpus);
    let mut current_layer = 0;

    let total_available: u64 = devices.iter().map(|d| d.available_memory).sum();

    for (i, device) in devices.iter().enumerate() {
        let gpu_layers = layers_per_gpu + if i < extra_layers { 1 } else { 0 };
        let layer_start = current_layer;
        let layer_end = current_layer + gpu_layers;

        let memory_fraction = device.available_memory as f64 / total_available as f64;
        let memory_budget = (model_size as f64 * memory_fraction) as u64;
        let parameter_fraction = gpu_layers as f32 / num_layers as f32;

        partitions.push(GpuPartition {
            gpu_index: device.index,
            layers: layer_start..layer_end,
            memory_budget,
            parameter_fraction,
        });

        current_layer = layer_end;
    }

    Ok(partitions)
}

/// Partition model by tensors (tensor parallelism).
pub fn partition_by_tensors(
    devices: &[Arc<GpuDevice>],
    model_size: u64,
) -> Result<Vec<GpuPartition>, MultiGpuError> {
    let num_gpus = devices.len();
    let size_per_gpu = model_size / num_gpus as u64;

    let partitions: Vec<GpuPartition> = devices
        .iter()
        .map(|device| GpuPartition {
            gpu_index: device.index,
            layers: 0..usize::MAX,
            memory_budget: size_per_gpu,
            parameter_fraction: 1.0 / num_gpus as f32,
        })
        .collect();

    Ok(partitions)
}

/// Partition model by pipeline stages.
pub fn partition_by_pipeline(
    devices: &[Arc<GpuDevice>],
    num_layers: usize,
    model_size: u64,
) -> Result<Vec<GpuPartition>, MultiGpuError> {
    partition_by_layers(devices, num_layers, model_size)
}

/// Partition model for MoE expert parallelism.
pub fn partition_by_experts(
    devices: &[Arc<GpuDevice>],
    model_size: u64,
) -> Result<Vec<GpuPartition>, MultiGpuError> {
    partition_by_tensors(devices, model_size)
}

/// Determine optimal strategy based on model and hardware.
pub fn determine_strategy(
    config: &MultiGpuConfig,
    devices: &[Arc<GpuDevice>],
    num_layers: usize,
) -> MultiGpuStrategy {
    if config.strategy != MultiGpuStrategy::Auto {
        return config.strategy;
    }

    let num_gpus = devices.len();
    let memory_variance = compute_memory_variance(devices);
    let is_homogeneous = memory_variance < 0.2;
    let has_p2p = config.enable_p2p && check_p2p_support(devices);

    if num_layers > 48 && num_gpus >= 4 && has_p2p {
        return MultiGpuStrategy::PipelineParallelism;
    }

    if is_homogeneous && has_p2p && num_gpus <= 8 {
        return MultiGpuStrategy::TensorParallelism;
    }

    MultiGpuStrategy::LayerParallelism
}

/// Compute variance in GPU memory sizes.
pub fn compute_memory_variance(devices: &[Arc<GpuDevice>]) -> f32 {
    if devices.is_empty() {
        return 0.0;
    }

    let memories: Vec<u64> = devices.iter().map(|d| d.total_memory).collect();
    let mean = memories.iter().sum::<u64>() as f32 / memories.len() as f32;

    if mean == 0.0 {
        return 0.0;
    }

    let variance: f32 = memories
        .iter()
        .map(|&m| {
            let diff = m as f32 - mean;
            diff * diff
        })
        .sum::<f32>()
        / memories.len() as f32;

    (variance.sqrt() / mean).abs()
}

/// Check if peer-to-peer communication is supported.
fn check_p2p_support(devices: &[Arc<GpuDevice>]) -> bool {
    let backends: std::collections::HashSet<GpuBackend> =
        devices.iter().map(|d| d.backend).collect();

    if backends.len() != 1 {
        return false;
    }

    #[cfg(feature = "cuda")]
    {
        if backends.contains(&GpuBackend::Cuda) {
            return true;
        }
    }

    false
}

/// Cross-GPU Communication Manager
pub struct CrossGpuCommunication {
    /// Source GPU
    _source: usize,
    /// Destination GPU
    _destination: usize,
    /// Whether P2P is enabled
    p2p_enabled: bool,
}

impl CrossGpuCommunication {
    /// Create a new cross-GPU communication channel
    pub fn new(source: usize, destination: usize, p2p_enabled: bool) -> Self {
        Self {
            _source: source,
            _destination: destination,
            p2p_enabled,
        }
    }

    /// Check if direct P2P transfer is possible
    pub fn can_direct_transfer(&self) -> bool {
        self.p2p_enabled
    }

    /// Get transfer method description
    pub fn transfer_method(&self) -> &'static str {
        if self.p2p_enabled {
            "P2P Direct"
        } else {
            "Host Staging"
        }
    }
}
