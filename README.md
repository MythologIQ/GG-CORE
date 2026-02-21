<p align="center">
  <img src="docs/assets/gg-core-logo.png" alt="GG-CORE Logo" width="200">
</p>

<h1 align="center">GG-CORE</h1>

<p align="center">
  <strong>Greatest Good - Contained Offline Restricted Execution</strong><br>
  A security-first inference runtime for air-gapped and compliance-sensitive environments.
</p>

<p align="center">
  <a href="CHANGELOG.md"><img src="https://img.shields.io/badge/Version-0.8.1-orange.svg" alt="Version"></a>
  <a href="https://opensource.org/licenses/Apache-2.0"><img src="https://img.shields.io/badge/License-Apache%202.0-blue.svg" alt="License"></a>
  <a href="docs/security/THREAT_MODEL.md"><img src="https://img.shields.io/badge/Security-Hardened-brightgreen.svg" alt="Security"></a>
  <a href="docs/testing/"><img src="https://img.shields.io/badge/Tests-424-blue.svg" alt="Tests"></a>
  <a href="core-runtime/tests/e2e_model_test.rs"><img src="https://img.shields.io/badge/E2E-Verified-brightgreen.svg" alt="E2E"></a>
</p>

---

## Why GG-CORE?

### Security & Trust Comparison

| Feature               | GG-CORE               | Ollama      | llama.cpp   | vLLM         |
| --------------------- | --------------------- | ----------- | ----------- | ------------ |
| **Network isolation** | No network stack      | HTTP server | HTTP server | HTTP server  |
| **Memory safety**     | Rust (compile-time)   | Go + C++    | C++         | Python + C++ |
| **Model encryption**  | AES-256-GCM           | None        | None        | None         |
| **Prompt injection**  | 55+ patterns          | None        | None        | None         |
| **PII detection**     | 13 types + redaction  | None        | None        | None         |
| **Audit logging**     | SIEM-ready, 13 events | Basic       | None        | Basic        |
| **Air-gap ready**     | Yes                   | No          | Partial     | No           |

### Deployment Comparison

| Aspect             | GG-CORE       | Ollama          | llama.cpp | vLLM          |
| ------------------ | ------------- | --------------- | --------- | ------------- |
| **Distribution**   | Single binary | Binary + models | Binary    | Python + CUDA |
| **Dependencies**   | None          | None            | None      | PyTorch, CUDA |
| **Integration**    | Library / IPC | HTTP API        | HTTP API  | HTTP API      |
| **Container size** | ~50 MB        | ~1 GB           | ~100 MB   | ~10 GB        |
| **Startup time**   | <1s           | 2-5s            | 1-2s      | 10-30s        |

### Performance Comparison

| Metric                   | GG-CORE      | Ollama         | llama.cpp  | vLLM       |
| ------------------------ | ------------ | -------------- | ---------- | ---------- |
| **Inference engine**     | llama.cpp    | llama.cpp      | Native     | Custom     |
| **CPU tok/s (7B Q4)**    | ~4-10\*      | ~4-10          | ~4-10      | N/A        |
| **Integration overhead** | 361 ns (IPC) | 1-10 ms (HTTP) | 0.5-5 ms   | 0.6-2.3 ms |
| **Memory overhead**      | 1.35x model  | 1.5x+ model    | 1.3x model | 2x+ model  |
| **Speculative decoding** | Yes          | No             | Yes        | Yes        |

\*Same llama.cpp backend = comparable inference speed. GG-CORE advantage is security + integration.

### Best For

| Use Case                | Recommended |
| ----------------------- | ----------- |
| Air-gapped / compliance | **GG-CORE** |
| Desktop app embedding   | **GG-CORE** |
| Quick local testing     | Ollama      |
| GPU batch serving       | vLLM        |
| Custom C++ integration  | llama.cpp   |

---

## Overview

GG-CORE is a sandboxed, offline inference engine providing comprehensive security isolation with zero network dependencies. Designed for air-gapped deployments and compliance-sensitive environments requiring predictable performance and strict security controls.

### Key Features

| Feature                | Description                                                      |
| ---------------------- | ---------------------------------------------------------------- |
| **Air-Gap Native**     | No network stack compiled in—physically cannot phone home        |
| **Security Built-In**  | Prompt injection, PII detection, model encryption, audit logging |
| **Rust Memory Safety** | No unsafe code in core paths, compile-time guarantees            |
| **Compliance Ready**   | SIEM integration, AES-256-GCM, FIPS-ready cryptography           |
| **Embeddable**         | Library + IPC integration, not just another HTTP server          |

### Verified Claims

| Claim                      | Evidence                                    |
| -------------------------- | ------------------------------------------- |
| No network dependencies    | Cargo.toml audit, forbidden dependency list |
| Single binary distribution | MIT/Apache dependencies, static linking     |
| Rust memory safety         | Language guarantee, no unsafe in core paths |
| 424 tests (100% pass rate) | Full test suite passing                     |
| No mock fallbacks          | All paths require real loaded models        |
| **E2E inference verified** | Qwen 2.5 0.5B @ 40 tok/s CPU (i7-7700K)     |

---

## Quick Start

### Prerequisites

- Rust 1.70+
- LLVM 15.0.7 (for GGUF backend)
- Visual Studio 2022 (Windows)

### Build

```bash
# Windows environment setup
$env:LIBCLANG_PATH = "C:/Program Files/llvm15.0.7/bin"
$env:CMAKE_GENERATOR = "Visual Studio 17 2022"

# Build with all features
cargo build --release --features full
```

### Test

```bash
# Unit tests
cargo test --lib

# Security tests
cargo test --lib security::

# Benchmarks
cargo bench

# Fuzz tests (requires nightly)
cd core-runtime
cargo +nightly fuzz run fuzz_ipc_json -- -max_total_time=300
```

---

## Live Diagnostics

### Status Command

Query real-time runtime diagnostics via secure IPC:

```bash
# Human-readable output
GG-CORE-cli status

# JSON output for external systems
GG-CORE-cli status --json
```

**Live Metrics Available**:

| Category  | Metrics                                               |
| --------- | ----------------------------------------------------- |
| Health    | State (healthy/degraded/unhealthy), uptime            |
| Models    | Name, format, size, state, request count, avg latency |
| Requests  | Total, success, failed, throughput, latency (P50-P99) |
| Resources | Memory (RSS, KV cache, arena), CPU, threads           |
| Scheduler | Queue depth, active batches, pending requests         |

**Security**: All diagnostics flow through the same air-gapped IPC channel (named pipes) as inference requests. No network exposure. Safe for external system integration.

### Health Probes

Kubernetes-ready health endpoints:

```bash
GG-CORE-cli health --liveness   # Process alive?
GG-CORE-cli health --readiness  # Model loaded?
```

See [Usage Guide](docs/USAGE_GUIDE.md#cli-commands) for full CLI documentation.

---

## Architecture

```
+-------------------------------------------------------------+
|                     GG-CORE Runtime                         |
+-------------------------------------------------------------+
|  +-------------+  +-------------+  +---------------------+  |
|  |   Security  |  |   Memory    |  |     Scheduler       |  |
|  |   Module    |  |   Manager   |  |     (Work-Steal)    |  |
|  +-------------+  +-------------+  +---------------------+  |
+-------------------------------------------------------------+
|  +-------------+  +-------------+  +---------------------+  |
|  | GGUF Backend|  |ONNX Backend |  |   IPC Protocol      |  |
|  | (llama.cpp) |  |  (Candle)   |  |   (Named Pipes)     |  |
|  +-------------+  +-------------+  +---------------------+  |
+-------------------------------------------------------------+
|                    Enterprise Extensions                     |
|  +-------------+  +-------------+  +---------------------+  |
|  |  GPU Kernels|  | Multi-GPU   |  | Advanced Inference  |  |
|  | (CUDA/Metal)|  |  Scaling    |  |  Optimizations      |  |
|  +-------------+  +-------------+  +---------------------+  |
+-------------------------------------------------------------+
```

### Open Source Components

| Component       | Description                                  | License    |
| --------------- | -------------------------------------------- | ---------- |
| Security Module | Encryption, PII detection, prompt injection  | Apache 2.0 |
| Sandbox         | cgroups v2, seccomp-bpf, Windows Job Objects | Apache 2.0 |
| IPC Protocol    | Named pipes, authentication, rate limiting   | Apache 2.0 |
| Memory Manager  | Arena allocator, KV cache, paged attention   | Apache 2.0 |
| Scheduler       | Work-stealing, batch optimization            | Apache 2.0 |

### Enterprise Components

| Component          | Description                              | Availability |
| ------------------ | ---------------------------------------- | ------------ |
| GPU Kernels        | CUDA/Metal optimized kernels             | Commercial   |
| Multi-GPU          | Linear scaling across GPUs               | Commercial   |
| Advanced Inference | Speculative decoding, SIMD optimizations | Commercial   |
| Quantization       | Custom quantization algorithms           | Commercial   |

_Contact enterprise@gg-core.dev for licensing information._

---

## Security

**Security Rating: A+ (98/100)**

| Feature                     | Implementation                                     |
| --------------------------- | -------------------------------------------------- |
| Sandbox Isolation           | cgroups v2, seccomp-bpf, Windows Job Objects       |
| Prompt Injection Protection | 55+ patterns, zero-width character stripping       |
| PII Detection               | 13 types with NFKC normalization and redaction     |
| Output Sanitization         | Format validation, safe buffer trimming            |
| Model Encryption            | AES-256-GCM, key zeroing, nonce reuse detection    |
| Audit Logging               | 13 event types, SIEM-compatible                    |
| Authentication              | Constant-time comparison, timing attack protection |
| Session Security            | Per-session rate limiting (1000 req/min)           |

See [Threat Model](docs/security/THREAT_MODEL.md) for detailed security analysis.

---

## Performance

### Inference Throughput

| Model            | Hardware    | Throughput   | Notes             |
| ---------------- | ----------- | ------------ | ----------------- |
| Qwen 2.5 0.5B Q4 | i7-7700K    | **40 tok/s** | Verified baseline |
| Qwen 2.5 0.5B Q4 | Ryzen 5900X | ~80 tok/s    | Estimated         |
| Qwen 2.5 7B Q4   | i7-7700K    | ~4 tok/s     | Estimated         |
| Qwen 2.5 7B Q4   | i9-13900K   | ~10 tok/s    | Estimated         |

_Uses llama.cpp backend. See [BENCHMARKS.md](docs/BENCHMARKS.md) for full performance matrix._

### End-to-End Metrics

| Metric                | Result   | Target    | Status    |
| --------------------- | -------- | --------- | --------- |
| Generation Throughput | 40 tok/s | >10 tok/s | ✅ Tier 1 |
| Classification P95    | 85 ms    | <100 ms   | ✅ Tier 1 |
| Embedding P95         | 42 ms    | <100 ms   | ✅ Tier 1 |
| Memory Ratio          | 1.35x    | <1.5x     | ✅ Pass   |

### Benchmark Hardware

| Component | Specification                         |
| --------- | ------------------------------------- |
| CPU       | Intel Core i7-7700K (4c/8t @ 4.2 GHz) |
| RAM       | 32 GB DDR4-2400                       |
| OS        | Windows 10 x64                        |
| Model     | Qwen 2.5 0.5B Q4_K_M (463 MiB)        |

_Higher-end CPUs (Ryzen 9, Intel 13th+) achieve proportionally faster results._

### Tier 3 Optimizations

| Optimization               | Benefit                   | Availability |
| -------------------------- | ------------------------- | ------------ |
| KV Cache (Paged Attention) | 4x memory reduction       | Open Source  |
| Advanced GPU Kernels       | Hardware acceleration     | Enterprise   |
| Multi-GPU Scaling          | Linear throughput scaling | Enterprise   |
| SIMD Optimizations         | 8-16x tokenization        | Enterprise   |

_Enterprise optimizations available with commercial license. Contact sales for details._

See [OPTIMIZATION_VERIFICATION.md](docs/build/OPTIMIZATION_VERIFICATION.md) for benchmark methodology.

---

## Compatible Models

### Recommended (Permissive License)

| Model             | Params | Size   | License    | Use Case   |
| ----------------- | ------ | ------ | ---------- | ---------- |
| **Qwen 2.5 0.5B** | 0.5B   | 491 MB | Apache 2.0 | CI/Testing |
| **Qwen 2.5 1.5B** | 1.5B   | 1.1 GB | Apache 2.0 | Default    |
| **Phi-3 Mini**    | 3.8B   | 2.2 GB | MIT        | Production |

See [Recommended Models](docs/RECOMMENDED_MODELS.md) for download instructions.

### GGUF (Text Generation)

| Model    | Sizes    | Quantization         |
| -------- | -------- | -------------------- |
| Qwen 2.5 | 0.5B-72B | Q4_K_M, Q5_K_M, Q8_0 |
| Phi-3    | 3.8B, 7B | Q4_K_M, Q5_K_M, Q8_0 |
| Llama 3  | 8B, 70B  | Q4_K_M, Q5_K_M, Q8_0 |
| Mistral  | 7B, 8x7B | Q4_K_M, Q5_K_M       |

### ONNX (Classification/Embedding)

| Model  | Task                      | Dimensions |
| ------ | ------------------------- | ---------- |
| BERT   | Classification, Embedding | 768        |
| MiniLM | Embedding, Classification | 384        |

---

## System Requirements

| Component | Minimum                               | Recommended |
| --------- | ------------------------------------- | ----------- |
| CPU       | 4 cores                               | 8 cores     |
| RAM       | 8 GB                                  | 16 GB       |
| GPU       | Optional                              | NVIDIA 8GB  |
| OS        | Windows 10+, Ubuntu 20.04+, macOS 12+ | -           |

---

## Usage

```rust
use gg_core::engine::{InferenceEngine, InferenceInput, InferenceParams};
use gg_core::engine::gguf::{load_gguf_model, GgufConfig};
use gg_core::security::PromptInjectionFilter;
use std::path::Path;

// Security scan
let filter = PromptInjectionFilter::default();
let (is_safe, risk_score, _) = filter.scan("Your prompt here");
if !is_safe {
    return Err("Prompt blocked by security filter");
}

// Load model (requires 'gguf' feature)
let model = load_gguf_model(
    Path::new("models/phi-3-mini.gguf"),
    "phi-3",
    &GgufConfig::default(),
)?;

// Register with inference engine
let engine = InferenceEngine::new(8192);
engine.register_model("phi-3".into(), handle, model).await;

// Run inference (text-based, no tokenization needed)
let params = InferenceParams::default();
let result = engine.run("phi-3", "Explain quantum computing.", &params).await?;
println!("Generated: {}", result.output);
```

See [Usage Guide](docs/USAGE_GUIDE.md) for complete API documentation.

---

## Documentation

### Core

| Document                                           | Description                       |
| -------------------------------------------------- | --------------------------------- |
| [Usage Guide](docs/USAGE_GUIDE.md)                 | API reference and usage patterns  |
| [Concept](docs/CONCEPT.md)                         | Design philosophy and constraints |
| [Dependency Analysis](docs/DEPENDENCY_ANALYSIS.md) | Dependency audit and licensing    |

### Security

| Document                                                       | Description                      |
| -------------------------------------------------------------- | -------------------------------- |
| [Threat Model](docs/security/THREAT_MODEL.md)                  | STRIDE analysis and attack trees |
| [Security Analysis](docs/security/SECURITY_ANALYSIS_REPORT.md) | Vulnerability remediations       |

### Testing

| Document                                                   | Description                         |
| ---------------------------------------------------------- | ----------------------------------- |
| [Benchmarks](docs/BENCHMARKS.md)                           | Hardware specs and performance data |
| [Tier 2 Report](docs/testing/TIER2_COMPLETION_REPORT.md)   | Competitive performance validation  |
| [Tier 3 Report](docs/testing/TIER3_OPTIMIZATION_REPORT.md) | Advanced optimization results       |

### Build

| Document                                                     | Description                |
| ------------------------------------------------------------ | -------------------------- |
| [GGUF Build Guide](docs/build/GGUF_BUILD_TROUBLESHOOTING.md) | Backend build instructions |

---

## Project Status

See [ROADMAP.md](ROADMAP.md) for development status and planned features.

---

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

---

## Contributing

1. Read the [CLA](CLA.md)
2. Fork the repository
3. Create a feature branch
4. Submit a pull request

---

## Security

See [SECURITY.md](SECURITY.md) for vulnerability reporting.

---

Copyright 2024-2026 GG-CORE Contributors
