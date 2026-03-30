# PROJECT_CONTEXT.md

Narrative overview of the **rusvel** / **RUSVEL** workspace: purpose, architecture, crates, stack, data flow, decisions, state, and how to work on it. **Live counts** (workspace members, LOC, routes, tests) are maintained in [`docs/status/current-state.md`](docs/status/current-state.md); treat that file as the written source of truth for metrics.

---

## 1. Project Overview

### What is Rusvel / rusvel?

**RUSVEL** is a **single Rust binary** that bundles a multi-department “virtual agency”: AI-assisted workflows across code, content, opportunity discovery, go-to-market, and seven additional business domains. The repository **rusvel** is the monorepo for that product: Rust workspace + embedded SvelteKit frontend + auxiliary Python tooling.

### What problem does it solve?

Solo builders juggle many disconnected tools (CRM, writing, dev tools, support, finance spreadsheets). RUSVEL aims to **collapse agency-style work into one local-first system** with a unified data model (sessions, events, jobs), shared AI agent runtime, and multiple surfaces (web, CLI, REPL, TUI, MCP).

### Who is the target user?

The **solo founder / indie builder**: one human operating one workspace, not a multi-tenant SaaS team product.

### What is the core value proposition?

- **One binary, one composition root** (`rusvel-app` → binary name `rusvel`) with optional embedded SPA.
- **Hexagonal architecture**: domain engines depend only on port traits in `rusvel-core`; adapters are swappable.
- **Agents as the execution layer** (via `AgentPort`, ADR-009), with department-specific prompts and capabilities from a **department registry**.
- **Local-first**: SQLite WAL, optional Ollama; cloud LLMs are adapters, not requirements.

---

## 2. Architecture Overview

### High-level architecture (ASCII)

```
┌────────────────────────── SURFACES ─────────────────────────────┐
│  Web (Axum + SvelteKit build) │ CLI │ REPL │ TUI │ MCP (stdio)   │
│  SPA: rust-embed or ServeDir  │     │(reedline)│(ratatui)│       │
└───────────────────────────────┬─────────────────────────────────┘
                                │
                    DepartmentRegistry (14 department apps → string IDs)
                                │
┌───────────────────────────────┴─────────────────────────────────┐
│  DOMAIN ENGINES (crates)                                          │
│  Forge · Code · Harvest · Content · GTM + Finance · Product ·     │
│  Growth · Distro · Legal · Support · Infra · Flow (DAG workflows) │
└───────────────────────────────┬─────────────────────────────────┘
                                │ uses trait objects only
┌───────────────────────────────┴─────────────────────────────────┐
│  FOUNDATION                                                      │
│  rusvel-core: ports + domain types + registry                    │
│  Adapters: db, llm, agent, event, memory, tool, jobs, auth,      │
│            config, embed (fastembed), vector (LanceDB)           │
└──────────────────────────────────────────────────────────────────┘
```

### Monorepo structure

| Area | Path | Role |
|------|------|------|
| Workspace root | `Cargo.toml`, `Cargo.lock` | Defines workspace members (see [`docs/status/current-state.md`](docs/status/current-state.md) for current count). |
| Crates | `crates/*` | Foundation adapters, engines, API/CLI/MCP/TUI, app binary. |
| Frontend | `frontend/` | SvelteKit 5 + Vite + Tailwind 4; build output `frontend/build/`. |
| Design / plans | `docs/design/`, `docs/plans/` | Architecture v2, ADRs, roadmap, phase plans, flow-engine plan. |
| Book site | `docs-site/` | mdBook sources; deployed via `.github/workflows/docs.yml`. |
| Python | `pyproject.toml`, `uv.lock` | **`rusvel-scripts`**: optional `analysis` extras (e.g. Anthropic), dev ruff. |

### How crates relate

- **`rusvel-core`**: contracts only (traits, types, errors, IDs, `DepartmentRegistry`). No framework/IO.
- **Engines** (`*-engine`, `flow-engine`): implement `Engine` and domain logic; depend **only** on `rusvel-core`.
- **Adapters** (`rusvel-db`, `rusvel-llm`, …): implement `rusvel-core` ports; may use `rusqlite`, `reqwest`, `fastembed`, `lancedb`, etc.
- **Surfaces** (`rusvel-api`, `rusvel-cli`, `rusvel-mcp`, `rusvel-tui`): take `Arc<dyn …Port>` and engines; no business rules duplicated in engines.
- **`dept-*` wrappers** (`dept-forge`, `dept-code`, etc.): implement `DepartmentApp` trait (ADR-014), declaring `DepartmentManifest` and registration logic. Departments use **string IDs** (EngineKind enum was removed).
- **`rusvel-app`**: **composition root** — constructs `Database`, `EventBus`, `AgentRuntime`, `JobQueue`, registers `DepartmentApp` crates, builds `AppState`, starts Axum or dispatches CLI/MCP/TUI.

### Frontend, backend, Python

- **Backend** serves JSON under `/api/*`, SSE for streaming chat, static SPA at `/` when a frontend directory exists or assets were embedded at compile time.
- **Frontend** talks to the API (`frontend/src/lib/api.ts` and route components); uses department ids from the registry.
- **Python** is **not** in the request path of the Rust server; it supports scripts (e.g. visual analysis) via `uv run` with deps declared in `pyproject.toml`.

---

## 3. Crate Breakdown

Below: each workspace member under `crates/`, with purpose, notable dependencies, what it exposes, and maturity.

### Foundation

| Crate | Purpose | Key dependencies | Public API / exposes | State |
|-------|---------|------------------|----------------------|--------|
| **rusvel-core** | Port traits, shared domain model, registry, errors, `DepartmentApp` trait | `serde`, `async-trait`, `thiserror`, `uuid`, `chrono`, `toml` | `ports` (**19** traits: 14 Port — `LlmPort`, `AgentPort`, `ToolPort`, `EventPort`, `StoragePort`, `MemoryPort`, `JobPort`, `SessionPort`, `AuthPort`, `ConfigPort`, `EmbeddingPort`, `VectorStorePort`, `DeployPort`, `TerminalPort` + 5 Store sub-traits), `domain`, `department` (`DepartmentApp`, `DepartmentManifest`), `registry`, `id` | **Stable** (contract crate) |
| **rusvel-db** | `StoragePort` → SQLite WAL, migrations, five sub-stores | `rusqlite`, `tokio` | `Database` and store implementations | **Stable**, heavily tested |
| **rusvel-llm** | `LlmPort` for Ollama, Claude API, OpenAI, Claude CLI + router | `reqwest` | `OllamaProvider`, `ClaudeProvider`, `OpenAiProvider`, `ClaudeCliProvider`, `MultiProvider`, streaming helpers | **Stable** |
| **rusvel-agent** | `AgentPort`: agent loop (LLM + tools + memory), workflows | `tokio`, `serde` | `AgentRuntime` and workflow types | **Stable** / evolving |
| **rusvel-event** | `EventPort` + persistence | `tokio`, `serde` | Event bus adapter | **Stable** |
| **rusvel-memory** | `MemoryPort` with FTS5 | `rusqlite` | Session-scoped memory store | **Stable** |
| **rusvel-tool** | `ToolPort` registry + execution | `futures`, `serde` | `ToolRegistry` | **Stable** |
| **rusvel-jobs** | `JobPort` queue over `JobStore` | `tokio`, `serde` | `JobQueue` | **Stable** |
| **rusvel-auth** | `AuthPort` (env-backed / in-memory style) | `tokio`, `serde` | Credential adapter | **Stable** |
| **rusvel-config** | `ConfigPort` TOML layers | `toml`, `serde` | `TomlConfig` | **Stable** |
| **rusvel-embed** | `EmbeddingPort` via **fastembed** | `fastembed` 4, `tokio` | `FastEmbedAdapter` (default **all-MiniLM-L6-v2**, 384-dim) | **Stable** |
| **rusvel-vector** | `VectorStorePort` via **LanceDB** | `lancedb`, `arrow-*` | `LanceVectorStore` | **Stable** |
| **rusvel-deploy** | `DeployPort` adapter | `tokio`, `serde` | Deployment adapter | **Stable** |
| **rusvel-terminal** | `TerminalPort` adapter | `tokio` | Terminal interaction adapter | **Stable** |
| **rusvel-builtin-tools** | 9 built-in tools for agent execution | `serde`, `tokio` | Tool implementations | **Stable** |
| **rusvel-engine-tools** | Engine-specific tool wiring | `serde`, `tokio` | Engine tool bridge | **Stable** |
| **rusvel-mcp-client** | MCP client for external MCP servers | `tokio`, `serde` | MCP client adapter | **Stable** |
| **rusvel-schema** | Database schema introspection (RusvelBase) | `rusqlite` | Schema introspection | **Stable** |

### Domain engines

| Crate | Purpose | Key dependencies | Exposes | State |
|-------|---------|------------------|---------|--------|
| **forge-engine** | Meta-engine: missions, goals, planning, personas | `tokio`, `serde`, `async-trait` | `ForgeEngine` | **Wired** in `rusvel-app` |
| **code-engine** | Rust parsing (tree-sitter), graph, BM25, metrics | `tree-sitter`, `tree-sitter-rust` | `CodeEngine` | **Wired** + API/CLI/job worker |
| **harvest-engine** | Pipeline, scanning, scoring (`reqwest` for HTTP) | `reqwest` | `HarvestEngine` | **Wired** + jobs |
| **content-engine** | Drafting, publishing flows | `serde`, `chrono` | `ContentEngine` | **Wired** + jobs |
| **gtm-engine** | CRM / GTM domain (stubs growing) | core only | Engine types | **Partial** — chat via generic agent; not all job paths complete |
| **finance-engine** | Ledger, runway, tax (domain stubs) | core only | Engine impl | **Stub / WIP** |
| **product-engine** | Roadmap, pricing (stubs) | core only | Engine impl | **Stub / WIP** |
| **growth-engine** | Funnel/KPI (stubs) | core only | Engine impl | **Stub / WIP** |
| **distro-engine** | Distribution/SEO (stubs) | core only | Engine impl | **Stub / WIP** |
| **legal-engine** | Legal ops (stubs) | core only | Engine impl | **Stub / WIP** |
| **support-engine** | Support/tickets (stubs) | core only | Engine impl | **Stub / WIP** |
| **infra-engine** | Infra/incident (stubs) | core only | Engine impl | **Stub / WIP** |
| **flow-engine** | DAG execution: **code** (Rhai), **condition**, **agent** nodes; `FlowDef` in core | `petgraph`, `rhai`, `tokio`, `serde` | `FlowEngine`, `executor`, `NodeRegistry` | **Wired** in `rusvel-app` |

### Department wrapper crates (DepartmentApp pattern, ADR-014)

Each department has a `dept-*` crate implementing the `DepartmentApp` trait. These declare a `DepartmentManifest` (tools, routes, metadata) and handle registration. Departments are identified by **string IDs** (not an enum).

| Crate | Department |
|-------|-----------|
| **dept-forge** | Forge |
| **dept-code** | Code |
| **dept-harvest** | Harvest |
| **dept-content** | Content |
| **dept-gtm** | GTM |
| **dept-finance** | Finance |
| **dept-product** | Product |
| **dept-growth** | Growth |
| **dept-distro** | Distribution |
| **dept-legal** | Legal |
| **dept-support** | Support |
| **dept-infra** | Infra |
| **dept-flow** | Flow |

### Surfaces

| Crate | Purpose | Key dependencies | Exposes | State |
|-------|---------|------------------|---------|--------|
| **rusvel-api** | Axum HTTP API, CORS, static dir, SSE | `axum`, `tower-http`, selected engines | `AppState`, `build_router_with_frontend`, modules: `chat`, `department`, `workflows`, `capability`, `visual_report`, … | **Stable** surface; evolves with routes |
| **rusvel-cli** | Clap CLI + reedline REPL | `clap`, `reedline`, `crossterm` | `Cli` entry | **Stable** |
| **rusvel-mcp** | MCP-style **stdio JSON-RPC** (custom implementation) | `tokio`, `serde` | `RusvelMcp` | **Stable** |
| **rusvel-tui** | Ratatui dashboard | `ratatui`, `crossterm` | TUI app | **Stable** |
| **rusvel-app** | Binary `rusvel`, wiring, `rust-embed` of `frontend/build` | all of the above as needed | `main.rs` composition root | **Stable** entry |

---

## 4. Tech Stack

### Rust: key crates and why

| Crate | Role in RUSVEL |
|-------|----------------|
| **tokio** | Async runtime for server, agents, jobs, blocking offload (embeddings). |
| **axum** + **tower** / **tower-http** | HTTP API, CORS, tracing, static files, SPA fallback. |
| **serde** / **serde_json** | All domain types and API payloads; `metadata: Value` pattern (ADR-007). |
| **rusqlite** | Single-file DB, WAL, five-store layout. |
| **reqwest** | LLM HTTP providers and harvest HTTP. |
| **clap** / **reedline** / **ratatui** | CLI surfaces. |
| **thiserror** / **anyhow** | Typed errors in core; app-level bubbling. |
| **uuid** (v7) | Time-sortable IDs. |
| **chrono** | Timestamps. |
| **tree-sitter** | Code engine parsing. |
| **petgraph** + **rhai** | Flow engine DAG and sandboxed script nodes. |
| **fastembed** | Local embedding model for `EmbeddingPort`. |
| **lancedb** + **arrow-*** | Embedded vector store for `VectorStorePort`. |
| **rust-embed** | Ship `frontend/build` inside the binary. |

Edition: **2024**; workspace lint: deny `unsafe_code`, warn clippy pedantic (with targeted allows).

### Python (`pyproject.toml` / `uv.lock`)

- Project name: **`rusvel-scripts`**.
- **Runtime deps:** none in the base `[project.dependencies]` (empty list).
- **Optional `analysis`:** `anthropic` for auxiliary analysis scripts.
- **Dev:** `ruff` in `[dependency-groups] dev`.
- **Tooling:** **`uv`** for install/run (`uv sync`, `uv run`); Python **≥ 3.12**.

### Frontend

- **SvelteKit 5** + **Vite 6**, **TypeScript**.
- **Tailwind CSS 4** via `@tailwindcss/vite`; **bits-ui**, **lucide-svelte**, **@xyflow/svelte** (workflow builder), charts (**layerchart**, **d3-***), onboarding (**driver.js**), toasts (**svelte-sonner**), etc. (see `frontend/package.json`).
- **Package manager:** **pnpm** 9.15.4 (enforced via `packageManager` field).
- **E2E / visual:** Playwright (`test:visual`, `test:e2e`).

### Infrastructure: Docker

`Dockerfile` (multi-stage):

1. **frontend**: Node 22 Alpine, `pnpm install --frozen-lockfile`, `pnpm build`.
2. **builder**: Rust 1.87, copies `frontend/build` into context, `cargo build --release --bin rusvel-app`.
3. **runtime**: `debian:bookworm-slim`, `ca-certificates`, `sqlite3`; binary installed as `/usr/local/bin/rusvel`, **`EXPOSE 3000`**, `ENTRYPOINT ["rusvel"]`.

### AI / ML

- **LLMs:** Ollama (local), Anthropic Claude (HTTP + CLI), OpenAI-compatible API, combined via **`MultiProvider`**.
- **Embeddings:** **`fastembed`** in `rusvel-embed` (default MiniLM 384-d); **`LlmPort::embed`** also used where the active LLM supports embeddings (e.g. Ollama/OpenAI).
- **Vectors:** LanceDB-backed store in `rusvel-vector` for semantic retrieval workflows.

---

## 5. Data Flow

### How data enters the system

- **HTTP:** REST/JSON to `/api/*` (sessions, CRUD for agents/skills/rules/hooks/workflows/MCP servers, department chat, capability build, engine routes, health).
- **CLI / REPL / TUI:** Same domain operations through `rusvel-cli` / `rusvel-tui` backed by shared types and engines where wired.
- **MCP:** Stdio JSON-RPC tools hitting `ForgeEngine` / `SessionPort` patterns.
- **Hooks:** On events (e.g. chat completed), configured hooks may shell out, POST HTTP, or prompt CLI (see `rusvel-api` hook modules).

### How it is processed

- **Chat:** Department prompt + rules + skills resolution; streaming via SSE; agent loop in `rusvel-agent`.
- **Engines:** Forge orchestrates mission-style flows; Code/Harvest/Content run structured operations exposed via API/CLI and **job worker** for specific `JobKind`s.
- **Jobs:** Enqueued through `JobPort`; background task in `rusvel-app` dequeues and dispatches to code/content/harvest engines for supported job types.
- **Events:** `EventPort` emit + query; `kind: String` (ADR-005).
- **Flows (future):** `flow-engine` executes `FlowDef` graphs, persists definitions/executions under ObjectStore keys `flows` / `flow_executions`, emits `flow.execution.completed`.

### How it is stored

- **SQLite** (default `~/.rusvel/rusvel.db` beside the app data dir; **`RUSVEL_DB_PATH` is not read by the binary today**): events, objects, sessions, jobs, metrics — via **`StoragePort`** facades.
- **LanceDB** (on disk under data dir): vector tables for embedding search.
- **Config:** TOML files + layered overrides (`rusvel-config`).

### How it is served to the frontend

- **Development:** often `pnpm dev` on **:5173** with API proxy or direct calls to **:3000**.
- **Production binary:** Axum `ServeDir` from extracted embed dir or filesystem path; non-API routes fall through to **`index.html`** for SPA routing.
- **API modules** (from crate docs): agents, analytics, approvals, build_cmd, capability, chat, config, department, engine_routes, help, hooks, mcp_servers, rules, skills, system, visual_report, workflows, knowledge, etc.

---

## 6. Key Design Decisions

### Why Rust?

Single deployable binary, strong typing across a large domain surface, async performance for concurrent agents and SSE, and clean crate boundaries for hexagonal architecture.

### Why this crate structure?

- **Engines never import adapters** (ADR-010): tests and future swaps stay cheap.
- **Small crates** (convention: keep each under ~2000 lines) with a **single responsibility**.
- **Department registry** (ADR-011): parameterized `/api/dept/{dept}/*` routes instead of duplicating per-department handlers.

### Notable patterns

- **Hexagonal / ports & adapters:** `rusvel-core` is the inner hexagon.
- **Agent boundary (ADR-009):** engines use **`AgentPort`**, not **`LlmPort`**, so prompting/tools/memory stay centralized.
- **Single job queue (ADR-003)** vs many schedulers.
- **Five canonical stores (ADR-004)** instead of a generic KV `StoragePort`.
- **Event kinds as strings (ADR-005)** to avoid core enum churn.
- **metadata JSON on domain types (ADR-007)** for forward-compatible schemas.
- **Approvals in the model (ADR-008)** for publish/outreach — UI/API still catching up in places.

### Hard problems (historical / ongoing)

- Keeping **documentation and counts** in sync as crates and routes multiply (see `docs/design/audit-2026-03-23.md`).
- **Tailwind dynamic classes** in Svelte (audit C1): production purge vs dev JIT — addressed by static class maps where needed.
- **Incremental wiring** of seven “extended” engines: registry and chat work; **domain managers** not all constructed in `rusvel-app` yet.
- **Workflow evolution:** bridging flat workflow storage/API with graph-native **`flow-engine`** (`docs/plans/flow-engine.md`).

---

## 7. Current State & Roadmap

### What is working today (high level)

- **Multi-surface app:** default HTTP server on **port 3000**, `--mcp`, `--tui`, CLI subcommands and `shell` REPL.
- **14 department apps** in UI/API via registry; **parameterized** department routes.
- **Forge + Code + Harvest + Content** engines instantiated in composition root; **GTM** crate exists with partial/stub behavior; **extended seven** largely stubby but present in workspace.
- **Chat** with SSE; **CRUD** for agents, skills, rules, hooks, workflows, MCP servers; **capability** build and `!build`; **hooks** on events.
- **Job queue worker** processes selected job kinds through code/content/harvest engines.
- **Embeddings + LanceDB** wired when adapters initialize successfully in app startup.
- **CI:** format, clippy, tests, workspace build, frontend check+build, visual Playwright job (with snapshot update fallback), llvm-cov → Codecov (optional token).
- **Git hooks (lefthook):** pre-commit fmt/clippy/prettier check; pre-push `cargo test --workspace`.

### In progress (from typical git state / new files)

- **`Cargo.lock`** changes when workspace dependencies shift.

### Planned next

See **`docs/plans/roadmap-v2.md`**: phases for deeper agent graphs, revenue engines, GTM completion, cross-engine intelligence, ecosystem/plugins. **`docs/plans/flow-engine.md`** details n8n-style flows aligned with `flow-engine`.

### Known issues / tech debt

- **Approval workflow:** types exist; end-to-end **API/UI** not fully productized (per project docs).
- **GTM / CRM:** OutreachSend worker path is wired; deeper CRM surfaces and polish remain (see `docs/status/current-state.md`).
- **Older docs** (e.g. `docs/design/vision.md` port list) may still describe pre-v2 ports; **source of truth** for ports is `crates/rusvel-core/src/ports.rs`.
- **Metrics:** use [`docs/status/current-state.md`](docs/status/current-state.md) for crate counts and scale; avoid hard-coding numbers in prose here.

---

## 8. Development Workflow

### Run locally

```bash
cargo build          # Full workspace
cargo run            # HTTP server :3000 (ensure LLM backend e.g. Ollama if needed)
cargo run -- --help
cargo run -- --mcp
cargo run -- --tui
cargo run -- shell
```

Frontend:

```bash
cd frontend && pnpm install && pnpm dev    # Dev server (Vite)
pnpm build                                 # Output to frontend/build/ for embed
pnpm check                                 # svelte-check
```

Python:

```bash
uv sync && uv run <script.py>
```

### Key commands

| Mechanism | Command / behavior |
|-----------|-------------------|
| **Cargo** | `cargo test --workspace`, `cargo clippy --workspace --all-targets` |
| **lefthook** | Pre-commit: `cargo fmt --check`, `cargo clippy … -D warnings`, Prettier on frontend sources; pre-push: full workspace tests |
| **Docker** | Multi-stage build producing `rusvel` image listening on **3000** |
| **CI** | `.github/workflows/ci.yml` — fmt, clippy, test, build, frontend, visual tests artifact, coverage |

### Tests

- **Rust:** unit/integration tests per crate under `src` / `tests`; largest suites often **`rusvel-db`**, **`rusvel-api`**, **`forge-engine`**. Approx. **~476** tests and **~61** test targets (see [`docs/status/current-state.md`](docs/status/current-state.md)); 0 failures in a full local run.
- **Frontend:** Playwright E2E and **visual** projects; `pnpm test:analyze` for optional AI-assisted diff analysis.

### CI/CD

- **ci.yml** on `push`/`pull_request` to `main`.
- **docs.yml** deploys **mdBook** from `docs-site` to GitHub Pages on changes to that tree.
- **release.yml** present for release automation (see file for triggers).

---

## 9. Open Questions / Decisions Pending

- **Flow engine integration:** When to add `flow-engine` to `rusvel-app`, unify with existing **`workflows`** API/storage, and expose triggers (webhook/cron/event) from `docs/plans/flow-engine.md`.
- **Extended engines:** Order of instantiation for finance/product/growth/distro/legal/support/infra vs keeping generic-agent-only behavior.
- **Approval UX:** How approvals surface in API and frontend for content and outreach (ADR-008 intent vs implementation gap).
- **Vector + memory productization:** UX and defaults for hybrid FTS5 + Lance retrieval across departments.
- **Optional:** Ensure living docs link or match [`docs/status/current-state.md`](docs/status/current-state.md) for workspace scale.

### TODOs in code worth flagging

Periodic search for `TODO`, `FIXME`, `unimplemented!` in engines and API is still useful; prior audits called out **mock storage** gaps in tests — many addressed, but new crates (**flow-engine**) should follow the same **mock port** discipline.

---

*Generated from repository sources: root `Cargo.toml`, all `crates/*/Cargo.toml`, `README.md`, `CLAUDE.md`, `crates/rusvel-core/src/lib.rs` & `ports.rs`, `crates/rusvel-app/src/main.rs`, `crates/flow-engine/src/lib.rs`, `frontend/package.json`, `Dockerfile`, `pyproject.toml`, `lefthook.yml`, `.github/workflows/*.yml`, and `docs/design/*`, `docs/plans/*`.*
