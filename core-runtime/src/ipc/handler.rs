//! Request/response handling for IPC connections.

use std::sync::Arc;
use thiserror::Error;

use super::auth::{AuthError, SessionAuth, SessionToken};
use super::health_handler::HealthHandler;
use super::protocol::{
    decode_message, encode_message, InferenceRequest, InferenceResponse, IpcMessage, ModelInfo,
    ModelsListResponse, ProtocolError, ProtocolVersion, StreamChunk, WarmupResponse,
};
use crate::engine::InferenceEngine;
use crate::health::HealthChecker;
use crate::models::{ModelHandle, ModelRegistry};
use crate::scheduler::Priority;
use crate::scheduler::RequestQueue;
use crate::shutdown::ShutdownCoordinator;
use crate::telemetry::MetricsStore;

#[derive(Error, Debug)]
pub enum HandlerError {
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Authentication error: {0}")]
    Auth(#[from] AuthError),

    #[error("Not authenticated")]
    NotAuthenticated,

    #[error("Queue error: {0}")]
    QueueFull(String),

    #[error("Server is shutting down")]
    ShuttingDown,

    #[error("Stream send error: {0}")]
    StreamSend(String),
}

/// Configuration for IPC handler.
#[derive(Debug, Clone)]
pub struct IpcHandlerConfig {
    pub require_auth: bool,
}

impl Default for IpcHandlerConfig {
    fn default() -> Self {
        Self { require_auth: true }
    }
}

/// Trait for sending streaming responses over IPC.
#[async_trait::async_trait]
pub trait StreamSender: Send + Sync {
    /// Send a message to the stream. Returns error if stream is closed.
    async fn send(&self, message: IpcMessage) -> Result<(), HandlerError>;
}

/// Format SystemTime as ISO 8601 string for IPC responses.
fn format_system_time(time: std::time::SystemTime) -> String {
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs();
            let datetime = chrono::DateTime::from_timestamp(secs as i64, 0);
            datetime
                .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
                .unwrap_or_else(|| format!("{}s", secs))
        })
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Handles IPC message processing with authentication.
pub struct IpcHandler {
    /// Session authentication manager (public for FFI access)
    pub auth: Arc<SessionAuth>,
    queue: Arc<RequestQueue>,
    config: IpcHandlerConfig,
    shutdown: Arc<ShutdownCoordinator>,
    health_handler: HealthHandler,
    metrics_store: Arc<MetricsStore>,
    model_registry: Arc<ModelRegistry>,
    inference_engine: Arc<InferenceEngine>,
}

impl IpcHandler {
    pub fn new(
        auth: Arc<SessionAuth>,
        queue: Arc<RequestQueue>,
        config: IpcHandlerConfig,
        shutdown: Arc<ShutdownCoordinator>,
        health: Arc<HealthChecker>,
        model_registry: Arc<ModelRegistry>,
        metrics_store: Arc<MetricsStore>,
        inference_engine: Arc<InferenceEngine>,
    ) -> Self {
        let health_handler = HealthHandler::new(
            health,
            Arc::clone(&shutdown),
            Arc::clone(&model_registry),
            Arc::clone(&queue),
        );
        Self {
            auth,
            queue,
            config,
            shutdown,
            health_handler,
            metrics_store,
            model_registry,
            inference_engine,
        }
    }

    /// Process incoming message bytes and return response bytes.
    pub async fn process(
        &self,
        bytes: &[u8],
        session: Option<&SessionToken>,
    ) -> Result<(Vec<u8>, Option<SessionToken>), HandlerError> {
        let message = decode_message(bytes)?;
        let (response, new_session) = self.handle_message(message, session).await?;
        let response_bytes = encode_message(&response)?;
        Ok((response_bytes, new_session))
    }

    async fn handle_message(
        &self,
        message: IpcMessage,
        session: Option<&SessionToken>,
    ) -> Result<(IpcMessage, Option<SessionToken>), HandlerError> {
        match message {
            IpcMessage::Handshake {
                token,
                protocol_version,
            } => {
                let session_token = self.auth.authenticate(&token).await?;
                // Negotiate protocol version with client
                let negotiated_version = ProtocolVersion::negotiate(protocol_version);
                let response = IpcMessage::HandshakeAck {
                    session_id: session_token.as_str().to_string(),
                    protocol_version: negotiated_version,
                };
                Ok((response, Some(session_token)))
            }

            IpcMessage::InferenceRequest(request) => {
                self.require_auth(session).await?;
                let response = self.handle_inference(request).await;
                Ok((IpcMessage::InferenceResponse(response), None))
            }

            IpcMessage::HealthCheck { check_type } => {
                // NO AUTH REQUIRED for health checks (orchestrator pattern)
                let response = self.health_handler.handle(check_type).await;
                Ok((IpcMessage::HealthResponse(response), None))
            }

            IpcMessage::MetricsRequest => {
                // NO AUTH REQUIRED for metrics (orchestrator pattern, same as health)
                let snapshot = self.metrics_store.snapshot();
                Ok((IpcMessage::MetricsResponse(snapshot), None))
            }

            IpcMessage::ModelsRequest => {
                // NO AUTH REQUIRED for model listing (orchestrator pattern, same as health/metrics)
                let response = self.handle_models_request().await;
                Ok((IpcMessage::ModelsResponse(response), None))
            }

            IpcMessage::CancelRequest { request_id } => {
                // AUTH REQUIRED for cancellation (session-scoped operation)
                self.require_auth(session).await?;
                let cancelled = self.queue.cancel(request_id.0).await;
                Ok((
                    IpcMessage::CancelResponse {
                        request_id,
                        cancelled,
                    },
                    None,
                ))
            }

            IpcMessage::WarmupRequest(request) => {
                // NO AUTH REQUIRED (orchestrator pattern, same as health/metrics)
                let response = self.handle_warmup(request.model_id, request.tokens).await;
                Ok((IpcMessage::WarmupResponse(response), None))
            }

            _ => {
                let error = IpcMessage::Error {
                    code: 400,
                    message: "Unexpected message type".into(),
                };
                Ok((error, None))
            }
        }
    }

    async fn require_auth(&self, session: Option<&SessionToken>) -> Result<(), HandlerError> {
        if !self.config.require_auth {
            return Ok(());
        }

        let token = session.ok_or(HandlerError::NotAuthenticated)?;
        self.auth.validate(token).await?;
        Ok(())
    }

    async fn handle_inference(&self, request: InferenceRequest) -> InferenceResponse {
        // Check shutdown state before accepting new request
        let _guard = match self.shutdown.track() {
            Some(g) => g,
            None => {
                return InferenceResponse::error(
                    request.request_id,
                    "Server is shutting down".into(),
                );
            }
        };

        if let Err(e) = request.validate() {
            return InferenceResponse::error(request.request_id, e.to_string());
        }

        // Track request in queue for metrics
        let enqueue_result = self
            .queue
            .enqueue(
                request.model_id.clone(),
                request.prompt_tokens.clone(),
                request.parameters.clone(),
                Priority::Normal,
            )
            .await;

        if let Err(e) = enqueue_result {
            return InferenceResponse::error(request.request_id, e.to_string());
        }

        // Run inference synchronously for IPC requests
        // TODO: Use model_id to look up actual model handle from registry
        let model_handle = ModelHandle::new(0);
        let start = std::time::Instant::now();

        match self
            .inference_engine
            .run(model_handle, &request.prompt_tokens, &request.parameters)
            .await
        {
            Ok(result) => {
                // Record latency in model registry
                let latency_ms = start.elapsed().as_millis() as f64;
                self.model_registry
                    .record_request(model_handle, latency_ms)
                    .await;

                InferenceResponse::success(
                    request.request_id,
                    result.output_tokens,
                    result.finished,
                )
            }
            Err(e) => InferenceResponse::error(request.request_id, e.to_string()),
        }
        // guard dropped here, decrementing in-flight count
    }

    async fn handle_warmup(&self, model_id: String, _tokens: usize) -> WarmupResponse {
        let start = std::time::Instant::now();
        let result = self
            .queue
            .enqueue(
                model_id.clone(),
                vec![1], // Minimal warmup prompt
                crate::engine::InferenceParams::default(),
                Priority::Low,
            )
            .await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        match result {
            Ok(_) => WarmupResponse::success(model_id, elapsed_ms),
            Err(e) => WarmupResponse::error(model_id, e.to_string(), elapsed_ms),
        }
    }

    async fn handle_models_request(&self) -> ModelsListResponse {
        let models = self.model_registry.list_models().await;
        let total_memory_bytes = models.iter().map(|m| m.memory_bytes).sum();

        let model_infos: Vec<ModelInfo> = models
            .into_iter()
            .map(|m| {
                let avg_latency_ms = if m.request_count > 0 {
                    m.total_latency_ms / m.request_count as f64
                } else {
                    0.0
                };
                ModelInfo {
                    handle_id: m.handle_id,
                    name: m.name,
                    format: m.format,
                    size_bytes: m.size_bytes,
                    memory_bytes: m.memory_bytes,
                    state: m.state.as_str().to_string(),
                    request_count: m.request_count,
                    avg_latency_ms,
                    loaded_at: format_system_time(m.loaded_at),
                }
            })
            .collect();

        ModelsListResponse {
            models: model_infos,
            total_memory_bytes,
        }
    }

    /// Process streaming inference request. Sends chunks via sender.
    pub async fn process_streaming(
        &self,
        request: InferenceRequest,
        session: &SessionToken,
        sender: &dyn StreamSender,
    ) -> Result<(), HandlerError> {
        self.auth.validate(session).await?;

        let _guard = self.shutdown.track().ok_or(HandlerError::ShuttingDown)?;

        if let Err(e) = request.validate() {
            let chunk = StreamChunk::error(request.request_id, e.to_string());
            sender.send(IpcMessage::StreamChunk(chunk)).await?;
            return Ok(());
        }

        // Enqueue for streaming inference
        let enqueue_result = self
            .queue
            .enqueue(
                request.model_id.clone(),
                request.prompt_tokens.clone(),
                request.parameters.clone(),
                Priority::Normal,
            )
            .await;

        if let Err(e) = enqueue_result {
            let chunk = StreamChunk::error(request.request_id, e.to_string());
            sender.send(IpcMessage::StreamChunk(chunk)).await?;
            return Ok(());
        }

        // Send final chunk (actual streaming would integrate with TokenStream)
        let chunk = StreamChunk::final_token(request.request_id, 0);
        sender.send(IpcMessage::StreamChunk(chunk)).await?;
        Ok(())
    }
}
