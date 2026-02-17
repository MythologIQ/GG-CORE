// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Parallel MoE expert execution across devices.

use std::collections::HashMap;

use super::combiner::{ExpertCombiner, ExpertOutput};
use super::config::{MoeConfig, MoeError};
use super::router::RoutingDecision;

/// Device assignment for an expert.
#[derive(Debug, Clone)]
pub struct ExpertDeviceAssignment {
    /// Expert index.
    pub expert_idx: usize,
    /// Device ID (GPU index or CPU=-1).
    pub device_id: i32,
    /// Memory offset on device.
    pub memory_offset: usize,
}

/// Trait for expert network implementations.
pub trait Expert: Send + Sync {
    /// Process tokens through the expert FFN.
    fn forward(&self, hidden_states: &[f32], hidden_dim: usize) -> Result<Vec<f32>, MoeError>;

    /// Get expert index.
    fn expert_idx(&self) -> usize;
}

/// Executor for parallel MoE expert computation.
pub struct MoeExecutor {
    config: MoeConfig,
    device_assignments: Vec<ExpertDeviceAssignment>,
    combiner: ExpertCombiner,
}

impl MoeExecutor {
    /// Create a new MoE executor with device assignments.
    pub fn new(config: MoeConfig, assignments: Vec<ExpertDeviceAssignment>) -> Self {
        let combiner = ExpertCombiner::new(config.hidden_dim);
        Self {
            config,
            device_assignments: assignments,
            combiner,
        }
    }

    /// Create executor with automatic CPU assignment.
    pub fn cpu_only(config: MoeConfig) -> Self {
        let assignments: Vec<ExpertDeviceAssignment> = (0..config.num_experts)
            .map(|i| ExpertDeviceAssignment {
                expert_idx: i,
                device_id: -1, // CPU
                memory_offset: 0,
            })
            .collect();
        Self::new(config, assignments)
    }

    /// Get device assignment for an expert.
    pub fn get_assignment(&self, expert_idx: usize) -> Option<&ExpertDeviceAssignment> {
        self.device_assignments
            .iter()
            .find(|a| a.expert_idx == expert_idx)
    }

    /// Group tokens by expert for batch processing.
    pub fn group_by_expert(&self, routing: &RoutingDecision) -> HashMap<usize, Vec<(usize, f32)>> {
        let mut groups: HashMap<usize, Vec<(usize, f32)>> = HashMap::new();

        for (token_idx, (experts, weights)) in routing
            .expert_indices
            .iter()
            .zip(routing.routing_weights.iter())
            .enumerate()
        {
            for (&expert_idx, &weight) in experts.iter().zip(weights.iter()) {
                groups
                    .entry(expert_idx)
                    .or_default()
                    .push((token_idx, weight));
            }
        }

        groups
    }

    /// Execute experts sequentially (CPU fallback).
    pub fn execute_sequential<E: Expert>(
        &self,
        experts: &[E],
        routing: &RoutingDecision,
        hidden_states: &[f32],
    ) -> Result<Vec<f32>, MoeError> {
        let batch_size = routing.expert_indices.len();
        let groups = self.group_by_expert(routing);
        let mut outputs = Vec::with_capacity(groups.len());

        for (expert_idx, token_infos) in &groups {
            let expert = experts
                .iter()
                .find(|e| e.expert_idx() == *expert_idx)
                .ok_or_else(|| {
                    MoeError::ExecutionFailed(format!("Expert {expert_idx} not found"))
                })?;

            let token_indices: Vec<usize> = token_infos.iter().map(|(idx, _)| *idx).collect();

            // Gather input hidden states for this expert
            let input = self.gather_hidden_states(hidden_states, &token_indices);
            let output_states = expert.forward(&input, self.config.hidden_dim)?;

            outputs.push(ExpertOutput {
                expert_idx: *expert_idx,
                token_indices,
                hidden_states: output_states,
                hidden_dim: self.config.hidden_dim,
            });
        }

        self.combiner.combine(&outputs, routing, batch_size)
    }

    /// Gather hidden states for specific tokens.
    fn gather_hidden_states(&self, hidden_states: &[f32], token_indices: &[usize]) -> Vec<f32> {
        let mut gathered = Vec::with_capacity(token_indices.len() * self.config.hidden_dim);
        for &idx in token_indices {
            let offset = idx * self.config.hidden_dim;
            gathered.extend_from_slice(&hidden_states[offset..offset + self.config.hidden_dim]);
        }
        gathered
    }

    /// Compute load balancing statistics.
    pub fn load_statistics(&self, routing: &RoutingDecision) -> LoadStats {
        let load = routing
            .load_per_expert
            .as_ref()
            .map(|l| l.clone())
            .unwrap_or_else(|| vec![0; self.config.num_experts]);

        let total: u32 = load.iter().sum();
        let max_load = *load.iter().max().unwrap_or(&0);
        let min_load = *load.iter().min().unwrap_or(&0);

        LoadStats {
            total_tokens: total,
            max_load_per_expert: max_load,
            min_load_per_expert: min_load,
            load_imbalance: if total > 0 {
                (max_load - min_load) as f32 / (total as f32 / self.config.num_experts as f32)
            } else {
                0.0
            },
        }
    }
}

/// Load balancing statistics.
#[derive(Debug, Clone)]
pub struct LoadStats {
    pub total_tokens: u32,
    pub max_load_per_expert: u32,
    pub min_load_per_expert: u32,
    pub load_imbalance: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    struct MockExpert(usize);

    impl Expert for MockExpert {
        fn forward(&self, hidden_states: &[f32], _hidden_dim: usize) -> Result<Vec<f32>, MoeError> {
            // Just return scaled input
            Ok(hidden_states.iter().map(|x| x * 2.0).collect())
        }

        fn expert_idx(&self) -> usize {
            self.0
        }
    }

    #[test]
    fn test_group_by_expert() {
        let config = MoeConfig {
            num_experts: 3,
            top_k: 2,
            hidden_dim: 4,
            ..Default::default()
        };
        let executor = MoeExecutor::cpu_only(config);

        let routing = RoutingDecision {
            expert_indices: vec![vec![0, 1], vec![1, 2]],
            routing_weights: vec![vec![0.6, 0.4], vec![0.7, 0.3]],
            load_per_expert: None,
        };

        let groups = executor.group_by_expert(&routing);
        assert_eq!(groups.get(&0).map(|v| v.len()), Some(1)); // token 0
        assert_eq!(groups.get(&1).map(|v| v.len()), Some(2)); // tokens 0,1
        assert_eq!(groups.get(&2).map(|v| v.len()), Some(1)); // token 1
    }
}
