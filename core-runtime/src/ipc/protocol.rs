//! Wire format and schema validation for IPC messages.
//!
//! # Security
//! - Message size limits prevent memory exhaustion attacks
//! - Protocol versioning enables backward-compatible security updates
//! - Response size limits prevent resource exhaustion

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::engine::InferenceParams;
use crate::health::HealthReport;
use crate::telemetry::{ExportableSpan, MetricsSnapshot};

/// Model information for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model handle ID
    pub handle_id: u64,
    /// Model name
    pub name: String,
    /// Model format (gguf, onnx, etc.)
    pub format: String,
    /// Model size in bytes
    pub size_bytes: u64,
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// Current state
    pub state: String,
    /// Total requests processed
    pub request_count: u64,
    /// Average latency in milliseconds
    pub avg_latency_ms: f64,
    /// Timestamp when loaded (ISO 8601)
    pub loaded_at: String,
}

/// Models list response for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsListResponse {
    /// List of loaded models
    pub models: Vec<ModelInfo>,
    /// Total memory used by all models
    pub total_memory_bytes: u64,
}

/// Current protocol version for new connections.
pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::V1;

/// Minimum supported protocol version.
pub const MIN_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::V1;

/// Protocol version for negotiating encoding strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolVersion {
    /// V1: JSON encoding of token arrays (current default).
    V1,
    /// V2: Packed varint encoding (experimental).
    V2,
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self::V1
    }
}

impl ProtocolVersion {
    /// Check if this version is supported.
    pub fn is_supported(&self) -> bool {
        matches!(self, ProtocolVersion::V1 | ProtocolVersion::V2)
    }

    /// Negotiate the highest common version with a client.
    pub fn negotiate(client_requested: Option<ProtocolVersion>) -> ProtocolVersion {
        let requested = client_requested.unwrap_or_default();
        if requested.is_supported() {
            requested
        } else {
            CURRENT_PROTOCOL_VERSION
        }
    }
}

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Invalid message format: {0}")]
    InvalidFormat(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Message too large: {size} bytes (max {max})")]
    MessageTooLarge { size: usize, max: usize },
}

/// Unique request identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub u64);

/// Inference request from caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub request_id: RequestId,
    pub model_id: String,
    pub prompt_tokens: Vec<u32>,
    pub parameters: InferenceParams,
}

impl InferenceRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.model_id.is_empty() {
            return Err(ProtocolError::MissingField("model_id".into()));
        }
        if self.prompt_tokens.is_empty() {
            return Err(ProtocolError::MissingField("prompt_tokens".into()));
        }
        Ok(())
    }
}

/// Inference response to caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub request_id: RequestId,
    pub output_tokens: Vec<u32>,
    pub finished: bool,
    pub error: Option<String>,
}

impl InferenceResponse {
    pub fn success(request_id: RequestId, output_tokens: Vec<u32>, finished: bool) -> Self {
        Self {
            request_id,
            output_tokens,
            finished,
            error: None,
        }
    }

    pub fn error(request_id: RequestId, error: String) -> Self {
        Self {
            request_id,
            output_tokens: Vec::new(),
            finished: true,
            error: Some(error),
        }
    }
}

/// Single token chunk for streaming responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub request_id: RequestId,
    pub token: u32,
    pub is_final: bool,
    pub error: Option<String>,
}

impl StreamChunk {
    /// Create a non-final token chunk.
    pub fn token(request_id: RequestId, token: u32) -> Self {
        Self {
            request_id,
            token,
            is_final: false,
            error: None,
        }
    }

    /// Create the final token chunk.
    pub fn final_token(request_id: RequestId, token: u32) -> Self {
        Self {
            request_id,
            token,
            is_final: true,
            error: None,
        }
    }

    /// Create an error chunk (always final).
    pub fn error(request_id: RequestId, error: String) -> Self {
        Self {
            request_id,
            token: 0,
            is_final: true,
            error: Some(error),
        }
    }
}

/// Warmup request to prime a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupRequest {
    pub model_id: String,
    /// Number of tokens to generate (default: 1).
    #[serde(default = "default_warmup_tokens")]
    pub tokens: usize,
}

fn default_warmup_tokens() -> usize {
    1
}

/// Warmup response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupResponse {
    pub model_id: String,
    pub success: bool,
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

impl WarmupResponse {
    pub fn success(model_id: String, elapsed_ms: u64) -> Self {
        Self {
            model_id,
            success: true,
            error: None,
            elapsed_ms,
        }
    }

    pub fn error(model_id: String, error: String, elapsed_ms: u64) -> Self {
        Self {
            model_id,
            success: false,
            error: Some(error),
            elapsed_ms,
        }
    }
}

/// Health check request types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthCheckType {
    Liveness,
    Readiness,
    Full,
}

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub check_type: HealthCheckType,
    pub ok: bool,
    pub report: Option<HealthReport>,
}

/// All possible IPC message types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcMessage {
    #[serde(rename = "handshake")]
    Handshake {
        token: String,
        /// Optional protocol version request. Defaults to V1 if not specified.
        #[serde(default)]
        protocol_version: Option<ProtocolVersion>,
    },

    #[serde(rename = "handshake_ack")]
    HandshakeAck {
        session_id: String,
        /// Negotiated protocol version for this session.
        #[serde(default)]
        protocol_version: ProtocolVersion,
    },

    #[serde(rename = "inference_request")]
    InferenceRequest(InferenceRequest),

    #[serde(rename = "inference_response")]
    InferenceResponse(InferenceResponse),

    #[serde(rename = "stream_chunk")]
    StreamChunk(StreamChunk),

    #[serde(rename = "health_check")]
    HealthCheck { check_type: HealthCheckType },

    #[serde(rename = "health_response")]
    HealthResponse(HealthCheckResponse),

    #[serde(rename = "metrics_request")]
    MetricsRequest,

    #[serde(rename = "metrics_response")]
    MetricsResponse(MetricsSnapshot),

    #[serde(rename = "prometheus_request")]
    PrometheusMetricsRequest,

    #[serde(rename = "prometheus_response")]
    PrometheusMetricsResponse { text: String },

    #[serde(rename = "spans_request")]
    SpansRequest { max_count: usize },

    #[serde(rename = "spans_response")]
    SpansResponse { spans: Vec<ExportableSpan> },

    #[serde(rename = "cancel_request")]
    CancelRequest { request_id: RequestId },

    #[serde(rename = "cancel_response")]
    CancelResponse {
        request_id: RequestId,
        cancelled: bool,
    },

    #[serde(rename = "warmup_request")]
    WarmupRequest(WarmupRequest),

    #[serde(rename = "warmup_response")]
    WarmupResponse(WarmupResponse),

    #[serde(rename = "models_request")]
    ModelsRequest,

    #[serde(rename = "models_response")]
    ModelsResponse(ModelsListResponse),

    #[serde(rename = "error")]
    Error { code: u32, message: String },
}

const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16 MB
/// Maximum response size to prevent memory exhaustion
const MAX_RESPONSE_SIZE: usize = 16 * 1024 * 1024; // 16 MB

/// Encode message to JSON bytes with size limit enforcement.
///
/// # Security
/// Enforces maximum message size to prevent memory exhaustion attacks.
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
///
/// # Security
/// Enforces maximum response size to prevent resource exhaustion.
pub fn encode_response(message: &IpcMessage) -> Result<Vec<u8>, ProtocolError> {
    let bytes = serde_json::to_vec(message)?;
    if bytes.len() > MAX_RESPONSE_SIZE {
        // For oversized responses, return an error response instead
        let error_response = IpcMessage::Error {
            code: 413, // HTTP 413 Payload Too Large
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
///
/// # Security
/// Enforces maximum message size to prevent memory exhaustion attacks.
/// Size check happens BEFORE parsing to prevent allocation attacks.
pub fn decode_message(bytes: &[u8]) -> Result<IpcMessage, ProtocolError> {
    // SECURITY: Check size BEFORE parsing to prevent memory exhaustion
    if bytes.len() > MAX_MESSAGE_SIZE {
        return Err(ProtocolError::MessageTooLarge {
            size: bytes.len(),
            max: MAX_MESSAGE_SIZE,
        });
    }
    Ok(serde_json::from_slice(bytes)?)
}

/// Encode message to bytes for IPC transport.
///
/// Uses JSON encoding; bincode is incompatible with the internally-tagged
/// `IpcMessage` enum (`#[serde(tag = "type")]` requires `deserialize_any`
/// which bincode v1 does not implement).
pub fn encode_message_binary(message: &IpcMessage) -> Result<Vec<u8>, ProtocolError> {
    encode_message(message)
}

/// Decode message from IPC transport bytes.
///
/// Uses JSON decoding; see `encode_message_binary` for rationale.
pub fn decode_message_binary(bytes: &[u8]) -> Result<IpcMessage, ProtocolError> {
    decode_message(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_default() {
        assert_eq!(ProtocolVersion::default(), ProtocolVersion::V1);
    }

    #[test]
    fn test_protocol_version_is_supported() {
        assert!(ProtocolVersion::V1.is_supported());
        assert!(ProtocolVersion::V2.is_supported());
    }

    #[test]
    fn test_protocol_version_negotiate_none() {
        // None should default to V1
        assert_eq!(ProtocolVersion::negotiate(None), ProtocolVersion::V1);
    }

    #[test]
    fn test_protocol_version_negotiate_v1() {
        assert_eq!(
            ProtocolVersion::negotiate(Some(ProtocolVersion::V1)),
            ProtocolVersion::V1
        );
    }

    #[test]
    fn test_protocol_version_negotiate_v2() {
        assert_eq!(
            ProtocolVersion::negotiate(Some(ProtocolVersion::V2)),
            ProtocolVersion::V2
        );
    }

    #[test]
    fn test_encode_decode_message() {
        let msg = IpcMessage::HealthCheck {
            check_type: HealthCheckType::Liveness,
        };
        let encoded = encode_message(&msg).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert!(matches!(
            decoded,
            IpcMessage::HealthCheck {
                check_type: HealthCheckType::Liveness
            }
        ));
    }

    #[test]
    fn test_encode_response_within_limit() {
        let msg = IpcMessage::Error {
            code: 500,
            message: "test".to_string(),
        };
        let result = encode_response(&msg).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_decode_message_too_large() {
        // Create a message that exceeds the size limit
        let large_data = vec![0u8; MAX_MESSAGE_SIZE + 1];
        let result = decode_message(&large_data);
        assert!(matches!(result, Err(ProtocolError::MessageTooLarge { .. })));
    }

    #[test]
    fn test_encode_message_binary_roundtrip() {
        let msg = IpcMessage::HealthCheck {
            check_type: HealthCheckType::Readiness,
        };
        let encoded = encode_message_binary(&msg).unwrap();
        let decoded = decode_message_binary(&encoded).unwrap();
        assert!(matches!(
            decoded,
            IpcMessage::HealthCheck {
                check_type: HealthCheckType::Readiness
            }
        ));
    }

    #[test]
    fn test_decode_message_binary_too_large() {
        let large_data = vec![0u8; MAX_MESSAGE_SIZE + 1];
        let result = decode_message_binary(&large_data);
        assert!(matches!(result, Err(ProtocolError::MessageTooLarge { .. })));
    }

    #[test]
    fn test_inference_request_validation() {
        let valid = InferenceRequest {
            request_id: RequestId(1),
            model_id: "test-model".to_string(),
            prompt_tokens: vec![1, 2, 3],
            parameters: InferenceParams::default(),
        };
        assert!(valid.validate().is_ok());

        let invalid_model = InferenceRequest {
            request_id: RequestId(1),
            model_id: "".to_string(),
            prompt_tokens: vec![1, 2, 3],
            parameters: InferenceParams::default(),
        };
        assert!(invalid_model.validate().is_err());

        let invalid_tokens = InferenceRequest {
            request_id: RequestId(1),
            model_id: "test".to_string(),
            prompt_tokens: vec![],
            parameters: InferenceParams::default(),
        };
        assert!(invalid_tokens.validate().is_err());
    }

    #[test]
    fn test_inference_response_success() {
        let response = InferenceResponse::success(RequestId(1), vec![1, 2, 3], true);
        assert_eq!(response.request_id.0, 1);
        assert!(response.finished);
        assert!(response.error.is_none());
    }

    #[test]
    fn test_inference_response_error() {
        let response = InferenceResponse::error(RequestId(1), "test error".to_string());
        assert!(response.finished);
        assert!(response.error.is_some());
        assert!(response.output_tokens.is_empty());
    }

    #[test]
    fn test_stream_chunk_token() {
        let chunk = StreamChunk::token(RequestId(1), 42);
        assert_eq!(chunk.token, 42);
        assert!(!chunk.is_final);
        assert!(chunk.error.is_none());
    }

    #[test]
    fn test_stream_chunk_final() {
        let chunk = StreamChunk::final_token(RequestId(1), 42);
        assert!(chunk.is_final);
    }

    #[test]
    fn test_stream_chunk_error() {
        let chunk = StreamChunk::error(RequestId(1), "error".to_string());
        assert!(chunk.is_final);
        assert!(chunk.error.is_some());
    }

    #[test]
    fn test_warmup_response() {
        let success = WarmupResponse::success("model".to_string(), 100);
        assert!(success.success);

        let error = WarmupResponse::error("model".to_string(), "err".to_string(), 50);
        assert!(!error.success);
        assert!(error.error.is_some());
    }

    #[test]
    fn test_handshake_message_encoding() {
        let msg = IpcMessage::Handshake {
            token: "test-token".to_string(),
            protocol_version: Some(ProtocolVersion::V2),
        };
        let encoded = encode_message(&msg).unwrap();
        let decoded: IpcMessage = serde_json::from_slice(&encoded).unwrap();
        assert!(matches!(
            decoded,
            IpcMessage::Handshake {
                protocol_version: Some(ProtocolVersion::V2),
                ..
            }
        ));
    }

    #[test]
    fn test_handshake_ack_message() {
        let msg = IpcMessage::HandshakeAck {
            session_id: "session-123".to_string(),
            protocol_version: ProtocolVersion::V1,
        };
        let encoded = encode_message(&msg).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert!(matches!(
            decoded,
            IpcMessage::HandshakeAck {
                session_id,
                protocol_version: ProtocolVersion::V1,
            } if session_id == "session-123"
        ));
    }

    #[test]
    fn test_protocol_error_display() {
        let err = ProtocolError::MessageTooLarge { size: 100, max: 50 };
        let msg = err.to_string();
        assert!(msg.contains("100"));
        assert!(msg.contains("50"));

        let err = ProtocolError::MissingField("test".to_string());
        assert!(err.to_string().contains("test"));
    }
}
