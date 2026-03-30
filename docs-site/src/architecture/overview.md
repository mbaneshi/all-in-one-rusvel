
## Design Philosophy

RUSVEL follows **hexagonal architecture** (ports and adapters). The core principle: domain logic never depends on infrastructure. Engines express what they need through port traits; adapters provide the implementations.

## Four Layers

```
┌─────────────────── SURFACES ───────────────────┐
│  CLI (Clap)  │  TUI (Ratatui)  │  Web (Svelte) │
│                  MCP Server                      │
└──────────────────────┬─────────────────────────┘
                       │
┌──────────────────────┴─────────────────────────┐
│              DOMAIN ENGINES (13)                 │
│                                                 │
│  Forge         │ Code       │ Harvest           │
│  (+ Mission)   │            │                   │
│                │            │                   │
│  Content       │ GoToMarket                     │
│                │ (Ops+Connect+Outreach)          │
└──────────────────────┬─────────────────────────┘
                       │ uses (traits only)
┌──────────────────────┴─────────────────────────┐
│              FOUNDATION                          │
│                                                 │
│  ┌──────────── rusvel-core ──────────────┐      │
│  │  Port Traits + Shared Domain Types    │      │
│  └───────────────────────────────────────┘      │
│                                                 │
│  ┌──────────── Adapters ─────────────────┐      │
│  │  rusvel-llm     (model providers)     │      │
│  │  rusvel-agent   (agent runtime)       │      │
│  │  rusvel-db      (SQLite + 5 stores)   │      │
│  │  rusvel-event   (event bus + persist)  │      │
│  │  rusvel-memory  (context + search)    │      │
│  │  rusvel-tool    (tool registry)       │      │
│  │  rusvel-jobs    (central job queue)   │      │
│  │  rusvel-auth    (credentials)         │      │
│  │  rusvel-config  (settings)            │      │
│  └───────────────────────────────────────┘      │
└─────────────────────────────────────────────────┘
```

### Layer 1: rusvel-core (Foundation)

The heart of the system. Contains:

- **22 port traits** in `ports.rs` — includes five `*Store` subtraits under the storage model, **`BrowserPort`**, **`ChannelPort`**, **`RusvelBasePort`**, and primary ports (LlmPort, AgentPort, ToolPort, EventPort, StoragePort, MemoryPort, JobPort, SessionPort, AuthPort, ConfigPort, EmbeddingPort, VectorStorePort, DeployPort, TerminalPort). `DepartmentApp` is defined under `department/`.
- **~100** `pub struct` / `pub enum` in `domain.rs`, plus shared types — Session, Goal, Event, Agent, Content, Opportunity, Contact, Task, DepartmentManifest, etc.
- Zero framework dependencies

### Layer 2: Adapters

Concrete implementations of the port traits:

| Crate | Implements | Notes |
|-------|-----------|-------|
| `rusvel-llm` | LlmPort | 4 providers: Ollama, OpenAI, Claude API, Claude CLI |
| `rusvel-agent` | AgentPort | Wraps LLM + Tool + Memory into orchestration |
| `rusvel-db` | StoragePort | SQLite WAL with 5 sub-stores, migrations |
| `rusvel-event` | EventPort | Event bus with persistence |
| `rusvel-memory` | MemoryPort | FTS5 session-namespaced search |
| `rusvel-tool` | ToolPort | Tool registry with JSON Schema |
| `rusvel-jobs` | JobPort | Central SQLite job queue |
| `rusvel-auth` | AuthPort | In-memory credential storage from env |
| `rusvel-config` | ConfigPort | TOML config with per-session overrides |
| `rusvel-embed` | EmbeddingPort | Local embeddings via fastembed |
| `rusvel-vector` | VectorStorePort | LanceDB vector store |
| `rusvel-deploy` | DeployPort | Deployment adapter |
| `rusvel-terminal` | TerminalPort | Terminal interaction adapter |
| `rusvel-builtin-tools` | — | Built-in tools for agents (file, shell, git, etc.) + optional `tool_search` meta-tool |
| `rusvel-cdp` | — | Chrome DevTools Protocol client (Browser/CDP wiring) |
| `rusvel-engine-tools` | — | Engine-specific tool wiring |
| `rusvel-mcp-client` | — | MCP client for external MCP servers |
| `rusvel-schema` | — | Database schema introspection (RusvelBase) |

### Layer 3: Engines

Domain logic crates. Each engine depends **only** on `rusvel-core` traits:

| Engine | Focus |
|--------|-------|
| `forge-engine` | Agent orchestration + Mission (goals, planning, reviews) |
| `code-engine` | Code intelligence: parser, dependency graph, BM25 search, metrics |
| `harvest-engine` | Opportunity discovery: scanning, scoring, proposals, pipeline |
| `content-engine` | Content creation: writer, calendar, platform adapters, analytics |
| `gtm-engine` | GoToMarket: CRM, outreach sequences, invoicing, deal stages |
| `finance-engine` | Ledger, runway calculator, tax estimation |
| `product-engine` | Roadmap, pricing analysis, feedback aggregation |
| `growth-engine` | Funnel analysis, cohort tracking, KPI dashboard |
| `distro-engine` | SEO, marketplace listings, affiliate channels |
| `legal-engine` | Contract drafting, compliance checks, IP management |
| `support-engine` | Ticket management, knowledge base, NPS tracking |
| `infra-engine` | Deployment, monitoring, incident response |
| `flow-engine` | DAG workflow engine: petgraph, code/condition/agent nodes |

Each engine also has a corresponding **`dept-*` wrapper crate** (e.g. `dept-forge`, `dept-code`, `dept-finance`) that implements the `DepartmentApp` trait (ADR-014), declaring the department's manifest, tools, and registration logic. There are 14 `dept-*` crates (13 engine-backed departments + `dept-messaging`). Of the 13 engines, 6 are fully wired (Forge, Code, Content, Harvest, GTM, Flow) and 7 are skeletons (Finance, Product, Growth, Distro, Legal, Support, Infra).

### Layer 4: Surfaces

User-facing interfaces that wire adapters into engines:

| Surface | Technology | Status |
|---------|-----------|--------|
| `rusvel-api` | Axum HTTP | Active |
| `rusvel-cli` | Clap 4 | Active |
| `rusvel-mcp` | stdio JSON-RPC | Active |
| `rusvel-tui` | Ratatui | Active |
| `frontend/` | SvelteKit 5 + Tailwind 4 | Active |

### Composition Root: rusvel-app

The binary entry point. It constructs all adapters, injects them into engines, and starts the chosen surface (web server, CLI, or MCP). This is the only place where concrete types meet.

## Workspace Layout

```
rusvel/
├── crates/                   55 workspace members
│   ├── rusvel-core/          22 port traits in ports.rs + domain types + DepartmentApp
│   ├── rusvel-db/            SQLite WAL + 5 canonical stores
│   ├── rusvel-llm/           4 LLM providers
│   ├── rusvel-agent/         Agent runtime (LLM+Tool+Memory)
│   ├── rusvel-event/         Event bus + persistence
│   ├── rusvel-memory/        FTS5 session-namespaced search
│   ├── rusvel-tool/          Tool registry + JSON Schema
│   ├── rusvel-builtin-tools/ Built-in + meta tools for agent execution
│   ├── rusvel-engine-tools/  Engine-specific tool wiring
│   ├── rusvel-mcp-client/    MCP client for external servers
│   ├── rusvel-jobs/          Central job queue
│   ├── rusvel-auth/          Credential storage
│   ├── rusvel-config/        TOML config + overrides
│   ├── rusvel-deploy/        Deployment port adapter
│   ├── rusvel-embed/         Text embedding (fastembed)
│   ├── rusvel-vector/        Vector store (LanceDB)
│   ├── rusvel-schema/        Database schema introspection
│   ├── rusvel-terminal/      Terminal interaction adapter
│   ├── rusvel-cdp/           CDP client (browser automation)
│   ├── forge-engine/         Agent orchestration + Mission
│   ├── code-engine/          Code intelligence
│   ├── harvest-engine/       Opportunity discovery
│   ├── content-engine/       Content creation + publishing
│   ├── gtm-engine/           GoToMarket (CRM + outreach)
│   ├── finance-engine/       Ledger, runway, tax
│   ├── product-engine/       Roadmap, pricing, feedback
│   ├── growth-engine/        Funnel, cohorts, KPIs
│   ├── distro-engine/        SEO, marketplace, affiliates
│   ├── legal-engine/         Contracts, compliance, IP
│   ├── support-engine/       Tickets, knowledge base, NPS
│   ├── infra-engine/         Deploy, monitor, incidents
│   ├── flow-engine/          DAG workflow engine
│   ├── dept-forge/           DepartmentApp wrapper for Forge
│   ├── dept-code/            DepartmentApp wrapper for Code
│   ├── dept-harvest/         ... (14 dept-* wrappers total)
│   ├── dept-flow/            DepartmentApp wrapper for Flow
│   ├── rusvel-api/           Axum HTTP API
│   ├── rusvel-cli/           Clap CLI + REPL
│   ├── rusvel-tui/           Ratatui TUI
│   ├── rusvel-mcp/           MCP server (stdio JSON-RPC)
│   └── rusvel-app/           Binary entry point
├── frontend/                 SvelteKit 5 + Tailwind 4
├── Cargo.toml                Workspace manifest (55 members)
└── CLAUDE.md                 Project conventions
```

## Key Rules

1. **Engines never import adapter crates.** They receive port implementations via constructor injection.
2. **Engines never call LlmPort directly.** They use AgentPort, which wraps LLM + Tool + Memory (ADR-009).
3. **All domain types have `metadata: serde_json::Value`** for schema evolution without migrations (ADR-007).
4. **Event.kind is a String**, not an enum. Engines define their own constants (ADR-005).
5. **Single job queue** for all async work. No per-engine scheduling (ADR-003).
6. **Human approval gates** on content publishing and outreach sending (ADR-008).
7. **Each crate stays under 2000 lines.** Single responsibility.

## Port traits (`rusvel-core/src/ports.rs`)

There are **21** `pub trait` definitions in `ports.rs` (including the five `*Store` subtraits and `ChannelPort`). The table below lists the main contracts; see the source for the exact set.

| Port | Responsibility |
|------|---------------|
| `LlmPort` | Raw model access: generate, stream, embed |
| `AgentPort` | Agent orchestration: create, run, stop, status |
| `ToolPort` | Tool registry + execution |
| `EventPort` | System-wide typed event bus (append-only) |
| `StoragePort` | 5 canonical sub-stores (see below) |
| `EventStore` | Append-only event log |
| `ObjectStore` | CRUD for domain objects |
| `SessionStore` | Session/Run/Thread hierarchy |
| `JobStore` | Job queue persistence |
| `MetricStore` | Time-series metrics |
| `MemoryPort` | Context, knowledge, semantic search |
| `JobPort` | Central job queue with approval support |
| `SessionPort` | Session hierarchy management |
| `AuthPort` | Opaque credential handles |
| `ConfigPort` | Settings and preferences |
| `EmbeddingPort` | Text embedding (fastembed) |
| `VectorStorePort` | Vector storage and search (LanceDB) |
| `DeployPort` | Deployment operations |
| `TerminalPort` | Terminal interaction and display |
| `BrowserPort` | Browser/CDP observation and actions |
| `ChannelPort` | Outbound notification channels (Telegram, etc.) |
