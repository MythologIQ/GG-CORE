// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! IPC client for CLI commands.
//!
//! Connects to running Veritas SDR instance via named pipe/Unix socket
//! to perform health checks and other operations.

use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

use crate::ipc::protocol::{
    decode_message, encode_message, HealthCheckResponse, HealthCheckType, IpcMessage,
    ModelsListResponse,
};
use crate::telemetry::MetricsSnapshot;

/// CLI client errors.
#[derive(Error, Debug)]
pub enum CliError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Timeout waiting for response")]
    Timeout,

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Health check returned unhealthy")]
    Unhealthy,
}

/// IPC client for CLI health probe commands.
pub struct CliIpcClient {
    socket_path: String,
    timeout_duration: Duration,
}

impl CliIpcClient {
    /// Create a new CLI IPC client.
    pub fn new(socket_path: String) -> Self {
        Self {
            socket_path,
            timeout_duration: Duration::from_secs(5),
        }
    }

    /// Set custom timeout duration.
    pub fn with_timeout(mut self, duration: Duration) -> Self {
        self.timeout_duration = duration;
        self
    }

    /// Perform a health check via IPC.
    pub async fn check_health(&self, check_type: HealthCheckType) -> Result<bool, CliError> {
        let response = self.send_health_request(check_type).await?;
        Ok(response.ok)
    }

    /// Get full health report via IPC.
    pub async fn get_health_report(&self) -> Result<HealthCheckResponse, CliError> {
        self.send_health_request(HealthCheckType::Full).await
    }

    /// Get metrics snapshot via IPC.
    pub async fn get_metrics(&self) -> Result<MetricsSnapshot, CliError> {
        let message = IpcMessage::MetricsRequest;
        let request_bytes =
            encode_message(&message).map_err(|e| CliError::Protocol(e.to_string()))?;

        let response_bytes = self.send_receive(&request_bytes).await?;

        let response =
            decode_message(&response_bytes).map_err(|e| CliError::Protocol(e.to_string()))?;

        match response {
            IpcMessage::MetricsResponse(snapshot) => Ok(snapshot),
            IpcMessage::Error { message, .. } => Err(CliError::Protocol(message)),
            _ => Err(CliError::Protocol("Unexpected response type".to_string())),
        }
    }

    /// Get loaded models list via IPC.
    pub async fn get_models(&self) -> Result<ModelsListResponse, CliError> {
        let message = IpcMessage::ModelsRequest;
        let request_bytes =
            encode_message(&message).map_err(|e| CliError::Protocol(e.to_string()))?;

        let response_bytes = self.send_receive(&request_bytes).await?;

        let response =
            decode_message(&response_bytes).map_err(|e| CliError::Protocol(e.to_string()))?;

        match response {
            IpcMessage::ModelsResponse(models) => Ok(models),
            IpcMessage::Error { message, .. } => Err(CliError::Protocol(message)),
            _ => Err(CliError::Protocol("Unexpected response type".to_string())),
        }
    }

    async fn send_health_request(
        &self,
        check_type: HealthCheckType,
    ) -> Result<HealthCheckResponse, CliError> {
        let message = IpcMessage::HealthCheck { check_type };
        let request_bytes =
            encode_message(&message).map_err(|e| CliError::Protocol(e.to_string()))?;

        let response_bytes = self.send_receive(&request_bytes).await?;

        let response =
            decode_message(&response_bytes).map_err(|e| CliError::Protocol(e.to_string()))?;

        match response {
            IpcMessage::HealthResponse(health_response) => Ok(health_response),
            IpcMessage::Error { message, .. } => Err(CliError::Protocol(message)),
            _ => Err(CliError::Protocol("Unexpected response type".to_string())),
        }
    }

    #[cfg(unix)]
    async fn send_receive(&self, request: &[u8]) -> Result<Vec<u8>, CliError> {
        use tokio::net::UnixStream;

        let connect_future = UnixStream::connect(&self.socket_path);
        let mut stream = timeout(self.timeout_duration, connect_future)
            .await
            .map_err(|_| CliError::Timeout)?
            .map_err(|e| CliError::ConnectionFailed(e.to_string()))?;

        self.exchange_data(&mut stream, request).await
    }

    #[cfg(windows)]
    async fn send_receive(&self, request: &[u8]) -> Result<Vec<u8>, CliError> {
        use tokio::net::windows::named_pipe::ClientOptions;

        let connect_future = ClientOptions::new().open(&self.socket_path);
        let mut pipe = timeout(self.timeout_duration, async { connect_future })
            .await
            .map_err(|_| CliError::Timeout)?
            .map_err(|e| CliError::ConnectionFailed(e.to_string()))?;

        self.exchange_data(&mut pipe, request).await
    }

    async fn exchange_data<S>(&self, stream: &mut S, request: &[u8]) -> Result<Vec<u8>, CliError>
    where
        S: AsyncReadExt + AsyncWriteExt + Unpin,
    {
        // Send length-prefixed request
        let len = request.len() as u32;
        stream.write_all(&len.to_le_bytes()).await?;
        stream.write_all(request).await?;
        stream.flush().await?;

        // Read length-prefixed response
        let mut len_buf = [0u8; 4];
        let read_future = stream.read_exact(&mut len_buf);
        timeout(self.timeout_duration, read_future)
            .await
            .map_err(|_| CliError::Timeout)??;

        let response_len = u32::from_le_bytes(len_buf) as usize;
        if response_len > 16 * 1024 * 1024 {
            return Err(CliError::Protocol("Response too large".to_string()));
        }

        let mut response = vec![0u8; response_len];
        stream.read_exact(&mut response).await?;

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_ipc_client_creation() {
        let client = CliIpcClient::new("/test/socket".to_string());
        assert_eq!(client.socket_path, "/test/socket");
        assert_eq!(client.timeout_duration, Duration::from_secs(5));
    }

    #[test]
    fn test_cli_ipc_client_with_timeout() {
        let client = CliIpcClient::new("/test/socket".to_string())
            .with_timeout(Duration::from_secs(10));
        assert_eq!(client.timeout_duration, Duration::from_secs(10));
    }

    #[test]
    fn test_cli_ipc_client_custom_timeout_chaining() {
        let client = CliIpcClient::new("/socket".to_string())
            .with_timeout(Duration::from_millis(500))
            .with_timeout(Duration::from_secs(30)); // Override
        assert_eq!(client.timeout_duration, Duration::from_secs(30));
    }

    #[test]
    fn test_cli_error_display() {
        let err = CliError::ConnectionFailed("refused".to_string());
        assert!(err.to_string().contains("refused"));

        let err = CliError::Timeout;
        assert!(err.to_string().contains("Timeout"));

        let err = CliError::Protocol("invalid".to_string());
        assert!(err.to_string().contains("invalid"));

        let err = CliError::Unhealthy;
        assert!(err.to_string().contains("unhealthy"));
    }

    #[test]
    fn test_cli_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let cli_err: CliError = io_err.into();
        assert!(matches!(cli_err, CliError::Io(_)));
    }

    #[tokio::test]
    async fn test_check_health_connection_failure() {
        let client = CliIpcClient::new("/nonexistent/socket".to_string());
        let result = client.check_health(HealthCheckType::Full).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_check_health_liveness_connection_failure() {
        let client = CliIpcClient::new("/nonexistent/socket".to_string());
        let result = client.check_health(HealthCheckType::Liveness).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_check_health_readiness_connection_failure() {
        let client = CliIpcClient::new("/nonexistent/socket".to_string());
        let result = client.check_health(HealthCheckType::Readiness).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_health_report_connection_failure() {
        let client = CliIpcClient::new("/nonexistent/socket".to_string());
        let result = client.get_health_report().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_client_with_short_timeout() {
        let client = CliIpcClient::new("/nonexistent/socket".to_string())
            .with_timeout(Duration::from_millis(1));
        let result = client.check_health(HealthCheckType::Full).await;
        // Should fail quickly due to short timeout or connection refused
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_error_variants() {
        // Ensure all error variants are constructible
        let errors: Vec<CliError> = vec![
            CliError::ConnectionFailed("test".into()),
            CliError::Timeout,
            CliError::Protocol("test".into()),
            CliError::Io(std::io::Error::new(std::io::ErrorKind::Other, "test")),
            CliError::Unhealthy,
        ];

        for err in errors {
            // All errors should have a displayable message
            assert!(!err.to_string().is_empty());
        }
    }
}
