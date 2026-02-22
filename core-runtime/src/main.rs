//! GG-CORE Runtime entry point.
//!
//! Bootstraps the sandboxed inference engine with FIPS 140-3 self-tests,
//! configuration loading, IPC listener setup, and signal handling.

mod cli_parser;
mod runtime_init;

use std::process::ExitCode;

use gg_core::cli::{get_socket_path, run_health, run_liveness, run_readiness, run_status};
use gg_core::security::fips_tests;
use gg_core::Runtime;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("serve");

    match command {
        "serve" | "" => run_serve().await,
        "health" => run_probe(|p| Box::pin(run_health(p))).await,
        "live" | "liveness" => run_probe(|p| Box::pin(run_liveness(p))).await,
        "ready" | "readiness" => run_probe(|p| Box::pin(run_readiness(p))).await,
        "help" | "--help" | "-h" => {
            if let Some(sub) = args.get(2) {
                cli_parser::print_command_help(sub);
            } else {
                cli_parser::print_usage();
            }
            ExitCode::SUCCESS
        }
        "version" | "--version" | "-V" => {
            println!("GG-CORE {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "status" => {
            let sp = get_socket_path();
            let json = args.get(2).map(|s| s.as_str()) == Some("--json");
            ExitCode::from(run_status(&sp, json).await as u8)
        }
        "infer" => ExitCode::from(runtime_init::run_inference(&args).await as u8),
        "verify" => {
            eprintln!("Verify command not yet implemented. Use 'GG-CORE health'.");
            ExitCode::from(2u8)
        }
        "models" => run_models_cmd(&args).await,
        "config" => run_config_cmd(&args).await,
        _ => {
            eprintln!("Unknown command: {}", command);
            cli_parser::print_usage();
            ExitCode::FAILURE
        }
    }
}

async fn run_serve() -> ExitCode {
    if let Err(e) = fips_tests::run_power_on_self_tests() {
        eprintln!("FIPS self-test FAILED: {}", e);
        eprintln!("Cryptographic operations disabled. Aborting startup.");
        return ExitCode::FAILURE;
    }
    eprintln!("FIPS 140-3 self-tests: PASSED");

    let config = runtime_init::load_config();
    let runtime = Runtime::new(config);
    match runtime_init::run_ipc_server(runtime).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Server error: {}", e);
            ExitCode::FAILURE
        }
    }
}

async fn run_probe<F>(f: F) -> ExitCode
where
    F: FnOnce(&str) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + Send + '_>>,
{
    let sp = get_socket_path();
    ExitCode::from(f(&sp).await as u8)
}

async fn run_models_cmd(args: &[String]) -> ExitCode {
    let sub = args.get(2).map(|s| s.as_str()).unwrap_or("list");
    match sub {
        "list" => {
            let sp = get_socket_path();
            ExitCode::from(gg_core::cli::models_cmd::run_list(&sp).await as u8)
        }
        _ => {
            eprintln!("Unknown models subcommand: {}", sub);
            cli_parser::print_command_help("models");
            ExitCode::FAILURE
        }
    }
}

async fn run_config_cmd(args: &[String]) -> ExitCode {
    let sub = args.get(2).map(|s| s.as_str()).unwrap_or("show");
    match sub {
        "show" => {
            gg_core::cli::config_cmd::run_show();
            ExitCode::SUCCESS
        }
        "defaults" => {
            gg_core::cli::config_cmd::run_defaults();
            ExitCode::SUCCESS
        }
        "validate" => ExitCode::from(gg_core::cli::config_cmd::run_validate() as u8),
        _ => {
            eprintln!("Unknown config subcommand: {}", sub);
            cli_parser::print_command_help("config");
            ExitCode::FAILURE
        }
    }
}
