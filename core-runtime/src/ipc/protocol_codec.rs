//! Encode/decode functions for IPC protocol messages.
//!
//! # Security
//! Enforces maximum message/response sizes to prevent memory exhaustion.

use super::protocol_types::{IpcMessage, ProtocolError};

const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16 MB
const MAX_RESPONSE_SIZE: usize = 16 * 1024 * 1024; // 16 MB

/// Encode message to JSON bytes with size limit enforcement.
pub fn encode_message(message: &IpcMessage) -> Result<Vec<u8>, ProtocolError> {
    let bytes = serde_json::to_vec(message)?;
    if bytes.len() > MAX_MESSAGE_SIZE {
        return Err(ProtocolError::MessageTooLarge {
            size: bytes.len(),
            max: MAX_MESSAGE_SIZE,
        });
    }
    Ok(bytes)
}

/// Encode response message with response-specific size limit.
pub fn encode_response(message: &IpcMessage) -> Result<Vec<u8>, ProtocolError> {
    let bytes = serde_json::to_vec(message)?;
    if bytes.len() > MAX_RESPONSE_SIZE {
        let error_response = IpcMessage::Error {
            code: 413,
            message: format!(
                "Response too large: {} bytes (max {})",
                bytes.len(),
                MAX_RESPONSE_SIZE
            ),
        };
        return encode_message(&error_response);
    }
    Ok(bytes)
}

/// Decode message from JSON bytes with size limit enforcement.
pub fn decode_message(bytes: &[u8]) -> Result<IpcMessage, ProtocolError> {
    if bytes.len() > MAX_MESSAGE_SIZE {
        return Err(ProtocolError::MessageTooLarge {
            size: bytes.len(),
            max: MAX_MESSAGE_SIZE,
        });
    }
    Ok(serde_json::from_slice(bytes)?)
}

/// Encode message to bytes for IPC transport.
pub fn encode_message_binary(message: &IpcMessage) -> Result<Vec<u8>, ProtocolError> {
    encode_message(message)
}

/// Decode message from IPC transport bytes.
pub fn decode_message_binary(bytes: &[u8]) -> Result<IpcMessage, ProtocolError> {
    decode_message(bytes)
}

#[cfg(test)]
pub(crate) const TEST_MAX_MESSAGE_SIZE: usize = MAX_MESSAGE_SIZE;
