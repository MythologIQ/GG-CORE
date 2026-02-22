//! GG-CORE Runtime entry point.
//!
//! Bootstraps the sandboxed inference engine with:
//! - FIPS 140-3 power-on self-tests (fail-fast)
//! - Configuration loading
//! - IPC listener setup
//! - Signal handling for graceful shutdown
//!
//! ## CLI Subcommands
//!
//! - `GG-CORE` or `GG-CORE serve` - Run IPC server (default)
//! - `GG-CORE health` - Full health check (exit 0/1)
//! - `GG-CORE live` - Liveness probe (exit 0/1)
//! - `GG-CORE ready` - Readiness probe (exit 0/1)

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use gg_core::cli::{
    get_socket_path, run_health, run_liveness, run_readiness, run_status, CliIpcClient,
};
use gg_core::engine::InferenceParams;
use gg_core::ipc::server;
use gg_core::security::fips_tests;
use gg_core::shutdown::ShutdownResult;
use gg_core::{Runtime, RuntimeConfig};

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
            println!("GG-CORE {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "status" => {
            let socket_path = get_socket_path();
            let json_output = args.get(2).map(|s| s.as_str()) == Some("--json");
            let code = run_status(&socket_path, json_output).await;
            ExitCode::from(code as u8)
        }
        "infer" => {
            let code = run_inference(&args).await;
            ExitCode::from(code as u8)
        }
        "verify" => {
            // TODO: Implement verify command
            eprintln!(
                "Verify command not yet implemented. Use 'GG-CORE health' for health checks."
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
        "GG-CORE - Secure Performance-Accelerated Runtime Kernel v{}

USAGE:
    GG-CORE [COMMAND] [OPTIONS]

COMMANDS:
    serve        Run the IPC server (default when no command given)
    infer        Run inference on a model (supports streaming)
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
    GG-CORE                          # Run IPC server (default)
    GG-CORE serve                    # Explicitly run IPC server
    GG-CORE infer --model phi-3 --prompt \"Hello\"  # Run inference
    GG-CORE infer --model phi-3 --prompt \"Hi\" --stream  # Streaming
    GG-CORE health                   # Full health check
    GG-CORE live                     # Liveness probe
    GG-CORE ready                    # Readiness probe
    GG-CORE status                   # Show system status
    GG-CORE models list              # List loaded models
    GG-CORE config validate          # Validate configuration
    GG-CORE --socket /custom/path    # Use custom socket path

ENVIRONMENT:
    GG_CORE_SOCKET_PATH  IPC socket path (default: /var/run/gg-core/GG-CORE.sock on Unix)
    CORE_AUTH_TOKEN      Authentication token for server mode
    RUST_LOG             Log level (debug, info, warn, error)
    GG_CORE_ENV          Environment (development, staging, production)

EXIT CODES:
    0  Success / Healthy
    1  Failure / Unhealthy
    2  Configuration error
    3  Connection error

DOCUMENTATION:
    https://docs.GG-CORE.io

SUPPORT:
    GitHub Issues: https://github.com/GG-CORE/core/issues
    Community:     https://slack.GG-CORE.io
",
        version
    );
}

/// Print detailed help for a specific command.
fn print_command_help(command: &str) {
    match command {
        "serve" => {
            eprintln!(
                "GG-CORE serve - Run the IPC server

USAGE:
    GG-CORE serve [OPTIONS]

OPTIONS:
    --socket PATH     Override IPC socket path
    --config FILE     Load configuration from file
    --auth-token TKN  Set authentication token

DESCRIPTION:
    Starts the GG-CORE IPC server, which handles inference requests
    through inter-process communication. This is the default command when
    no command is specified.

    The server performs FIPS 140-3 power-on self-tests before starting
    and will fail-fast if any cryptographic self-test fails.

EXAMPLES:
    GG-CORE serve
    GG-CORE serve --socket /custom/gg-core.sock
    GG-CORE serve --config /etc/gg-core/config.toml
"
            );
        }
        "health" => {
            eprintln!(
                "GG-CORE health - Full health check

USAGE:
    GG-CORE health [OPTIONS]

OPTIONS:
    --socket PATH  Override IPC socket path
    --timeout SEC  Connection timeout in seconds (default: 5)
    --json         Output in JSON format

DESCRIPTION:
    Performs a comprehensive health check of the GG-CORE runtime.
    Checks model loading, memory status, and inference capability.

EXIT CODES:
    0  System is healthy
    1  System is unhealthy
    3  Connection error

EXAMPLES:
    GG-CORE health
    GG-CORE health --json
    GG-CORE health --timeout 10
"
            );
        }
        "live" | "liveness" => {
            eprintln!(
                "GG-CORE live - Liveness probe

USAGE:
    GG-CORE live [OPTIONS]

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
        command: [GG-CORE, live]
      initialDelaySeconds: 30
      periodSeconds: 10
"
            );
        }
        "ready" | "readiness" => {
            eprintln!(
                "GG-CORE ready - Readiness probe

USAGE:
    GG-CORE ready [OPTIONS]

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
        command: [GG-CORE, ready]
      initialDelaySeconds: 60
      periodSeconds: 5
"
            );
        }
        "status" => {
            eprintln!(
                "GG-CORE status - Show system status

USAGE:
    GG-CORE status [OPTIONS]

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
    GG-CORE status
    GG-CORE status --json
    GG-CORE status --watch
"
            );
        }
        "infer" => {
            eprintln!(
                "GG-CORE infer - Run inference

USAGE:
    GG-CORE infer --model <MODEL> --prompt <PROMPT> [OPTIONS]

OPTIONS:
    --model <MODEL>      Model ID to use for inference
    --prompt <PROMPT>    Input prompt for generation
    --max-tokens <N>     Maximum tokens to generate (default: 256)
    --stream             Enable token-by-token streaming output
    --socket PATH        Override IPC socket path

DESCRIPTION:
    Sends an inference request to the running GG-CORE server
    and prints the generated output. Use --stream for real-time
    token streaming.

EXIT CODES:
    0  Inference completed successfully
    1  Inference failed or connection error

EXAMPLES:
    GG-CORE infer --model phi-3 --prompt \"Hello, world!\"
    GG-CORE infer --model phi-3 --prompt \"Count to 5\" --stream
    GG-CORE infer --model qwen --prompt \"Hi\" --max-tokens 100
"
            );
        }
        "verify" => {
            eprintln!(
                "GG-CORE verify - Verify deployment

USAGE:
    GG-CORE verify [OPTIONS]

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
    GG-CORE verify
    GG-CORE verify --all
"
            );
        }
        "models" => {
            eprintln!(
                "GG-CORE models - Manage models

USAGE:
    GG-CORE models <SUBCOMMAND> [OPTIONS]

SUBCOMMANDS:
    list           List loaded models
    load <NAME>    Load a model
    unload <NAME>  Unload a model
    info <NAME>    Show model information

OPTIONS:
    --socket PATH  Override IPC socket path
    --json         Output in JSON format

EXAMPLES:
    GG-CORE models list
    GG-CORE models load llama-2-7b-chat
    GG-CORE models info llama-2-7b-chat
    GG-CORE models unload llama-2-7b-chat
"
            );
        }
        "config" => {
            eprintln!(
                "GG-CORE config - Manage configuration

USAGE:
    GG-CORE config <SUBCOMMAND> [OPTIONS]

SUBCOMMANDS:
    show           Show current configuration
    validate       Validate configuration file
    defaults       Show default configuration

OPTIONS:
    --socket PATH  Override IPC socket path
    --file PATH    Configuration file path

EXAMPLES:
    GG-CORE config show
    GG-CORE config validate --file values.yaml
    GG-CORE config defaults
"
            );
        }
        _ => {
            eprintln!(
                "No detailed help available for '{}'. Use 'GG-CORE help' for general usage.",
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

/// Run the inference CLI command.
async fn run_inference(args: &[String]) -> i32 {
    let mut model_id = String::new();
    let mut prompt = String::new();
    let mut max_tokens = 256usize;
    let mut stream = false;

    // Parse arguments
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--model" => {
                if i + 1 < args.len() {
                    model_id = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("Missing value for --model");
                    return 1;
                }
            }
            "--prompt" => {
                if i + 1 < args.len() {
                    prompt = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("Missing value for --prompt");
                    return 1;
                }
            }
            "--max-tokens" => {
                if i + 1 < args.len() {
                    max_tokens = args[i + 1].parse().unwrap_or(256);
                    i += 2;
                } else {
                    eprintln!("Missing value for --max-tokens");
                    return 1;
                }
            }
            "--stream" => {
                stream = true;
                i += 1;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                return 1;
            }
        }
    }

    if model_id.is_empty() || prompt.is_empty() {
        eprintln!(
            "Usage: GG-CORE infer --model <MODEL> --prompt <PROMPT> [--max-tokens N] [--stream]"
        );
        return 1;
    }

    let socket_path = get_socket_path();
    let client = CliIpcClient::new(socket_path);
    let params = InferenceParams {
        max_tokens,
        ..Default::default()
    };

    let result = if stream {
        client
            .send_streaming_inference(&model_id, &prompt, &params)
            .await
    } else {
        client.send_inference(&model_id, &prompt, &params).await
    };

    match result {
        Ok(output) => {
            if !stream {
                println!("{}", output);
            }
            0
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    }
}

async fn run_ipc_server(runtime: Runtime) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = get_socket_path();
    let handler = std::sync::Arc::new(runtime.ipc_handler);
    let connections = runtime.connections;
    let shutdown = runtime.shutdown;
    let shutdown_timeout = runtime.config.shutdown_timeout;

    // Spawn the inference worker (single dequeue-execute loop)
    let worker_shutdown = tokio_util::sync::CancellationToken::new();
    let worker_handle = gg_core::scheduler::spawn_worker_with_registry(
        runtime.request_queue.clone(),
        runtime.inference_engine.clone(),
        Some(runtime.model_lifecycle.clone()),
        Some(runtime.model_registry.clone()),
        worker_shutdown.clone(),
    );

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

    // Shut down the worker
    worker_shutdown.cancel();
    runtime.request_queue.wake();
    let _ = worker_handle.await;

    // Wait for server task to finish
    if let Err(e) = server_handle.await? {
        eprintln!("Server error: {}", e);
    }

    Ok(())
}
