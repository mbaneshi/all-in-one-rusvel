# Phase 0 — Foundation v2 (Post-Review)

> **Historical milestones.** Checkboxes below describe original Phase 0 intent. **Implemented vs not** is tracked in [`../status/current-state.md`](../status/current-state.md) and [`../status/verification-log-2026-03-27.md`](../status/verification-log-2026-03-27.md). Do not infer current product status from unchecked boxes alone.

> Incorporates Perplexity feedback. Supersedes phase-0-foundation.md.
> Goal: Prove hexagonal architecture with one vertical slice.
> Deliverable: `rusvel forge mission today` works from CLI, API, and Web.

---

## Changes from v1

- Mission is now part of Forge Engine (not a separate engine)
- Central job queue replaces AutomationPort + SchedulePort
- SessionPort has Session → Run → Thread hierarchy
- StoragePort has 5 canonical stores (not generic key-value)
- Event.kind is a String (not giant enum)
- All domain types have `metadata: serde_json::Value` for evolution
- Human approval model included from the start

---

## Milestone 0.1 — Core Traits + Types

**rusvel-core: 19 port traits (14 Port + 5 Store) + 82 domain types. Zero framework deps.**

- [x] Port traits: LlmPort, AgentPort, ToolPort, EventPort, StoragePort (5 sub-stores), MemoryPort, JobPort, SessionPort, AuthPort, ConfigPort, EmbeddingPort, VectorStorePort, DeployPort, Engine
- [ ] StoragePort with 5 sub-stores: EventStore, ObjectStore, SessionStore, JobStore, MetricStore
- [ ] Session hierarchy: Session, Run, Thread + SessionKind, RunStatus, ThreadChannel
- [ ] Job types: Job, JobKind, JobStatus, NewJob, JobFilter, JobResult
- [ ] Approval model: ApprovalStatus, ApprovalPolicy
- [ ] Domain types: Content/Part, AgentProfile, Opportunity, ContentItem, Contact, Goal, Task, Event
- [ ] ID newtypes: SessionId, RunId, ThreadId, JobId, AgentProfileId, EventId, OpportunityId, ContentId, ContactId, GoalId, TaskId, SnapshotId, UserId, WorkspaceId
- [ ] Enums: ModelProvider, OpportunitySource, OpportunityStage, ContentKind, ContentStatus, Timeframe, GoalStatus, TaskStatus, Priority, SessionKind (EngineKind removed in ADR-014)
- [ ] Error type: RusvelError
- [ ] Engine trait: name, capabilities, initialize, shutdown, health
- [ ] All types: Serialize + Deserialize + Clone + Debug
- [ ] All domain types have `metadata: serde_json::Value`
- [ ] `cargo build` succeeds
- [ ] `cargo test` passes (unit tests for type construction)

## Milestone 0.2 — First Adapters

**Minimum adapters to support forge mission.**

- [ ] `rusvel-config` — Load TOML from `~/.rusvel/config.toml`, per-session overrides
- [ ] `rusvel-db` — SQLite WAL, migration runner, implement 5 canonical stores
  - [ ] EventStore: append, query by session/time range
  - [ ] ObjectStore: CRUD for domain objects (typed by table prefix)
  - [ ] SessionStore: Session/Run/Thread CRUD
  - [ ] JobStore: enqueue, dequeue (FOR UPDATE SKIP LOCKED pattern), complete, fail
  - [ ] MetricStore: time-series insert + query
- [ ] `rusvel-event` — In-memory broadcast bus + persist to EventStore
- [ ] `rusvel-llm` — Ollama adapter: generate, generate_stream (no embed yet)
- [ ] `rusvel-memory` — SQLite FTS5 for text search, session-namespaced
- [ ] `rusvel-tool` — Tool registry, JSON Schema validation, basic tool calling
- [ ] `rusvel-auth` — Store/retrieve API keys from SQLite (encrypted at rest)
- [ ] `rusvel-jobs` — Job queue on top of JobStore, simple in-process worker pool
- [ ] Unit tests for each adapter
- [ ] Integration test: create session → store memory → search memory → verify

## Milestone 0.3 — Forge Engine (with Mission)

**Forge engine using only port traits. Includes mission sub-domain.**

- [ ] `ForgeEngine` struct with injected ports
- [ ] Implements `Engine` trait
- [ ] Mission sub-module:
  - [ ] `mission_today(session_id)` — read goals + recent events → LLM → daily plan → store as Tasks
  - [ ] `set_goal(session_id, goal)` — store goal, emit event
  - [ ] `list_goals(session_id)` — query goals
  - [ ] `review(session_id, period)` — summarize progress via LLM
- [ ] Agent sub-module:
  - [ ] `create_agent(profile)` — register an agent
  - [ ] `run_agent(agent_id, input, session_id)` — create Run, execute, stream output, complete Run
- [ ] Creates Run for every execution
- [ ] Emits events for all actions
- [ ] Respects session boundaries
- [ ] Integration test: create session → set goals → generate today → verify plan + events

## Milestone 0.4 — CLI Surface (3-Tier)

**Wire forge engine to CLI with three interaction modes.**

- [x] `rusvel-cli` with Clap 4
- [x] `rusvel session create <name>` → creates session, prints ID
- [x] `rusvel session list` → lists sessions
- [x] `rusvel session switch <id>` → sets active session
- [x] `rusvel forge mission today` → daily plan for active session
- [x] `rusvel forge mission goals` → list goals
- [x] `rusvel forge mission goal add "..."` → create goal
- [x] `rusvel forge mission review --weekly` → weekly review
- [x] `rusvel-app` binary wires adapters → engine → CLI
- [x] Pretty terminal output
- [x] End-to-end test: binary produces output
- [x] **Tier 1 — Department subcommands:** `rusvel <dept> status|list|events` for all 11 departments
- [x] **Tier 2 — Interactive REPL:** `rusvel shell` with reedline (autocomplete, history, `use <dept>` context switching)
- [x] **Tier 3 — TUI dashboard:** `rusvel --tui` wired to ratatui (Tasks, Goals, Pipeline, Events panels)

## Milestone 0.5 — API + Web Surface

**HTTP API and minimal SvelteKit frontend.**

- [ ] `rusvel-api` — Axum server:
  - [ ] `GET /api/health`
  - [ ] `GET /api/sessions` + `POST /api/sessions`
  - [ ] `GET /api/sessions/:id`
  - [ ] `GET /api/sessions/:id/mission/today`
  - [ ] `GET /api/sessions/:id/mission/goals` + `POST`
  - [ ] `GET /api/sessions/:id/runs` (run history)
  - [ ] `GET /api/sessions/:id/events` (event timeline)
  - [ ] `WS /api/sessions/:id/stream` (live events)
- [ ] `frontend/` — SvelteKit 5:
  - [ ] Session switcher in sidebar
  - [ ] Dashboard: today's plan + recent events
  - [ ] Goals list with add form
  - [ ] Run history with status
  - [ ] Event timeline
- [ ] Build frontend → embed via rust-embed
- [ ] `rusvel` (no args) starts Axum + opens browser
- [ ] `rusvel --headless` starts server only

## Milestone 0.6 — MCP Surface

- [ ] `rusvel-mcp` — rmcp server:
  - [ ] `session_create`, `session_list`, `session_switch`
  - [ ] `mission_today`, `mission_goals`, `mission_add_goal`
- [ ] stdio mode (for Claude Code)
- [ ] SSE mode (for web clients)

---

## Definition of Done for Phase 0

- [x] `cargo build --release` → single binary
- [x] 55 workspace members compile (see root `Cargo.toml`; includes `rusvel-pipeline`, `dept-messaging`, webhooks, cron, channel, etc.)
- [x] `rusvel session create "my-project"` → creates session
- [x] `rusvel forge mission today` → generates daily plan via LLM
- [x] `rusvel` → opens web dashboard with session view
- [x] `rusvel --mcp` → works as MCP server (6 tools)
- [x] 19 port traits defined in rusvel-core (14 Port + 5 Store, including TerminalPort)
- [x] Session → Run → Thread hierarchy working
- [x] Central job queue processing jobs (CodeAnalyze, ContentPublish, HarvestScan)
- [x] Human approval model in domain types + API endpoints + frontend UI
- [x] All domain types have metadata field
- [x] Events emitted and persisted for all actions
- [x] ≥ 50 tests passing (222 tests in 30 binaries, 0 failures)
- [x] CLAUDE.md with project conventions
- [x] 5 engines fully wired (Forge, Code, Content, Harvest, Flow)
- [x] 124 API handler functions across 23 modules
- [x] 12+ frontend pages including database browser, flows, knowledge
- [x] DepartmentApp trait + 14 dept-* crates (ADR-014, EngineKind removed)
- [x] AgentRuntime streaming with multi-turn tool loop
- [x] 22+ registered tools (10 built-in incl. tool_search + 12 engine)
- [x] ModelTier routing + CostTracker
- [x] TerminalPort + rusvel-terminal adapter

---

## What Phase 0 Intentionally Skips

- ~~Code Engine (Phase 1)~~ — Now wired with parser, graph, BM25 search, metrics
- ~~Harvest Engine (Phase 2)~~ — Now wired with source scanning, scoring, proposals
- ~~Content Engine (Phase 2)~~ — Now wired with drafting, platform adapters, code-to-content
- GoToMarket Engine (Phase 3) — still a stub
- ~~TUI surface (Phase 2)~~ — Moved to Phase 0, basic dashboard wired via `--tui`
- ~~Cloud LLM adapters (Phase 1)~~ — Claude API, Claude CLI, OpenAI all implemented
- ~~Semantic vector search (Phase 1)~~ — LanceDB + embeddings wired via rusvel-vector/rusvel-embed
- Multi-language code parsing (Phase 1, Rust only)
- Browser extension (Phase 5)
- A2A protocol (Phase 5)

> **Note:** Phase 0 significantly exceeded original scope. Code, Harvest, Content, and Flow engines are fully wired. Cloud LLM providers and vector search are implemented. The project is effectively in Phase 1-2 territory.
