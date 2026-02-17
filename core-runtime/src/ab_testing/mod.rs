// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! A/B testing support for model variants.
//!
//! Provides traffic splitting, variant management, and per-variant metrics
//! for comparing model performance in production.

pub mod metrics;
pub mod traffic;
pub mod variant;

pub use metrics::{VariantMetrics, VariantStats, VariantStatsSnapshot};
pub use traffic::{TrafficConfig, TrafficSplitter};
pub use variant::{Variant, VariantLabel};
