// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Kubernetes deployment profiles for different hardware configurations.

use serde::{Deserialize, Serialize};

/// Hardware-specific deployment profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeploymentProfile {
    CpuOnly,
    SingleGpu,
    MultiGpu { device_count: u32 },
    HighMemory,
}

/// Generated resource specification from a profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSpec {
    pub profile: DeploymentProfile,
    pub cpu_request: String,
    pub cpu_limit: String,
    pub memory_request: String,
    pub memory_limit: String,
    pub gpu_count: u32,
    pub node_selector: Vec<(String, String)>,
    pub tolerations: Vec<Toleration>,
    pub affinity: Option<NodeAffinity>,
    pub rollout: RolloutStrategy,
}

/// Kubernetes toleration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Toleration {
    pub key: String,
    pub operator: String,
    pub value: String,
    pub effect: String,
}

/// Node affinity configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeAffinity {
    pub required: Vec<NodeSelector>,
    pub preferred: Vec<PreferredNodeSelector>,
}

/// Node selector requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSelector {
    pub key: String,
    pub operator: String,
    pub values: Vec<String>,
}

/// Weighted preferred node selector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferredNodeSelector {
    pub weight: i32,
    pub selector: NodeSelector,
}

/// Rollout strategy for deployments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RolloutStrategy {
    RollingUpdate { max_unavailable: u32, max_surge: u32 },
    Recreate,
}

/// Validation error for profile specs.
#[derive(Debug, Clone, PartialEq)]
pub enum ProfileError {
    InvalidDeviceCount(String),
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDeviceCount(msg) => write!(f, "Invalid device count: {}", msg),
        }
    }
}

impl std::error::Error for ProfileError {}

fn nvidia_toleration() -> Toleration {
    Toleration {
        key: "nvidia.com/gpu".to_string(),
        operator: "Exists".to_string(),
        value: String::new(),
        effect: "NoSchedule".to_string(),
    }
}

fn gpu_node_affinity() -> NodeAffinity {
    NodeAffinity {
        required: vec![NodeSelector {
            key: "nvidia.com/gpu.present".to_string(),
            operator: "In".to_string(),
            values: vec!["true".to_string()],
        }],
        preferred: vec![],
    }
}

impl DeploymentProfile {
    /// Generate a `ProfileSpec` with appropriate defaults.
    pub fn to_spec(&self) -> ProfileSpec {
        match self {
            Self::CpuOnly => self.cpu_only_spec(),
            Self::SingleGpu => self.single_gpu_spec(),
            Self::MultiGpu { device_count } => self.multi_gpu_spec(*device_count),
            Self::HighMemory => self.high_memory_spec(),
        }
    }

    fn cpu_only_spec(&self) -> ProfileSpec {
        ProfileSpec {
            profile: self.clone(),
            cpu_request: "2".to_string(),
            cpu_limit: "4".to_string(),
            memory_request: "4Gi".to_string(),
            memory_limit: "8Gi".to_string(),
            gpu_count: 0,
            node_selector: vec![],
            tolerations: vec![],
            affinity: None,
            rollout: RolloutStrategy::RollingUpdate {
                max_unavailable: 1,
                max_surge: 1,
            },
        }
    }

    fn single_gpu_spec(&self) -> ProfileSpec {
        ProfileSpec {
            profile: self.clone(),
            cpu_request: "4".to_string(),
            cpu_limit: "8".to_string(),
            memory_request: "8Gi".to_string(),
            memory_limit: "16Gi".to_string(),
            gpu_count: 1,
            node_selector: vec![],
            tolerations: vec![nvidia_toleration()],
            affinity: Some(gpu_node_affinity()),
            rollout: RolloutStrategy::RollingUpdate {
                max_unavailable: 0,
                max_surge: 1,
            },
        }
    }

    fn multi_gpu_spec(&self, device_count: u32) -> ProfileSpec {
        ProfileSpec {
            profile: self.clone(),
            cpu_request: "8".to_string(),
            cpu_limit: "16".to_string(),
            memory_request: "16Gi".to_string(),
            memory_limit: "32Gi".to_string(),
            gpu_count: device_count,
            node_selector: vec![],
            tolerations: vec![nvidia_toleration()],
            affinity: Some(gpu_node_affinity()),
            rollout: RolloutStrategy::Recreate,
        }
    }

    fn high_memory_spec(&self) -> ProfileSpec {
        ProfileSpec {
            profile: self.clone(),
            cpu_request: "4".to_string(),
            cpu_limit: "8".to_string(),
            memory_request: "32Gi".to_string(),
            memory_limit: "64Gi".to_string(),
            gpu_count: 0,
            node_selector: vec![],
            tolerations: vec![],
            affinity: None,
            rollout: RolloutStrategy::RollingUpdate {
                max_unavailable: 1,
                max_surge: 1,
            },
        }
    }
}

impl ProfileSpec {
    /// Validate the profile spec.
    ///
    /// # Errors
    /// Returns `ProfileError` if MultiGpu has `device_count` of 0.
    pub fn validate(&self) -> Result<(), ProfileError> {
        if let DeploymentProfile::MultiGpu { device_count } = &self.profile {
            if *device_count == 0 {
                return Err(ProfileError::InvalidDeviceCount(
                    "MultiGpu device_count must be > 0".to_string(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod tests;
