//! IPC module for CORE Runtime.
//!
//! Handles named pipe/Unix socket communication with authenticated callers.
//! This is the ONLY external interface - no HTTP/REST/WebSocket allowed.

mod auth;
mod auth_session;
mod connections;
pub mod encoding;
mod handler;
mod health_handler;
pub mod protocol_types;
pub mod protocol_codec;
pub mod protocol;
pub mod server;
mod stream_bridge;

pub use auth::{AuthError, SessionAuth, SessionToken};
pub use connections::{
    ConnectionConfig, ConnectionGuard, ConnectionPool, IpcServerConfig, OwnedConnectionGuard,
};
pub use encoding::{get_encoder, TokenEncoder, V1Encoder, V2Encoder};
pub use handler::{HandlerError, IpcHandler, IpcHandlerConfig, StreamSender};
pub use stream_bridge::IpcStreamBridge;
pub use protocol::{
    decode_message, decode_message_binary, encode_message, encode_message_binary,
    HealthCheckResponse, HealthCheckType, InferenceRequest, InferenceResponse, IpcMessage,
    ModelInfo, ModelsListResponse, ProtocolError, ProtocolVersion, RequestId, StreamChunk,
    WarmupRequest, WarmupResponse,
};
// Re-export MetricsSnapshot for IPC consumers
pub use crate::telemetry::MetricsSnapshot;
