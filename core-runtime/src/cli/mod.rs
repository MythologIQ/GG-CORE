// Copyright 2024-2026 GG-CORE Contributors
// SPDX-License-Identifier: Apache-2.0

//! CLI module for GG-CORE runtime commands.
//!
//! Provides subcommands for health checks via IPC, enabling K8s exec probes
//! without requiring HTTP endpoints. Maintains the Alcatraz principle.
//!
//! ## Usage
//!
//! ```bash
//! GG-CORE health   # Full health check, exits 0 on healthy
//! GG-CORE live     # Liveness probe, exits 0 if alive
//! GG-CORE ready    # Readiness probe, exits 0 if ready
//! GG-CORE status   # Show system status and statistics
//! ```

pub mod config_cmd;
pub mod health;
pub mod ipc_client;
pub mod models_cmd;
pub mod status;
pub mod status_format;

pub use health::{run_health, run_liveness, run_readiness};
pub use ipc_client::{CliError, CliIpcClient};
pub use status::{run_status, SystemStatus};

/// Default socket path for IPC communication.
#[cfg(unix)]
pub const DEFAULT_SOCKET_PATH: &str = "/var/run/gg-core/GG-CORE.sock";

#[cfg(windows)]
pub const DEFAULT_SOCKET_PATH: &str = r"\\.\pipe\GG-CORE";

/// Get socket path from environment or use default.
pub fn get_socket_path() -> String {
    std::env::var("GG_CORE_SOCKET_PATH").unwrap_or_else(|_| DEFAULT_SOCKET_PATH.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_socket_path_unix() {
        #[cfg(unix)]
        assert_eq!(DEFAULT_SOCKET_PATH, "/var/run/gg-core/GG-CORE.sock");
    }

    #[test]
    fn test_default_socket_path_windows() {
        #[cfg(windows)]
        assert_eq!(DEFAULT_SOCKET_PATH, r"\\.\pipe\GG-CORE");
    }

    #[test]
    fn test_get_socket_path_default() {
        // Clear env var if set
        std::env::remove_var("GG_CORE_SOCKET_PATH");
        let path = get_socket_path();
        assert_eq!(path, DEFAULT_SOCKET_PATH);
    }

    #[test]
    fn test_get_socket_path_from_env() {
        let custom_path = "/custom/socket.sock";
        std::env::set_var("GG_CORE_SOCKET_PATH", custom_path);
        let path = get_socket_path();
        assert_eq!(path, custom_path);
        // Clean up
        std::env::remove_var("GG_CORE_SOCKET_PATH");
    }

    #[test]
    fn test_module_exports() {
        // Verify public exports are accessible
        let _ = get_socket_path();
        // Type checks for exported items
        fn _check_exports() {
            let _: fn(&str) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + Send>> =
                |_| Box::pin(async { 0 });
        }
    }
}
