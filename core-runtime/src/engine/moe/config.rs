// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! MoE configuration and error types.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error types for MoE operations.
#[derive(Debug, Error)]
pub enum MoeError {
    #[error("Invalid expert count: {0}")]
    InvalidExpertCount(usize),

    #[error("Invalid top-k value: {0} (must be <= num_experts)")]
    InvalidTopK(usize),

    #[error("Routing failed: {0}")]
    RoutingFailed(String),

    #[error("Expert execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Expert {0} is at capacity")]
    CapacityExceeded(usize),

    #[error("Dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
}

/// Configuration for Mixture of Experts layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoeConfig {
    /// Number of expert networks.
    pub num_experts: usize,

    /// Number of experts to activate per token (top-k).
    pub top_k: usize,

    /// Capacity factor for load balancing (1.0 = exact capacity).
    pub capacity_factor: f32,

    /// Temperature for router softmax (lower = sharper selection).
    pub router_temperature: f32,

    /// Hidden dimension size.
    pub hidden_dim: usize,

    /// Intermediate dimension for FFN experts.
    pub intermediate_dim: usize,

    /// Use auxiliary load balancing loss.
    pub use_aux_loss: bool,

    /// Coefficient for load balancing loss.
    pub aux_loss_coef: f32,
}

impl Default for MoeConfig {
    fn default() -> Self {
        Self {
            num_experts: 8,
            top_k: 2,
            capacity_factor: 1.25,
            router_temperature: 1.0,
            hidden_dim: 4096,
            intermediate_dim: 14336,
            use_aux_loss: true,
            aux_loss_coef: 0.01,
        }
    }
}

impl MoeConfig {
    /// Validate configuration parameters.
    pub fn validate(&self) -> Result<(), MoeError> {
        if self.num_experts == 0 {
            return Err(MoeError::InvalidExpertCount(0));
        }
        if self.top_k == 0 || self.top_k > self.num_experts {
            return Err(MoeError::InvalidTopK(self.top_k));
        }
        Ok(())
    }

    /// Create config for Mixtral-style models (8 experts, top-2).
    pub fn mixtral() -> Self {
        Self {
            num_experts: 8,
            top_k: 2,
            capacity_factor: 1.25,
            router_temperature: 1.0,
            hidden_dim: 4096,
            intermediate_dim: 14336,
            use_aux_loss: true,
            aux_loss_coef: 0.01,
        }
    }

    /// Create config for DeepSeek-style models (64 experts, top-6).
    pub fn deepseek() -> Self {
        Self {
            num_experts: 64,
            top_k: 6,
            capacity_factor: 1.5,
            router_temperature: 0.7,
            hidden_dim: 5120,
            intermediate_dim: 12288,
            use_aux_loss: true,
            aux_loss_coef: 0.001,
        }
    }
}
