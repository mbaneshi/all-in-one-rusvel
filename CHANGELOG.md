# Changelog

All notable changes to RUSVEL are documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Docs
- Refresh metrics and verification evidence to 2026-03-30: `docs/status/current-state.md`, `verification-log-2026-03-30.md`, mdBook sources under `docs-site/src/`, README, `CLAUDE.md`, `PROJECT_CONTEXT.md`, architecture/sprint cross-references. GitHub Actions rebuilds the published book from `docs-site` on push (see `.github/workflows/docs.yml`).

### Added
- Claude Code agents, skills, path-based rules, PR review workflow, justfile (`8b0b0c8`)
- Second Ctrl+C for immediate force quit (`8d3a261`)

### Fixed
- Force exit after 5s when SSE connections block graceful shutdown (`ba440dc`)

### Security
- Restrict CORS to localhost origins (was allow-all)
- Add SQL identifier validation in rusvel-db store (prevent injection)
- Add path traversal guard to all built-in file tools (canonicalize + cwd check)

### Performance
- Wrap all sync DB I/O in `spawn_blocking` (18 async trait methods across 5 stores)

### Architecture
- Move `ChannelPort` trait from rusvel-channel to rusvel-core
- Add department ID constants module (`rusvel-core/src/constants.rs`)
- Extract shared SSE helper from chat.rs and department.rs
- Add `metadata: serde_json::Value` to 9 domain structs (ADR-007 compliance)

### Reliability
- Add request ID middleware with `x-request-id` header and tracing spans
- Add configurable rate limiting (`RUSVEL_RATE_LIMIT`, default 100/sec)
- Deep health check: probe DB, optional services, boot failures, uptime
- Graceful shutdown for HTTP server and job worker via watch channel
- Track failed department registrations in boot artifacts

### Auth
- Add read-only API token support (`RUSVEL_API_READ_TOKEN`) — GET-only scope

### API
- Add `GET /api/mcp-servers/{id}` and `GET /api/hooks/{id}` handlers

### GTM
- Add SMTP retry with exponential backoff (3 retries) and 30s connection timeout

### Testing
- 25 integration tests for code-engine, content-engine, harvest-engine, gtm-engine, rusvel-vector
- 65 DepartmentApp contract tests for all 13 departments
- 14 CLI arg parsing tests, 8 TUI widget rendering tests

### Docs
- Sync all metrics across CLAUDE.md, current-state.md, architecture-v2.md, roadmap-v2.md
- Generate app narrative prompt template

### Dependencies
- Bump tokio-tungstenite 0.24 → 0.28, run cargo update
- Document transitive duplicate deps from LanceDB/DataFusion

---

## [0.1.0] — 2026-03-22 → 2026-03-28

First working release — 190 commits across 7 days. 54 crates, ~62K lines of Rust, 14 departments, ~554 tests.

### Architecture
- Hexagonal ports & adapters — 21 port traits in `rusvel-core`
- DepartmentApp pattern (ADR-014) — all 14 `dept-*` crates implement `DepartmentApp`
- `EngineKind` enum removed — string IDs everywhere
- 14 ADRs documenting key decisions
- Single binary via `rusvel-app` composition root
- Frontend embedded via `rust-embed`

### Core Platform
- **rusvel-core** — 21 port traits, domain types, `DepartmentApp` trait, `DepartmentManifest`
- **rusvel-db** — SQLite WAL with migrations
- **rusvel-llm** — 4 providers: Ollama, OpenAI, Claude API, Claude CLI + multi-router + ModelTier routing
- **rusvel-agent** — Agent runtime with streaming, tool-use loop, persona, workflow
- **rusvel-event** — Event bus + persistence (string-based event kinds, ADR-005)
- **rusvel-memory** — FTS5 session-namespaced search
- **rusvel-tool** — Tool registry + ScopedToolRegistry + JSON Schema validation
- **rusvel-builtin-tools** — 10 built-in tools (read_file, write_file, edit_file, glob, grep, bash, git_status, git_diff, git_log, tool_search)
- **rusvel-engine-tools** — 12 engine tools (harvest 5, content 5, code 2)
- **rusvel-jobs** — Central job queue + approval gates (ADR-008)
- **rusvel-config** — TOML config + per-session overrides
- **rusvel-auth** — In-memory credential storage from env
- **rusvel-embed** — Text embedding (all-MiniLM-L6-v2, 384d)
- **rusvel-vector** — LanceDB vector store for semantic search
- **rusvel-channel** — ChannelPort + Telegram adapter
- **rusvel-webhook** — Webhook registration + HMAC dispatch
- **rusvel-cron** — Cron scheduling adapter
- **rusvel-mcp-client** — MCP client for external MCP servers
- **rusvel-terminal** — Terminal multiplexer (PTY management)
- **rusvel-cdp** — Chrome DevTools Protocol client (BrowserPort)
- **rusvel-deploy** — Deployment port adapter
- **rusvel-schema** — Database schema introspection (RusvelBase)

### Engines (13)
- **forge-engine** — Agent orchestration, missions, goals, plans, reviews, 10 personas
- **code-engine** — Parser, dependency graph, BM25 search, metrics
- **content-engine** — Writer, calendar, platform adapters (LinkedIn, Twitter, DEV.to), analytics
- **harvest-engine** — Source scanning, scorer, proposal generation, pipeline, optional RAG
- **flow-engine** — DAG workflow engine (petgraph), code/condition/agent nodes, durable checkpoints
- **gtm-engine** — CRM contacts/deals, outreach sequences, invoicing, SMTP + mock email
- **finance-engine** — Ledger, runway calculator, tax estimation
- **product-engine** — Roadmap, pricing, feedback
- **growth-engine** — Funnel analysis, cohort tracking, KPI dashboard
- **distro-engine** — SEO, marketplace, affiliates
- **legal-engine** — Contracts, compliance, IP
- **support-engine** — Tickets, knowledge base, NPS
- **infra-engine** — Deployment, monitoring, incidents

### Surfaces
- **rusvel-cli** — 3-tier CLI: one-shot (12 departments) + REPL (reedline) + TUI (ratatui)
- **rusvel-api** — Axum HTTP with ~141 route chains, 36 handler modules, SSE streaming, rate limiting, request IDs, deep health check
- **rusvel-mcp** — MCP server (stdio JSON-RPC) via `--mcp` flag
- **rusvel-tui** — TUI dashboard (ratatui, 4-panel layout) via `--tui` flag

### Frontend (SvelteKit 5 + Tailwind 4)
- Dynamic department route `/dept/[id]` with section subroutes
- Chat with SSE streaming per department + God Agent
- CRUD for agents, skills, rules, MCP servers, hooks, workflows
- Harvest pipeline with drag-and-drop and score filtering
- Content calendar (week/month views)
- GTM CRM: contacts, deals Kanban, invoices, outreach sequences
- Approval queue with sidebar badge
- Command palette with layout toggles and Alt+1-9 dept shortcuts
- Database browser (RusvelBase) with schema introspection and SQL runner
- Flow builder page
- Knowledge/RAG management
- Settings with LLM spend tracking
- Onboarding: CommandPalette, OnboardingChecklist, ProductTour
- Collapsible context panel, bottom panel (terminal, jobs, events)
- Lucide department icons, two-level shell (TopBar + DeptBar)

### API Highlights
- Department chat SSE with shared SSE helper
- 15 engine-specific endpoints (`/api/dept/code/analyze`, `/api/dept/content/draft`, etc.)
- Job queue with background worker (CodeAnalyze, ContentPublish, HarvestScan)
- Webhook → forge pipeline (event-driven)
- Cron scheduler REST API with background tick
- Telegram notifications via `POST /api/system/notify`
- Bearer auth on AppState
- Configurable rate limiting (`RUSVEL_RATE_LIMIT`, default 100/sec)
- Request ID middleware (`x-request-id`)
- Deep health check with DB probing
- LLM spend aggregation by department

### Testing
- ~636 tests across workspace (sum of `running N tests` lines; see `docs/status/current-state.md`)
- 65 DepartmentApp contract tests for all 13 departments
- 25 integration tests for 5 engine/adapter crates
- 14 CLI arg parsing tests + 8 TUI widget rendering tests
- API smoke tests with shared Axum harness
- E2E tests: harvest pipeline, outreach sequence, webhook + cron
- Coverage floor: 42% lines (CI enforced)
- Playwright visual regression + Claude Vision analysis

### Security & Performance
- CORS restricted
- SQL identifier validation
- File tool path guards
- All sync DB I/O wrapped in `spawn_blocking`
- Graceful shutdown with 5s hard-exit timeout

### Developer Tooling
- Claude Code: 3 subagents, 3 skills, 5 path-based rules
- Claude PR review GitHub Actions workflow
- justfile with 16 task runner recipes
- CI: cargo build + test with coverage floor + frontend build
- GitHub Pages docs deployment
- Criterion boot benchmark
