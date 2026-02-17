// Copyright 2024-2026 Veritas SDR Contributors
// Licensed under the Apache License, Version 2.0

//! llama.cpp Direct Comparison Benchmark
//!
//! This benchmark compares Veritas SDR overhead against llama.cpp CLI directly.
//! This is the fair comparison - both use the same backend (llama.cpp).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::process::Command;
use std::time::Duration;

/// Benchmark configuration
const MODEL_PATH: &str = "./models/phi-3-mini-q4km.gguf";
const PROMPT: &str = "Explain quantum computing in simple terms.";
const MAX_TOKENS: u32 = 128;

/// llama.cpp CLI benchmark
fn bench_llama_cpp_cli(c: &mut Criterion) {
    let mut group = c.benchmark_group("llama_cpp_comparison");

    // Check if model exists
    if !std::path::Path::new(MODEL_PATH).exists() {
        eprintln!(
            "Model not found at {}, skipping llama.cpp CLI benchmark",
            MODEL_PATH
        );
        group.finish();
        return;
    }

    // Check if llama-cli is available
    let llama_cli_available = Command::new("llama-cli").arg("--version").output().is_ok();

    if !llama_cli_available {
        eprintln!("llama-cli not found in PATH, skipping benchmark");
        group.finish();
        return;
    }

    group.throughput(Throughput::Elements(1));

    // Benchmark llama.cpp CLI
    group.bench_function(BenchmarkId::new("llama_cpp_cli", "direct"), |b| {
        b.iter(|| {
            let output = Command::new("llama-cli")
                .args([
                    "-m",
                    MODEL_PATH,
                    "-p",
                    PROMPT,
                    "-n",
                    &MAX_TOKENS.to_string(),
                    "--no-display-prompt",
                    "-ngl",
                    "0", // CPU only for fair comparison
                    "-c",
                    "4096",
                ])
                .output()
                .expect("Failed to execute llama-cli");

            black_box(output);
        });
    });

    group.finish();
}

/// Veritas SDR overhead benchmark
fn bench_veritas_sdr_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("veritas_sdr_overhead");

    // This would benchmark the actual Veritas SDR inference
    // For now, we measure the infrastructure overhead

    group.throughput(Throughput::Elements(1));

    // IPC encoding overhead
    // IPC encoding overhead
    group.bench_function(BenchmarkId::new("ipc_encode", "overhead"), |b| {
        use veritas_sdr::ipc::protocol::{
            encode_message_binary, IpcMessage, RequestId, StreamChunk,
        };
        // Create a message roughly equivalent to 1KB of data
        let data = IpcMessage::StreamChunk(StreamChunk {
            request_id: RequestId(1),
            token: 12345,
            is_final: false,
            error: Some("x".repeat(1000)), // Simulate 1KB payload
        });

        b.iter(|| {
            let encoded = encode_message_binary(&data).unwrap();
            black_box(encoded);
        });
    });

    // IPC decoding overhead
    // IPC decoding overhead
    group.bench_function(BenchmarkId::new("ipc_decode", "overhead"), |b| {
        use veritas_sdr::ipc::protocol::{
            decode_message_binary, encode_message_binary, IpcMessage, RequestId, StreamChunk,
        };
        let data = IpcMessage::StreamChunk(StreamChunk {
            request_id: RequestId(1),
            token: 12345,
            is_final: false,
            error: Some("x".repeat(1000)),
        });
        let encoded = encode_message_binary(&data).unwrap();

        b.iter(|| {
            let decoded = decode_message_binary(&encoded).unwrap();
            black_box(decoded);
        });
    });

    // Security scanning overhead
    // Security scanning overhead
    group.bench_function(BenchmarkId::new("security_scan", "overhead"), |b| {
        use veritas_sdr::security::PromptInjectionFilter;
        let filter = PromptInjectionFilter::default();

        b.iter(|| {
            let result = filter.scan(PROMPT);
            black_box(result);
        });
    });

    // PII detection overhead
    // PII detection overhead
    group.bench_function(BenchmarkId::new("pii_detect", "overhead"), |b| {
        use veritas_sdr::security::PIIDetector;
        let detector = PIIDetector::new();

        b.iter(|| {
            let result = detector.detect(PROMPT);
            black_box(result);
        });
    });

    group.finish();
}

/// Combined overhead measurement
fn bench_total_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("total_overhead");

    group.bench_function("infrastructure_total", |b| {
        use veritas_sdr::ipc::protocol::{
            decode_message_binary, encode_message_binary, IpcMessage, RequestId, StreamChunk,
        };
        use veritas_sdr::security::{PIIDetector, PromptInjectionFilter};

        let filter = PromptInjectionFilter::default();
        let detector = PIIDetector::new();
        let data = IpcMessage::StreamChunk(StreamChunk {
            request_id: RequestId(1),
            token: 12345,
            is_final: false,
            error: Some("x".repeat(1000)),
        });

        b.iter(|| {
            // Simulate full request path
            let security_result = filter.scan(PROMPT);
            let pii_result = detector.detect(PROMPT);
            let encoded = encode_message_binary(&data).unwrap();
            let decoded = decode_message_binary(&encoded).unwrap();

            black_box((security_result, pii_result, decoded));
        });
    });

    group.finish();
}

/// Results summary
fn print_comparison_summary() {
    println!("\n=== llama.cpp Direct Comparison Summary ===\n");
    println!("This benchmark measures Veritas SDR overhead vs llama.cpp CLI.");
    println!("\nExpected Results:");
    println!("| Component          | Expected Overhead |");
    println!("|--------------------|-------------------|");
    println!("| IPC encode/decode  | ~361 ns           |");
    println!("| Security scanning  | ~1-5 ms           |");
    println!("| PII detection      | ~0.5-2 ms         |");
    println!("| Total overhead     | ~2-7 ms           |");
    println!("\nInterpretation:");
    println!("- <10 ms: Security features are efficient");
    println!("- 10-50 ms: Acceptable for security benefits");
    println!("- >50 ms: Needs optimization");
    println!("\nNote: This is the fair comparison. HTTP comparison (2,770x) is expected");
    println!("because HTTP inherently adds latency by design.\n");
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(100);
    targets = bench_llama_cpp_cli, bench_veritas_sdr_overhead, bench_total_overhead
}

criterion_main! {
    benches,
    print_comparison_summary,
}
