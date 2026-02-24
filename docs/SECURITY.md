# SENTINEL — Security Hardening Analysis

## Overview

SENTINEL's architecture implements **Defense in Depth** through four reinforcing isolation layers. This document analyzes how the design mitigates the most critical attack vectors for autonomous AI agent systems.

---

## 1. Prompt Injection Escalation

**Threat**: An adversary manipulates the agent's LLM reasoning (via crafted inputs, poisoned documents, or tool output injection) to execute unauthorized actions—reading sensitive files, exfiltrating data, or running shell commands.

**Mitigation — Architectural Boundary Enforcement:**

| Layer | Defense |
|---|---|
| **Wasm Sandbox** | The agent's reasoning loop runs inside a `wasmtime` sandbox with **zero direct syscalls**. Even if prompt injection causes the LLM to "decide" to read `/etc/shadow`, the code path simply does not exist in the sandbox. |
| **Capability Tokens** | Every resource access requires a scoped, ephemeral token. The host validates each request against policy (allowed directories, URL whitelist) *before* minting a token. A prompt-injected `fs.read("/etc/passwd")` is rejected at mint time if `/etc` isn't in `allowed_read_dirs`. |
| **HITL Protocol** | Critical actions (shell commands, network writes, financial operations) require a human-approved, Ed25519-signed `ExecutionManifest`. The agent cannot forge a signature — the signing key never enters the sandbox. |
| **LLM Backend Isolation** | The LLM provider (Ollama/API) is accessed exclusively by the Host. The guest submits reasoning requests through the WIT `reasoning` interface — it never holds API keys or network sockets. |

**Key Invariant**: *No amount of prompt injection can bypass a capability gate.* The LLM can only generate *requests* — it cannot execute them. The Host (Jailer) is the sole executor, and it follows configurable policy, not LLM output.

---

## 2. Resource Exhaustion (DoS)

**Threat**: A malicious or buggy agent consumes unbounded CPU, memory, or network resources, starving the host system.

**Mitigation — Multi-Layer Resource Budgets:**

| Resource | Mechanism | Configuration |
|---|---|---|
| **CPU** | Wasmtime **fuel metering** — every Wasm instruction consumes fuel. When fuel runs out, execution traps immediately. | `fuel_limit: 1_000_000_000` (~1B instructions) |
| **CPU (hard)** | Wasmtime **epoch interruption** — the host advances an epoch counter on a timer; the guest traps after 1 epoch tick without yielding. | `epoch_deadline_trap(1)` |
| **Memory** | `StoreLimits` caps linear memory, tables, and instances at the Wasm runtime level. The guest *cannot* allocate beyond these limits. | `max_memory_bytes: 256 MiB`, `memories: 1` |
| **Instances** | Only 1 Wasm instance per store — prevents fork-bomb patterns. | `instances: 1` |
| **Network** | URL whitelist + request timeout prevents the agent from spraying requests or opening long-lived connections. | `request_timeout: 30s` |
| **Filesystem** | `max_read_size` prevents the agent from loading multi-GB files into memory. | `max_read_size: 10 MiB` |
| **Token TTL** | Capability tokens expire after a configurable TTL. Even if the agent hoards tokens, they become useless. | `default_ttl: 300s` |

**Key Invariant**: *Resource limits are enforced at the Wasm runtime level.* They cannot be bypassed by guest code, regardless of what the LLM generates.

---

## 3. Privilege Escalation

**Threat**: The agent acquires permissions beyond its intended scope — e.g., a token for `/workspace/src` is used to read `/workspace/.env`.

**Mitigation:**

- **Path Canonicalization**: Every filesystem path is resolved through `std::path::Path::canonicalize()` before validation, neutralizing `..`, symlink, and Unicode normalization attacks.
- **Scope Validation**: Token-gated operations re-validate the resource against the token's scope on *every call*, not just at mint time. A token for `/workspace/src/**` cannot be used to read `/workspace/.env`.
- **Principle of Least Privilege**: Tokens are scoped to the narrowest possible pattern. `request_fs_read("/workspace/src/main.rs", ...)` mints a token for exactly that file, not the entire directory.
- **Revocation**: Tokens can be revoked at any time by the host. The `release_capability()` function allows the guest to voluntarily reduce its attack surface.
- **Nonce Tracking**: Each `ExecutionManifest` carries a 32-byte cryptographic nonce. The host tracks used nonces and rejects replays.

---

## 4. Data Exfiltration

**Threat**: The agent reads sensitive local data and sends it to an attacker-controlled server.

**Mitigation:**

- **Network Whitelist**: Outbound requests are restricted to explicitly configured URL patterns. The agent cannot contact `https://evil.com/exfil` unless it's in the whitelist.
- **HITL for Network Writes**: POST/PUT requests to external APIs can be configured as `High` risk, requiring manifest approval. The user sees exactly what data is being sent.
- **Local-First LLM**: When using Ollama, *no data leaves the machine*. The LLM inference happens locally. Even API-based providers are contacted exclusively by the Host — the guest never holds credentials.
- **Filesystem Scoping**: The agent can only read from `allowed_read_dirs`. Sensitive directories (`~/.ssh`, `~/.aws`, browser profiles) are excluded by default.

---

## 5. Supply Chain & Runtime Integrity

| Concern | Defense |
|---|---|
| **Wasm module tampering** | In production, the host should verify a cryptographic hash of the guest module before loading. |
| **Dependency poisoning** | The guest has no network access during compilation. `sentinel-guest-api` has minimal deps (serde only). |
| **Runtime escape** | Wasmtime is a production-grade, fuzzer-tested sandbox. Linear memory isolation prevents the guest from accessing host memory. |
| **Key management** | The Ed25519 signing key is generated fresh per session and lives only in host memory. It never enters the sandbox. |

---

## Latency Considerations

The target is **< 50ms frame processing latency**. Contributing factors:

| Component | Expected Latency | Optimization |
|---|---|---|
| Wasm instantiation | ~1ms (amortized with pre-compilation) | Cranelift `Speed` opt level |
| Host-call overhead | ~10µs per call | Direct function linking, no serialization on hot path |
| Capability validation | ~1µs (HashMap lookup + scope check) | `RwLock` for concurrent reads |
| Path canonicalization | ~50µs (syscall) | Cached for repeated accesses |
| LLM inference (local) | 100ms–10s (model-dependent) | Async, non-blocking; does not count toward frame budget |
| LLM inference (API) | 200ms–30s (network-dependent) | Async with configurable timeout |

> **Note**: LLM inference latency is intentionally excluded from the 50ms budget. The frame processing target applies to the security and capability layer — not to the inherently variable LLM reasoning time.

---

## Summary

```
┌─────────────────────────────────────────────┐
│              Host (The Jailer)               │
│  ┌────────────────────────────────────────┐  │
│  │  Capability Manager  │  HITL Bridge    │  │
│  │  (token mint/revoke) │  (Ed25519 sign) │  │
│  └────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────┐  │
│  │  LLM Provider Layer                    │  │
│  │  Ollama│OpenAI│Claude│Deepseek│Grok    │  │
│  └────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────┐  │
│  │  StoreLimits │ Fuel │ Epoch Interrupt  │  │
│  └──────────────┬─────────────────────────┘  │
│ ════════════════╪══════════ WIT Bridge ═══╪══│
│  ┌──────────────┴─────────────────────────┐  │
│  │        Guest (The Sandbox)             │  │
│  │   Zero syscalls · Zero networking      │  │
│  │   Zero filesystem · Zero API keys      │  │
│  └────────────────────────────────────────┘  │
└─────────────────────────────────────────────┘
```
