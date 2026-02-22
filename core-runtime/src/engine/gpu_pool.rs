// Copyright 2024-2026 GG-CORE Contributors
// Licensed under the Apache License, Version 2.0

//! GPU Memory Pool for efficient allocation.
//!
//! Extracted from `gpu.rs` for Section 4 compliance (files <= 250 lines).

use std::sync::Arc;

use super::gpu::{GpuDevice, GpuError, GpuMemory};

/// GPU Memory Pool for efficient allocation
pub struct GpuMemoryPool {
    /// Device for this pool
    device: Arc<GpuDevice>,
    /// Allocated blocks
    blocks: Vec<GpuMemory>,
    /// Total allocated size
    total_allocated: u64,
    /// Maximum pool size
    max_size: u64,
}

impl GpuMemoryPool {
    /// Create a new memory pool
    pub fn new(device: Arc<GpuDevice>, max_size: u64) -> Self {
        Self {
            device,
            blocks: Vec::new(),
            total_allocated: 0,
            max_size,
        }
    }

    /// Allocate from pool
    pub fn allocate(&mut self, size: u64) -> Result<&GpuMemory, GpuError> {
        if self.total_allocated + size > self.max_size {
            return Err(GpuError::OutOfMemory {
                required: size,
                available: self.max_size - self.total_allocated,
            });
        }

        let memory = GpuMemory {
            size,
            device: self.device.clone(),
            ptr: std::ptr::null_mut(),
        };

        self.blocks.push(memory);
        self.total_allocated += size;

        Ok(self.blocks.last().unwrap())
    }

    /// Get pool utilization
    pub fn utilization(&self) -> f32 {
        if self.max_size == 0 {
            return 0.0;
        }
        self.total_allocated as f32 / self.max_size as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_memory_pool() {
        let device = Arc::new(GpuDevice::cpu());
        let mut pool = GpuMemoryPool::new(device, 1024);

        let mem = pool.allocate(512).unwrap();
        assert_eq!(mem.size, 512);
        assert_eq!(pool.utilization(), 0.5);
    }

    #[test]
    fn test_gpu_memory_pool_out_of_memory() {
        let device = Arc::new(GpuDevice::cpu());
        let mut pool = GpuMemoryPool::new(device, 1024);

        pool.allocate(512).unwrap();
        let result = pool.allocate(1024);

        assert!(matches!(result, Err(GpuError::OutOfMemory { .. })));
    }
}
