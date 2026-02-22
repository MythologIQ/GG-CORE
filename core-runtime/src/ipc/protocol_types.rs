//! Wire format types and schema for IPC messages.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::engine::InferenceParams;
use crate::health::HealthReport;
use crate::telemetry::{ExportableSpan, MetricsSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub handle_id: u64,
    pub name: String,
    pub format: String,
    pub size_bytes: u64,
    pub memory_bytes: u64,
    pub state: String,
    pub request_count: u64,
    pub avg_latency_ms: f64,
    pub loaded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsListResponse {
    pub models: Vec<ModelInfo>,
    pub total_memory_bytes: u64,
}

pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::V1;
pub const MIN_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::V1;

/// Protocol version for negotiating encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolVersion {
    V1,
    V2,
}

impl Default for ProtocolVersion {
    fn default() -> Self { Self::V1 }
}

impl ProtocolVersion {
    pub fn is_supported(&self) -> bool {
        matches!(self, ProtocolVersion::V1 | ProtocolVersion::V2)
    }

    pub fn negotiate(client_requested: Option<ProtocolVersion>) -> ProtocolVersion {
        let requested = client_requested.unwrap_or_default();
        if requested.is_supported() { requested } else { CURRENT_PROTOCOL_VERSION }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub u64);

/// Inference request from caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub request_id: RequestId,
    pub model_id: String,
    pub prompt: String,
    pub parameters: InferenceParams,
}

impl InferenceRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.model_id.is_empty() {
            return Err(ProtocolError::MissingField("model_id".into()));
        }
        if self.prompt.is_empty() {
            return Err(ProtocolError::MissingField("prompt".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceErrorCode {
    AdmissionRejected,
    ExecutionFailed,
    ModelNotLoaded,
    InputInvalid,
    ShuttingDown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub request_id: RequestId,
    pub output: String,
    pub tokens_generated: usize,
    pub finished: bool,
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<InferenceErrorCode>,
}

impl InferenceResponse {
    pub fn success(request_id: RequestId, output: String, tokens_generated: usize, finished: bool) -> Self {
        Self { request_id, output, tokens_generated, finished, error: None, error_code: None }
    }

    pub fn error(request_id: RequestId, error: String) -> Self {
        Self {
            request_id, output: String::new(), tokens_generated: 0, finished: true,
            error: Some(error), error_code: Some(InferenceErrorCode::ExecutionFailed),
        }
    }

    pub fn error_coded(request_id: RequestId, error: String, code: InferenceErrorCode) -> Self {
        Self {
            request_id, output: String::new(), tokens_generated: 0, finished: true,
            error: Some(error), error_code: Some(code),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub request_id: RequestId,
    pub token: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub is_final: bool,
    pub error: Option<String>,
}

impl StreamChunk {
    pub fn token(request_id: RequestId, token: u32) -> Self {
        Self { request_id, token, text: None, is_final: false, error: None }
    }

    pub fn token_with_text(request_id: RequestId, token: u32, text: String) -> Self {
        Self { request_id, token, text: Some(text), is_final: false, error: None }
    }

    pub fn final_token(request_id: RequestId, token: u32) -> Self {
        Self { request_id, token, text: None, is_final: true, error: None }
    }

    pub fn final_token_with_text(request_id: RequestId, token: u32, text: String) -> Self {
        Self { request_id, token, text: Some(text), is_final: true, error: None }
    }

    pub fn error(request_id: RequestId, error: String) -> Self {
        Self { request_id, token: 0, text: None, is_final: true, error: Some(error) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupRequest {
    pub model_id: String,
    #[serde(default = "default_warmup_tokens")]
    pub tokens: usize,
}

fn default_warmup_tokens() -> usize { 1 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupResponse {
    pub model_id: String,
    pub success: bool,
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

impl WarmupResponse {
    pub fn success(model_id: String, elapsed_ms: u64) -> Self {
        Self { model_id, success: true, error: None, elapsed_ms }
    }

    pub fn error(model_id: String, error: String, elapsed_ms: u64) -> Self {
        Self { model_id, success: false, error: Some(error), elapsed_ms }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthCheckType { Liveness, Readiness, Full }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub check_type: HealthCheckType,
    pub ok: bool,
    pub report: Option<HealthReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcMessage {
    #[serde(rename = "handshake")]
    Handshake { token: String, #[serde(default)] protocol_version: Option<ProtocolVersion> },
    #[serde(rename = "handshake_ack")]
    HandshakeAck { session_id: String, #[serde(default)] protocol_version: ProtocolVersion },
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
    CancelResponse { request_id: RequestId, cancelled: bool },
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
