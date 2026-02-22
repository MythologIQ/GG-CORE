//! Formatting helpers for the status command display.

use super::status::{
    EventSeverity, GpuStatus, HealthState, SystemStatus,
};

/// Print status in human-readable format.
pub fn print_status_human(status: &SystemStatus) {
    print_header(status);
    print_models(status);
    print_request_stats(status);
    print_resources(status);
    print_gpus(status);
    print_scheduler(status);
    print_events(status);
}

fn print_header(status: &SystemStatus) {
    let health_icon = match status.health {
        HealthState::Healthy => "V",
        HealthState::Degraded => "!",
        HealthState::Unhealthy => "X",
    };

    println!("====================================================");
    println!("  GG-CORE Status                         v{}", status.version.version);
    println!("====================================================");
    println!(
        "  Health: {} {:10}  Uptime: {}",
        health_icon,
        status.health,
        format_uptime(status.uptime_secs)
    );
    println!("====================================================");
}

fn print_models(status: &SystemStatus) {
    println!("\nModels ({} loaded)", status.models.len());
    println!("  Name                        | State      | Size     | Req/s");
    println!("  ----------------------------+------------+----------+-------");
    for model in &status.models {
        println!(
            "  {:27} | {:10} | {:>8} | {:>5.1}",
            truncate(&model.name, 27),
            model.state,
            format_bytes(model.size_bytes),
            model.request_count as f64 / status.uptime_secs.max(1) as f64
        );
    }
}

fn print_request_stats(status: &SystemStatus) {
    println!("\nRequest Statistics");
    println!(
        "  Total: {}  Success: {}  Failed: {}",
        status.requests.total_requests,
        status.requests.successful_requests,
        status.requests.failed_requests
    );
    println!(
        "  Throughput: {:.1} req/s    Token Gen: {:.1} tok/s",
        status.requests.requests_per_second, status.requests.tokens_per_second
    );
    println!(
        "  Latency:  Avg {:.1}ms  P50 {:.1}ms  P95 {:.1}ms  P99 {:.1}ms",
        status.requests.avg_latency_ms,
        status.requests.p50_latency_ms,
        status.requests.p95_latency_ms,
        status.requests.p99_latency_ms
    );
}

fn print_resources(status: &SystemStatus) {
    println!("\nResources");
    println!(
        "  Memory: {} / {} ({:.1}%)",
        format_bytes(status.resources.memory_rss_bytes),
        format_bytes(status.resources.memory_limit_bytes),
        status.resources.memory_utilization_percent
    );
    println!(
        "    KV Cache: {}   Arena: {}",
        format_bytes(status.resources.kv_cache_bytes),
        format_bytes(status.resources.arena_bytes)
    );
    println!(
        "  CPU: {:.1}%    Threads: {}",
        status.resources.cpu_utilization_percent, status.resources.active_threads
    );
}

fn print_gpus(status: &SystemStatus) {
    if let Some(ref gpus) = status.gpus {
        if !gpus.is_empty() {
            println!("\nGPUs ({} devices)", gpus.len());
            for gpu in gpus {
                print_gpu(gpu);
            }
        }
    }
}

fn print_gpu(gpu: &GpuStatus) {
    println!(
        "  GPU {}: {} | {} | {:.0}% | {:.0}C",
        gpu.gpu_id,
        truncate(&gpu.name, 26),
        format_bytes(gpu.memory_used_bytes),
        gpu.utilization_percent,
        gpu.temperature_celsius
    );
}

fn print_scheduler(status: &SystemStatus) {
    println!("\nScheduler");
    println!(
        "  Queue Depth: {}   Active Batches: {}   Pending: {}",
        status.scheduler.queue_depth,
        status.scheduler.active_batches,
        status.scheduler.pending_requests
    );
    println!(
        "  Completed: {}   Avg Batch Size: {:.1}",
        status.scheduler.completed_requests, status.scheduler.avg_batch_size
    );
}

fn print_events(status: &SystemStatus) {
    if !status.recent_events.is_empty() {
        println!("\nRecent Events (last {})", status.recent_events.len());
        for event in &status.recent_events {
            let icon = match event.severity {
                EventSeverity::Info => "I",
                EventSeverity::Warning => "W",
                EventSeverity::Error => "E",
            };
            println!(
                "  {} {} {}",
                icon,
                truncate(&event.timestamp, 10),
                truncate(&event.message, 54)
            );
        }
    }
}

/// Format uptime in human-readable form.
pub fn format_uptime(secs: u64) -> String {
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
pub fn format_bytes(bytes: u64) -> String {
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
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
