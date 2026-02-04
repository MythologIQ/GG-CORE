# Tandem Experimental Architecture Proposal

Status: Proposal only. Not finalized. Must pass `ql-plan` and `ql-audit` before any implementation.

## Overview

This proposal defines a tandem approach where `main` remains the hardened runtime and experimental work occurs on controlled `exp/<slug>` branches with measurable gates and one-commit rollback.

## 1. Main Branch Architecture

### 1.1 Non-Negotiable Contract

- CORE Runtime is compute only: tokens in, tokens out, no authority, no network, minimal filesystem.
- See: `docs/architecture/CORE_RUNTIME_ARCHITECTURE.md`, `docs/CONCEPT.md`.

### 1.2 Process Placement and Boundaries

Main branch must enforce:

- Separate OS process, never embedded. (CORE_RUNTIME_ARCHITECTURE)
- Named IPC only, no HTTP/ports. (CORE_RUNTIME_ARCHITECTURE, ARCHITECTURE_PLAN)
- Network blocked inbound and outbound. (CORE_RUNTIME_ARCHITECTURE)
- Filesystem allowlist:
  - Read: `models/`, `tokenizers/`
  - Write: `temp/`, `cache/`
  - Deny all else (CORE_RUNTIME_ARCHITECTURE, ARCHITECTURE_PLAN)
- No persistent memory store inside CORE. (CORE_RUNTIME_ARCHITECTURE, CONCEPT)

### 1.3 Module Layout as Stable Spine

Keep the sealed module tree as the `main` architecture baseline:

- `ipc/`: auth, protocol validation, handler routing
- `scheduler/`: queue, priority, batching
- `engine/`: tokenizer, inference, streaming
- `models/`: loader, registry, swap
- `memory/`: pool, GPU tracking, cache

Dependencies must remain as currently approved in `docs/SYSTEM_STATE.md`.

### 1.4 Main Branch Invariants

Never break rules for `main`:

- IPC validation happens before allocation and before scheduler enqueue. (ARCHITECTURE_PLAN)
- Hard caps exist for:
  - max prompt token count
  - max request bytes
  - max output tokens
  - max queue depth
  - max concurrent sessions (ARCHITECTURE_PLAN)
- Crash-safe behavior: fail closed, restartable, no content logging. (CORE_RUNTIME_ARCHITECTURE)
- Models treated as untrusted blobs; validate on load, never grant privileges. (CORE_RUNTIME_ARCHITECTURE)

## 2. Fail-Fast Experiment Framework (Parallel Track)

### 2.1 Branching Strategy

- `main`: hardened runtime only.
- Experiment branches: `exp/<slug>`.
- Rule: experiments merge to `main` only after passing gates. Otherwise delete the branch. This preserves one-revert rollback.

### 2.2 Experiment Boundaries

Experiments may touch only one surface at a time:

- `memory/` representation, pooling, cache strategies
- `scheduler/` batching and queue behavior
- `models/` load-time transformations
- `engine/` kernel selection and layout choices

Experiments must not introduce:

- new network code
- new filesystem traversal
- new authority or persistence behaviors (ARCHITECTURE_PLAN, CONCEPT)

### 2.3 Standard Experiment Harness (Required)

Add a benchmark layer that does not change production behavior:

- `benches/` (Criterion or minimal internal harness)
- `fixtures/` with:
  - representative prompts (small, medium, large)
  - representative model set (at least one small model for fast iteration)
- Metrics emitted as numbers only:
  - tokens per second
  - p50/p95 latency
  - peak RSS
  - peak VRAM (if relevant)
  - allocations per request (if measurable)
  - queue wait time (scheduler experiments)

Output must be machine-readable for CI comparison.

### 2.4 Experiment Gates

Security gates:

- No new dependencies outside the approved list. (SYSTEM_STATE)
- No new forbidden modules. (ARCHITECTURE_PLAN)
- IPC limits and validation unchanged or strengthened. (ARCHITECTURE_PLAN)
- No logging of prompts or tokens. (CORE_RUNTIME_ARCHITECTURE)

Correctness gates:

- Property tests: encode then decode returns original (for any encoding experiment)
- Determinism test: same model + same tokens + same params yields identical outputs across N runs (within defined tolerance if using sampling)
- Existing TDD-light tests remain green. (META_LEDGER, SYSTEM_STATE)

Performance gates:

Must show improvement on at least one primary metric without unacceptable regression in others:

- tokens/sec improves by at least 5–10%
- memory reduces by at least 10%
- p95 latency does not worsen by more than 3–5%

If gains are within noise, declare “no decision” and delete or re-scope the experiment.

### 2.5 Traceability Rules for Main Merges

Every merged experiment must update traceability artifacts:

- `FEATURE_MAP.md`: new capability or change recorded
- `FEATURE_BUILD_PLANS.md`: plan item marked complete with commit or PR reference
- Link to `exp/<slug>` and acceptance metrics

If these do not exist yet, create minimal stubs to enforce discipline.

## 3. First Fail-Fast Experiment (001)

### Experiment 001: IPC Payload and Token Buffer Packing

Goal:

- Reduce request payload size and memory bandwidth by tightening token representation.

Why it fits:

- Touches IPC protocol and internal buffers, not model math.
- Fully gated with property tests and benchmark metrics.
- Reverts cleanly if it underperforms.

Scope:

- Add alternate protocol message format version:
  - `v1`: `Vec<u32>` tokens
  - `v2`: packed varint or `u16` where possible, with fallback for larger token IDs
- Decode at IPC boundary into existing `Vec<u32>` initially

Success metrics:

- IPC request bytes reduced by at least 30% on typical prompts
- p95 latency not worse by more than 3%
- no increase in allocation count per request

If it passes:

- Consider second-step experiment to keep packed tokens deeper into scheduler queues to reduce memory further

If it fails:

- Delete branch
- Keep benchmark harness

This experiment aligns with strict IPC and schema validation emphasis in ARCHITECTURE_PLAN.

## 4. Tandem Execution Plan (Sequence, No Dates)

1. Lock Main invariants.
2. Document invariants as a short checklist in the repo.
3. Add CI checks that fail if forbidden dependencies appear. (SYSTEM_STATE, ARCHITECTURE_PLAN)
4. Add benchmark harness to `main` (`benches/` + `fixtures/` + simple runner).
5. Run Experiment 001 branch.
6. Implement v2 protocol and decode boundary.
7. Add property tests and bench comparisons.
8. Merge only if gates pass.
9. Repeat with next experiments ranked by payoff:
   - KV cache memory layout
   - allocator pooling improvements
   - scheduler micro-batching heuristics

