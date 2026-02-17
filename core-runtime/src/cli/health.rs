// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Health check CLI commands for K8s exec probes.
//!
//! Provides exit-code based health checks for Kubernetes:
//! - Exit 0: Healthy/alive/ready
//! - Exit 1: Unhealthy/dead/not ready

use crate::ipc::protocol::HealthCheckType;

use super::ipc_client::CliIpcClient;

/// Exit codes for health probes.
pub const EXIT_HEALTHY: i32 = 0;
pub const EXIT_UNHEALTHY: i32 = 1;

/// Run full health check - exits 0 on healthy, 1 on unhealthy.
pub async fn run_health(socket_path: &str) -> i32 {
    run_check(socket_path, HealthCheckType::Full, "health").await
}

/// Run liveness probe - exits 0 if alive, 1 if dead.
pub async fn run_liveness(socket_path: &str) -> i32 {
    run_check(socket_path, HealthCheckType::Liveness, "liveness").await
}

/// Run readiness probe - exits 0 if ready, 1 if not ready.
pub async fn run_readiness(socket_path: &str) -> i32 {
    run_check(socket_path, HealthCheckType::Readiness, "readiness").await
}

async fn run_check(socket_path: &str, check_type: HealthCheckType, name: &str) -> i32 {
    let client = CliIpcClient::new(socket_path.to_string());

    match client.check_health(check_type).await {
        Ok(true) => {
            eprintln!("{} check: OK", name);
            EXIT_HEALTHY
        }
        Ok(false) => {
            eprintln!("{} check: FAILED", name);
            EXIT_UNHEALTHY
        }
        Err(e) => {
            eprintln!("{} check error: {}", name, e);
            EXIT_UNHEALTHY
        }
    }
}

/// Print detailed health report (for debugging).
pub async fn run_health_verbose(socket_path: &str) -> i32 {
    let client = CliIpcClient::new(socket_path.to_string());

    match client.get_health_report().await {
        Ok(report) => {
            if let Some(ref health) = report.report {
                eprintln!("Health Report:");
                eprintln!("  State: {:?}", health.state);
                eprintln!("  Ready: {}", health.ready);
                eprintln!("  Accepting: {}", health.accepting_requests);
                eprintln!("  Models: {}", health.models_loaded);
                eprintln!("  Memory: {} bytes", health.memory_used_bytes);
                eprintln!("  Queue: {}", health.queue_depth);
                eprintln!("  Uptime: {}s", health.uptime_secs);
            }
            if report.ok {
                EXIT_HEALTHY
            } else {
                EXIT_UNHEALTHY
            }
        }
        Err(e) => {
            eprintln!("Health check error: {}", e);
            EXIT_UNHEALTHY
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_codes() {
        assert_eq!(EXIT_HEALTHY, 0);
        assert_eq!(EXIT_UNHEALTHY, 1);
    }

    #[test]
    fn test_exit_code_values_are_standard() {
        // Standard Unix convention: 0 = success, non-zero = failure
        assert!(EXIT_HEALTHY == 0, "Healthy exit code should be 0");
        assert!(EXIT_UNHEALTHY != 0, "Unhealthy exit code should be non-zero");
    }

    #[tokio::test]
    async fn test_run_health_connection_failure() {
        // Test with non-existent socket path - should return unhealthy
        let result = run_health("/nonexistent/socket.sock").await;
        assert_eq!(result, EXIT_UNHEALTHY);
    }

    #[tokio::test]
    async fn test_run_liveness_connection_failure() {
        // Test with non-existent socket path - should return unhealthy
        let result = run_liveness("/nonexistent/socket.sock").await;
        assert_eq!(result, EXIT_UNHEALTHY);
    }

    #[tokio::test]
    async fn test_run_readiness_connection_failure() {
        // Test with non-existent socket path - should return unhealthy
        let result = run_readiness("/nonexistent/socket.sock").await;
        assert_eq!(result, EXIT_UNHEALTHY);
    }

    #[tokio::test]
    async fn test_run_health_verbose_connection_failure() {
        // Test with non-existent socket path - should return unhealthy
        let result = run_health_verbose("/nonexistent/socket.sock").await;
        assert_eq!(result, EXIT_UNHEALTHY);
    }

    #[tokio::test]
    async fn test_health_check_types_all_covered() {
        // Verify all health check types map correctly to exit codes on failure
        let socket = "/invalid/test/socket";

        let health = run_health(socket).await;
        let live = run_liveness(socket).await;
        let ready = run_readiness(socket).await;

        // All should fail with connection error
        assert_eq!(health, EXIT_UNHEALTHY);
        assert_eq!(live, EXIT_UNHEALTHY);
        assert_eq!(ready, EXIT_UNHEALTHY);
    }
}
