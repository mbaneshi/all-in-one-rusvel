# 02 — Boundaries & dependency hygiene

## Context (for next agents)

- **Composition root:** `crates/rusvel-app` wires stores, engines, departments, API/MCP/TUI.
- **Pattern:** `dept-*` implements `DepartmentApp`; engines sit on `rusvel-core` ports only.
- **Convention:** Crate line budget ~2000 lines per `CLAUDE.md`.
- **Handoff:** Circular deps, “god modules,” and API handlers doing domain logic feed into audits 03 (security) and 07 (performance).

---

## Agent prompt (copy below)

```
Audit RUSVEL boundary hygiene.

Tasks:
1. Summarize how rusvel-app composes dept-* → engines → ports → adapters (short diagram or bullet tree).
2. Reason about cargo workspace cycles (cargo tree -i on suspicious edges if needed).
3. Flag crates near or over ~2000 lines; note responsibility blur.
4. Find API-layer logic that belongs in engines (rusvel-api handlers vs engine methods) — sample 3–5 cases if any.

Write results into docs/audit/agents/02-boundaries-dependencies.md under Report using the template tables.

Include Fix proposals and Space for improvement. Handoff: list modules the security auditor must re-trace.
```

---

## Report

### Executive summary

RUSVEL keeps the **intended dependency direction** intact: `*-engine` crates do not depend on infrastructure adapters (`just check-boundaries`), and `forge-engine` does not pull `rusvel-db` (`cargo tree -p forge-engine` has no `rusvel-db`). The **composition root** (`rusvel-app`) owns adapter construction, department boot (`boot::boot_departments`), and passes `Arc<dyn …Port>` plus optional engines into `rusvel-api::AppState`.

The main hygiene issues are **scale and layering**, not cycles: **`rusvel-api` is very large** (~14.5k lines of Rust) and **directly depends on multiple engines plus `rusvel-db::Database`**, so HTTP stays coupled to domain engines and a concrete DB type. Several modules implement **multi-step domain orchestration** (pipelines, code→content, `!build`) that could live behind a smaller API façade or inside an engine / `dept-*` boundary for clearer testing and security review.

**Workspace cycles:** Cargo cannot resolve cyclic package dependencies; the workspace builds with a DAG. Spot checks (`cargo tree -i rusvel-api -p rusvel-app`) show `rusvel-api` is only reached from `rusvel-app`, not from `rusvel-core` or engines.

---

### Composition (how pieces stack)

**Bullet tree (dependency / call flow):**

- **`rusvel-app` (binary, composition root)**  
  - Builds **adapters**: `rusvel_db::Database`, `rusvel_agent::AgentRuntime`, `rusvel_event::EventBus`, `rusvel_memory::MemoryStore`, optional `rusvel_vector`, `rusvel_embed`, bridges (`SessionAdapter` implements `SessionPort`), LLM stack (`rusvel_llm`), etc.  
  - Calls **`boot::installed_departments()` → `boot::boot_departments(...)`**: each `dept-*` receives `RegistrationContext` with `Arc<dyn AgentPort | StoragePort | …>` and registers tools, events, job handlers into a shared **`DepartmentRegistry`**.  
  - Constructs **`AppState`** (`rusvel-api`): `ForgeEngine` + optional `CodeEngine`, `ContentEngine`, `HarvestEngine`, `GtmEngine`, `FlowEngine`, `registry`, `database: Arc<Database>`, jobs, auth, webhooks, cron, etc.

- **`dept-*` (DepartmentApp)**  
  - Depends on **`rusvel-core` + one `*-engine`**.  
  - In `register()`, constructs **`FooEngine::new(ctx.agent, ctx.storage, …)`** (ports only from `RegistrationContext`) and exposes department tools / subscriptions.

- **`*-engine` (domain)**  
  - Depends only on **`rusvel-core`** (ports + domain types). No `rusvel-db` / `rusvel-llm` / `rusvel-agent` in engine `Cargo.toml` (verified by `just check-boundaries`).

- **`rusvel-core`**  
  - Port traits (`StoragePort`, `AgentPort`, …), `DepartmentApp`, domain models, registry types.

- **`rusvel-api` (HTTP surface)**  
  - Depends on **`rusvel-core`**, **engines** (direct `Arc<CodeEngine>` etc.), and **adapters** (`rusvel-db`, `rusvel-llm`, `rusvel-agent`, …). Handlers map HTTP ↔ engine / port calls.

**Tiny diagram:**

```text
rusvel-app
  ├─ adapters (db, agent, event, memory, llm, …)
  ├─ boot_departments → dept-* → *-engine ──► rusvel-core (ports)
  └─ AppState ──► rusvel-api (Axum) ──► engines + ports + Arc<Database>
```

---

### Findings

| Severity | Topic | Evidence (`path:line` or command) | Notes |
|----------|-------|----------------------------------|-------|
| Medium | `rusvel-api` mega-crate; HTTP + orchestration + adapter types | `wc` over `crates/rusvel-api/**/*.rs` ≈ **14,552** lines; `AppState` holds `Arc<Database>` and multiple `Arc<*Engine>` (`crates/rusvel-api/src/lib.rs` ~87–120) | Harder to enforce “thin handlers”; security and performance audits must touch many modules. |
| Medium | Engine-sized crates over ~2000 LOC budget | Python line count (`.rs` per crate under `crates/`): **rusvel-core ~4,372**, **rusvel-llm ~4,267**, **harvest-engine ~3,566**, **content-engine ~2,901**, **rusvel-app ~2,685**, **rusvel-db ~2,661**, **rusvel-agent ~2,546**, **forge-engine ~2,440**, **gtm-engine ~2,362**, **flow-engine ~2,278**, **rusvel-builtin-tools ~2,156** | Ports/domain (`rusvel-core`) and LLM adapter are expected to be large; several **engines** and **`rusvel-api`** blur “one responsibility per crate.” |
| Low | Cross-engine pipeline implementation in API | `crates/rusvel-api/src/pipeline_runner.rs` — `HarvestContentPipelineRunner` implements `forge_engine::pipeline::PipelineStepRunner` using `harvest_engine` + `content_engine` | Orchestration is domain-level; belongs in forge/dept-forge or a dedicated `pipeline` crate consumed by app/API. |
| Low | Multi-step “from code” use case in HTTP module | `content_from_code` loops analyze → `build_code_prompt` → `draft` (`crates/rusvel-api/src/engine_routes.rs` ~256–291) | Could be `ContentEngine::draft_from_path` (or `dept-content` façade) so API only forwards args. |
| Low | `!build` / capability flow in API with LLM + persistence | `crates/rusvel-api/src/build_cmd.rs` — uses `rusvel_llm::stream::ClaudeCliStreamer`, parses JSON, writes via `StoragePort` | Product/capability domain; keeps API coupled to Claude CLI streaming adapter. |
| Low | Playbook definitions + run orchestration in API | `crates/rusvel-api/src/playbooks.rs` — static `builtin_playbooks()`, module-local store, agent/flow steps | Fine for prototyping; long term better as forge-engine or persisted domain service. |
| Info | No engine → adapter dependency violations | `just check-boundaries` — all `*-engine`: OK | ADR / hex boundary respected for engines. |
| Info | No package-level dependency cycles (Cargo) | `cargo check --workspace` succeeds; `cargo tree -i rusvel-api -p rusvel-app` → only `rusvel-app` → `rusvel-api` | Cycles would fail resolution; layering is acyclic at crate graph level. |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| F1 | Split `rusvel-api` by vertical slices (e.g. `api-engine`, `api-chat`, `api-system`) or by layer (`api-handlers` vs `api-state`), keeping a thin `lib` router. | L | Platform / API |
| F2 | Replace `AppState.database: Arc<Database>` with a **`RusvelBasePort`** (or schema introspection trait) in `rusvel-core` so HTTP does not name SQLite adapter. | M | `rusvel-core` + `rusvel-api` + `rusvel-db` |
| F3 | Move `HarvestContentPipelineRunner` to **`forge-engine`** or **`dept-forge`**, inject into webhook/job paths from `rusvel-app`. | M | Forge / pipeline |
| F4 | Add **`ContentEngine::from_code_analysis`** (or single method taking path + kinds) and make `content_from_code` a one-liner delegate. | S | `content-engine` + `engine_routes` |
| F5 | Extract **`!build`** generation into a small **`capability-engine`** (or extend product-engine) depending on `AgentPort`/`LlmPort` only; API calls one method. | M | Capability / API |
| F6 | Track crate LOC in CI (`just crate-lines` or script) and gate **new** growth in crates already >2k lines. | S | DevEx |

### Space for improvement

- **Department-only routing:** Long term, prefer resolving engine operations through **`DepartmentRegistry` + tools** consistently, instead of parallel `AppState.code_engine` / `content_engine` fields, to shrink `AppState` and duplicate wiring paths.
- **`rusvel-core` size:** Consider splitting **generated/heavy domain** vs **ports** only if compile times or cognitive load become painful; today it is still the correct single “inner hex” anchor.
- **Transitive duplication:** Workspace `Cargo.toml` notes duplicate transitive deps (Arrow/DataFusion ecosystem); not a boundary violation but affects binary size and audit surface (see roadmap / dependency hygiene docs).

### Handoff (for audits 03+)

Security auditor (**03**) should **re-trace** these modules for authz, injection, secrets, and trust boundaries:

| Module | Why |
|--------|-----|
| `crates/rusvel-api/src/auth.rs` | Bearer / optional API token behavior. |
| `crates/rusvel-api/src/db_routes.rs` | SQL runner + schema exposure (RusvelBase). |
| `crates/rusvel-api/src/chat.rs` | Streaming, tool execution, session scoping. |
| `crates/rusvel-api/src/build_cmd.rs` | LLM-generated JSON → persisted agents/skills/rules/MCP/hooks. |
| `crates/rusvel-api/src/capability.rs` | Online capability / install paths if networked. |
| `crates/rusvel-api/src/webhooks.rs` | HMAC verification, payload handling. |
| `crates/rusvel-api/src/hook_dispatch.rs` | Post-chat hooks, spawned work. |
| `crates/rusvel-api/src/terminal.rs` | PTY / command execution surface. |
| `crates/rusvel-api/src/browser.rs` | CDP-driven actions. |
| `crates/rusvel-api/src/system.rs` | Health, notify, visual-test triggers. |
| `crates/rusvel-api/src/jobs.rs` + `approvals.rs` | Job listing, approval gates, payloads. |
| `crates/rusvel-api/src/engine_routes.rs` | Dept engine endpoints; session_id and path handling. |
| `crates/rusvel-api/src/mcp_servers.rs` | External MCP config persistence. |
| `crates/rusvel-app/src/main.rs` | Job worker, env-based wiring, seed data, outbound notify. |

Also confirm **`AppState.database`** usage: any raw SQL or schema APIs must align with **session / tenant** assumptions documented in 03.
