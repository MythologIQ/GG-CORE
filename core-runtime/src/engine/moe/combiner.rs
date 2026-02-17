// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Expert output combination for MoE layers.

use super::config::MoeError;
use super::router::RoutingDecision;

/// Output from a single expert computation.
#[derive(Debug, Clone)]
pub struct ExpertOutput {
    /// Expert index.
    pub expert_idx: usize,
    /// Token indices this expert processed.
    pub token_indices: Vec<usize>,
    /// Output hidden states [num_tokens, hidden_dim].
    pub hidden_states: Vec<f32>,
    /// Hidden dimension size.
    pub hidden_dim: usize,
}

impl ExpertOutput {
    /// Get output for a specific token.
    pub fn get_token_output(&self, local_idx: usize) -> Option<&[f32]> {
        if local_idx >= self.token_indices.len() {
            return None;
        }
        let offset = local_idx * self.hidden_dim;
        Some(&self.hidden_states[offset..offset + self.hidden_dim])
    }
}

/// Combines outputs from multiple experts using routing weights.
pub struct ExpertCombiner {
    hidden_dim: usize,
}

impl ExpertCombiner {
    /// Create a new combiner for the given hidden dimension.
    pub fn new(hidden_dim: usize) -> Self {
        Self { hidden_dim }
    }

    /// Combine expert outputs using routing decision weights.
    pub fn combine(
        &self,
        expert_outputs: &[ExpertOutput],
        routing: &RoutingDecision,
        batch_size: usize,
    ) -> Result<Vec<f32>, MoeError> {
        let mut combined = vec![0.0f32; batch_size * self.hidden_dim];

        // Build a map of expert_idx -> ExpertOutput for lookup
        let expert_map: std::collections::HashMap<usize, &ExpertOutput> = expert_outputs
            .iter()
            .map(|o| (o.expert_idx, o))
            .collect();

        for (token_idx, (experts, weights)) in routing
            .expert_indices
            .iter()
            .zip(routing.routing_weights.iter())
            .enumerate()
        {
            let output_offset = token_idx * self.hidden_dim;

            for (&expert_idx, &weight) in experts.iter().zip(weights.iter()) {
                if let Some(expert_output) = expert_map.get(&expert_idx) {
                    // Find this token in the expert's processed tokens
                    if let Some(local_idx) = expert_output
                        .token_indices
                        .iter()
                        .position(|&t| t == token_idx)
                    {
                        if let Some(token_output) = expert_output.get_token_output(local_idx) {
                            for (i, &val) in token_output.iter().enumerate() {
                                combined[output_offset + i] += weight * val;
                            }
                        }
                    }
                }
            }
        }

        Ok(combined)
    }

    /// Compute auxiliary load balancing loss.
    pub fn compute_aux_loss(routing: &RoutingDecision, num_experts: usize) -> f32 {
        let Some(ref load) = routing.load_per_expert else {
            return 0.0;
        };

        let total: u32 = load.iter().sum();
        if total == 0 {
            return 0.0;
        }

        let expected = total as f32 / num_experts as f32;
        let variance: f32 = load
            .iter()
            .map(|&l| (l as f32 - expected).powi(2))
            .sum::<f32>()
            / num_experts as f32;

        variance / (expected * expected + 1e-8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combine_single_expert() {
        let combiner = ExpertCombiner::new(4);

        let expert_output = ExpertOutput {
            expert_idx: 0,
            token_indices: vec![0],
            hidden_states: vec![1.0, 2.0, 3.0, 4.0],
            hidden_dim: 4,
        };

        let routing = RoutingDecision {
            expert_indices: vec![vec![0]],
            routing_weights: vec![vec![1.0]],
            load_per_expert: None,
        };

        let combined = combiner.combine(&[expert_output], &routing, 1).unwrap();
        assert_eq!(combined, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_weighted_combination() {
        let combiner = ExpertCombiner::new(2);

        let expert0 = ExpertOutput {
            expert_idx: 0,
            token_indices: vec![0],
            hidden_states: vec![1.0, 0.0],
            hidden_dim: 2,
        };
        let expert1 = ExpertOutput {
            expert_idx: 1,
            token_indices: vec![0],
            hidden_states: vec![0.0, 1.0],
            hidden_dim: 2,
        };

        let routing = RoutingDecision {
            expert_indices: vec![vec![0, 1]],
            routing_weights: vec![vec![0.7, 0.3]],
            load_per_expert: None,
        };

        let combined = combiner.combine(&[expert0, expert1], &routing, 1).unwrap();
        assert!((combined[0] - 0.7).abs() < 1e-5);
        assert!((combined[1] - 0.3).abs() < 1e-5);
    }
}
