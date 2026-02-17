// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! C Foreign Function Interface for Veritas SDR
//!
//! Provides a stable C ABI for integration with non-Rust languages.
//! All functions use error codes and thread-local error messages.

mod auth;
mod error;
mod health;
mod inference;
mod models;
mod runtime;
mod streaming;
mod types;

pub use auth::*;
pub use error::{core_clear_last_error, core_get_last_error, CoreErrorCode};
pub use health::*;
pub use inference::*;
pub use models::*;
pub use runtime::*;
pub use streaming::*;
pub use types::*;
