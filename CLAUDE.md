# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Identity

**Hearthlink CORE Runtime** - A sandboxed, offline inference engine that performs model execution only and has no authority over data, tools, or system actions.

Part of the Hearthlink stack: Control (governance) → Vault (data) → Construct (persona) → Synapse (external) → **CORE** (compute only)

## QoreLogic A.E.G.I.S. Lifecycle

This project uses the A.E.G.I.S. governance framework with Merkle-chained decision tracking.

**Current Status**: Check `docs/META_LEDGER.md` for lifecycle stage and next required action.

| Phase | Command | Purpose |
|-------|---------|---------|
| ALIGN/ENCODE | `/ql-bootstrap` | Initialize project DNA (CONCEPT.md, ARCHITECTURE_PLAN.md, META_LEDGER.md) |
| GATE | `/ql-audit` | Security review for L2/L3 risk grades (MANDATORY before implementation) |
| IMPLEMENT | `/ql-implement` | Execute the blueprint |
| VERIFY | `/ql-verify` | Validate implementation matches contract |

**Risk Grades**: L1 (UI only) → L2 (logic changes) → L3 (security/auth). L2/L3 require `/ql-audit` before implementation.

## Architecture Constraints

### Design Principles (C.O.R.E.)
- **Contained**: Sandbox with no ambient privileges
- **Offline**: Zero network access (inbound/outbound blocked)
- **Restricted**: IPC-only communication with authenticated callers
- **Execution**: Pure compute, no business logic or decision authority

### Security Boundaries
| Boundary | Rule |
|----------|------|
| Process | Separate OS process, restricted user, seccomp/AppContainer |
| Filesystem | Read: `models/`, `tokenizers/`. Write: `temp/`, `cache/`. Deny all else. |
| Network | Deny all |
| IPC | Named pipes/Unix sockets only. No HTTP/REST/WebSocket/localhost ports. |

### Forbidden Modules
If these exist, scope creep has occurred—ABORT:
- `auth/`, `vault/`, `synapse/`, `plugins/`, `network/`

### Forbidden Dependencies
- `reqwest`, `hyper`, any WebSocket library, filesystem traversal libraries

## Code Quality Rules (Section 4 Razor)

All code must satisfy:
- Functions ≤ 40 lines
- Files ≤ 250 lines
- Nesting ≤ 3 levels

Violations block implementation.

## Current Structure (Rust)

```
core-runtime/
├── src/
│   ├── main.rs             # Entry point
│   ├── lib.rs              # Public API
│   ├── ipc/                # IPC handling, auth, protocol
│   ├── scheduler/          # Queue, priority, batching
│   ├── engine/             # Inference, tokenizer, streaming
│   │   └── moe/            # Mixture of Experts (router, executor, combiner)
│   ├── models/             # Loader, registry, hot swap, versioning
│   ├── memory/             # Pool, GPU, cache
│   ├── telemetry/          # Prometheus metrics, OpenTelemetry spans
│   ├── ab_testing/         # A/B testing (traffic splitting, metrics)
│   │   ├── traffic/        # Traffic allocation
│   │   └── metrics/        # Per-variant statistics
│   ├── ffi/                # C FFI bindings
│   ├── python/             # PyO3 Python bindings
│   └── cli/                # Health probe CLI
├── include/                # Generated C headers (cbindgen)
├── python/                 # Python package structure
└── Cargo.toml
```

Recommended crates: `candle` or `llama-cpp-rs`, `serde`, `tokio`, `interprocess`

## Key Documents

- `docs/CONCEPT.md` - The "Why" and design philosophy
- `docs/ARCHITECTURE_PLAN.md` - File tree contract, interface specs, risk grade
- `docs/META_LEDGER.md` - Merkle-chained decision log with hash integrity
- `docs/architecture/CORE_RUNTIME_ARCHITECTURE.md` - Full technical specification
