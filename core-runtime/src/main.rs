//! Veritas SDR Runtime entry point.
//!
//! Bootstraps the sandboxed inference engine with:
//! - FIPS 140-3 power-on self-tests (fail-fast)
//! - Configuration loading
//! - IPC listener setup
//! - Signal handling for graceful shutdown
//!
//! ## CLI Subcommands
//!
//! - `veritas-sdr` or `veritas-sdr serve` - Run IPC server (default)
//! - `veritas-sdr health` - Full health check (exit 0/1)
//! - `veritas-sdr live` - Liveness probe (exit 0/1)
//! - `veritas-sdr ready` - Readiness probe (exit 0/1)

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use veritas_sdr::cli::{get_socket_path, run_health, run_liveness, run_readiness, run_status};
use veritas_sdr::ipc::server;
use veritas_sdr::security::fips_tests;
use veritas_sdr::shutdown::ShutdownResult;
use veritas_sdr::{Runtime, RuntimeConfig};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("serve");

    match command {
        "serve" | "" => {
            // FIPS 140-3 power-on self-tests (fail-fast)
            if let Err(e) = fips_tests::run_power_on_self_tests() {
                eprintln!("FIPS self-test FAILED: {}", e);
                eprintln!("Cryptographic operations disabled. Aborting startup.");
                return ExitCode::FAILURE;
            }
            eprintln!("FIPS 140-3 self-tests: PASSED");

            let config = load_config();
            let runtime = Runtime::new(config);
            match run_ipc_server(runtime).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("Server error: {}", e);
                    ExitCode::FAILURE
                }
            }
        }
        "health" => {
            let socket_path = get_socket_path();
            let code = run_health(&socket_path).await;
            ExitCode::from(code as u8)
        }
        "live" | "liveness" => {
            let socket_path = get_socket_path();
            let code = run_liveness(&socket_path).await;
            ExitCode::from(code as u8)
        }
        "ready" | "readiness" => {
            let socket_path = get_socket_path();
            let code = run_readiness(&socket_path).await;
            ExitCode::from(code as u8)
        }
        "help" | "--help" | "-h" => {
            // Check if help is requested for a specific command
            if let Some(subcommand) = args.get(2) {
                print_command_help(subcommand);
            } else {
                print_usage();
            }
            ExitCode::SUCCESS
        }
        "version" | "--version" | "-V" => {
            println!("veritas-sdr {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "status" => {
            let socket_path = get_socket_path();
            let json_output = args.get(2).map(|s| s.as_str()) == Some("--json");
            let code = run_status(&socket_path, json_output).await;
            ExitCode::from(code as u8)
        }
        "verify" => {
            // TODO: Implement verify command
            eprintln!(
                "Verify command not yet implemented. Use 'veritas-sdr health' for health checks."
            );
            ExitCode::from(2u8)
        }
        "models" => {
            let subcommand = args.get(2).map(|s| s.as_str()).unwrap_or("list");
            match subcommand {
                "list" => {
                    eprintln!("Models list not yet implemented.");
                    ExitCode::from(2u8)
                }
                _ => {
                    eprintln!("Unknown models subcommand: {}", subcommand);
                    print_command_help("models");
                    ExitCode::FAILURE
                }
            }
        }
        "config" => {
            let subcommand = args.get(2).map(|s| s.as_str()).unwrap_or("show");
            match subcommand {
                "show" | "validate" | "defaults" => {
                    eprintln!("Config {} not yet implemented.", subcommand);
                    ExitCode::from(2u8)
                }
                _ => {
                    eprintln!("Unknown config subcommand: {}", subcommand);
                    print_command_help("config");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            print_usage();
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!(
        "Veritas SDR - Secure LLM Inference Runtime v{}

USAGE:
    veritas-sdr [COMMAND] [OPTIONS]

COMMANDS:
    serve        Run the IPC server (default when no command given)
    health       Full health check (exit 0 if healthy, 1 if unhealthy)
    live         Liveness probe for Kubernetes (exit 0 if alive)
    ready        Readiness probe for Kubernetes (exit 0 if ready)
    status       Show system status and statistics
    verify       Verify deployment health and configuration
    models       Manage loaded models (list, load, unload)
    config       Manage configuration (validate, show)
    version      Show version information
    help         Show this help message

OPTIONS:
    -h, --help     Show help for command
    -V, --version  Show version information
    -v, --verbose  Enable verbose output
    --socket PATH  Override IPC socket path

EXAMPLES:
    veritas-sdr                          # Run IPC server (default)
    veritas-sdr serve                    # Explicitly run IPC server
    veritas-sdr health                   # Full health check
    veritas-sdr live                     # Liveness probe
    veritas-sdr ready                    # Readiness probe
    veritas-sdr status                   # Show system status
    veritas-sdr models list              # List loaded models
    veritas-sdr config validate          # Validate configuration
    veritas-sdr --socket /custom/path    # Use custom socket path

ENVIRONMENT:
    VERITAS_SOCKET_PATH  IPC socket path (default: /var/run/veritas/veritas-sdr.sock on Unix)
    CORE_AUTH_TOKEN      Authentication token for server mode
    RUST_LOG             Log level (debug, info, warn, error)
    VERITAS_ENV          Environment (development, staging, production)

EXIT CODES:
    0  Success / Healthy
    1  Failure / Unhealthy
    2  Configuration error
    3  Connection error

DOCUMENTATION:
    https://docs.veritas-sdr.io

SUPPORT:
    GitHub Issues: https://github.com/veritas-sdr/core/issues
    Community:     https://slack.veritas-sdr.io
",
        version
    );
}

/// Print detailed help for a specific command.
fn print_command_help(command: &str) {
    match command {
        "serve" => {
            eprintln!(
                "veritas-sdr serve - Run the IPC server

USAGE:
    veritas-sdr serve [OPTIONS]

OPTIONS:
    --socket PATH     Override IPC socket path
    --config FILE     Load configuration from file
    --auth-token TKN  Set authentication token

DESCRIPTION:
    Starts the Veritas SDR IPC server, which handles inference requests
    through inter-process communication. This is the default command when
    no command is specified.

    The server performs FIPS 140-3 power-on self-tests before starting
    and will fail-fast if any cryptographic self-test fails.

EXAMPLES:
    veritas-sdr serve
    veritas-sdr serve --socket /custom/veritas.sock
    veritas-sdr serve --config /etc/veritas/config.toml
"
            );
        }
        "health" => {
            eprintln!(
                "veritas-sdr health - Full health check

USAGE:
    veritas-sdr health [OPTIONS]

OPTIONS:
    --socket PATH  Override IPC socket path
    --timeout SEC  Connection timeout in seconds (default: 5)
    --json         Output in JSON format

DESCRIPTION:
    Performs a comprehensive health check of the Veritas SDR runtime.
    Checks model loading, memory status, and inference capability.

EXIT CODES:
    0  System is healthy
    1  System is unhealthy
    3  Connection error

EXAMPLES:
    veritas-sdr health
    veritas-sdr health --json
    veritas-sdr health --timeout 10
"
            );
        }
        "live" | "liveness" => {
            eprintln!(
                "veritas-sdr live - Liveness probe

USAGE:
    veritas-sdr live [OPTIONS]

OPTIONS:
    --socket PATH  Override IPC socket path

DESCRIPTION:
    Kubernetes liveness probe. Returns success (0) if the process is alive
    and responsive to IPC requests. Use for kubelet livenessProbe.

EXIT CODES:
    0  Process is alive
    1  Process is unresponsive
    3  Connection error

KUBERNETES USAGE:
    livenessProbe:
      exec:
        command: [veritas-sdr, live]
      initialDelaySeconds: 30
      periodSeconds: 10
"
            );
        }
        "ready" | "readiness" => {
            eprintln!(
                "veritas-sdr ready - Readiness probe

USAGE:
    veritas-sdr ready [OPTIONS]

OPTIONS:
    --socket PATH  Override IPC socket path

DESCRIPTION:
    Kubernetes readiness probe. Returns success (0) if the runtime is
    ready to accept inference requests (models loaded, warmed up).
    Use for kubelet readinessProbe.

EXIT CODES:
    0  Ready to serve traffic
    1  Not ready (still loading models, warming up)
    3  Connection error

KUBERNETES USAGE:
    readinessProbe:
      exec:
        command: [veritas-sdr, ready]
      initialDelaySeconds: 60
      periodSeconds: 5
"
            );
        }
        "status" => {
            eprintln!(
                "veritas-sdr status - Show system status

USAGE:
    veritas-sdr status [OPTIONS]

OPTIONS:
    --socket PATH  Override IPC socket path
    --json         Output in JSON format
    --watch        Continuously update status

DESCRIPTION:
    Displays current system status including:
    - Health state
    - Loaded models
    - Request statistics
    - Resource utilization
    - Recent events

EXAMPLES:
    veritas-sdr status
    veritas-sdr status --json
    veritas-sdr status --watch
"
            );
        }
        "verify" => {
            eprintln!(
                "veritas-sdr verify - Verify deployment

USAGE:
    veritas-sdr verify [OPTIONS]

OPTIONS:
    --socket PATH  Override IPC socket path
    --all          Run all verification checks
    --quick        Run quick verification only

DESCRIPTION:
    Verifies deployment health and configuration. Checks:
    - IPC connectivity
    - Model availability
    - Configuration validity
    - Security settings

EXIT CODES:
    0  All checks passed
    1  One or more checks failed

EXAMPLES:
    veritas-sdr verify
    veritas-sdr verify --all
"
            );
        }
        "models" => {
            eprintln!(
                "veritas-sdr models - Manage models

USAGE:
    veritas-sdr models <SUBCOMMAND> [OPTIONS]

SUBCOMMANDS:
    list           List loaded models
    load <NAME>    Load a model
    unload <NAME>  Unload a model
    info <NAME>    Show model information

OPTIONS:
    --socket PATH  Override IPC socket path
    --json         Output in JSON format

EXAMPLES:
    veritas-sdr models list
    veritas-sdr models load llama-2-7b-chat
    veritas-sdr models info llama-2-7b-chat
    veritas-sdr models unload llama-2-7b-chat
"
            );
        }
        "config" => {
            eprintln!(
                "veritas-sdr config - Manage configuration

USAGE:
    veritas-sdr config <SUBCOMMAND> [OPTIONS]

SUBCOMMANDS:
    show           Show current configuration
    validate       Validate configuration file
    defaults       Show default configuration

OPTIONS:
    --socket PATH  Override IPC socket path
    --file PATH    Configuration file path

EXAMPLES:
    veritas-sdr config show
    veritas-sdr config validate --file values.yaml
    veritas-sdr config defaults
"
            );
        }
        _ => {
            eprintln!(
                "No detailed help available for '{}'. Use 'veritas-sdr help' for general usage.",
                command
            );
        }
    }
}

fn load_config() -> RuntimeConfig {
    // In production, load from environment or config file
    // For now, use secure defaults
    RuntimeConfig {
        base_path: PathBuf::from("."),
        auth_token: std::env::var("CORE_AUTH_TOKEN").unwrap_or_default(),
        session_timeout: Duration::from_secs(3600),
        max_context_length: 4096,
        ..Default::default()
    }
}

async fn run_ipc_server(runtime: Runtime) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = get_socket_path();
    let handler = std::sync::Arc::new(runtime.ipc_handler);
    let connections = runtime.connections;
    let shutdown = runtime.shutdown;
    let shutdown_timeout = runtime.config.shutdown_timeout;

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let server_handle = tokio::spawn(server::run_server(
        socket_path,
        handler,
        connections,
        shutdown_rx,
    ));

    // Wait for Ctrl+C, then initiate graceful shutdown
    tokio::signal::ctrl_c().await?;
    eprintln!("Shutdown signal received, draining...");

    // Signal the server loop to stop accepting
    let _ = shutdown_tx.send(true);

    // Drain in-flight requests
    match shutdown.initiate(shutdown_timeout).await {
        ShutdownResult::Complete => eprintln!("Shutdown complete"),
        ShutdownResult::Timeout { remaining } => {
            eprintln!("Shutdown timeout, {} requests remaining", remaining);
        }
    }

    // Wait for server task to finish
    if let Err(e) = server_handle.await? {
        eprintln!("Server error: {}", e);
    }

    Ok(())
}
