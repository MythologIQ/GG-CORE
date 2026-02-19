# Recommended Models for Veritas SDR

**Version**: 0.6.7
**Last Updated**: 2026-02-19

---

## Overview

Veritas SDR supports GGUF and ONNX model formats. This document lists recommended models with permissive licenses (MIT/Apache 2.0) suitable for bundling and commercial use.

## Tiered Model Strategy

| Tier | Model | Size | License | Use Case |
|------|-------|------|---------|----------|
| **CI/Testing** | Qwen 2.5 0.5B Q4_K_M | 491 MB | Apache 2.0 | Unit tests, CI pipelines |
| **Default** | Qwen 2.5 1.5B Q4_K_M | 1.1 GB | Apache 2.0 | Standard installation |
| **Quality** | Phi-3 Mini Q4_K_M | 2.2 GB | MIT | Production, best quality |

---

## Model Details

### Tier 1: CI/Testing - Qwen 2.5 0.5B

**Source**: [Qwen/Qwen2.5-0.5B-Instruct-GGUF](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF)

| Property | Value |
|----------|-------|
| Parameters | 0.5B |
| Context Length | 32K tokens |
| License | Apache 2.0 |
| Quantization | Q4_K_M |
| File Size | 491 MB |
| RAM Required | ~1 GB |

**Download**:
```bash
huggingface-cli download Qwen/Qwen2.5-0.5B-Instruct-GGUF \
  qwen2.5-0.5b-instruct-q4_k_m.gguf \
  --local-dir models/ \
  --local-dir-use-symlinks False
```

**Use Cases**:
- Continuous integration pipelines
- Unit and integration tests
- Development/debugging
- Quick smoke tests

---

### Tier 2: Default - Qwen 2.5 1.5B

**Source**: [Qwen/Qwen2.5-1.5B-Instruct-GGUF](https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF)

| Property | Value |
|----------|-------|
| Parameters | 1.5B |
| Context Length | 128K tokens |
| License | Apache 2.0 |
| Quantization | Q4_K_M |
| File Size | 1.1 GB |
| RAM Required | ~2 GB |

**Download**:
```bash
huggingface-cli download Qwen/Qwen2.5-1.5B-Instruct-GGUF \
  qwen2.5-1.5b-instruct-q4_k_m.gguf \
  --local-dir models/ \
  --local-dir-use-symlinks False
```

**Use Cases**:
- Default installation bundle
- General-purpose inference
- Embedded deployments
- Resource-constrained environments

---

### Tier 3: Quality - Phi-3 Mini

**Source**: [microsoft/Phi-3-mini-4k-instruct-gguf](https://huggingface.co/microsoft/Phi-3-mini-4k-instruct-gguf)

| Property | Value |
|----------|-------|
| Parameters | 3.8B |
| Context Length | 4K tokens |
| License | MIT |
| Quantization | Q4_K_M |
| File Size | 2.2 GB |
| RAM Required | ~4 GB |

**Download**:
```bash
huggingface-cli download microsoft/Phi-3-mini-4k-instruct-gguf \
  Phi-3-mini-4k-instruct-q4.gguf \
  --local-dir models/ \
  --local-dir-use-symlinks False
```

**Use Cases**:
- Production deployments
- High-quality inference required
- Complex reasoning tasks
- Enterprise applications

---

## Quantization Options

| Quantization | Bits | Quality | Speed | Memory |
|--------------|------|---------|-------|--------|
| Q4_K_M | 4 | Good | Fast | Low |
| Q5_K_M | 5 | Better | Medium | Medium |
| Q8_0 | 8 | Best | Slower | High |

**Recommendation**: Q4_K_M provides the best balance of quality and efficiency for most use cases.

---

## License Summary

| Model | License | Commercial Use | Derivatives | Attribution |
|-------|---------|----------------|-------------|-------------|
| Qwen 2.5 | Apache 2.0 | Yes | Yes | Required |
| Phi-3 | MIT | Yes | Yes | Required |

Both licenses are fully permissive for commercial use and bundling.

---

## Verification

After downloading, verify model integrity:

```bash
# Register model with Veritas SDR
veritas-sdr-cli model register \
  --name qwen-1.5b \
  --path models/qwen2.5-1.5b-instruct-q4_k_m.gguf \
  --format gguf

# Verify model loads
veritas-sdr-cli status --json | jq '.models'

# Run test inference
veritas-sdr-cli infer \
  --model qwen-1.5b \
  --prompt "Hello, world!"
```

---

## Hardware Requirements

| Tier | CPU | RAM | Disk |
|------|-----|-----|------|
| CI (0.5B) | 2 cores | 2 GB | 1 GB |
| Default (1.5B) | 4 cores | 4 GB | 2 GB |
| Quality (3.8B) | 4 cores | 8 GB | 4 GB |

---

## TierSynergy Architecture

The tiered model system integrates with the SmartLoader and speculative decoding for optimal performance.

### SmartLoader (Lazy Loading)

Models are registered but **not loaded** until first use:

```rust
// Zero-overhead registration
loader.register("light", path, ModelTier::Light).await;

// Semantic hints drive prefetching
loader.hint(LoadHint::QuickQuery).await;     // Prefetch Light tier
loader.hint(LoadHint::ComplexTask).await;    // Prefetch Quality tier
```

**Benefits**:
- Zero idle memory overhead
- Background prefetching based on usage hints
- OS page cache leveraging for automatic memory management

### TierSynergy (Speculative Decoding)

When both Light and Quality tiers are available, the system automatically enables speculative decoding:

| Light Tier (Draft) | Quality Tier (Target) | Speedup |
|-------------------|----------------------|---------|
| Qwen 0.5B (~300MB) | Phi-3 Mini (~2.2GB) | 1.5-2x |

**How it works**:
1. Light model generates draft tokens (fast, ~5-10 tokens)
2. Quality model verifies drafts in a single batch
3. Accepted tokens pass through; rejected tokens get corrected

**Automatic Mode Selection**:
```rust
// Auto-detects best synergy mode
let result = synergy.request(LoadHint::ComplexTask).await?;
match result.mode {
    SynergyMode::SpeculativeLightQuality => {
        // 1.5-2x throughput with Light drafting, Quality verification
    }
    SynergyMode::Single => {
        // Single model fallback when only one tier available
    }
}
```

### CPU Optimization

This architecture is **optimized for CPU-only systems**:

- **SIMD Acceleration**: AVX2/FMA (Intel/AMD) and NEON (ARM64)
- **Light Model Cache**: ~300MB fits in L3 cache → extremely fast drafting
- **Memory Bandwidth**: Speculative decoding reduces bandwidth bottleneck
- **Q4_K_M Quantization**: 4-bit weights designed for CPU inference

---

## Future Models

Models under evaluation for future support:

| Model | Parameters | License | Status |
|-------|------------|---------|--------|
| Qwen 2.5 3B | 3B | Apache 2.0 | Evaluating |
| SmolLM2 1.7B | 1.7B | Apache 2.0 | Evaluating |
| Phi-3.5 Mini | 3.8B | MIT | Evaluating |

---

Copyright 2024-2026 Veritas SDR Contributors
