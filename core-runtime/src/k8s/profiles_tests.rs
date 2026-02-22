// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Tests for K8s deployment profiles.

use super::*;

#[test]
fn test_cpu_only_profile_no_gpu() {
    let spec = DeploymentProfile::CpuOnly.to_spec();
    assert_eq!(spec.gpu_count, 0);
    assert!(spec.tolerations.is_empty());
    assert!(spec.affinity.is_none());
    assert!(spec.validate().is_ok());
}

#[test]
fn test_single_gpu_has_nvidia_toleration() {
    let spec = DeploymentProfile::SingleGpu.to_spec();
    assert_eq!(spec.gpu_count, 1);
    assert_eq!(spec.tolerations.len(), 1);
    assert_eq!(spec.tolerations[0].key, "nvidia.com/gpu");
    assert!(spec.affinity.is_some());
    assert!(spec.validate().is_ok());
}

#[test]
fn test_multi_gpu_correct_resources() {
    let spec = DeploymentProfile::MultiGpu { device_count: 4 }.to_spec();
    assert_eq!(spec.gpu_count, 4);
    assert_eq!(spec.tolerations.len(), 1);
    assert_eq!(spec.tolerations[0].key, "nvidia.com/gpu");
    assert_eq!(spec.memory_limit, "32Gi");
    assert_eq!(spec.rollout, RolloutStrategy::Recreate);
    assert!(spec.validate().is_ok());
}

#[test]
fn test_high_memory_profile() {
    let spec = DeploymentProfile::HighMemory.to_spec();
    assert_eq!(spec.gpu_count, 0);
    assert_eq!(spec.memory_request, "32Gi");
    assert_eq!(spec.memory_limit, "64Gi");
    assert!(spec.tolerations.is_empty());
    assert!(spec.validate().is_ok());
}

#[test]
fn test_multi_gpu_zero_devices_rejected() {
    let spec = DeploymentProfile::MultiGpu { device_count: 0 }.to_spec();
    assert!(spec.validate().is_err());
    let err = spec.validate().unwrap_err();
    assert!(matches!(err, ProfileError::InvalidDeviceCount(_)));
}

#[test]
fn test_rolling_update_strategy_values() {
    let spec = DeploymentProfile::SingleGpu.to_spec();
    assert_eq!(
        spec.rollout,
        RolloutStrategy::RollingUpdate {
            max_unavailable: 0,
            max_surge: 1,
        }
    );
}
