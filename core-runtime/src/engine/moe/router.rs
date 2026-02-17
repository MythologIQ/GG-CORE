// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! MoE routing/gating network for expert selection.

use super::config::{MoeConfig, MoeError};

/// Routing decision containing expert indices and weights.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// Expert indices for each token [batch_size, top_k].
    pub expert_indices: Vec<Vec<usize>>,
    /// Routing weights for each selected expert [batch_size, top_k].
    pub routing_weights: Vec<Vec<f32>>,
    /// Optional load balancing statistics.
    pub load_per_expert: Option<Vec<u32>>,
}

/// Trait for MoE routing implementations.
pub trait MoeRouter: Send + Sync {
    /// Route hidden states to experts, returning top-k selections per token.
    fn route(
        &self,
        hidden_states: &[f32],
        batch_size: usize,
        config: &MoeConfig,
    ) -> Result<RoutingDecision, MoeError>;
}

/// Linear router with learned gating weights.
pub struct LinearRouter {
    /// Gating weights [hidden_dim, num_experts].
    weights: Vec<f32>,
    hidden_dim: usize,
    num_experts: usize,
}

impl LinearRouter {
    /// Create a new linear router with given weights.
    pub fn new(weights: Vec<f32>, hidden_dim: usize, num_experts: usize) -> Result<Self, MoeError> {
        let expected = hidden_dim * num_experts;
        if weights.len() != expected {
            return Err(MoeError::DimensionMismatch {
                expected,
                actual: weights.len(),
            });
        }
        Ok(Self {
            weights,
            hidden_dim,
            num_experts,
        })
    }

    /// Compute router logits for a single token.
    fn compute_logits(&self, hidden: &[f32]) -> Vec<f32> {
        let mut logits = vec![0.0f32; self.num_experts];
        for (e, logit) in logits.iter_mut().enumerate() {
            let offset = e * self.hidden_dim;
            *logit = hidden
                .iter()
                .zip(&self.weights[offset..offset + self.hidden_dim])
                .map(|(h, w)| h * w)
                .sum();
        }
        logits
    }

    /// Apply softmax with temperature.
    fn softmax_with_temp(logits: &[f32], temperature: f32) -> Vec<f32> {
        let scaled: Vec<f32> = logits.iter().map(|&x| x / temperature).collect();
        let max = scaled.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exp_sum: f32 = scaled.iter().map(|&x| (x - max).exp()).sum();
        scaled.iter().map(|&x| (x - max).exp() / exp_sum).collect()
    }

    /// Select top-k experts from probabilities.
    fn top_k_selection(probs: &[f32], k: usize) -> (Vec<usize>, Vec<f32>) {
        let mut indexed: Vec<(usize, f32)> = probs.iter().cloned().enumerate().collect();
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let indices: Vec<usize> = indexed.iter().take(k).map(|(i, _)| *i).collect();
        let weights: Vec<f32> = indexed.iter().take(k).map(|(_, w)| *w).collect();

        // Renormalize weights
        let sum: f32 = weights.iter().sum();
        let normalized: Vec<f32> = weights.iter().map(|w| w / sum).collect();

        (indices, normalized)
    }
}

impl MoeRouter for LinearRouter {
    fn route(
        &self,
        hidden_states: &[f32],
        batch_size: usize,
        config: &MoeConfig,
    ) -> Result<RoutingDecision, MoeError> {
        config.validate()?;

        let expected_len = batch_size * self.hidden_dim;
        if hidden_states.len() != expected_len {
            return Err(MoeError::DimensionMismatch {
                expected: expected_len,
                actual: hidden_states.len(),
            });
        }

        let mut expert_indices = Vec::with_capacity(batch_size);
        let mut routing_weights = Vec::with_capacity(batch_size);
        let mut load_per_expert = vec![0u32; config.num_experts];

        for b in 0..batch_size {
            let offset = b * self.hidden_dim;
            let hidden = &hidden_states[offset..offset + self.hidden_dim];

            let logits = self.compute_logits(hidden);
            let probs = Self::softmax_with_temp(&logits, config.router_temperature);
            let (indices, weights) = Self::top_k_selection(&probs, config.top_k);

            for &idx in &indices {
                load_per_expert[idx] += 1;
            }

            expert_indices.push(indices);
            routing_weights.push(weights);
        }

        Ok(RoutingDecision {
            expert_indices,
            routing_weights,
            load_per_expert: Some(load_per_expert),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_router() {
        // 4-dim hidden, 3 experts
        let weights: Vec<f32> = (0..12).map(|i| i as f32 * 0.1).collect();
        let router = LinearRouter::new(weights, 4, 3).unwrap();

        let config = MoeConfig {
            num_experts: 3,
            top_k: 2,
            ..Default::default()
        };

        let hidden = vec![1.0, 2.0, 3.0, 4.0]; // batch_size = 1
        let decision = router.route(&hidden, 1, &config).unwrap();

        assert_eq!(decision.expert_indices.len(), 1);
        assert_eq!(decision.expert_indices[0].len(), 2);
        assert_eq!(decision.routing_weights[0].len(), 2);
    }

    #[test]
    fn test_softmax() {
        let logits = vec![1.0, 2.0, 3.0];
        let probs = LinearRouter::softmax_with_temp(&logits, 1.0);
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }
}
