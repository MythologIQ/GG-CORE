// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Status command implementation for Veritas SDR.
//!
//! Provides detailed system status including:
//! - Health state
//! - Loaded models
//! - Request statistics
//! - Resource utilization
//! - Recent events

use serde::{Deserialize, Serialize};

use super::ipc_client::{CliError, CliIpcClient};

/// System status response from the runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    /// Overall health state
    pub health: HealthState,
    /// Server uptime in seconds
    pub uptime_secs: u64,
    /// Version information
    pub version: VersionInfo,
    /// Loaded models
    pub models: Vec<ModelStatus>,
    /// Request statistics
    pub requests: RequestStats,
    /// Resource utilization
    pub resources: ResourceUtilization,
    /// Scheduler state
    pub scheduler: SchedulerStatus,
    /// GPU information (if available)
    pub gpus: Option<Vec<GpuStatus>>,
    /// Recent events (last 10)
    pub recent_events: Vec<Event>,
}

/// Health state enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Healthy,
    Degraded,
    Unhealthy,
}

impl std::fmt::Display for HealthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthState::Healthy => write!(f, "healthy"),
            HealthState::Degraded => write!(f, "degraded"),
            HealthState::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

/// Version information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub commit: String,
    pub build_date: String,
    pub rust_version: String,
}

/// Model status information.
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

/// Model loading state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    Loading,
    Ready,
    Unloading,
    Error,
}

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

/// Request statistics.
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

/// Resource utilization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUtilization {
    /// Process RSS in bytes
    pub memory_rss_bytes: u64,
    /// KV cache memory in bytes
    pub kv_cache_bytes: u64,
    /// Arena memory in bytes
    pub arena_bytes: u64,
    /// Memory limit in bytes
    pub memory_limit_bytes: u64,
    /// Memory utilization percentage
    pub memory_utilization_percent: f64,
    /// CPU utilization percentage
    pub cpu_utilization_percent: f64,
    /// Number of active threads
    pub active_threads: u32,
}

/// Scheduler status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStatus {
    pub queue_depth: u64,
    pub active_batches: u64,
    pub pending_requests: u64,
    pub completed_requests: u64,
    pub avg_batch_size: f64,
}

/// GPU status information.
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

/// Event record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub timestamp: String,
    pub event_type: String,
    pub message: String,
    pub severity: EventSeverity,
}

/// Event severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSeverity {
    Info,
    Warning,
    Error,
}

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
                CliError::ConnectionFailed(_) => 3,
                CliError::Timeout => 3,
                CliError::Protocol(_) => 1,
                _ => 1,
            }
        }
    }
}

/// Fetch status from the IPC server.
async fn fetch_status(socket_path: &str) -> Result<SystemStatus, CliError> {
    let client = CliIpcClient::new(socket_path.to_string());

    // Get health report and metrics from the runtime
    let health_response = client.get_health_report().await?;
    let report = health_response.report;

    // Get metrics snapshot (may fail if runtime doesn't support it yet)
    let metrics = client.get_metrics().await.ok();

    // Get loaded models (may fail if runtime doesn't support it yet)
    let models_response = client.get_models().await.ok();

    // Extract counters from metrics
    let total_requests = metrics
        .as_ref()
        .and_then(|m| m.counters.get("core_requests_total").copied())
        .unwrap_or(0);
    let successful_requests = metrics
        .as_ref()
        .and_then(|m| m.counters.get("core_requests_success").copied())
        .unwrap_or(0);
    let failed_requests = metrics
        .as_ref()
        .and_then(|m| m.counters.get("core_requests_failed").copied())
        .unwrap_or(0);
    let tokens_generated = metrics
        .as_ref()
        .and_then(|m| m.counters.get("core_tokens_output_total").copied())
        .unwrap_or(0);

    // Extract gauges from metrics
    let memory_pool_bytes = metrics
        .as_ref()
        .and_then(|m| m.gauges.get("core_memory_pool_used_bytes").copied())
        .unwrap_or(0.0) as u64;
    let arena_bytes = metrics
        .as_ref()
        .and_then(|m| m.gauges.get("core_arena_used_bytes").copied())
        .unwrap_or(0.0) as u64;
    let queue_depth = metrics
        .as_ref()
        .and_then(|m| m.gauges.get("core_queue_depth").copied())
        .unwrap_or(0.0) as u64;

    // Extract histogram data for latency
    let latency_hist = metrics
        .as_ref()
        .and_then(|m| m.histograms.get("core_inference_latency_ms"));
    let avg_latency_ms = latency_hist
        .map(|h| if h.count > 0 { h.sum / h.count as f64 } else { 0.0 })
        .unwrap_or(0.0);

    // Calculate uptime and rates
    let uptime_secs = report.as_ref().map(|r| r.uptime_secs).unwrap_or(1).max(1);
    let requests_per_second = total_requests as f64 / uptime_secs as f64;
    let tokens_per_second = tokens_generated as f64 / uptime_secs as f64;

    let status = SystemStatus {
        health: if health_response.ok {
            HealthState::Healthy
        } else {
            HealthState::Unhealthy
        },
        uptime_secs,
        version: VersionInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            commit: option_env!("VERGEN_GIT_SHA").unwrap_or("unknown").to_string(),
            build_date: option_env!("VERGEN_BUILD_DATE").unwrap_or("unknown").to_string(),
            rust_version: option_env!("VERGEN_RUSTC_SEMVER").unwrap_or("unknown").to_string(),
        },
        models: models_response
            .as_ref()
            .map(|r| {
                r.models
                    .iter()
                    .map(|m| {
                        let avg_latency = if m.request_count > 0 {
                            m.avg_latency_ms / m.request_count as f64
                        } else {
                            0.0
                        };
                        ModelStatus {
                            name: m.name.clone(),
                            format: m.format.clone(),
                            size_bytes: m.size_bytes,
                            loaded_at: m.loaded_at.clone(),
                            request_count: m.request_count,
                            avg_latency_ms: avg_latency,
                            state: match m.state.as_str() {
                                "loading" => ModelState::Loading,
                                "ready" => ModelState::Ready,
                                "unloading" => ModelState::Unloading,
                                "error" => ModelState::Error,
                                _ => ModelState::Ready,
                            },
                        }
                    })
                    .collect()
            })
            .unwrap_or_default(),
        requests: RequestStats {
            total_requests,
            successful_requests,
            failed_requests,
            requests_per_second,
            avg_latency_ms,
            p50_latency_ms: latency_hist.map(|h| h.min).unwrap_or(0.0), // Approximation
            p95_latency_ms: latency_hist.map(|h| h.max * 0.95).unwrap_or(0.0), // Approximation
            p99_latency_ms: latency_hist.map(|h| h.max * 0.99).unwrap_or(0.0), // Approximation
            tokens_generated,
            tokens_per_second,
        },
        resources: ResourceUtilization {
            memory_rss_bytes: report
                .as_ref()
                .map(|r| r.memory_used_bytes as u64)
                .unwrap_or(memory_pool_bytes),
            kv_cache_bytes: 0, // TODO: Add KV cache metrics
            arena_bytes,
            memory_limit_bytes: 0, // TODO: Add memory limit config
            memory_utilization_percent: 0.0,
            cpu_utilization_percent: 0.0,
            active_threads: 0,
        },
        scheduler: SchedulerStatus {
            queue_depth: report
                .as_ref()
                .map(|r| r.queue_depth as u64)
                .unwrap_or(queue_depth),
            active_batches: 0,
            pending_requests: queue_depth,
            completed_requests: total_requests,
            avg_batch_size: 0.0,
        },
        gpus: None, // TODO: Add GPU metrics collection
        recent_events: vec![], // TODO: Add event log query
    };

    Ok(status)
}

/// Print status in human-readable format.
fn print_status_human(status: &SystemStatus) {
    // Header with health state
    let health_icon = match status.health {
        HealthState::Healthy => "✓",
        HealthState::Degraded => "⚠",
        HealthState::Unhealthy => "✗",
    };

    println!("╔════════════════════════════════════════════════════════════════╗");
    println!(
        "║  Veritas SDR Status                                    v{}   ║",
        status.version.version
    );
    println!("╠════════════════════════════════════════════════════════════════╣");
    println!(
        "║  Health: {} {:10}  Uptime: {}              ║",
        health_icon,
        status.health,
        format_uptime(status.uptime_secs)
    );
    println!("╚════════════════════════════════════════════════════════════════╝");

    // Models section
    println!("\n📦 Models ({} loaded)", status.models.len());
    println!("┌─────────────────────────────┬────────────┬──────────┬─────────┐");
    println!("│ Name                        │ State      │ Size     │ Req/s   │");
    println!("├─────────────────────────────┼────────────┼──────────┼─────────┤");
    for model in &status.models {
        println!(
            "│ {:27} │ {:10} │ {:>8} │ {:>7.1} │",
            truncate(&model.name, 27),
            model.state,
            format_bytes(model.size_bytes),
            model.request_count as f64 / status.uptime_secs.max(1) as f64
        );
    }
    println!("└─────────────────────────────┴────────────┴──────────┴─────────┘");

    // Request statistics
    println!("\n📊 Request Statistics");
    println!("┌─────────────────────────────────────────────────────────────────┐");
    println!(
        "│ Total Requests: {:>12}  Success: {:>12}  Failed: {:>8} │",
        status.requests.total_requests,
        status.requests.successful_requests,
        status.requests.failed_requests
    );
    println!(
        "│ Throughput: {:>8.1} req/s    Token Gen: {:>8.1} tok/s            │",
        status.requests.requests_per_second, status.requests.tokens_per_second
    );
    println!("├─────────────────────────────────────────────────────────────────┤");
    println!(
        "│ Latency:  Avg {:>7.1}ms  P50 {:>7.1}ms  P95 {:>7.1}ms  P99 {:>6.1}ms │",
        status.requests.avg_latency_ms,
        status.requests.p50_latency_ms,
        status.requests.p95_latency_ms,
        status.requests.p99_latency_ms
    );
    println!("└─────────────────────────────────────────────────────────────────┘");

    // Resource utilization
    println!("\n💾 Resources");
    println!("┌─────────────────────────────────────────────────────────────────┐");
    println!(
        "│ Memory: {:>10} / {:>10} ({:>5.1}%)                      │",
        format_bytes(status.resources.memory_rss_bytes),
        format_bytes(status.resources.memory_limit_bytes),
        status.resources.memory_utilization_percent
    );
    println!(
        "│   KV Cache: {:>10}   Arena: {:>10}                       │",
        format_bytes(status.resources.kv_cache_bytes),
        format_bytes(status.resources.arena_bytes)
    );
    println!(
        "│ CPU: {:>5.1}%    Threads: {:>3}                                     │",
        status.resources.cpu_utilization_percent, status.resources.active_threads
    );
    println!("└─────────────────────────────────────────────────────────────────┘");

    // GPU status (if available)
    if let Some(ref gpus) = status.gpus {
        if !gpus.is_empty() {
            println!("\n🖥️  GPUs ({} devices)", gpus.len());
            println!("┌──────┬────────────────────────────┬──────────┬───────┬──────────┐");
            println!("│ ID   │ Name                        │ Memory   │ Util  │ Temp     │");
            println!("├──────┼────────────────────────────┼──────────┼───────┼──────────┤");
            for gpu in gpus {
                println!(
                    "│ {:4} │ {:26} │ {:>8} │ {:>5.0}% │ {:>6.0}°C │",
                    gpu.gpu_id,
                    truncate(&gpu.name, 26),
                    format_bytes(gpu.memory_used_bytes),
                    gpu.utilization_percent,
                    gpu.temperature_celsius
                );
            }
            println!("└──────┴────────────────────────────┴──────────┴───────┴──────────┘");
        }
    }

    // Scheduler status
    println!("\n⚙️  Scheduler");
    println!("┌─────────────────────────────────────────────────────────────────┐");
    println!(
        "│ Queue Depth: {:>5}   Active Batches: {:>3}   Pending: {:>5}    │",
        status.scheduler.queue_depth,
        status.scheduler.active_batches,
        status.scheduler.pending_requests
    );
    println!(
        "│ Completed: {:>8}   Avg Batch Size: {:>5.1}                      │",
        status.scheduler.completed_requests, status.scheduler.avg_batch_size
    );
    println!("└─────────────────────────────────────────────────────────────────┘");

    // Recent events
    if !status.recent_events.is_empty() {
        println!("\n📋 Recent Events (last {})", status.recent_events.len());
        println!("┌─────────────────────────────────────────────────────────────────┐");
        for event in &status.recent_events {
            let severity_icon = match event.severity {
                EventSeverity::Info => "ℹ️",
                EventSeverity::Warning => "⚠️",
                EventSeverity::Error => "❌",
            };
            println!(
                "│ {} {} {:54} │",
                severity_icon,
                truncate(&event.timestamp, 10),
                truncate(&event.message, 54)
            );
        }
        println!("└─────────────────────────────────────────────────────────────────┘");
    }
}

/// Format uptime in human-readable form.
fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

/// Format bytes in human-readable form.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Truncate a string to a maximum length.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_uptime() {
        assert_eq!(format_uptime(0), "0m");
        assert_eq!(format_uptime(59), "0m");
        assert_eq!(format_uptime(60), "1m");
        assert_eq!(format_uptime(3600), "1h 0m");
        assert_eq!(format_uptime(3661), "1h 1m");
        assert_eq!(format_uptime(86400), "1d 0h 0m");
        assert_eq!(format_uptime(90061), "1d 1h 1m");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1073741824), "1.0 GB");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("this is a very long string", 10), "this is...");
    }

    #[test]
    fn test_health_state_display() {
        assert_eq!(format!("{}", HealthState::Healthy), "healthy");
        assert_eq!(format!("{}", HealthState::Degraded), "degraded");
        assert_eq!(format!("{}", HealthState::Unhealthy), "unhealthy");
    }

    #[test]
    fn test_model_state_display() {
        assert_eq!(format!("{}", ModelState::Loading), "loading");
        assert_eq!(format!("{}", ModelState::Ready), "ready");
        assert_eq!(format!("{}", ModelState::Unloading), "unloading");
        assert_eq!(format!("{}", ModelState::Error), "error");
    }

    #[test]
    fn test_system_status_serialization() {
        let status = SystemStatus {
            health: HealthState::Healthy,
            uptime_secs: 3600,
            version: VersionInfo {
                version: "0.6.0".to_string(),
                commit: "abc123".to_string(),
                build_date: "2026-02-18".to_string(),
                rust_version: "1.75.0".to_string(),
                // Note: I left out commit/build_date in previous view but here they are required by struct.
                // Ah, the struct definition in the file had these fields.
                // The init code in fetch_status has them.
            },
            models: vec![],
            requests: RequestStats {
                total_requests: 1000,
                successful_requests: 990,
                failed_requests: 10,
                requests_per_second: 10.5,
                avg_latency_ms: 50.0,
                p50_latency_ms: 45.0,
                p95_latency_ms: 100.0,
                p99_latency_ms: 150.0,
                tokens_generated: 50000,
                tokens_per_second: 25.0,
            },
            resources: ResourceUtilization {
                memory_rss_bytes: 4 * 1024 * 1024 * 1024,
                kv_cache_bytes: 2 * 1024 * 1024 * 1024,
                arena_bytes: 512 * 1024 * 1024,
                memory_limit_bytes: 8 * 1024 * 1024 * 1024,
                memory_utilization_percent: 50.0,
                cpu_utilization_percent: 75.0,
                active_threads: 8,
            },
            scheduler: SchedulerStatus {
                queue_depth: 5,
                active_batches: 2,
                pending_requests: 10,
                completed_requests: 1000,
                avg_batch_size: 4.5,
            },
            gpus: None,
            recent_events: vec![],
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"health\":\"healthy\""));
        assert!(json.contains("\"uptime_secs\":3600"));
    }
}
