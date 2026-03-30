# RUSVEL — Project Conventions

> The Solo Builder's AI-Powered Virtual Agency
> Rust + SvelteKit | One binary, one human, infinite leverage

## Quick Commands

```bash
cargo build                    # Build all workspace members (55 crates)
cargo bench -p rusvel-app --bench boot  # Criterion boot slice (SQLite + registry); optional local perf
cargo test                     # Run full suite from repo root (~645 tests summed; see docs/status/current-state.md)
cargo run                      # Start API server on :3000 (requires Ollama)
cargo run -- --help            # Show CLI help
cargo run -- --mcp             # Start MCP server (stdio JSON-RPC)
cargo run -- --tui             # Launch TUI dashboard (ratatui)
cargo run -- shell             # Launch interactive REPL shell (reedline)
cargo run -- session create X  # Create a session
cargo run -- forge mission today  # Generate daily plan
cargo run -- finance status    # Department one-shot command
cargo run -- growth list       # List department items
```

**Runtime security & env:** HTTP bind, API tokens, DB SQL lock, MCP auth, `VITE_RUSVEL_API_TOKEN` — see [`docs/operators/security-hardening.md`](docs/operators/security-hardening.md).

## Architecture

Hexagonal (ports & adapters). See `docs/design/architecture-v2.md`.

- **rusvel-core** — 22 port traits in `ports.rs` (includes five `*Store` subtraits, `ChannelPort`, `BrowserPort`, `RusvelBasePort`, etc.) + domain types + `DepartmentApp` + `DepartmentManifest`. Zero framework deps.
- **Adapters** — Implement port traits (rusvel-db, rusvel-llm, rusvel-agent, etc.)
- **Engines** — Domain logic, depend ONLY on rusvel-core traits (13 engines: 6 wired + 7 skeletons, all migrated to DepartmentApp pattern via ADR-014)
- **dept-* wrappers** — 14 `dept-*` crates (including dept-flow, dept-messaging), each implementing DepartmentApp
- **Surfaces** — CLI (3-tier: one-shot + REPL + TUI), API, MCP — wire adapters into engines
- **rusvel-app** — Composition root, the single binary

## Key Rules

1. **Engines never import adapter crates.** They depend only on `rusvel-core` port traits.
2. **Engines never call LlmPort directly.** They use AgentPort (ADR-009).
3. **All domain types have `metadata: serde_json::Value`** for schema evolution (ADR-007).
4. **Event.kind is a String**, not an enum (ADR-005). Engines define their own constants.
5. **Single job queue** for all async work — no per-engine scheduling (ADR-003).
6. **Human approval gates** on content publishing and outreach (ADR-008).
7. **Each crate < 2000 lines.** Single responsibility.
8. **NEVER use npm.** Use `pnpm` for all frontend/Node.js work. (`pnpm install`, `pnpm build`, `pnpm exec`).
9. **NEVER use pip/pip3/python -m pip.** Use `uv` for all Python work. (`uv run`, `uv add`, `uv sync`).

## Workspace Layout (55 members)

```
crates/
├── rusvel-core/          Ports (22 traits in ports.rs) + domain types + DepartmentApp trait + DepartmentManifest
├── rusvel-schema/        Database schema introspection (RusvelBase)
├── rusvel-db/            SQLite WAL + migrations
├── rusvel-llm/           4 providers: Ollama, OpenAI, Claude API, Claude CLI + multi-router + ModelTier routing
├── rusvel-agent/         Agent runtime (tool-use loop, streaming, AgentEvent) + persona + workflow
├── rusvel-event/         Event bus + persistence
├── rusvel-memory/        FTS5 session-namespaced search
├── rusvel-tool/          Tool registry + ScopedToolRegistry + JSON Schema validation
├── rusvel-builtin-tools/ 10 built-in tools (9 + tool_search meta-tool)
├── rusvel-engine-tools/  12 engine tools (harvest 5, content 5, code 2)
├── rusvel-mcp-client/    MCP client for external MCP servers
├── rusvel-channel/       ChannelPort + Telegram adapter (outbound notify)
├── rusvel-pipeline/      Forge pipeline step runner (harvest + content; keeps forge-engine lean)
├── rusvel-jobs/          Central job queue + approval
├── rusvel-auth/          In-memory credential storage (from env)
├── rusvel-config/        TOML config + per-session overrides
├── rusvel-deploy/        Deployment port adapter
├── rusvel-embed/         Text embedding adapter
├── rusvel-vector/        Vector store (LanceDB) for semantic search
├── rusvel-terminal/      Terminal multiplexer (TerminalPort, PTY management)
├── rusvel-cdp/           Chrome DevTools Protocol client (BrowserPort / CDP wiring)
├── rusvel-webhook/       Webhook registration + dispatch
├── rusvel-cron/          Cron scheduling adapter
├── forge-engine/         Agent orchestration + Mission (goals, plans, reviews, 10 personas)
├── code-engine/          Code intelligence: parser, dependency graph, BM25 search, metrics
├── harvest-engine/       Opportunity discovery: source scanning, scorer, proposal gen, pipeline
├── content-engine/       Content creation: writer, calendar, platform adapters (LinkedIn, Twitter, DEV.to), analytics
├── gtm-engine/           GoToMarket: CRM, outreach, invoicing
├── finance-engine/       Ledger, runway, tax estimation
├── product-engine/       Roadmap, pricing, feedback
├── growth-engine/        Funnel analysis, cohort tracking
├── distro-engine/        SEO, marketplace, affiliates
├── legal-engine/         Contracts, compliance, IP
├── support-engine/       Tickets, knowledge base, NPS
├── infra-engine/         Deployment, monitoring, incidents
├── flow-engine/          DAG workflow engine: petgraph, code/condition/agent nodes
├── dept-forge/           Forge department (DepartmentApp)
├── dept-code/            Code department (DepartmentApp)
├── dept-content/         Content department (DepartmentApp)
├── dept-harvest/         Harvest department (DepartmentApp)
├── dept-flow/            Flow department (DepartmentApp)
├── dept-gtm/             GTM department (DepartmentApp)
├── dept-finance/         Finance department (DepartmentApp)
├── dept-product/         Product department (DepartmentApp)
├── dept-growth/          Growth department (DepartmentApp)
├── dept-distro/          Distro department (DepartmentApp)
├── dept-legal/           Legal department (DepartmentApp)
├── dept-support/         Support department (DepartmentApp)
├── dept-infra/           Infra department (DepartmentApp)
├── dept-messaging/       Messaging department (DepartmentApp)
├── rusvel-cli/           3-tier CLI: one-shot + REPL + TUI
├── rusvel-api/           Axum HTTP: ~153 `.route(` chains in lib.rs, 39 handler modules
├── rusvel-mcp/           MCP server (stdio JSON-RPC) — wired via --mcp flag
├── rusvel-tui/           TUI dashboard (ratatui) — wired via --tui flag
└── rusvel-app/           Binary entry point + composition root + rust-embed frontend
frontend/                 SvelteKit 5 + Tailwind 4 (dept/[id], chat, database, flows, knowledge, settings)
```

## Wired Features

- **DepartmentApp pattern (ADR-014)** — All 14 `dept-*` crates implement `DepartmentApp` (13 domain-engine-backed + `dept-messaging` shell registered last at boot)
- **EngineKind enum removed** — String IDs everywhere for department identification
- **AgentRuntime** — `run_streaming()` + `AgentEvent` replaced ClaudeCliStreamer
- **LlmStreamEvent** — `stream()` on LlmPort for character-by-character streaming
- **ModelTier routing** — Haiku/Sonnet/Opus + CostTracker in MetricStore
- **ScopedToolRegistry** — Per-department tool filtering
- **Deferred tool loading** — `tool_search` meta-tool (85% token savings)
- **22+ registered tools** — 10 built-in (read_file, write_file, edit_file, glob, grep, bash, git_status, git_diff, git_log + tool_search) + 12 engine tools (harvest 5, content 5, code 2)
- **MCP dispatch** — `--mcp` flag connects to `RusvelMcp::new()` in main.rs
- **Department Registry** — 14 booted departments, parameterized API routes, dynamic frontend route
- **Chat** — SSE streaming per department + God Agent chat
- **CRUD** — Agents, Skills, Rules, MCP Servers, Hooks, Workflows (all departments)
- **Skills execution** — `resolve_skill()` with `{{input}}` interpolation
- **Rules injection** — `load_rules_for_engine()` appended to system prompt
- **Hook dispatch** — `tokio::spawn` on chat completion events
- **Outbound notify** — `POST /api/system/notify` via `rusvel-channel` (`TelegramChannel` when `RUSVEL_TELEGRAM_BOT_TOKEN` is set)
- **Webhook → forge pipeline** — Webhook registration `event_kind` `forge.pipeline.requested` + JSON body `session_id` enqueues `JobKind::Custom("forge.pipeline")` (worker runs `ForgeEngine::orchestrate_pipeline`)
- **Flow node-type filter** — `parallel_evaluate` omitted from `GET /api/flows/node-types` unless `RUSVEL_FLOW_PARALLEL_EVALUATE=1`
- **Harvest outcome vectors** — `HarvestEngine::configure_rag` embeds outcomes and adds similarity hints when embedding + vector store are wired
- **Capability Engine** — AI-powered online discovery + JSON bundle install
- **`!build` command** — Generate agents/skills/rules/mcp/hooks from natural language
- **Frontend embedding** — rust-embed compiles `frontend/build/` into binary, ServeDir SPA fallback
- **Onboarding** — CommandPalette, OnboardingChecklist, ProductTour, DeptHelpTooltip components
- **Workflow Builder** — AgentNode + WorkflowBuilder visual components in frontend
- **Frontend** — ToolCallCard, ApprovalCard, ApprovalQueue with sidebar badge, manifest-aligned dept nav, /flows page
- **Domain engines wired** — 13 engines + 14 `DepartmentApp` departments (messaging dept has no separate engine crate yet)
- **Engine API routes** — 15 engine-specific endpoints (`/api/dept/code/analyze`, `/api/dept/content/draft`, `/api/dept/content/from-code`, etc.)
- **Engine CLI commands** — `rusvel code analyze`, `rusvel code search`, `rusvel content draft`, `rusvel content from-code`, `rusvel harvest pipeline`
- **Job queue worker** — Background worker processes CodeAnalyze, ContentPublish, HarvestScan jobs via real engines (session_id scoped)
- **Flow Engine** — DAG workflow engine (petgraph), 3 node types (code, condition, agent), multiple `/api/flows/*` routes (see `rusvel-api/src/lib.rs`)
- **RusvelBase** — Database browser UI with schema introspection, table viewer, SQL runner at `/api/db/*`
- **Knowledge/RAG** — Vector-backed knowledge base with multiple `/api/knowledge/*` routes
- **Code-to-Content** — Pipeline from code analysis to content drafts via `/api/dept/content/from-code`
- **Real platform adapters** — LinkedIn, Twitter, DEV.to for content publishing
- **MCP Client** — Connect to external MCP servers for tool discovery
- **3 LLM streaming providers** — ClaudeCliProvider (real streaming), others (batch fallback)
- **TerminalPort** — `rusvel-terminal` crate for PTY management

## Three-Tier CLI Interface

```
rusvel <dept> <action>     # Tier 1: One-shot commands (subset of departments; see CLI — API/registry lists 14)
rusvel shell               # Tier 2: Interactive REPL (reedline, autocomplete, history)
rusvel --tui               # Tier 3: TUI dashboard (ratatui, 4-panel layout)
```

**Tier 1 departments:** forge, finance, growth, distro, legal, support, infra, product, code, harvest, content, gtm
**Tier 1 actions:** `status`, `list [--kind X]`, `events`
**Engine-specific actions:** `code analyze [path]`, `code search <query>`, `content draft <topic>`, `harvest pipeline`
**Tier 2 REPL:** `use <dept>` to switch context, Tab completion, Ctrl+R history search
**Tier 3 TUI:** Tasks, Goals, Pipeline, Events panels — press `q` to exit

## Not Yet Wired

- **OutreachSend jobs** — Wired: `gtm-engine` outreach + `JobKind::OutreachSend` in `rusvel-app` job worker (approval → send → next step). SMTP/mock email via `RUSVEL_SMTP_*`; see `gtm-engine/src/outreach.rs`.
- **Authentication/authorization** — rusvel-auth is in-memory from env vars; optional bearer token env; API route middleware is phased (see `docs/plans/sprint-6-7-implementation.md`).

## Design Docs

- `docs/design/vision.md` — What RUSVEL is
- `docs/design/architecture-v2.md` — Current architecture (v2, post-review)
- `docs/design/decisions.md` — 14 ADRs with rationale
- `docs/design/audit-2026-03-23.md` — Comprehensive codebase audit
- `docs/status/current-state.md` — **Canonical** runtime snapshot + metrics glossary
- `docs/plans/phase-0-foundation-v2.md` — Historical Phase 0 milestones (see banner in file)
- `docs/plans/roadmap-v2.md` — 5-phase roadmap

## Frontend (pnpm)

**Package manager: pnpm** (not npm). All frontend commands use `pnpm`.

```bash
cd frontend
pnpm install                   # Install dependencies
pnpm dev                       # Start dev server on :5173
pnpm build                     # Build for production (output: build/)
pnpm check                     # TypeScript + Svelte type checking
pnpm test:e2e                  # Run all E2E tests (Playwright)
pnpm test:visual               # Run visual regression tests only
pnpm test:e2e:update           # Update visual baselines
pnpm test:analyze              # AI-powered visual diff analysis (Claude Vision)
```

## Testing

```bash
cargo test                     # Full workspace from repo root (~645 tests summed; counts vary by cargo output)
cargo test -p rusvel-core      # Single crate
cargo test -p forge-engine     # Engine tests (15 tests, use mock ports)
cargo test -p content-engine   # Content engine (7 tests)
cargo test -p harvest-engine   # Harvest engine (12 tests)
cargo test -p rusvel-db        # DB store
cargo test -p rusvel-api       # API tests
cargo test -p rusvel-llm       # LLM provider tests
pnpm test:visual               # Visual regression tests (Playwright)

# Line coverage (llvm-cov): rustup component add llvm-tools-preview && cargo install cargo-llvm-cov
./scripts/coverage.sh          # HTML report + summary; see docs/testing/coverage-strategy.md
```

## Visual E2E Testing

Self-correction loop: screenshot -> compare -> Claude Vision analysis -> auto-generate fix skills/rules.

```bash
pnpm test:visual                                    # Run visual tests
pnpm test:analyze                                   # Analyze diffs with Claude Vision
curl -X POST http://localhost:3000/api/system/visual-test   # Run via API
curl -X POST http://localhost:3000/api/system/visual-report/self-correct  # Auto-fix
```

MCP tool: `visual_inspect` — run visual tests from Claude sessions.

## API Modules (rusvel-api, 39 modules beside lib.rs)

agents, analytics, approvals, artifacts, auth, automation, browser, build_cmd, capability, chat, config, connectors, cron,
dashboard, db_routes, department, engine_routes, flow_routes, help, hook_dispatch, hooks, jobs, kits,
knowledge, mcp_servers, operator_runtime, pipeline_runner, playbooks, request_id (crate-internal), routes, rules, secrets, skills,
sse_helpers (crate-internal), system, terminal, visual_report, webhooks, workflows

## Claude Code Tooling

Custom agents, skills, and rules in `.claude/`. See `docs/claude-code-tooling.md` for details.

**Subagents** (`@name` to invoke):
- `@researcher` — research crates, patterns, LLM integrations before implementation
- `@arch-reviewer` — check hexagonal architecture compliance, ADR rules, crate size
- `@dept-auditor` — audit department wiring across all layers

**Skills** (`/name` to invoke):
- `/dept-scaffold <name>` — scaffold a new department (engine + dept-* + wiring)
- `/engine-check <crate>` — verify engine boundary compliance
- `/crate-audit [crate]` — line count, deps, test coverage health check

**Path-based Rules** (auto-loaded when editing matching files):
- `engines.md` → `crates/*-engine/**` — port-only deps, AgentPort, metadata
- `api.md` → `crates/rusvel-api/**` — route patterns, SSE, spawn_blocking
- `frontend.md` → `frontend/**` — pnpm, Svelte 5 runes, Tailwind 4
- `docs.md` → `docs/**/*.md` — formatting conventions
- `dept-crates.md` → `crates/dept-*/**` — DepartmentApp pattern

**Task Runner** (justfile):
```bash
just check              # cargo check + pnpm check
just ci                 # fmt-check + lint + test
just crate-lines        # flag crates over 2000 lines
just check-boundaries   # detect engine→adapter imports
just stats              # workspace statistics
```

## Python Scripts (uv)

**Package manager: uv** (not pip). Python is used for auxiliary scripts only — the app itself is Rust.

```bash
uv run <script.py>             # Run a script with managed deps
uv sync                        # Install all deps to .venv
uv add <package>               # Add a dependency
uv run --with anthropic ...    # One-off with extra deps
```

## Stack

- Rust edition 2024, SQLite WAL, Axum, Clap 4, reedline, ratatui, tokio (~73,058 lines Rust under `crates/`, 301 source files — see `docs/status/current-state.md`)
- SvelteKit 5, Tailwind CSS 4, **pnpm** package manager
- Python scripts: **uv** (pyproject.toml at workspace root)
- LLM: Ollama (local), Claude API, Claude CLI, OpenAI — all implemented
- Vector DB: LanceDB + Arrow for semantic search
- Frontend embedded in binary via rust-embed
- E2E: Playwright visual regression + Claude Vision analysis
