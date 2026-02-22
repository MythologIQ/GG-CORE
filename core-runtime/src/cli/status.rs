//! Status command implementation for GG-CORE.

use serde::{Deserialize, Serialize};

use super::ipc_client::{CliError, CliIpcClient};
use super::status_format::print_status_human;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub health: HealthState,
    pub uptime_secs: u64,
    pub version: VersionInfo,
    pub models: Vec<ModelStatus>,
    pub requests: RequestStats,
    pub resources: ResourceUtilization,
    pub scheduler: SchedulerStatus,
    pub gpus: Option<Vec<GpuStatus>>,
    pub recent_events: Vec<Event>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState { Healthy, Degraded, Unhealthy }

impl std::fmt::Display for HealthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthState::Healthy => write!(f, "healthy"),
            HealthState::Degraded => write!(f, "degraded"),
            HealthState::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub commit: String,
    pub build_date: String,
    pub rust_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStatus {
    pub name: String,
    pub format: String,
    pub size_bytes: u64,
    pub loaded_at: String,
    pub request_count: u64,
    pub avg_latency_ms: f64,
    pub state: ModelState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelState { Loading, Ready, Unloading, Error }

impl std::fmt::Display for ModelState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelState::Loading => write!(f, "loading"),
            ModelState::Ready => write!(f, "ready"),
            ModelState::Unloading => write!(f, "unloading"),
            ModelState::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestStats {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub requests_per_second: f64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub tokens_generated: u64,
    pub tokens_per_second: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUtilization {
    pub memory_rss_bytes: u64,
    pub kv_cache_bytes: u64,
    pub arena_bytes: u64,
    pub memory_limit_bytes: u64,
    pub memory_utilization_percent: f64,
    pub cpu_utilization_percent: f64,
    pub active_threads: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStatus {
    pub queue_depth: u64,
    pub active_batches: u64,
    pub pending_requests: u64,
    pub completed_requests: u64,
    pub avg_batch_size: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuStatus {
    pub gpu_id: u32,
    pub name: String,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub utilization_percent: f64,
    pub temperature_celsius: f64,
    pub power_draw_watts: f64,
    pub power_limit_watts: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub timestamp: String,
    pub event_type: String,
    pub message: String,
    pub severity: EventSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSeverity { Info, Warning, Error }

/// Run the status command and display results.
pub async fn run_status(socket_path: &str, json_output: bool) -> i32 {
    match fetch_status(socket_path).await {
        Ok(status) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&status).unwrap());
            } else {
                print_status_human(&status);
            }
            0
        }
        Err(e) => {
            eprintln!("Error fetching status: {}", e);
            match e {
                CliError::ConnectionFailed(_) | CliError::Timeout => 3,
                _ => 1,
            }
        }
    }
}

/// Fetch status from the IPC server.
async fn fetch_status(socket_path: &str) -> Result<SystemStatus, CliError> {
    let client = CliIpcClient::new(socket_path.to_string());
    let health_response = client.get_health_report().await?;
    let report = health_response.report;
    let metrics = client.get_metrics().await.ok();
    let models_response = client.get_models().await.ok();

    let status = build_status(health_response.ok, report, metrics, models_response);
    Ok(status)
}

fn build_status(
    health_ok: bool,
    report: Option<crate::health::HealthReport>,
    metrics: Option<crate::telemetry::MetricsSnapshot>,
    models_response: Option<crate::ipc::ModelsListResponse>,
) -> SystemStatus {
    let total_requests = get_counter(&metrics, "core_requests_total");
    let successful_requests = get_counter(&metrics, "core_requests_success");
    let failed_requests = get_counter(&metrics, "core_requests_failed");
    let tokens_generated = get_counter(&metrics, "core_tokens_output_total");
    let arena_bytes = get_gauge(&metrics, "core_arena_used_bytes") as u64;
    let queue_depth = get_gauge(&metrics, "core_queue_depth") as u64;
    let memory_pool_bytes = get_gauge(&metrics, "core_memory_pool_used_bytes") as u64;

    let latency_hist = metrics.as_ref().and_then(|m| m.histograms.get("core_inference_latency_ms"));
    let avg_latency_ms = latency_hist
        .map(|h| if h.count > 0 { h.sum / h.count as f64 } else { 0.0 })
        .unwrap_or(0.0);

    let uptime_secs = report.as_ref().map(|r| r.uptime_secs).unwrap_or(1).max(1);
    let rps = total_requests as f64 / uptime_secs as f64;
    let tps = tokens_generated as f64 / uptime_secs as f64;

    let models = build_models_list(&models_response);

    SystemStatus {
        health: if health_ok { HealthState::Healthy } else { HealthState::Unhealthy },
        uptime_secs,
        version: VersionInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            commit: option_env!("VERGEN_GIT_SHA").unwrap_or("unknown").to_string(),
            build_date: option_env!("VERGEN_BUILD_DATE").unwrap_or("unknown").to_string(),
            rust_version: option_env!("VERGEN_RUSTC_SEMVER").unwrap_or("unknown").to_string(),
        },
        models,
        requests: RequestStats {
            total_requests, successful_requests, failed_requests,
            requests_per_second: rps, avg_latency_ms,
            p50_latency_ms: latency_hist.map(|h| h.min).unwrap_or(0.0),
            p95_latency_ms: latency_hist.map(|h| h.max * 0.95).unwrap_or(0.0),
            p99_latency_ms: latency_hist.map(|h| h.max * 0.99).unwrap_or(0.0),
            tokens_generated, tokens_per_second: tps,
        },
        resources: ResourceUtilization {
            memory_rss_bytes: report.as_ref().map(|r| r.memory_used_bytes as u64).unwrap_or(memory_pool_bytes),
            kv_cache_bytes: 0, arena_bytes, memory_limit_bytes: 0,
            memory_utilization_percent: 0.0, cpu_utilization_percent: 0.0, active_threads: 0,
        },
        scheduler: SchedulerStatus {
            queue_depth: report.as_ref().map(|r| r.queue_depth as u64).unwrap_or(queue_depth),
            active_batches: 0, pending_requests: queue_depth,
            completed_requests: total_requests, avg_batch_size: 0.0,
        },
        gpus: None,
        recent_events: vec![],
    }
}

fn get_counter(m: &Option<crate::telemetry::MetricsSnapshot>, key: &str) -> u64 {
    m.as_ref().and_then(|m| m.counters.get(key).copied()).unwrap_or(0)
}

fn get_gauge(m: &Option<crate::telemetry::MetricsSnapshot>, key: &str) -> f64 {
    m.as_ref().and_then(|m| m.gauges.get(key).copied()).unwrap_or(0.0)
}

fn build_models_list(resp: &Option<crate::ipc::ModelsListResponse>) -> Vec<ModelStatus> {
    resp.as_ref().map(|r| {
        r.models.iter().map(|m| {
            let avg = if m.request_count > 0 { m.avg_latency_ms / m.request_count as f64 } else { 0.0 };
            ModelStatus {
                name: m.name.clone(), format: m.format.clone(),
                size_bytes: m.size_bytes, loaded_at: m.loaded_at.clone(),
                request_count: m.request_count, avg_latency_ms: avg,
                state: match m.state.as_str() {
                    "loading" => ModelState::Loading, "ready" => ModelState::Ready,
                    "unloading" => ModelState::Unloading, "error" => ModelState::Error,
                    _ => ModelState::Ready,
                },
            }
        }).collect()
    }).unwrap_or_default()
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
