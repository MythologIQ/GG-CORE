//! Single worker loop: dequeue requests and execute inference.
//!
//! All inference goes through this worker. The IPC handler enqueues
//! requests and awaits the oneshot response channel.

use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::engine::InferenceEngine;
use crate::models::lifecycle::ModelLifecycle;
use crate::models::registry::ModelRegistry;
use crate::telemetry;
use super::queue::{QueuedRequest, RequestQueue};

/// Spawn the worker loop. Returns a handle for shutdown.
pub fn spawn_worker(
    queue: Arc<RequestQueue>,
    engine: Arc<InferenceEngine>,
    shutdown: tokio_util::sync::CancellationToken,
) -> JoinHandle<()> {
    spawn_worker_with_registry(queue, engine, None, None, shutdown)
}

/// Spawn with optional registry for per-model stats recording.
pub fn spawn_worker_with_registry(
    queue: Arc<RequestQueue>,
    engine: Arc<InferenceEngine>,
    lifecycle: Option<Arc<ModelLifecycle>>,
    registry: Option<Arc<ModelRegistry>>,
    shutdown: tokio_util::sync::CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        worker_loop(&queue, &engine, lifecycle.as_deref(), registry.as_deref(), shutdown).await;
    })
}

async fn worker_loop(
    queue: &RequestQueue,
    engine: &InferenceEngine,
    lifecycle: Option<&ModelLifecycle>,
    registry: Option<&ModelRegistry>,
    shutdown: tokio_util::sync::CancellationToken,
) {
    loop {
        tokio::select! {
            biased;
            () = shutdown.cancelled() => {
                tracing::info!("worker: shutdown signal received");
                break;
            }
            req_opt = queue.wait_and_dequeue() => {
                if let Some(request) = req_opt {
                    execute_request(engine, lifecycle, registry, request).await;
                }
            }
        }
    }
}

async fn execute_request(
    engine: &InferenceEngine,
    lifecycle: Option<&ModelLifecycle>,
    registry: Option<&ModelRegistry>,
    request: QueuedRequest,
) {
    let model_id = request.model_id.clone();
    let prompt = request.prompt.clone();
    let params = request.params.clone();
    let cancelled = request.cancel_check();

    let start = std::time::Instant::now();

    // run_cancellable checks cancellation before and after inference.
    let result = engine
        .run_cancellable(&model_id, &prompt, &params, cancelled)
        .await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match &result {
        Ok(r) => {
            telemetry::record_request_success(
                &model_id, latency_ms, r.tokens_generated as u64,
            );
            // Record per-model stats in ModelRegistry if available
            if let (Some(lc), Some(reg)) = (lifecycle, registry) {
                if let Some(handle) = lc.get_handle(&model_id).await {
                    reg.record_request(handle, latency_ms as f64).await;
                }
            }
        }
        Err(e) => {
            telemetry::record_request_failure(&model_id, &e.to_string());
        }
    }

    let mapped = result.map_err(|e| e.to_string());
    send_response(request, mapped);
}

fn send_response(request: QueuedRequest, result: Result<crate::engine::inference::InferenceResult, String>) {
    if let Some(tx) = request.response_tx {
        let _ = tx.send(result);
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
