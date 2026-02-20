# Rust Inference Runtime Comparison

**GG-CORE vs Pure Rust ML Inference Engines**

This document compares GG-CORE against other Rust-based inference runtimes, analyzing performance characteristics, security properties, and use case fit.

---

## Executive Summary

| Runtime | Primary Use Case | Security Model | GG-CORE Advantage |
|---------|------------------|----------------|-------------------|
| **GG-CORE** | Air-gapped enterprise | Process sandbox + IPC | Baseline |
| Candle | Serverless/edge | Pure Rust, no sandbox | +Process isolation, +IPC auth |
| Mistral.rs | LLM inference | Pure Rust, no sandbox | +Multi-tenant, +Rate limiting |
| Tract | On-device ONNX | Pure Rust, no sandbox | +Audit logging, +PII detection |
| Burn | Training + inference | Pure Rust, no sandbox | +Zero network, +Compliance |
| Wonnx | WebGPU/browser | WASM sandbox | +Stronger isolation, +Auth |
| MicroFlow | TinyML/MCU | Static allocation | +Full inference, +Enterprise |
| WasmEdge | Edge/cloud sandbox | WASM isolation | +Native perf, +Lower latency |

---

## Detailed Comparison

### 1. Candle (Hugging Face)

**Focus**: Lightweight, minimalist ML framework for serverless and edge inference.

| Aspect | Candle | GG-CORE | Analysis |
|--------|--------|---------|----------|
| **Language** | Pure Rust | Rust + llama-cpp FFI | Candle avoids C++ |
| **Models** | LLMs, Whisper, SD | GGUF, ONNX | Similar coverage |
| **Sandbox** | None | seccomp/AppContainer | GG-CORE isolated |
| **Network** | Allowed | Blocked | GG-CORE air-gapped |
| **Auth** | None | IPC token auth | GG-CORE authenticated |
| **Deployment** | Library | Single binary | Both deployable |

**When to choose Candle**: Simple inference without isolation needs.
**When to choose GG-CORE**: Enterprise compliance, multi-tenant, air-gap requirements.

---

### 2. Mistral.rs

**Focus**: High-performance Rust-native engine for LLM inference (Mistral, LLaMA).

| Aspect | Mistral.rs | GG-CORE | Analysis |
|--------|------------|---------|----------|
| **Backend** | Candle-based | llama-cpp + ONNX | Different foundations |
| **Quantization** | Native Rust | GGUF Q4/Q5/Q8 | Both support quant |
| **Apple Silicon** | Optimized | Supported | Mistral.rs specialized |
| **Multi-tenant** | No | Yes (via shim) | GG-CORE designed for |
| **Rate limiting** | No | Extension point | GG-CORE extensible |
| **Streaming** | Yes | Yes | Both support |

**Performance Comparison** (estimated):

```
Mistral.rs (M2 Max):     ████████████████████ ~80 tok/s
GG-CORE (M2 Max):        ██████████████       ~50 tok/s (sandboxed)
GG-CORE (Unsandboxed):   ███████████████████  ~75 tok/s
```

**When to choose Mistral.rs**: Maximum raw performance on Apple Silicon.
**When to choose GG-CORE**: Security boundaries, audit requirements, multi-tenant.

---

### 3. Tract (Sonos)

**Focus**: Pure Rust neural network inference engine for ONNX/TensorFlow.

| Aspect | Tract | GG-CORE | Analysis |
|--------|-------|---------|----------|
| **Purity** | 100% Rust | Rust + FFI | Tract fully safe |
| **Formats** | ONNX, TF Lite | GGUF, ONNX | Different focus |
| **Use case** | On-device | Server/air-gap | Different targets |
| **Dependencies** | Minimal | Moderate | Tract lighter |
| **Security features** | None | 55+ injection patterns | GG-CORE hardened |
| **PII detection** | No | 13 types | GG-CORE compliant |

**Inference Performance** (ONNX classification):

| Runtime | P95 Latency | Throughput |
|---------|-------------|------------|
| Tract | ~2 ms | 500 inf/s |
| GG-CORE (sandboxed) | ~85 ms | 12 inf/s |
| GG-CORE (unsandboxed) | ~5 ms | 200 inf/s |

**Security Tax**: GG-CORE trades ~40-90% performance for complete isolation.

**When to choose Tract**: On-device inference, minimal footprint.
**When to choose GG-CORE**: Server deployment, compliance, security hardening.

---

### 4. Burn

**Focus**: Flexible deep learning framework with training and inference.

| Aspect | Burn | GG-CORE | Analysis |
|--------|------|---------|----------|
| **Scope** | Training + Inference | Inference only | Burn broader |
| **Backends** | WGPU, Candle, NdArray | GGUF, ONNX | Different targets |
| **WASM** | Supported | Not supported | Burn more portable |
| **Static binary** | Yes | Yes | Both single-binary |
| **Network** | Allowed | Blocked | GG-CORE isolated |
| **Audit logging** | No | 13 event types | GG-CORE compliant |

**When to choose Burn**: Need training + inference, WASM deployment.
**When to choose GG-CORE**: Inference-only, compliance, security isolation.

---

### 5. Wonnx

**Focus**: 100% Rust ONNX inference with WebGPU acceleration.

| Aspect | Wonnx | GG-CORE | Analysis |
|--------|-------|---------|----------|
| **GPU** | WebGPU | CUDA/Metal | Different APIs |
| **Browser** | Yes | No | Wonnx portable |
| **Sandbox** | WebGPU limits | Process sandbox | Both isolated |
| **Models** | ONNX only | GGUF, ONNX | GG-CORE broader |
| **Auth** | None | IPC tokens | GG-CORE authenticated |

**When to choose Wonnx**: Browser deployment, WebGPU available.
**When to choose GG-CORE**: Server, native performance, LLM support.

---

### 6. MicroFlow

**Focus**: TinyML inference for resource-constrained devices (8-bit MCUs).

| Aspect | MicroFlow | GG-CORE | Analysis |
|--------|-----------|---------|----------|
| **Target** | MCU/IoT | Server | Different scale |
| **Memory** | Static allocation | Dynamic pools | Different strategies |
| **Models** | TinyML | Full LLMs | Different scope |
| **Safety** | Compile-time | Runtime + compile | Both safe |

**When to choose MicroFlow**: Embedded/IoT, extreme constraints.
**When to choose GG-CORE**: Full inference, enterprise scale.

---

### 7. WasmEdge

**Focus**: Sandboxed WebAssembly runtime for edge/cloud deployment.

| Aspect | WasmEdge | GG-CORE | Analysis |
|--------|----------|---------|----------|
| **Isolation** | WASM sandbox | Process sandbox | Both isolated |
| **Performance** | Near-native | Native | GG-CORE faster |
| **Portability** | Universal WASM | OS-specific | WasmEdge portable |
| **Cold start** | Fast | Instant | GG-CORE no VM |
| **GPU** | Limited | Full CUDA/Metal | GG-CORE better |

**Performance Comparison**:

```
Native (GG-CORE):    ████████████████████ 100%
WasmEdge:            ████████████████     ~80% (WASM overhead)
```

**When to choose WasmEdge**: Universal portability, untrusted code.
**When to choose GG-CORE**: Maximum performance, native GPU, enterprise.

---

## Security Comparison Matrix

| Feature | GG-CORE | Candle | Mistral.rs | Tract | Burn | Wonnx | WasmEdge |
|---------|---------|--------|------------|-------|------|-------|----------|
| Memory safety | ✅ Rust | ✅ Rust | ✅ Rust | ✅ Rust | ✅ Rust | ✅ Rust | ✅ WASM |
| Process isolation | ✅ | ❌ | ❌ | ❌ | ❌ | ⚠️ WebGPU | ✅ WASM |
| Network blocked | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ⚠️ Config |
| IPC authentication | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Prompt injection | ✅ 55+ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| PII detection | ✅ 13 types | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Audit logging | ✅ SIEM | ❌ | ❌ | ❌ | ❌ | ❌ | ⚠️ Basic |
| No FFI | ❌ llama-cpp | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Model encryption | ✅ AES-256 | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **FIPS 140-2/3** | ⚠️ Path documented | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **SOC 2 controls** | ⚠️ Mapped | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **HIPAA controls** | ⚠️ Mapped | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

### FIPS 140-2/3 Compliance Path

GG-CORE is the **only Rust inference runtime** with documented FIPS compliance path (not certified):

| Requirement | GG-CORE Implementation |
|-------------|------------------------|
| **Approved algorithms** | AES-256-GCM, PBKDF2-HMAC-SHA256 |
| **Key derivation** | PBKDF2 with 100,000 iterations |
| **Random generation** | OS CSPRNG (getrandom) |
| **Key storage** | Zeroized on drop, no persistent keys |
| **Cryptographic boundary** | Process isolation enforces boundary |
| **Self-tests** | Algorithm validation on startup |

**Compliance path options**:
1. **Hybrid FIPS** (recommended): Use FIPS-validated OpenSSL module for crypto
2. **Full FIPS**: Replace RustCrypto with BoringSSL/OpenSSL FIPS module
3. **FIPS-equivalent**: Current implementation meets algorithm requirements

See [FIPS_ASSESSMENT.md](../security/FIPS_ASSESSMENT.md) and [FIPS_SECURITY_POLICY_DRAFT.md](../security/FIPS_SECURITY_POLICY_DRAFT.md) for details.

### Enterprise Compliance Readiness

> **Note**: GG-CORE provides compliance *readiness* and control mappings, not certifications.
> Actual certification requires third-party audits.

| Framework | GG-CORE Status | Other Rust Runtimes |
|-----------|----------------|---------------------|
| **FIPS 140-2/3** | Path documented, FIPS-approved algorithms | No compliance path |
| **SOC 2 Type II** | Control mappings documented | No audit controls |
| **HIPAA** | Technical safeguards mapped, PII detection | No PHI handling |
| **FedRAMP** | Boundary isolation architecture | No authorization boundary |
| **PCI DSS** | AES-256 encryption, key derivation | No key management |

### Pure Rust Security Advantage

Runtimes without FFI (Candle, Tract, Burn, Mistral.rs) have:
- **Compile-time memory safety**: No buffer overflows, use-after-free
- **Thread safety**: Rust's ownership prevents data races
- **Smaller attack surface**: No C/C++ dependency vulnerabilities

**GG-CORE Trade-off**: Uses llama-cpp FFI for mature GGUF support, but compensates with:
- Process-level isolation (seccomp/AppContainer)
- Network stack completely removed
- IPC-only communication with authentication

---

## Performance Comparison

### Infrastructure Overhead

| Runtime | Request Overhead | Isolation Cost | Total |
|---------|------------------|----------------|-------|
| **GG-CORE** | 361 ns | ~40% | ~361 ns |
| Candle | ~100 ns | 0% | ~100 ns |
| Mistral.rs | ~150 ns | 0% | ~150 ns |
| Tract | ~50 ns | 0% | ~50 ns |
| WasmEdge | ~500 ns | ~20% | ~500 ns |

### LLM Inference (Phi-3 Mini Q4, CPU)

| Runtime | Throughput | Latency (first token) |
|---------|------------|----------------------|
| Mistral.rs | ~80 tok/s | ~50 ms |
| Candle | ~70 tok/s | ~60 ms |
| GG-CORE (sandboxed) | ~50 tok/s | ~100 ms |
| WasmEdge + WASM | ~40 tok/s | ~150 ms |

**Security Tax Visualization**:

```
Unsandboxed Rust:     ████████████████████ 80 tok/s (100%)
GG-CORE Sandboxed:    ████████████         50 tok/s (62%)
Security Cost:        ████████             38% overhead
```

---

## Use Case Recommendations

### Choose GG-CORE When:

1. **Air-gapped deployment** - Zero network is mandatory
2. **Multi-tenant** - Need isolation between tenants
3. **Compliance** - SOC 2, HIPAA, FIPS requirements
4. **Enterprise** - Audit logging, PII detection, encryption
5. **Hostile input** - Prompt injection protection needed

### Choose Candle/Mistral.rs When:

1. **Maximum performance** - No security overhead acceptable
2. **Serverless** - Cold start latency critical
3. **Apple Silicon** - Native optimization priority
4. **Simple deployment** - No isolation requirements

### Choose Tract When:

1. **On-device** - Edge/mobile deployment
2. **Minimal footprint** - Binary size matters
3. **ONNX-only** - No LLM requirements

### Choose WasmEdge When:

1. **Universal portability** - Run anywhere
2. **Untrusted code** - Sandboxing unknown models
3. **Browser + server** - Same runtime everywhere

---

## Conclusion

GG-CORE occupies a unique position in the Rust inference ecosystem:

| Strength | Trade-off |
|----------|-----------|
| Process-level isolation | ~40% performance cost |
| Zero network attack surface | No cloud model fetching |
| Enterprise compliance | Higher complexity |
| Multi-tenant extensible | Requires integration |
| Prompt injection protection | Additional latency |

**GG-CORE is the only Rust runtime purpose-built for:**
- Air-gapped, compliance-sensitive environments
- Multi-tenant AI with resource governance
- Enterprise security requirements (SOC 2, HIPAA)

For raw performance without security constraints, Candle/Mistral.rs are faster. For universal portability, WasmEdge excels. For enterprise security, GG-CORE is unmatched.

---

Copyright 2024-2026 GG-CORE Contributors
