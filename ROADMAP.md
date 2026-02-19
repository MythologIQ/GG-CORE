# Veritas SDR Feature Roadmap

**Last Updated:** 2026-02-19
**Version:** 0.6.0

---

## Current Status: Pre-Production

Veritas SDR is in pre-production status. Core functionality is complete and tested, but production deployment requires additional work.

---

## Completed Features (v0.1.0)

### Core Runtime

- [x] IPC-based communication (no HTTP overhead)
- [x] Dual backend support (GGUF + ONNX)
- [x] Memory management with arena allocation
- [x] Work-stealing scheduler
- [x] Streaming output support

### Security

- [x] Prompt injection detection (55+ patterns)
- [x] PII detection (13 types)
- [x] Output sanitization
- [x] Model encryption (AES-256)
- [x] Sandbox isolation (Windows Job Objects)
- [x] Rate limiting
- [x] Audit logging

### Performance

- [x] KV Cache with paged attention
- [x] Speculative decoding v2
- [x] SIMD tokenizer v2
- [x] Thread pool tuning

### Testing

- [x] Tier 1: Unit tests (passing)
- [x] Tier 2: Integration tests (37/37 passing)
- [x] Tier 3: Optimization tests (30/30 passing)
- [x] Security tests (43/43 passing)

### Documentation

- [x] README with transparent claims
- [x] Usage guide
- [x] Comparative analysis
- [x] Honest assessment document
- [x] Rust enterprise analysis
- [x] Dependency analysis

---

## Completed Features (v0.2.0)

### GPU Support

- [x] CUDA backend implementation (cudarc bindings)
- [x] Metal backend (macOS with metal crate)
- [x] GPU memory management with actual allocations
- [x] Multi-GPU support (layer/tensor/pipeline parallelism)
- [x] Flash attention GPU kernels

**Status:** Framework implemented with CUDA/Metal bindings

### GPU Architecture

- [`GpuManager`](core-runtime/src/engine/gpu.rs) - Device detection and memory management
- [`CudaBackend`](core-runtime/src/engine/cuda.rs) - NVIDIA CUDA support via cudarc
- [`MetalBackend`](core-runtime/src/engine/metal.rs) - Apple Metal support (macOS)
- [`MultiGpuManager`](core-runtime/src/engine/multi_gpu.rs) - Multi-GPU coordination
- [`FlashAttnGpu`](core-runtime/src/engine/flash_attn_gpu.rs) - Memory-efficient attention

### GPU Features

- Device detection and capability querying
- Memory pool allocation
- Compute capability checks (CUDA 8.0+ for flash attention)
- Unified memory support (Apple Silicon)
- P2P communication detection
- Automatic strategy selection for multi-GPU

---

## Completed Features (v0.2.1)

### Benchmarking

- [x] llama.cpp direct comparison
- [x] Security overhead measurement
- [x] GPU vs CPU comparison
- [x] Memory profiling

**Status:** Complete

---

## Completed Features (v0.3.0)

### C FFI API

- [x] Opaque runtime and session handles
- [x] Error codes with thread-local messages
- [x] Blocking inference
- [x] Callback-based streaming
- [x] Model management
- [x] Health check API
- [x] cbindgen header generation

### Python Bindings

- [x] PyO3-based native module
- [x] Sync and async session support
- [x] Context manager protocol
- [x] Iterator-based streaming
- [x] Exception hierarchy
- [x] Type stubs (PEP 561)

### API Files

- [`ffi/`](core-runtime/src/ffi/) - C API implementation (8 files)
- [`python/`](core-runtime/src/python/) - Python bindings (6 files)
- [`include/veritas_sdr.h`](core-runtime/include/veritas_sdr.h) - Generated C header
- [`python/`](core-runtime/python/) - Python package structure

**Status:** Complete

---

## Completed Features (v0.4.0)

### Observability

- [x] Prometheus-compatible metrics export (text format)
- [x] Bucketed histograms with configurable boundaries
- [x] OpenTelemetry span export via IPC
- [x] Span collector with buffer management

### Observability Architecture

- [`BucketedHistogram`](core-runtime/src/telemetry/buckets.rs) - Prometheus-style bucketed histograms
- [`PrometheusEncoder`](core-runtime/src/telemetry/prometheus.rs) - Text format export
- [`SpanCollector`](core-runtime/src/telemetry/span_export.rs) - OpenTelemetry span collection
- [`MetricsStore`](core-runtime/src/telemetry/store.rs) - Enhanced with bucketed histogram support

### Mixture of Experts (MoE)

- [x] MoE configuration (Mixtral/DeepSeek presets)
- [x] Linear router with top-k selection
- [x] Expert output combination with weighted sum
- [x] Load balancing statistics and auxiliary loss
- [x] Sequential and parallel execution support

### MoE Architecture

- [`MoeConfig`](core-runtime/src/engine/moe/config.rs) - Configuration for MoE layers
- [`LinearRouter`](core-runtime/src/engine/moe/router.rs) - Gating network for expert selection
- [`ExpertCombiner`](core-runtime/src/engine/moe/combiner.rs) - Output combination
- [`MoeExecutor`](core-runtime/src/engine/moe/executor.rs) - Expert execution orchestration

**Status:** Complete

---

## Completed Features (v0.5.0)

### CLI Health Probes (Alcatraz-compliant)

- [x] Exec-based health probes for K8s (NO HTTP)
- [x] `veritas-sdr health|live|ready` subcommands
- [x] IPC client for CLI-to-runtime communication

### CLI Architecture

- [`cli/mod.rs`](core-runtime/src/cli/mod.rs) - CLI module with socket path config
- [`cli/ipc_client.rs`](core-runtime/src/cli/ipc_client.rs) - IPC client for CLI
- [`cli/health.rs`](core-runtime/src/cli/health.rs) - Health check commands

### Model Registry Enhancements

- [x] Semantic versioning with range matching
- [x] Model query builder pattern
- [x] JSON-based registry persistence
- [x] Version history tracking for rollback

### Registry Architecture

- [`models/version.rs`](core-runtime/src/models/version.rs) - ModelVersion, VersionRange
- [`models/search.rs`](core-runtime/src/models/search.rs) - ModelQuery, ModelSearchResult
- [`models/persistence.rs`](core-runtime/src/models/persistence.rs) - RegistryPersistence
- [`models/history.rs`](core-runtime/src/models/history.rs) - VersionHistory

### A/B Testing Foundation

- [x] Variant labels and definitions
- [x] Traffic splitting with sticky sessions
- [x] Per-variant metrics collection

### A/B Testing Architecture

- [`ab_testing/variant.rs`](core-runtime/src/ab_testing/variant.rs) - VariantLabel, Variant
- [`ab_testing/traffic/`](core-runtime/src/ab_testing/traffic/) - TrafficConfig, TrafficSplitter, bucket allocation
- [`ab_testing/metrics/`](core-runtime/src/ab_testing/metrics/) - VariantStats, VariantMetrics, snapshots

### Kubernetes Operator Foundation

- [x] VeritasRuntime CRD
- [x] VeritasModel CRD (with A/B variant support)
- [x] Helm chart with exec probes (NO HTTP)

### K8s Architecture

- [`k8s/types.rs`](core-runtime/src/k8s/types.rs) - CRD Rust types
- [`k8s/crds/`](k8s/crds/) - CRD YAML definitions
- [`k8s/helm/veritas-sdr/`](k8s/helm/veritas-sdr/) - Helm chart

**Status:** Complete

---

## Completed Features (v0.6.0)

### Functional GGUF Backend

- [x] Real model loading via llama-cpp-2 (v0.1.133)
- [x] Tokenization and detokenization support
- [x] Token streaming via async channels
- [x] Context management and batch processing
- [x] UTF-8 decoding for token pieces (encoding_rs)

### GGUF Architecture

- [`engine/gguf/backend.rs`](core-runtime/src/engine/gguf/backend.rs) - LlamaBackendInner implementation
- [`engine/tokenizer.rs`](core-runtime/src/engine/tokenizer.rs) - Unified tokenizer with backend delegation
- [`engine/gguf/generator.rs`](core-runtime/src/engine/gguf/generator.rs) - Model loading and generation

### Functional IPC Server

- [x] Platform-specific server loop (Unix sockets / Windows named pipes)
- [x] 4-byte length-prefixed framing protocol
- [x] Connection pooling with configurable limits
- [x] Graceful shutdown with request draining
- [x] Owned connection guards for async tasks

### IPC Architecture

- [`ipc/server.rs`](core-runtime/src/ipc/server.rs) - Server loop implementation
- [`ipc/connections.rs`](core-runtime/src/ipc/connections.rs) - Connection pooling and guards
- [`main.rs`](core-runtime/src/main.rs) - Signal handling and server integration

### Chaos Testing Suite

- [x] Protocol fault injection tests (malformed JSON, truncated messages)
- [x] Type confusion and extreme payload testing
- [x] Scheduler shutdown resilience tests
- [x] Health check chaos testing
- [x] Stream and model chaos testing
- [x] IPC server integration tests (framing, connections, routing)

### Test Architecture

- [`tests/chaos_resilience_test.rs`](core-runtime/tests/chaos_resilience_test.rs) - Protocol fault injection
- [`tests/ipc_server_test.rs`](core-runtime/tests/ipc_server_test.rs) - Server integration tests
- Plus 3 additional chaos test files for scheduler, health, and streaming

### Build System Improvements

- [x] Binary renamed to `veritas-sdr-cli` (fixes PDB collision)
- [x] Removed bincode (incompatible with internally-tagged enums)
- [x] Version-pinned llama-cpp-2 to 0.1.133
- [x] Added encoding_rs for UTF-8 token decoding
- [x] Readable mock output for development mode

**Status:** Complete

**Test Coverage:** 1,124 tests (100% pass rate)

---

## Planned Features (v0.7.0)

### Security

- [ ] Independent security audit
- [ ] SOC 2 compliance preparation
- [ ] FIPS 140-2 consideration

### Operations

- [ ] Canary deployment automation
- [ ] Blue-green deployment support

---

## Future Considerations

### Research

- [ ] Custom quantization methods
- [ ] Novel attention mechanisms
- [ ] Hardware-specific optimizations

### Ecosystem

- [ ] Plugin system
- [ ] Community model zoo
- [ ] Integration with ML frameworks

---

## Release Timeline

| Version   | Status    | Focus                                    |
| --------- | --------- | ---------------------------------------- |
| **0.1.0** | Complete  | Core functionality, security             |
| **0.2.0** | Complete  | GPU support (CUDA/Metal)                 |
| **0.2.1** | Complete  | Benchmarking, comparison                 |
| **0.3.0** | Complete  | C FFI & Python bindings                  |
| **0.4.0** | Complete  | Observability, MoE                       |
| **0.5.0** | Complete  | Enterprise features                      |
| **0.6.0** | Complete  | Functional GGUF backend, IPC server      |
| **0.7.0** | Planned   | Security audit, deployments              |
| **1.0.0** | Planned   | Production stable release                |

---

## Contributing

We welcome contributions! See [CLA.md](CLA.md) for contributor license agreement.

### Priority Areas

1. **GPU Support** - CUDA/Metal implementation
2. **Benchmarking** - Fair performance comparisons
3. **Documentation** - Examples, tutorials
4. **Testing** - Edge cases, stress tests

---

## Feedback

- **Issues:** GitHub Issues
- **Security:** See [SECURITY.md](SECURITY.md)
- **Discussions:** GitHub Discussions

---

Copyright 2024-2026 Veritas SDR Contributors  
Licensed under the Apache License, Version 2.0
