//! Runtime initialization and IPC server setup for GG-CORE.

use gg_core::cli::get_socket_path;
use gg_core::config as gg_config;
use gg_core::engine::InferenceParams;
use gg_core::ipc::server;
use gg_core::shutdown::ShutdownResult;
use gg_core::{Runtime, RuntimeConfig};

use gg_core::cli::CliIpcClient;

/// Load runtime configuration from environment.
pub fn load_config() -> RuntimeConfig {
    let env = gg_config::load();
    RuntimeConfig {
        base_path: env.base_path,
        auth_token: env.auth_token,
        session_timeout: env.session_timeout,
        max_context_length: env.max_context_length,
        request_queue: env.request_queue,
        resource_limits: env.resource_limits,
        batch: env.batch,
        shutdown_timeout: env.shutdown_timeout,
        connections: env.connections,
        ipc_server: env.ipc_server,
        ..Default::default()
    }
}

/// Run the inference CLI command.
pub async fn run_inference(args: &[String]) -> i32 {
    let mut model_id = String::new();
    let mut prompt = String::new();
    let mut max_tokens = 256usize;
    let mut stream = false;

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
        eprintln!("Usage: GG-CORE infer --model <MODEL> --prompt <PROMPT> [--max-tokens N] [--stream]");
        return 1;
    }

    let socket_path = get_socket_path();
    let client = CliIpcClient::new(socket_path);
    let params = InferenceParams { max_tokens, ..Default::default() };

    let result = if stream {
        client.send_streaming_inference(&model_id, &prompt, &params).await
    } else {
        client.send_inference(&model_id, &prompt, &params).await
    };

    match result {
        Ok(output) => {
            if !stream { println!("{}", output); }
            0
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    }
}

/// Run the IPC server with the given runtime.
pub async fn run_ipc_server(runtime: Runtime) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = get_socket_path();
    let handler = std::sync::Arc::new(runtime.ipc_handler);
    let connections = runtime.connections;
    let shutdown = runtime.shutdown;
    let shutdown_timeout = runtime.config.shutdown_timeout;
    let ipc_config = runtime.config.ipc_server.clone();

    let worker_shutdown = tokio_util::sync::CancellationToken::new();
    let worker_handle = gg_core::scheduler::spawn_worker_with_registry(
        runtime.request_queue.clone(),
        runtime.inference_engine.clone(),
        Some(runtime.model_lifecycle.clone()),
        Some(runtime.model_registry.clone()),
        Some(runtime.resource_limits.clone()),
        worker_shutdown.clone(),
    );

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let server_handle = tokio::spawn(server::run_server(
        socket_path, handler, connections, shutdown_rx, ipc_config,
    ));

    tokio::signal::ctrl_c().await?;
    eprintln!("Shutdown signal received, draining...");

    let _ = shutdown_tx.send(true);

    match shutdown.initiate(shutdown_timeout).await {
        ShutdownResult::Complete => eprintln!("Shutdown complete"),
        ShutdownResult::Timeout { remaining } => {
            eprintln!("Shutdown timeout, {} requests remaining", remaining);
        }
    }

    worker_shutdown.cancel();
    runtime.request_queue.wake();
    let _ = worker_handle.await;

    if let Err(e) = server_handle.await? {
        eprintln!("Server error: {}", e);
    }

    Ok(())
}
