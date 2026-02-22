//! Wire format and schema validation for IPC messages.
//!
//! Split into sub-modules for Section 4 compliance:
//! - `protocol_types`: Message types, enums, structs
//! - `protocol_codec`: Encode/decode functions

pub use super::protocol_types::*;
pub use super::protocol_codec::*;

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
