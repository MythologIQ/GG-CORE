//! Integration tests for the CLI module.
//!
//! Tests the CLI health probe commands for Kubernetes exec probes.
//! These tests verify CLI logic without requiring actual IPC connections.

use std::time::Duration;

use veritas_sdr::cli::{
    get_socket_path, run_health, run_liveness, run_readiness, CliError, CliIpcClient,
    DEFAULT_SOCKET_PATH,
};
use veritas_sdr::ipc::protocol::HealthCheckType;

// ============================================================================
// Socket Path Configuration Tests
// ============================================================================

#[test]
fn test_default_socket_path_platform_specific() {
    #[cfg(unix)]
    assert_eq!(DEFAULT_SOCKET_PATH, "/var/run/veritas/veritas-sdr.sock");

    #[cfg(windows)]
    assert_eq!(DEFAULT_SOCKET_PATH, r"\\.\pipe\veritas-sdr");
}

#[test]
fn test_get_socket_path_uses_default_when_env_not_set() {
    // Clear environment variable if set
    std::env::remove_var("VERITAS_SOCKET_PATH");
    let path = get_socket_path();
    assert_eq!(path, DEFAULT_SOCKET_PATH);
}

#[test]
fn test_get_socket_path_respects_environment_variable() {
    let custom_path = "/custom/path/to/socket.sock";
    std::env::set_var("VERITAS_SOCKET_PATH", custom_path);
    let path = get_socket_path();
    assert_eq!(path, custom_path);
    // Clean up
    std::env::remove_var("VERITAS_SOCKET_PATH");
}

#[test]
fn test_get_socket_path_handles_empty_env_var() {
    // Empty env var should still be used (not fallback to default)
    std::env::set_var("VERITAS_SOCKET_PATH", "");
    let path = get_socket_path();
    assert_eq!(path, "");
    std::env::remove_var("VERITAS_SOCKET_PATH");
}

// ============================================================================
// CliIpcClient Creation Tests
// ============================================================================

#[test]
fn test_cli_ipc_client_creation_with_path() {
    let client = CliIpcClient::new("/test/socket/path".to_string());
    // Client should be created without error
    // Internal state is verified through behavior
    drop(client);
}

#[test]
fn test_cli_ipc_client_with_custom_timeout() {
    let client = CliIpcClient::new("/socket".to_string())
        .with_timeout(Duration::from_secs(30));
    // Verify timeout is configurable (behavior test)
    drop(client);
}

#[test]
fn test_cli_ipc_client_timeout_chaining() {
    let client = CliIpcClient::new("/socket".to_string())
        .with_timeout(Duration::from_millis(100))
        .with_timeout(Duration::from_secs(60)); // Override previous
    drop(client);
}

#[test]
fn test_cli_ipc_client_accepts_various_path_formats() {
    // Unix-style path
    let _ = CliIpcClient::new("/var/run/app.sock".to_string());

    // Windows named pipe
    let _ = CliIpcClient::new(r"\\.\pipe\my-pipe".to_string());

    // Abstract socket (Linux)
    let _ = CliIpcClient::new("@abstract-socket".to_string());

    // Relative path (unusual but valid)
    let _ = CliIpcClient::new("./local.sock".to_string());
}

// ============================================================================
// CliError Tests
// ============================================================================

#[test]
fn test_cli_error_connection_failed_display() {
    let error = CliError::ConnectionFailed("Connection refused".to_string());
    let display = error.to_string();
    assert!(display.contains("Connection"));
    assert!(display.contains("refused"));
}

#[test]
fn test_cli_error_timeout_display() {
    let error = CliError::Timeout;
    let display = error.to_string();
    assert!(display.contains("Timeout") || display.contains("timeout"));
}

#[test]
fn test_cli_error_protocol_display() {
    let error = CliError::Protocol("Invalid message format".to_string());
    let display = error.to_string();
    assert!(display.contains("Invalid") || display.contains("Protocol"));
}

#[test]
fn test_cli_error_io_from_std_io_error() {
    let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let cli_error: CliError = io_error.into();
    assert!(matches!(cli_error, CliError::Io(_)));
}

#[test]
fn test_cli_error_unhealthy_display() {
    let error = CliError::Unhealthy;
    let display = error.to_string();
    assert!(display.contains("unhealthy") || display.contains("Unhealthy"));
}

#[test]
fn test_cli_error_all_variants_have_display() {
    let errors: Vec<CliError> = vec![
        CliError::ConnectionFailed("test".to_string()),
        CliError::Timeout,
        CliError::Protocol("test".to_string()),
        CliError::Io(std::io::Error::new(std::io::ErrorKind::Other, "test")),
        CliError::Unhealthy,
    ];

    for error in errors {
        let display = error.to_string();
        assert!(!display.is_empty(), "Error variant should have display text");
    }
}

// ============================================================================
// Exit Code Tests
// ============================================================================

#[test]
fn test_exit_codes_follow_unix_convention() {
    use veritas_sdr::cli::health::{EXIT_HEALTHY, EXIT_UNHEALTHY};

    // Unix convention: 0 = success
    assert_eq!(EXIT_HEALTHY, 0);

    // Non-zero = failure
    assert_ne!(EXIT_UNHEALTHY, 0);
    assert_eq!(EXIT_UNHEALTHY, 1);
}

// ============================================================================
// Health Check Connection Failure Tests (Async)
// ============================================================================

#[tokio::test]
async fn test_run_health_returns_unhealthy_on_connection_failure() {
    let result = run_health("/nonexistent/socket/path.sock").await;
    assert_eq!(result, 1); // EXIT_UNHEALTHY
}

#[tokio::test]
async fn test_run_liveness_returns_unhealthy_on_connection_failure() {
    let result = run_liveness("/nonexistent/socket/path.sock").await;
    assert_eq!(result, 1); // EXIT_UNHEALTHY
}

#[tokio::test]
async fn test_run_readiness_returns_unhealthy_on_connection_failure() {
    let result = run_readiness("/nonexistent/socket/path.sock").await;
    assert_eq!(result, 1); // EXIT_UNHEALTHY
}

#[tokio::test]
async fn test_all_health_checks_fail_consistently_on_invalid_socket() {
    let invalid_socket = "/definitely/not/a/real/socket.sock";

    let health = run_health(invalid_socket).await;
    let live = run_liveness(invalid_socket).await;
    let ready = run_readiness(invalid_socket).await;

    // All should return the same exit code for connection failure
    assert_eq!(health, live);
    assert_eq!(live, ready);
    assert_eq!(health, 1);
}

#[tokio::test]
async fn test_cli_client_check_health_connection_failure() {
    let client = CliIpcClient::new("/nonexistent/socket".to_string());
    let result = client.check_health(HealthCheckType::Full).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_cli_client_check_liveness_connection_failure() {
    let client = CliIpcClient::new("/nonexistent/socket".to_string());
    let result = client.check_health(HealthCheckType::Liveness).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_cli_client_check_readiness_connection_failure() {
    let client = CliIpcClient::new("/nonexistent/socket".to_string());
    let result = client.check_health(HealthCheckType::Readiness).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_cli_client_get_health_report_connection_failure() {
    let client = CliIpcClient::new("/nonexistent/socket".to_string());
    let result = client.get_health_report().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_cli_client_with_short_timeout_fails_quickly() {
    let client = CliIpcClient::new("/nonexistent/socket".to_string())
        .with_timeout(Duration::from_millis(1));

    let start = std::time::Instant::now();
    let _ = client.check_health(HealthCheckType::Full).await;
    let elapsed = start.elapsed();

    // Should fail quickly due to short timeout or immediate connection error
    assert!(elapsed < Duration::from_secs(5));
}

// ============================================================================
// HealthCheckType Tests
// ============================================================================

#[test]
fn test_health_check_type_full() {
    let check_type = HealthCheckType::Full;
    assert!(matches!(check_type, HealthCheckType::Full));
}

#[test]
fn test_health_check_type_liveness() {
    let check_type = HealthCheckType::Liveness;
    assert!(matches!(check_type, HealthCheckType::Liveness));
}

#[test]
fn test_health_check_type_readiness() {
    let check_type = HealthCheckType::Readiness;
    assert!(matches!(check_type, HealthCheckType::Readiness));
}

#[test]
fn test_health_check_type_equality() {
    assert_eq!(HealthCheckType::Full, HealthCheckType::Full);
    assert_eq!(HealthCheckType::Liveness, HealthCheckType::Liveness);
    assert_eq!(HealthCheckType::Readiness, HealthCheckType::Readiness);

    assert_ne!(HealthCheckType::Full, HealthCheckType::Liveness);
    assert_ne!(HealthCheckType::Liveness, HealthCheckType::Readiness);
    assert_ne!(HealthCheckType::Readiness, HealthCheckType::Full);
}

// ============================================================================
// Verbose Health Check Tests
// ============================================================================

#[tokio::test]
async fn test_run_health_verbose_connection_failure() {
    use veritas_sdr::cli::health::run_health_verbose;

    let result = run_health_verbose("/nonexistent/socket.sock").await;
    assert_eq!(result, 1); // EXIT_UNHEALTHY
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_socket_path_with_special_characters() {
    // Paths with spaces
    let _ = CliIpcClient::new("/path with spaces/socket.sock".to_string());

    // Paths with unicode
    let _ = CliIpcClient::new("/path/socket.sock".to_string());

    // Very long path
    let long_path = "/".to_string() + &"a".repeat(255) + ".sock";
    let _ = CliIpcClient::new(long_path);
}

#[tokio::test]
async fn test_concurrent_health_checks_on_invalid_socket() {
    let socket = "/invalid/concurrent/socket.sock";

    let handles: Vec<_> = (0..5)
        .map(|_| {
            let socket = socket.to_string();
            tokio::spawn(async move { run_health(&socket).await })
        })
        .collect();

    for handle in handles {
        let result = handle.await.unwrap();
        assert_eq!(result, 1); // All should fail with EXIT_UNHEALTHY
    }
}
