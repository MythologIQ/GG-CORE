//! Model Encryption Module
//!
//! Provides AES-256-GCM encryption for model files at rest.
//! Split into sub-modules for Section 4 compliance:
//! - `encryption_core`: Core encryption/decryption logic
//! - `encryption_key`: Key derivation and salt management

// Re-export all public items from sub-modules
pub use super::encryption_core::*;
pub use super::encryption_key::{get_or_create_installation_salt, MIN_SALT_SIZE};

#[cfg(test)]
#[path = "encryption_tests.rs"]
mod tests;
