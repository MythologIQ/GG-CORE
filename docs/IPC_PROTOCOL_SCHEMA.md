# Veritas SDR IPC Protocol Schema v0.6.7

**Contract Freeze Date**: 2026-02-19
**Protocol Version**: V1 (JSON encoding)
**Status**: FROZEN for Hearthlink integration

---

## Overview

Veritas SDR uses a named pipe IPC protocol for all communication. This document specifies the wire format and message schemas that integrators must implement.

## Transport Layer

| Property | Value |
|----------|-------|
| Transport | Named pipes (Windows) / Unix sockets (Linux/macOS) |
| Encoding | JSON (UTF-8) |
| Framing | 4-byte little-endian length prefix |
| Max Message Size | 16 MB |

## Authentication

All sessions begin with a handshake exchange:

```json
// Client → Server
{
  "type": "handshake",
  "token": "<auth_token>",
  "protocol_version": "V1"
}

// Server → Client
{
  "type": "handshake_ack",
  "session_id": "<uuid>",
  "protocol_version": "V1"
}
```

## Message Types

### Inference Request

```json
{
  "type": "inference_request",
  "request_id": 1234,
  "model_id": "phi-3-mini",
  "prompt": "Explain quantum computing in simple terms.",
  "parameters": {
    "max_tokens": 256,
    "temperature": 0.7,
    "top_p": 0.9,
    "top_k": 40,
    "stream": false,
    "timeout_ms": 30000
  }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| request_id | u64 | Yes | Unique request identifier |
| model_id | string | Yes | Registered model name |
| prompt | string | Yes | Text prompt (non-empty) |
| parameters.max_tokens | u32 | No | Max tokens to generate (default: 256) |
| parameters.temperature | f32 | No | Sampling temperature (default: 0.7) |
| parameters.top_p | f32 | No | Nucleus sampling (default: 0.9) |
| parameters.top_k | u32 | No | Top-k sampling (default: 40) |
| parameters.stream | bool | No | Enable streaming (default: false) |
| parameters.timeout_ms | u64 | No | Request timeout (default: 30000) |

### Inference Response

```json
{
  "type": "inference_response",
  "request_id": 1234,
  "output": "Quantum computing uses quantum bits...",
  "tokens_generated": 42,
  "finished": true,
  "error": null
}
```

| Field | Type | Description |
|-------|------|-------------|
| request_id | u64 | Matches request |
| output | string | Generated text |
| tokens_generated | u32 | Number of tokens produced |
| finished | bool | True when generation complete |
| error | string? | Error message if failed |

### Health Check

```json
// Request
{ "type": "health_check", "check_type": "Liveness" }
{ "type": "health_check", "check_type": "Readiness" }
{ "type": "health_check", "check_type": "Full" }

// Response
{
  "type": "health_response",
  "check_type": "Liveness",
  "ok": true,
  "report": null
}
```

**Check Types**:
- `Liveness`: Process alive check
- `Readiness`: Model loaded and ready
- `Full`: Complete health report

### Metrics Request

```json
// Request
{ "type": "metrics_request" }

// Response
{
  "type": "metrics_response",
  "total_requests": 1000,
  "successful_requests": 990,
  "failed_requests": 10,
  "average_latency_ms": 150.5,
  "tokens_generated": 50000,
  ...
}
```

### Models List

```json
// Request
{ "type": "models_request" }

// Response
{
  "type": "models_response",
  "models": [
    {
      "handle_id": 1,
      "name": "phi-3-mini",
      "format": "gguf",
      "size_bytes": 2147483648,
      "memory_bytes": 3221225472,
      "state": "ready",
      "request_count": 100,
      "avg_latency_ms": 145.2,
      "loaded_at": "2026-02-19T10:30:00Z"
    }
  ],
  "total_memory_bytes": 3221225472
}
```

| Field | Type | Description |
|-------|------|-------------|
| handle_id | u64 | Internal model handle |
| name | string | Model identifier |
| format | string | Model format (gguf, onnx) |
| size_bytes | u64 | File size on disk |
| memory_bytes | u64 | Runtime memory usage |
| state | string | loading, ready, unloading, error |
| request_count | u64 | Total requests processed |
| avg_latency_ms | f64 | Average inference latency |
| loaded_at | string | ISO 8601 timestamp |

### Warmup Request

```json
// Request
{
  "type": "warmup_request",
  "model_id": "phi-3-mini",
  "tokens": 1
}

// Response
{
  "type": "warmup_response",
  "model_id": "phi-3-mini",
  "success": true,
  "error": null,
  "elapsed_ms": 150
}
```

### Cancel Request

```json
// Request
{ "type": "cancel_request", "request_id": 1234 }

// Response
{
  "type": "cancel_response",
  "request_id": 1234,
  "cancelled": true
}
```

### Streaming (Experimental)

```json
// Stream chunk
{
  "type": "stream_chunk",
  "request_id": 1234,
  "token": 12345,
  "is_final": false,
  "error": null
}
```

**Note**: Token-based streaming FFI is deprecated in v0.6.5+. Use non-streaming inference with `stream: false`.

### Error Response

```json
{
  "type": "error",
  "code": 400,
  "message": "Invalid parameters: max_tokens must be > 0"
}
```

**Error Codes**:
| Code | Meaning |
|------|---------|
| 400 | Invalid request/parameters |
| 401 | Authentication failed |
| 404 | Model not found |
| 413 | Message too large |
| 500 | Internal server error |
| 503 | Server shutting down |

---

## Breaking Changes History

### v0.6.5 → v0.6.7

| Field | Change |
|-------|--------|
| `prompt_tokens` | **REMOVED** - Use `prompt` (text) |
| `output_tokens` | **REMOVED** - Use `tokens_generated` |

### Migration Notes

**From token-based to text-based protocol**:

```json
// OLD (v0.6.0)
{
  "prompt_tokens": [1, 2, 3, 4, 5],
  "output_tokens": [10, 11, 12]
}

// NEW (v0.6.5+)
{
  "prompt": "Hello, world!",
  "output": "Hi there!",
  "tokens_generated": 3
}
```

---

## Validation Rules

| Field | Validation |
|-------|------------|
| model_id | Non-empty string |
| prompt | Non-empty string |
| max_tokens | > 0 |
| temperature | >= 0.0 |
| top_p | (0.0, 1.0] |

---

## Security Considerations

| Requirement | Implementation |
|-------------|----------------|
| No network | Named pipes only, no HTTP/WebSocket |
| Auth required | Handshake with token before inference |
| Size limits | 16 MB max message size |
| Constant-time auth | Token comparison uses constant-time |

---

## Contract Compliance

This schema is FROZEN for Hearthlink integration. Any breaking changes require:

1. Version bump (v0.7.0+)
2. Migration notes with field diffs
3. Backward compatibility period (2 minor versions)

**Schema Hash**: SHA256 of this document for integrity verification.

---

Copyright 2024-2026 Veritas SDR Contributors
