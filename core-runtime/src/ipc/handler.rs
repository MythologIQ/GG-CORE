//! Request/response handling for IPC connections.

use std::sync::Arc;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::auth::{AuthError, SessionAuth, SessionToken};
use super::health_handler::HealthHandler;
use super::protocol::{
    decode_message, encode_message, InferenceErrorCode, InferenceRequest, InferenceResponse,
    IpcMessage, ModelInfo, ModelsListResponse, ProtocolError, ProtocolVersion, StreamChunk,
    WarmupResponse,
};
use crate::engine::InferenceEngine;
#[cfg(feature = "gguf")]
use crate::engine::TokenStream;
use crate::health::HealthChecker;
use crate::models::ModelRegistry;
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
    /// Used by streaming path (gguf feature).
    #[allow(dead_code)]
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
        let _guard = match self.shutdown.track() {
            Some(g) => g,
            None => {
                return InferenceResponse::error_coded(
                    request.request_id,
                    "Server is shutting down".into(),
                    InferenceErrorCode::ShuttingDown,
                );
            }
        };

        if let Err(e) = request.validate() {
            return InferenceResponse::error_coded(
                request.request_id,
                e.to_string(),
                InferenceErrorCode::InputInvalid,
            );
        }

        // Enqueue and await result from worker (queue is sole execution path)
        let enqueue_result = self
            .queue
            .enqueue_with_response(
                request.model_id.clone(),
                request.prompt.clone(),
                request.parameters.clone(),
                Priority::Normal,
            )
            .await;

        let (_id, rx) = match enqueue_result {
            Ok(r) => r,
            Err(e) => return InferenceResponse::error_coded(
                request.request_id,
                e.to_string(),
                InferenceErrorCode::AdmissionRejected,
            ),
        };

        // Await the worker's response — classify the error code from message content.
        match rx.await {
            Ok(Ok(result)) => InferenceResponse::success(
                request.request_id,
                result.output,
                result.tokens_generated,
                result.finished,
            ),
            Ok(Err(e)) => {
                let code = classify_worker_error(&e);
                InferenceResponse::error_coded(request.request_id, e, code)
            }
            Err(_) => InferenceResponse::error_coded(
                request.request_id,
                "worker dropped response channel".into(),
                InferenceErrorCode::ExecutionFailed,
            ),
        }
    }

    async fn handle_warmup(&self, model_id: String, _tokens: usize) -> WarmupResponse {
        let start = std::time::Instant::now();

        // Real warmup: 1-token inference through the queue
        let warmup_params = crate::engine::InferenceParams {
            max_tokens: 1,
            ..Default::default()
        };

        let enqueue_result = self
            .queue
            .enqueue_with_response(
                model_id.clone(),
                "Hello".to_string(),
                warmup_params,
                Priority::Low,
            )
            .await;

        let rx = match enqueue_result {
            Ok((_id, rx)) => rx,
            Err(e) => {
                let elapsed = start.elapsed().as_millis() as u64;
                return WarmupResponse::error(model_id, e.to_string(), elapsed);
            }
        };

        // Await actual inference result
        let elapsed_ms = match rx.await {
            Ok(Ok(_)) => start.elapsed().as_millis() as u64,
            Ok(Err(e)) => {
                let elapsed = start.elapsed().as_millis() as u64;
                return WarmupResponse::error(model_id, e, elapsed);
            }
            Err(_) => {
                let elapsed = start.elapsed().as_millis() as u64;
                return WarmupResponse::error(model_id, "worker unavailable".into(), elapsed);
            }
        };

        WarmupResponse::success(model_id, elapsed_ms)
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

    /// Process streaming inference request. Sends token chunks via sender.
    ///
    /// Creates a token stream channel, spawns inference on a blocking task,
    /// and relays tokens to the client until completion or cancellation.
    #[allow(unused_variables)]
    pub async fn process_streaming(
        &self,
        request: InferenceRequest,
        session: &SessionToken,
        sender: &dyn StreamSender,
        cancel: CancellationToken,
    ) -> Result<(), HandlerError> {
        self.auth.validate(session).await?;
        let _guard = self.shutdown.track().ok_or(HandlerError::ShuttingDown)?;

        if let Err(e) = request.validate() {
            let chunk = StreamChunk::error(request.request_id, e.to_string());
            sender.send(IpcMessage::StreamChunk(chunk)).await?;
            return Ok(());
        }

        // Streaming requires gguf feature
        #[cfg(not(feature = "gguf"))]
        {
            let chunk = StreamChunk::error(
                request.request_id,
                "Streaming requires GGUF feature. Rebuild with --features gguf.".into(),
            );
            sender.send(IpcMessage::StreamChunk(chunk)).await?;
            return Ok(());
        }

        #[cfg(feature = "gguf")]
        {
            self.run_streaming_inference(request, sender, cancel).await
        }
    }

    /// Internal streaming implementation (gguf feature only).
    ///
    /// Acquires a queue admission slot before streaming, ensuring the
    /// streaming path respects queue capacity (C-2 fix).
    #[cfg(feature = "gguf")]
    async fn run_streaming_inference(
        &self,
        request: InferenceRequest,
        sender: &dyn StreamSender,
        cancel: CancellationToken,
    ) -> Result<(), HandlerError> {
        let request_id = request.request_id;

        // Queue admission: capacity + tier-1 context check
        let _admission = self
            .queue
            .admit_streaming(&request.prompt)
            .await
            .map_err(|e| HandlerError::QueueFull(e.to_string()))?;

        let model_id = request.model_id.clone();
        let prompt = request.prompt.clone();
        let config = request.parameters.to_config();
        let engine = Arc::clone(&self.inference_engine);

        let (token_sender, mut stream) = TokenStream::new(32);

        let inf_handle = tokio::task::spawn_blocking(move || {
            engine.run_stream_sync(&model_id, &prompt, &config, token_sender)
        });

        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    let chunk = StreamChunk::error(request_id, "cancelled".into());
                    let _ = sender.send(IpcMessage::StreamChunk(chunk)).await;
                    break;
                }
                token_opt = stream.next() => {
                    match token_opt {
                        Some(output) => {
                            let chunk = if output.is_final {
                                StreamChunk::final_token(request_id, output.token)
                            } else {
                                StreamChunk::token(request_id, output.token)
                            };
                            sender.send(IpcMessage::StreamChunk(chunk)).await?;
                            if output.is_final {
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }

        let _ = inf_handle.await;
        // _admission guard drops here, releasing the slot
        Ok(())
    }
}

/// Map a worker error string to a structured `InferenceErrorCode`.
///
/// The worker stringifies `engine::InferenceError` variants. We classify by
/// matching known prefixes so callers can distinguish admission rejections
/// (retriable) from execution failures (not retriable without change).
fn classify_worker_error(msg: &str) -> InferenceErrorCode {
    if msg.contains("Memory limit exceeded") || msg.contains("queue full") || msg.contains("Queue full") {
        InferenceErrorCode::AdmissionRejected
    } else if msg.contains("Model not loaded") {
        InferenceErrorCode::ModelNotLoaded
    } else {
        InferenceErrorCode::ExecutionFailed
    }
}
