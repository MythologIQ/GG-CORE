//! Integration tests for Mixture of Experts (MoE) module.
//!
//! Tests MoE configuration, routing, expert combination, load balancing,
//! and executor functionality without requiring actual model inference.

use veritas_sdr::engine::{
    ExpertCombiner, ExpertDeviceAssignment, ExpertOutput, LinearRouter, MoeConfig, MoeError,
    MoeExecutor, MoeRouter, RoutingDecision,
};

// ============================================================================
// MoeConfig Tests
// ============================================================================

#[test]
fn config_default_values() {
    let config = MoeConfig::default();
    assert_eq!(config.num_experts, 8);
    assert_eq!(config.top_k, 2);
    assert!((config.capacity_factor - 1.25).abs() < 0.001);
    assert!((config.router_temperature - 1.0).abs() < 0.001);
    assert_eq!(config.hidden_dim, 4096);
    assert_eq!(config.intermediate_dim, 14336);
    assert!(config.use_aux_loss);
    assert!((config.aux_loss_coef - 0.01).abs() < 0.001);
}

#[test]
fn config_mixtral_preset() {
    let config = MoeConfig::mixtral();
    assert_eq!(config.num_experts, 8);
    assert_eq!(config.top_k, 2);
    assert!((config.capacity_factor - 1.25).abs() < 0.001);
    assert!((config.router_temperature - 1.0).abs() < 0.001);
    assert_eq!(config.hidden_dim, 4096);
    assert_eq!(config.intermediate_dim, 14336);
}

#[test]
fn config_deepseek_preset() {
    let config = MoeConfig::deepseek();
    assert_eq!(config.num_experts, 64);
    assert_eq!(config.top_k, 6);
    assert!((config.capacity_factor - 1.5).abs() < 0.001);
    assert!((config.router_temperature - 0.7).abs() < 0.001);
    assert_eq!(config.hidden_dim, 5120);
    assert_eq!(config.intermediate_dim, 12288);
}

#[test]
fn config_validation_valid() {
    let config = MoeConfig::default();
    assert!(config.validate().is_ok());
}

#[test]
fn config_validation_zero_experts() {
    let config = MoeConfig {
        num_experts: 0,
        ..Default::default()
    };
    let result = config.validate();
    assert!(matches!(result, Err(MoeError::InvalidExpertCount(0))));
}

#[test]
fn config_validation_zero_top_k() {
    let config = MoeConfig {
        top_k: 0,
        ..Default::default()
    };
    let result = config.validate();
    assert!(matches!(result, Err(MoeError::InvalidTopK(0))));
}

#[test]
fn config_validation_top_k_exceeds_experts() {
    let config = MoeConfig {
        num_experts: 4,
        top_k: 5,
        ..Default::default()
    };
    let result = config.validate();
    assert!(matches!(result, Err(MoeError::InvalidTopK(5))));
}

// ============================================================================
// LinearRouter Tests
// ============================================================================

#[test]
fn router_creation_valid() {
    let weights: Vec<f32> = (0..12).map(|i| i as f32 * 0.1).collect();
    let router = LinearRouter::new(weights, 4, 3);
    assert!(router.is_ok());
}

#[test]
fn router_creation_dimension_mismatch() {
    let weights: Vec<f32> = vec![0.0; 10]; // Wrong size: 10 != 4 * 3
    let router = LinearRouter::new(weights, 4, 3);
    assert!(matches!(
        router,
        Err(MoeError::DimensionMismatch {
            expected: 12,
            actual: 10
        })
    ));
}

#[test]
fn router_basic_routing() {
    let weights: Vec<f32> = (0..12).map(|i| i as f32 * 0.1).collect();
    let router = LinearRouter::new(weights, 4, 3).unwrap();

    let config = MoeConfig {
        num_experts: 3,
        top_k: 2,
        router_temperature: 1.0,
        ..Default::default()
    };

    let hidden = vec![1.0, 2.0, 3.0, 4.0];
    let decision = router.route(&hidden, 1, &config).unwrap();

    assert_eq!(decision.expert_indices.len(), 1);
    assert_eq!(decision.expert_indices[0].len(), 2);
    assert_eq!(decision.routing_weights[0].len(), 2);
}

#[test]
fn router_weights_sum_to_one() {
    let weights: Vec<f32> = (0..12).map(|i| i as f32 * 0.1).collect();
    let router = LinearRouter::new(weights, 4, 3).unwrap();

    let config = MoeConfig {
        num_experts: 3,
        top_k: 2,
        router_temperature: 1.0,
        ..Default::default()
    };

    let hidden = vec![1.0, 2.0, 3.0, 4.0];
    let decision = router.route(&hidden, 1, &config).unwrap();

    let sum: f32 = decision.routing_weights[0].iter().sum();
    assert!((sum - 1.0).abs() < 1e-5, "Weights sum {} should be 1.0", sum);
}

#[test]
fn router_batch_routing() {
    let weights: Vec<f32> = vec![0.1; 16]; // 4 hidden_dim * 4 experts
    let router = LinearRouter::new(weights, 4, 4).unwrap();

    let config = MoeConfig {
        num_experts: 4,
        top_k: 2,
        router_temperature: 1.0,
        ..Default::default()
    };

    let hidden = vec![1.0; 12]; // batch_size = 3
    let decision = router.route(&hidden, 3, &config).unwrap();

    assert_eq!(decision.expert_indices.len(), 3);
    assert_eq!(decision.routing_weights.len(), 3);
}

#[test]
fn router_load_tracking() {
    let weights: Vec<f32> = (0..12).map(|i| i as f32 * 0.1).collect();
    let router = LinearRouter::new(weights, 4, 3).unwrap();

    let config = MoeConfig {
        num_experts: 3,
        top_k: 2,
        router_temperature: 1.0,
        ..Default::default()
    };

    let hidden = vec![1.0, 2.0, 3.0, 4.0];
    let decision = router.route(&hidden, 1, &config).unwrap();

    assert!(decision.load_per_expert.is_some());
    let load = decision.load_per_expert.unwrap();
    assert_eq!(load.len(), 3);

    let total_load: u32 = load.iter().sum();
    assert_eq!(total_load, 2); // top_k = 2 experts selected
}

// ============================================================================
// ExpertCombiner Tests
// ============================================================================

#[test]
fn combiner_single_expert() {
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
fn combiner_weighted_sum() {
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

#[test]
fn combiner_aux_loss_balanced() {
    let routing = RoutingDecision {
        expert_indices: vec![vec![0], vec![1], vec![2], vec![3]],
        routing_weights: vec![vec![1.0], vec![1.0], vec![1.0], vec![1.0]],
        load_per_expert: Some(vec![1, 1, 1, 1]),
    };

    let aux_loss = ExpertCombiner::compute_aux_loss(&routing, 4);
    assert!(aux_loss < 0.01, "Balanced load should have low aux_loss: {}", aux_loss);
}

#[test]
fn combiner_aux_loss_imbalanced() {
    let routing = RoutingDecision {
        expert_indices: vec![vec![0], vec![0], vec![0], vec![0]],
        routing_weights: vec![vec![1.0], vec![1.0], vec![1.0], vec![1.0]],
        load_per_expert: Some(vec![4, 0, 0, 0]),
    };

    let aux_loss = ExpertCombiner::compute_aux_loss(&routing, 4);
    assert!(aux_loss > 1.0, "Imbalanced load should have high aux_loss: {}", aux_loss);
}

// ============================================================================
// MoeExecutor Tests
// ============================================================================

#[test]
fn executor_cpu_only_assignment() {
    let config = MoeConfig {
        num_experts: 4,
        hidden_dim: 8,
        ..Default::default()
    };

    let executor = MoeExecutor::cpu_only(config);

    for i in 0..4 {
        let assignment = executor.get_assignment(i);
        assert!(assignment.is_some());
        let a = assignment.unwrap();
        assert_eq!(a.expert_idx, i);
        assert_eq!(a.device_id, -1); // CPU
    }
}

#[test]
fn executor_custom_assignment() {
    let config = MoeConfig {
        num_experts: 2,
        hidden_dim: 4,
        ..Default::default()
    };

    let assignments = vec![
        ExpertDeviceAssignment {
            expert_idx: 0,
            device_id: 0,
            memory_offset: 0,
        },
        ExpertDeviceAssignment {
            expert_idx: 1,
            device_id: 1,
            memory_offset: 1024,
        },
    ];

    let executor = MoeExecutor::new(config, assignments);

    let a0 = executor.get_assignment(0).unwrap();
    assert_eq!(a0.device_id, 0);
    assert_eq!(a0.memory_offset, 0);

    let a1 = executor.get_assignment(1).unwrap();
    assert_eq!(a1.device_id, 1);
    assert_eq!(a1.memory_offset, 1024);
}

#[test]
fn executor_group_by_expert() {
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

    assert_eq!(groups.get(&0).map(|v| v.len()), Some(1));
    assert_eq!(groups.get(&1).map(|v| v.len()), Some(2));
    assert_eq!(groups.get(&2).map(|v| v.len()), Some(1));
}

#[test]
fn executor_load_statistics() {
    let config = MoeConfig {
        num_experts: 4,
        hidden_dim: 4,
        ..Default::default()
    };
    let executor = MoeExecutor::cpu_only(config);

    let routing = RoutingDecision {
        expert_indices: vec![],
        routing_weights: vec![],
        load_per_expert: Some(vec![10, 5, 8, 7]),
    };

    let stats = executor.load_statistics(&routing);

    assert_eq!(stats.total_tokens, 30);
    assert_eq!(stats.max_load_per_expert, 10);
    assert_eq!(stats.min_load_per_expert, 5);
    assert!(stats.load_imbalance > 0.0);
}

#[test]
fn executor_load_statistics_empty() {
    let config = MoeConfig {
        num_experts: 4,
        hidden_dim: 4,
        ..Default::default()
    };
    let executor = MoeExecutor::cpu_only(config);

    let routing = RoutingDecision {
        expert_indices: vec![],
        routing_weights: vec![],
        load_per_expert: Some(vec![0, 0, 0, 0]),
    };

    let stats = executor.load_statistics(&routing);

    assert_eq!(stats.total_tokens, 0);
    assert_eq!(stats.load_imbalance, 0.0);
}
