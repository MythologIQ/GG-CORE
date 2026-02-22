// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! Kubernetes integration types.
//!
//! Defines Rust types matching the GgCoreRuntime and GgCoreModel CRDs.

pub mod profiles;
pub mod types;
pub mod validation;

// K8s CRD types - names match the actual CRD kind for compatibility
pub use types::{GgCoreModel, GgCoreModelSpec, GgCoreRuntime, GgCoreRuntimeSpec};
