// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Kubernetes integration types.
//!
//! Defines Rust types matching the VeritasRuntime and VeritasModel CRDs.

pub mod types;

pub use types::{VeritasModel, VeritasModelSpec, VeritasRuntime, VeritasRuntimeSpec};
