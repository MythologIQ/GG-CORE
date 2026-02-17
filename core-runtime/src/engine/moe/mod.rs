// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Mixture of Experts (MoE) module for sparse model architectures.
//!
//! Supports modern MoE models like Mixtral, DeepSeek, and Qwen-MoE with:
//! - Configurable top-k expert selection
//! - Load-balanced routing with capacity factors
//! - Parallel expert execution across GPUs
//! - Weighted output combination

mod combiner;
mod config;
mod executor;
mod router;

pub use combiner::{ExpertCombiner, ExpertOutput};
pub use config::{MoeConfig, MoeError};
pub use executor::{ExpertDeviceAssignment, MoeExecutor};
pub use router::{LinearRouter, MoeRouter, RoutingDecision};
