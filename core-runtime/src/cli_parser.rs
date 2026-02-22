//! CLI argument parsing and help text for GG-CORE.

/// Print general usage information.
pub fn print_usage() {
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

ENVIRONMENT:
    GG_CORE_SOCKET_PATH  IPC socket path
    CORE_AUTH_TOKEN      Authentication token for server mode
    RUST_LOG             Log level (debug, info, warn, error)
    GG_CORE_ENV          Environment (development, staging, production)

EXIT CODES:
    0  Success / Healthy
    1  Failure / Unhealthy
    2  Configuration error
    3  Connection error
",
        version
    );
}

/// Print detailed help for a specific command.
pub fn print_command_help(command: &str) {
    match command {
        "serve" => print_serve_help(),
        "health" => print_health_help(),
        "live" | "liveness" => print_live_help(),
        "ready" | "readiness" => print_ready_help(),
        "status" => print_status_help(),
        "infer" => print_infer_help(),
        "verify" => print_verify_help(),
        "models" => print_models_help(),
        "config" => print_config_help(),
        _ => {
            eprintln!(
                "No detailed help available for '{}'. Use 'GG-CORE help' for general usage.",
                command
            );
        }
    }
}

fn print_serve_help() {
    eprintln!(
        "GG-CORE serve - Run the IPC server

USAGE:
    GG-CORE serve [OPTIONS]

OPTIONS:
    --socket PATH     Override IPC socket path
    --config FILE     Load configuration from file
    --auth-token TKN  Set authentication token

DESCRIPTION:
    Starts the GG-CORE IPC server. Default command when none is specified.
    Performs FIPS 140-3 power-on self-tests before starting.

EXAMPLES:
    GG-CORE serve
    GG-CORE serve --socket /custom/gg-core.sock
"
    );
}

fn print_health_help() {
    eprintln!(
        "GG-CORE health - Full health check

USAGE:
    GG-CORE health [OPTIONS]

OPTIONS:
    --socket PATH  Override IPC socket path
    --json         Output in JSON format

EXIT CODES:
    0  System is healthy
    1  System is unhealthy
    3  Connection error
"
    );
}

fn print_live_help() {
    eprintln!(
        "GG-CORE live - Liveness probe

USAGE:
    GG-CORE live [OPTIONS]

OPTIONS:
    --socket PATH  Override IPC socket path

EXIT CODES:
    0  Process is alive
    1  Process is unresponsive
    3  Connection error
"
    );
}

fn print_ready_help() {
    eprintln!(
        "GG-CORE ready - Readiness probe

USAGE:
    GG-CORE ready [OPTIONS]

OPTIONS:
    --socket PATH  Override IPC socket path

EXIT CODES:
    0  Ready to serve traffic
    1  Not ready
    3  Connection error
"
    );
}

fn print_status_help() {
    eprintln!(
        "GG-CORE status - Show system status

USAGE:
    GG-CORE status [OPTIONS]

OPTIONS:
    --socket PATH  Override IPC socket path
    --json         Output in JSON format
"
    );
}

fn print_infer_help() {
    eprintln!(
        "GG-CORE infer - Run inference

USAGE:
    GG-CORE infer --model <MODEL> --prompt <PROMPT> [OPTIONS]

OPTIONS:
    --model <MODEL>      Model ID to use for inference
    --prompt <PROMPT>    Input prompt for generation
    --max-tokens <N>     Maximum tokens to generate (default: 256)
    --stream             Enable token-by-token streaming output

EXIT CODES:
    0  Inference completed successfully
    1  Inference failed or connection error

EXAMPLES:
    GG-CORE infer --model phi-3 --prompt \"Hello, world!\"
    GG-CORE infer --model phi-3 --prompt \"Count to 5\" --stream
"
    );
}

fn print_verify_help() {
    eprintln!(
        "GG-CORE verify - Verify deployment

USAGE:
    GG-CORE verify [OPTIONS]

OPTIONS:
    --socket PATH  Override IPC socket path
    --all          Run all verification checks

EXIT CODES:
    0  All checks passed
    1  One or more checks failed
"
    );
}

fn print_models_help() {
    eprintln!(
        "GG-CORE models - Manage models

USAGE:
    GG-CORE models <SUBCOMMAND> [OPTIONS]

SUBCOMMANDS:
    list           List loaded models

OPTIONS:
    --socket PATH  Override IPC socket path
    --json         Output in JSON format
"
    );
}

fn print_config_help() {
    eprintln!(
        "GG-CORE config - Manage configuration

USAGE:
    GG-CORE config <SUBCOMMAND>

SUBCOMMANDS:
    show           Show current configuration
    validate       Validate configuration file
    defaults       Show default configuration
"
    );
}
