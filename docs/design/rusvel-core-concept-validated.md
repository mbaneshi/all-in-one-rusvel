# RUSVEL — Core Concept & Design (Code-Validated)

> **Purpose:** Validate the high-level pitch (“single binary agency”, hexagonal architecture, 55 crates, scoped tools, etc.) against this repository.  
> **Companion:** [rusvel-domain-minibook.md](./rusvel-domain-minibook.md) (runtime / dept chat detail).  
> **Last validated:** 2026-03-30 (metrics aligned with [`docs/status/current-state.md`](../status/current-state.md); narrative claims in §2–7 not all re-audited).

---

## Index

1. [Executive summary](#1-executive-summary)
2. [Claim-by-claim validation](#2-claim-by-claim-validation)
3. [Architecture layers (with citations)](#3-architecture-layers-with-citations)
4. [Boot & chat path (with citations)](#4-boot--chat-path-with-citations)
5. [Workspace member count — table sanity](#5-workspace-member-count--table-sanity)
6. [Local-first & offline](#6-local-first--offline)
7. [Important corrections (tool scoping)](#7-important-corrections-tool-scoping)
8. [Suggested narrative edits](#8-suggested-narrative-edits)

---

## 1. Executive summary

Most of the pitch is **directionally right**: one binary (`rusvel-app`), hexagonal boundaries, 14 `DepartmentApp` departments, SQLite WAL, SvelteKit embedded, central job queue, `AgentRuntime` streaming, ADR-style decisions (`Event.kind` string, `metadata` on domain types, engines on ports).

Corrections that matter for accuracy:

| Area | Pitch says | Code says |
|------|------------|-----------|
| `rusvel-core` | “Zero dependencies” | Has normal library deps (`serde`, `tokio`, `chrono`, …) — **no framework**, not zero deps. |
| Port traits | “20” | **22** top-level / store traits in `ports.rs` (see §3), including `RusvelBasePort`. |
| Per-dept tools | Scoped tools / “only tools department declared” | **`AgentRuntime` does not filter the LLM tool list with `AgentConfig.tools` today**; initial tools = all **non-searchable** tools from the shared `ToolPort`. `ScopedToolRegistry` exists in `rusvel-tool` but is **not** constructed in `rusvel-app` `main.rs`. See §7. |
| `cargo install rusvel` | Everything offline | App can run locally, but **LLM/embeddings/vector** often need network or local Ollama; not “full agency offline” without setup. |

---

## 2. Claim-by-claim validation

### What it is / solo founder / single binary

**Right.** The workspace ships `rusvel-app` as the composition root; `README` / `CLAUDE.md` align with “one binary” positioning.

### Name: Rust + SvelteKit

**Marketing** — not verifiable in code; fine as brand story.

### 14 departments, Forge as meta, Messaging, Flow, etc.

**Right** for member count and boot. Root `Cargo.toml` lists 14 `dept-*` paths (including `dept-messaging` with the foundation block); `docs/status/current-state.md` states **14** booted `DepartmentApp` instances.

### Hexagonal — 3 layers; engines never import adapters

**Right as design rule** (ADR / `DepartmentApp` docs). **Verify with** `just check-boundaries` or engine crate manifests — not re-run here.

### Foundation — `rusvel-core`

**Partially wrong wording:** “zero dependencies” is inaccurate.

```10:18:crates/rusvel-core/Cargo.toml
[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
async-trait = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
toml = { workspace = true }
tokio = { workspace = true }
```

**Better phrase:** *Zero application/framework coupling* — only shared primitives and port definitions.

**Port count:** `rg '^pub trait ' crates/rusvel-core/src/ports.rs` yields **22** traits (including `EventStore`, `ObjectStore`, `SessionStore`, `JobStore`, `MetricStore` under the storage split, plus `RusvelBasePort`). `docs/status/current-state.md` uses **22**.

**Domain types “82+”:** `domain.rs` has on the order of **100+** `pub struct` / `pub enum` lines (repo metric ~111 in `current-state.md`). “82+” is conservative, not wrong.

**“Everything is a Rust trait”** — **Overstated.** `rusvel-core` is mostly **types + traits**; large `domain.rs` is concrete structs/enums.

### Adapters — “18 adapter crates”

**Fuzzy.** The workspace groups **23** crates before the pure engine block in root `Cargo.toml` (core through `dept-messaging`, including `rusvel-schema`, `dept-messaging`, engine-tool registration crates). The pitch table’s “18 adapters” is an **accounting choice**, not a Cargo feature. Prefer: **“~20 infrastructure crates”** or list by name from `Cargo.toml`.

### Surfaces — CLI, Web UI, MCP; “4 + binary”

**Right in spirit.** Members include `rusvel-api`, `rusvel-cli`, `rusvel-tui`, `rusvel-mcp`, `rusvel-app` — five workspace packages for surfaces + root binary; calling four “surfaces” plus `rusvel-app` is reasonable labeling.

### Boot sequence (numbered list)

**Right**, matching `crates/rusvel-app/src/main.rs` + `boot::boot_departments` (adapter construction → department registration → registry → Axum → job worker → embed). See §4.

### Department chat: registry → system prompt + scoped tools → `run_streaming`

**Mixed:**

- **Registry + resolved config + `AgentRuntime::run_streaming` + SSE** — **Right** (`department.rs`, `sse_helpers`).
- **“Scoped tools = only tools department declared”** — **Misleading today** — see §7.

### Job queue, `AwaitingApproval`, human gates

**Right** at a high level (`JobPort`, worker, approval flows — see ADR-003/008 and `current-state.md`). Exact job kinds are enumerated in code/worker.

### Design bullets (event kind string, metadata, AgentPort not LlmPort, ModelTier, tool_search)

**Right** — reflected in `domain.rs`, `AgentRuntime` module docs, and `tool_search` registration in `main.rs`.

### 55-member workspace table

**Right total:** `Cargo.toml` `members` has **55** entries (verify with `cargo metadata --no-deps`). Individual row counts in older pitch decks are **illustrative**; small re-bucketing may not sum the same way if you move `rusvel-schema`, `dept-messaging`, or `rusvel-pipeline`.

### “Every crate &lt; 2000 lines”

**Policy** (`CLAUDE.md` / `just crate-lines`) — **aspirational rule**, not guaranteed without running the line-count script.

### Local-first, SQLite, rust-embed, Ollama

**Right** for storage and embedding story. **“cargo install rusvel → everything works offline”** — **Too strong** unless you define “works” as “binary starts” only; LLM and optional Lance/embed paths need configuration.

---

## 3. Architecture layers (with citations)

### Port traits (count)

Illustrative excerpt — full list in `crates/rusvel-core/src/ports.rs`:

```54:178:crates/rusvel-core/src/ports.rs
pub trait LlmPort: Send + Sync {
    // ...
}
// ...
pub trait AgentPort: Send + Sync {
    // ...
}
pub trait ToolPort: Send + Sync {
    // ...
}
pub trait EventPort: Send + Sync {
    // ...
}
pub trait StoragePort: Send + Sync {
    // ...
}
```

Sub-traits `EventStore`, `ObjectStore`, `SessionStore`, `JobStore`, `MetricStore` follow in the same file (total **21** `pub trait` lines matched).

### `DepartmentApp`

```19:33:crates/rusvel-core/src/department/app.rs
pub trait DepartmentApp: Send + Sync {
    fn manifest(&self) -> DepartmentManifest;
    async fn register(&self, ctx: &mut RegistrationContext) -> Result<()>;
    // ...
}
```

### `ScopedToolRegistry` (exists; composition root does not use it here)

```325:348:crates/rusvel-tool/src/lib.rs
/// A filtered view of a [`ToolPort`] that only exposes tools matching
/// allowed name prefixes or exact names.
pub struct ScopedToolRegistry {
    inner: Arc<dyn ToolPort>,
    allowed: Vec<String>,
}
```

---

## 4. Boot & chat path (with citations)

### Tool registration + `AgentRuntime` wiring

```791:807:crates/rusvel-app/src/main.rs
    let tool_registry = Arc::new(ToolRegistry::new());
    rusvel_builtin_tools::register_all(&tool_registry).await;
    let tools: Arc<dyn rusvel_core::ports::ToolPort> = tool_registry.clone();
    rusvel_builtin_tools::tool_search::register(&tool_registry, tools.clone()).await;
    // ...
    let agent_runtime = Arc::new(AgentRuntime::new(
        llm.clone(),
        tools.clone(),
        memory.clone(),
    ));
```

Note: **`ToolRegistry` is passed through**, not `ScopedToolRegistry::new(...)`.

### Department boot

```810:825:crates/rusvel-app/src/main.rs
    let departments = boot::installed_departments();
    let dept_registry = boot::boot_departments(
        &departments,
        agent_runtime.clone(),
        events.clone(),
        db.clone() as Arc<dyn StoragePort>,
        // ...
    )
    .await?;
```

### Deferred tool loading (agent loop)

```651:661:crates/rusvel-agent/src/lib.rs
    // Deferred tool loading: only non-searchable tools go into the initial prompt.
    // Searchable tools are discovered via `tool_search` and added dynamically.
    let mut tool_defs: Vec<ToolDefinition> =
        tools.list().into_iter().filter(|t| !t.searchable).collect();

    for iteration in 0..MAX_ITERATIONS {
        // ...
        let request = AgentRuntime::build_request(config, &messages, &tool_defs);
```

There is **no** `config.tools` filter in this snippet — `AgentConfig.tools` is populated from department `allowed_tools` in `department.rs` but **not consumed here** for narrowing `tool_defs`.

---

## 5. Workspace member count — table sanity

Root workspace members (excerpt):

```3:66:Cargo.toml
members = [
    "crates/rusvel-core",
    "crates/rusvel-schema",
    // ... adapters, rusvel-pipeline, dept-messaging ...
    "crates/forge-engine",
    // ... 13 engines ...
    "crates/dept-forge",
    // ... department apps ...
    "crates/rusvel-api",
    "crates/rusvel-cli",
    "crates/rusvel-tui",
    "crates/rusvel-mcp",
    "crates/rusvel-app",
]
```

**55** members — matches `docs/status/current-state.md`.

---

## 6. Local-first & offline

**Accurate elements:**

- SQLite WAL via `rusvel-db` / `Database::open` pattern.
- Frontend served from embedded `frontend/build` (rust-embed) when built that way.
- Ollama supported as a provider (`rusvel-llm`).

**Nuance:** Many flows expect an LLM; cloud providers need keys/network. Vector/RAG paths add optional native deps. Prefer: *local-first data plane; LLM is pluggable (local or cloud)*.

---

## 7. Important corrections (tool scoping)

1. **`ScopedToolRegistry` is implemented** in `rusvel-tool` but **`rusvel-app` does not wrap** the global `ToolRegistry` with it (see `main.rs` citations above).
2. **`AgentRuntime` seeds `tool_defs` from `tools.list()`** (minus searchable tools for the initial prompt), **not** from `AgentConfig.tools`.
3. **Department `allowed_tools` / manifest defaults** still matter for **config UX and API responses**, and **may** interact with other layers (e.g. permissions via `ToolRegistry::check_permission` when `__department_id` is present) — but the **narrative line “LLM only sees department-declared tools” is not what `run_streaming_loop` does today**.

**Action item (product/engineering):** Either wire `ScopedToolRegistry::new(tool_registry, resolved.allowed_tools)` (plus `tool_search`) per run, or filter `tool_defs` inside `AgentRuntime` using `config.tools` and document the actual behavior until then.

---

## 8. Suggested narrative edits

Use these replacements in slide decks / onboarding copy:

| Instead of | Say |
|------------|-----|
| “rusvel-core has zero dependencies” | “rusvel-core has **no framework deps** — only serde/tokio/chrono/uuid and port definitions.” |
| “20 port traits” | “**22** port/store traits in `ports.rs` (see metrics doc).” |
| “ScopedToolRegistry restricts tools per department in chat” | “**Design:** per-department tool filtering via `ScopedToolRegistry` / allowed lists. **Current `AgentRuntime`:** initial LLM tools = all non-searchable registered tools + `tool_search` discovery; see `rusvel-agent` loop.” |
| “cargo install → offline agency” | “**Local-first** storage and UI; **LLM** requires Ollama or configured cloud provider.” |

---

## Changelog

| Date | Change |
|------|--------|
| 2026-03-28 | Initial validation of core concept pitch against `Cargo.toml`, `ports.rs`, `main.rs`, `rusvel-agent` loop, `rusvel-tool` ScopedToolRegistry. |
