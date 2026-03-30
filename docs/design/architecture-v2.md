# RUSVEL — Architecture v2 (Post-Review)

> Incorporates Perplexity feedback. This supersedes architecture.md.

---

## What Changed from v1

| v1 | v2 | Why |
|----|-----|-----|
| 7 engines | 5 engines | Ops + Connect merged → GoToMarket. Mission folded into Forge. |
| AutomationPort + SchedulePort separate | Single Job Queue substrate | Eliminate 4 overlapping workflow DSLs |
| StoragePort "persist anything" | 5 canonical stores | Clear boundaries, no re-inventing ORM |
| Code Engine: 12+ languages | Code Engine v0: Rust-only + symbol graph + BM25 | Ship thin, expand later |
| Engines call LlmPort directly | Engines go through AgentPort only | Clean boundary: LlmPort = raw, AgentPort = orchestration |
| No Session hierarchy | Session → Run → Thread | Everything keyed by session |
| No approval model | Explicit human-in-the-loop approvals | Solo founder = "propose then approve" |
| No job queue | Central SQLite job queue | All async work goes through one substrate |
| No Inbox/Capture | Inbox as cross-cutting concern | Funnel everything into sessions |

---

## Revised Architecture

```
┌──────────────────────── SURFACES ─────────────────────────┐
│  CLI (Clap)  │  REPL (reedline)  │  TUI (Ratatui)        │
│  Web (Svelte)│  MCP Server                                │
│           rust-embed serves SPA at /                      │
└──────────────────────────┬────────────────────────────────┘
                       │
┌──────────────────────┴─────────────────────────┐
│         DEPARTMENT REGISTRY (14 depts)           │
│  DepartmentApp + DepartmentManifest (ADR-014)    │
│  String IDs everywhere (EngineKind removed)       │
│  6 parameterized /api/dept/{dept}/* routes        │
│  + 15 engine-specific routes                      │
│  + 7 flow routes + 5 knowledge routes            │
└──────────────────────┬─────────────────────────┘
                       │
┌──────────────────────┴─────────────────────────┐
│          DOMAIN ENGINES (13: 6 wired + 7 ext)    │
│          + 14 dept-* DepartmentApp crates        │
│                                                  │
│  Wired:   Forge  │ Code  │ Harvest │ Content    │
│            Flow   │ GoToMarket                   │
│                                                  │
│  Extended: Finance │ Product │ Growth │ Distro   │
│            Legal   │ Support │ Infra             │
└──────────────────────┬─────────────────────────┘
                       │ uses (traits only)
┌──────────────────────┴─────────────────────────┐
│              FOUNDATION                          │
│                                                 │
│  ┌──────────── rusvel-core ──────────────┐      │
│  │  20 Port Traits + 82 Domain Types     │      │
│  │  DepartmentApp + DepartmentManifest   │      │
│  └───────────────────────────────────────┘      │
│                                                 │
│  ┌──────────── Adapters (18 crates) ────┐      │
│  │  rusvel-llm     (model providers)     │      │
│  │  rusvel-agent   (AgentRuntime)        │      │
│  │  rusvel-db      (SQLite + 5 stores)   │      │
│  │  rusvel-schema  (DB introspection)    │      │
│  │  rusvel-event   (event bus + persist)  │      │
│  │  rusvel-memory  (context + search)    │      │
│  │  rusvel-tool    (ScopedToolRegistry)  │      │
│  │  rusvel-builtin-tools (9 agent tools) │      │
│  │  rusvel-engine-tools (12 engine tools)│      │
│  │  rusvel-mcp-client (external MCP)     │      │
│  │  rusvel-jobs    (central job queue)   │      │
│  │  rusvel-embed   (text embeddings)     │      │
│  │  rusvel-vector  (LanceDB vectors)     │      │
│  │  rusvel-deploy  (deployment ops)      │      │
│  │  rusvel-auth    (credentials)         │      │
│  │  rusvel-config  (settings)            │      │
│  │  rusvel-terminal (TerminalPort)       │      │
│  └───────────────────────────────────────┘      │
│                                                 │
│  ┌──────────── Cross-cutting ────────────┐      │
│  │  Hook dispatch (command/http/prompt)  │      │
│  │  Capability Engine (AI entity builder)│      │
│  │  Approval flow (human-in-the-loop)    │      │
│  └───────────────────────────────────────┘      │
└─────────────────────────────────────────────────┘
```

## The 12 Departments (was 5 engines)

Each department implements the `DepartmentApp` trait (ADR-014) and declares a `DepartmentManifest`.
String IDs (e.g. `"forge"`, `"code"`) replace the former `EngineKind` enum, which has been removed.
The manifest declares name, icon, color, system prompt, capabilities, routes, tools, and quick actions.

### Wired (6 — engines with substantial implementation)

1. **Forge** (`forge-engine`) — Agent orchestration + Mission (goals, daily planning, reviews). The meta-engine.
2. **Code** (`code-engine`) — Code intelligence: parsing, symbol graph, BM25 search, metrics.
3. **Harvest** (`harvest-engine`) — Opportunity discovery: source scanning, scoring, proposal generation.
4. **Content** (`content-engine`) — Content creation, platform adaptation, publishing. Human approval gate.
5. **GoToMarket** (`gtm-engine`) — CRM, outreach sequences, deal pipeline, invoicing. Human approval gate.
6. **Flow** (`flow-engine`) — DAG workflow engine (petgraph), code/condition/agent nodes.

### Skeleton (7 — added to cover full business operations)

7. **Finance** (`finance-engine`) — Revenue tracking, expenses, tax, runway forecasting, P&L.
8. **Product** (`product-engine`) — Roadmaps, feature prioritization, pricing strategy, user feedback.
9. **Growth** (`growth-engine`) — Funnel optimization, conversion tracking, cohort analysis, KPI dashboards.
10. **Distribution** (`distro-engine`) — Marketplace listings, SEO, affiliate programs, partnerships.
11. **Legal** (`legal-engine`) — Contracts, IP protection, compliance, licensing, privacy policies.
12. **Support** (`support-engine`) — Customer support tickets, knowledge base, NPS tracking, auto-triage.
13. **Infra** (`infra-engine`) — CI/CD pipelines, deployments, monitoring, incident response.

### DepartmentApp Trait + dept-* Crates (ADR-014)

Each department lives in its own `dept-*` crate and implements the `DepartmentApp` trait.
The host collects manifests, resolves dependencies, and calls `register()` in order.

```rust
pub trait DepartmentApp: Send + Sync {
    fn manifest(&self) -> DepartmentManifest;
    async fn register(&self, ctx: &mut RegistrationContext) -> Result<()>;
    async fn shutdown(&self) -> Result<()> { Ok(()) }
}

pub struct DepartmentManifest {
    pub id: String,              // URL slug: "forge", "code", "gtm", etc.
    pub name: String,            // Display name
    pub description: String,
    pub icon: String,
    pub color: String,           // oklch color token
    pub system_prompt: String,
    pub capabilities: Vec<String>,
    pub quick_actions: Vec<QuickAction>,
    pub routes: Vec<RouteContribution>,
    pub tools: Vec<ToolContribution>,
    pub dependencies: Vec<String>,
}
```

14 dept-* crates: `dept-forge`, `dept-code`, `dept-content`, `dept-harvest`, `dept-flow`,
`dept-gtm`, `dept-finance`, `dept-product`, `dept-growth`, `dept-distro`, `dept-legal`,
`dept-support`, `dept-infra`, `dept-messaging`. Each wraps its engine crate and wires it
into the host via `DepartmentApp::register()`.

### Parameterized Department Routing

6 parameterized API routes replace what would be 72 hardcoded routes:

```
/api/dept/{dept}/chat                  — department-scoped chat
/api/dept/{dept}/chat/conversations    — list conversations
/api/dept/{dept}/chat/conversations/{id} — get history
/api/dept/{dept}/config               — GET/PUT department config
/api/dept/{dept}/events               — department event stream
```

The `{dept}` parameter is resolved against `DepartmentRegistry` to load the correct
system prompt, capabilities, and config. Adding a new department requires zero route changes.

### Hook Dispatch System

Hooks fire when events occur (e.g., `code.chat.completed`). Three hook types:
- `command` — runs a shell command via `sh -c`
- `http` — POSTs event payload to a URL
- `prompt` — sends action text to `claude -p`

Hooks are stored in ObjectStore and matched by event kind. Fire-and-forget via tokio tasks.

### Capability Engine

`POST /api/capability/build` takes a natural language description and:
1. Uses Claude with WebSearch/WebFetch to discover resources online
2. Generates a bundle of entities (agents, skills, rules, MCP servers, hooks, workflows)
3. Persists all entities to ObjectStore
4. Returns what was installed

Also available in department chat via `!build <description>` prefix.

---

## The 20 Port Traits (15 Port + 5 Store) — was 13 in v1, 10 in early v2

Evolved from 10 to 20 as the system grew — added sub-store traits, embedding/vector ports, deploy, and terminal:

| Port | Responsibility | Notes |
|------|---------------|-------|
| `LlmPort` | Raw model access: generate, stream | Never called directly by engines |
| `AgentPort` | Agent orchestration: create, run, stop, status | Wraps LlmPort + ToolPort + MemoryPort |
| `ToolPort` | Tool registry + execution | JSON Schema declarations |
| `EventPort` | System-wide typed event bus | Immutable, append-only |
| `StoragePort` | 5 canonical sub-stores | Not "persist anything" |
| `EventStore` | Append-only event log | Sub-store of StoragePort |
| `ObjectStore` | CRUD for domain objects | Sub-store of StoragePort |
| `SessionStore` | Session/Run/Thread hierarchy | Sub-store of StoragePort |
| `JobStore` | Job queue persistence | Sub-store of StoragePort |
| `MetricStore` | Time-series metrics | Sub-store of StoragePort |
| `MemoryPort` | Context, knowledge, semantic search | Session-namespaced |
| `JobPort` | Central job queue | All async work |
| `SessionPort` | Session management | Everything keyed by session |
| `AuthPort` | Credentials (opaque handles) | Engines never see raw tokens |
| `ConfigPort` | Settings, preferences | Per-session overrides |
| `EmbeddingPort` | Text → dense vectors | Used by knowledge/RAG |
| `VectorStorePort` | Similarity search | LanceDB adapter |
| `DeployPort` | Deployment operations | CI/CD, hosting |
| `TerminalPort` | Terminal interaction | Shell commands, output capture |

**Plus:** `Engine` trait (name, capabilities, health) — implemented by all 13 engines. Not counted as a port trait.

**Removed from v1:** `AutomationPort`, `SchedulePort`, `HarvestPort`, `PublishPort` — consolidated or moved to engine-internal traits (ADR-003, ADR-006).

---

## StoragePort: 5 Canonical Stores

Instead of "persist anything":

```rust
pub trait StoragePort: Send + Sync {
    fn events(&self) -> &dyn EventStore;      // Append-only event log
    fn objects(&self) -> &dyn ObjectStore;     // Domain objects (Content, Opportunity, Contact, etc.)
    fn sessions(&self) -> &dyn SessionStore;   // Session/Run/Thread hierarchy
    fn jobs(&self) -> &dyn JobStore;           // Job queue (pending, running, completed)
    fn metrics(&self) -> &dyn MetricStore;     // Time-series metrics (engagement, spend, velocity)
}
```

Each store has a focused API. No generic key-value "put anything."

---

## Central Job Queue (replaces AutomationPort + SchedulePort)

All async work goes through one substrate (inspired by Windmill):

```rust
pub struct Job {
    pub id: JobId,
    pub session_id: SessionId,
    pub kind: JobKind,
    pub payload: serde_json::Value,
    pub status: JobStatus,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub retries: u32,
    pub max_retries: u32,
    pub error: Option<String>,
}

pub enum JobKind {
    AgentRun,          // Forge engine handles
    ContentPublish,    // Content engine handles
    OutreachSend,      // GoToMarket engine handles
    HarvestScan,       // Harvest engine handles
    CodeAnalyze,       // Code engine handles
    ScheduledCron,     // Recurring job
    Custom(String),    // Extensible
}

pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    AwaitingApproval,  // Human-in-the-loop gate
}

#[async_trait]
pub trait JobPort: Send + Sync {
    async fn enqueue(&self, job: NewJob) -> Result<JobId>;
    async fn dequeue(&self, kinds: &[JobKind]) -> Result<Option<Job>>;
    async fn complete(&self, id: &JobId, result: JobResult) -> Result<()>;
    async fn fail(&self, id: &JobId, error: String) -> Result<()>;
    async fn schedule(&self, job: NewJob, cron: &str) -> Result<JobId>;
    async fn cancel(&self, id: &JobId) -> Result<()>;
    async fn approve(&self, id: &JobId) -> Result<()>;  // Human approval
    async fn list(&self, filter: JobFilter) -> Result<Vec<Job>>;
}
```

One queue. One worker pool. All engines submit jobs. The worker routes to the right engine based on `JobKind`.

---

## Session Hierarchy (new)

```rust
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub kind: SessionKind,
    pub tags: Vec<String>,
    pub config: SessionConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum SessionKind {
    Project,           // A codebase / product
    Lead,              // A potential client
    ContentCampaign,   // A content series
    General,           // Catch-all
}

pub struct Run {
    pub id: RunId,
    pub session_id: SessionId,
    pub engine: String,           // Department ID string (EngineKind removed)
    pub input_summary: String,
    pub status: RunStatus,
    pub llm_budget_used: f64,
    pub tool_calls_count: u32,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

pub struct Thread {
    pub id: ThreadId,
    pub run_id: RunId,
    pub channel: ThreadChannel,
    pub messages: Vec<ThreadMessage>,
}

pub enum ThreadChannel { User, Agent, System, Event }
// Note: EngineKind enum was REMOVED (ADR-014). String department IDs used everywhere.
```

---

## Human-in-the-Loop Approval Model

Critical for a solo founder — agents propose, human approves:

```rust
pub enum ApprovalStatus {
    NotRequired,       // Auto-execute (e.g., code analysis)
    Pending,           // Waiting for human
    Approved,          // Human said yes
    Rejected,          // Human said no
    AutoApproved,      // Policy allowed auto-approval
}

pub struct ApprovalPolicy {
    pub engine: String,          // Department ID string (EngineKind removed)
    pub action: String,          // "publish", "send_outreach", "spend > $1"
    pub requires_approval: bool,
    pub auto_approve_below: Option<f64>,  // Auto-approve if cost < threshold
}
```

Applied to:
- Content publishing (always requires approval by default)
- Outreach sending (always requires approval by default)
- Agent runs above cost threshold
- Invoice creation

---

## Shared Domain Types (updated)

All in rusvel-core. All have `metadata: serde_json::Value` for schema evolution.

```rust
// Identity
pub struct UserId(Uuid);
pub struct WorkspaceId(Uuid);

// LLM
pub struct ModelRef { pub provider: ModelProvider, pub model: String }
pub enum ModelProvider { Claude, OpenAI, Gemini, Ollama, Other(String) }

// Content (universal message type from adk-rust)
pub struct Content { pub parts: Vec<Part> }
pub enum Part { Text(String), Image(Bytes), Audio(Bytes), Video(Bytes), File { name: String, data: Bytes } }

// Agent
pub struct AgentProfile {
    pub id: AgentProfileId,
    pub name: String,
    pub role: String,
    pub instructions: String,
    pub default_model: ModelRef,
    pub allowed_tools: Vec<String>,
    pub capabilities: Vec<Capability>,
    pub budget_limit: Option<f64>,
}

// Opportunity
pub struct Opportunity {
    pub id: OpportunityId,
    pub session_id: SessionId,
    pub source: OpportunitySource,
    pub title: String,
    pub url: Option<String>,
    pub description: String,
    pub score: f64,
    pub stage: OpportunityStage,
    pub value_estimate: Option<f64>,
    pub metadata: serde_json::Value,
}
pub enum OpportunitySource { Upwork, Freelancer, LinkedIn, GitHub, Manual, Other(String) }
pub enum OpportunityStage { Cold, Contacted, Qualified, ProposalSent, Won, Lost }

// Content
pub struct ContentItem {
    pub id: ContentId,
    pub session_id: SessionId,
    pub kind: ContentKind,
    pub title: String,
    pub body_markdown: String,
    pub platform_targets: Vec<Platform>,
    pub status: ContentStatus,
    pub approval: ApprovalStatus,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}
pub enum ContentKind { LongForm, Tweet, Thread, LinkedInPost, Blog, VideoScript, Email, Proposal }
pub enum ContentStatus { Draft, Adapted, Scheduled, Published, Archived }

// Contact
pub struct Contact {
    pub id: ContactId,
    pub session_id: SessionId,
    pub name: String,
    pub emails: Vec<String>,
    pub links: Vec<String>,
    pub company: Option<String>,
    pub role: Option<String>,
    pub tags: Vec<String>,
    pub last_contacted_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

// Goal
pub struct Goal {
    pub id: GoalId,
    pub session_id: SessionId,
    pub title: String,
    pub description: String,
    pub timeframe: Timeframe,
    pub status: GoalStatus,
    pub progress: f64,
    pub metadata: serde_json::Value,
}
pub enum Timeframe { Day, Week, Month, Quarter }
pub enum GoalStatus { Active, Completed, Abandoned, Deferred }

// Task
pub struct Task {
    pub id: TaskId,
    pub session_id: SessionId,
    pub goal_id: Option<GoalId>,
    pub title: String,
    pub status: TaskStatus,
    pub due_at: Option<DateTime<Utc>>,
    pub priority: Priority,
    pub metadata: serde_json::Value,
}
pub enum TaskStatus { Todo, InProgress, Done, Cancelled }
pub enum Priority { Low, Medium, High, Urgent }

// Code
pub struct RepoRef { pub local_path: PathBuf, pub remote_url: Option<String> }
pub struct CodeSnapshotRef { pub id: SnapshotId, pub repo: RepoRef, pub analyzed_at: DateTime<Utc> }

// Events
pub struct Event {
    pub id: EventId,
    pub session_id: Option<SessionId>,
    pub run_id: Option<RunId>,
    pub source: String,          // Department ID string (EngineKind removed)
    pub kind: String,           // Flexible string, not giant enum
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}
```

**Key change:** `Event.kind` is now a `String` (not a giant enum). This avoids rusvel-core knowing about every possible event type. Engines define their own event kind constants.

---

## Dependency Graph (updated 2026-03-26)

```
rusvel-app (binary, composition root)
├── rusvel-cli
├── rusvel-api (Axum, 153 route chains across 39 handler modules beside lib.rs) ── serves SPA via fallback
├── rusvel-tui (Ratatui)
├── rusvel-mcp (rmcp, 6 tools)
│
├── dept-forge ───────┐
├── dept-code ────────┤
├── dept-content ─────┤
├── dept-harvest ─────┤
├── dept-flow ────────┤── implement DepartmentApp trait
├── dept-gtm ─────────┤   (each wraps its engine crate)
├── dept-finance ─────┤
├── dept-product ─────┤
├── dept-growth ──────┤
├── dept-distro ──────┤
├── dept-legal ───────┤
├── dept-support ─────┤
├── dept-infra ───────┤
├── dept-messaging ───┘
│
├── forge-engine ─────┐
├── code-engine ──────┤
├── harvest-engine ───┤
├── content-engine ───┤
├── flow-engine ──────┤── depend on rusvel-core ONLY
├── gtm-engine ───────┤
├── finance-engine ───┤
├── product-engine ───┤
├── growth-engine ────┤
├── distro-engine ────┤
├── legal-engine ─────┤
├── support-engine ───┤
├── infra-engine ─────┘
│
├── rusvel-llm ───────┐
├── rusvel-agent ─────┤  (AgentRuntime + run_streaming)
├── rusvel-db ────────┤
├── rusvel-schema ────┤
├── rusvel-event ─────┤
├── rusvel-memory ────┤── implement rusvel-core traits
├── rusvel-tool ──────┤  (ScopedToolRegistry + deferred loading)
├── rusvel-builtin-tools ┤
├── rusvel-engine-tools ┤  (12 engine-specific tools)
├── rusvel-mcp-client ┤
├── rusvel-jobs ──────┤
├── rusvel-embed ─────┤
├── rusvel-vector ────┤
├── rusvel-deploy ────┤
├── rusvel-auth ──────┤
├── rusvel-terminal ──┤  (TerminalPort adapter)
└── rusvel-config ────┘
```

## Workspace (updated 2026-03-30)

```
rusvel/
├── crates/
│   ├── rusvel-core/          ← 22 port traits (15 primary + 5 Store + ChannelPort + BrowserPort + RusvelBasePort) + domain types + DepartmentApp/Manifest
│   ├── rusvel-schema/        ← DB schema introspection (RusvelBase)
│   ├── rusvel-db/            ← SQLite WAL + 5 canonical stores
│   ├── rusvel-llm/           ← LlmPort: Ollama, OpenAI, Claude API, CLI + ModelTier + CostTracker
│   ├── rusvel-agent/         ← AgentRuntime: run_streaming(), AgentEvent, tool loop
│   ├── rusvel-event/         ← EventPort bus + persistence
│   ├── rusvel-memory/        ← MemoryPort + session-namespaced search
│   ├── rusvel-tool/          ← ScopedToolRegistry + deferred loading + JSON Schema
│   ├── rusvel-builtin-tools/ ← 10 built-in tools (9 + tool_search meta-tool)
│   ├── rusvel-engine-tools/  ← 12 engine-specific tools (code, content, harvest, etc.)
│   ├── rusvel-mcp-client/    ← MCP client for external MCP server connections
│   ├── rusvel-jobs/          ← Central job queue
│   ├── rusvel-embed/         ← EmbeddingPort adapter
│   ├── rusvel-vector/        ← VectorStorePort (LanceDB + Arrow)
│   ├── rusvel-deploy/        ← DeployPort adapter
│   ├── rusvel-auth/          ← AuthPort (opaque credential handles)
│   ├── rusvel-config/        ← ConfigPort (TOML + per-session overrides)
│   ├── rusvel-terminal/      ← TerminalPort adapter
│   ├── rusvel-webhook/       ← Webhook registration + dispatch
│   ├── rusvel-cron/          ← Cron scheduling adapter
│   ├── rusvel-cdp/           ← BrowserPort / Chrome DevTools client
│   ├── rusvel-channel/       ← ChannelPort + Telegram (outbound notify)
│   ├── rusvel-pipeline/      ← Forge pipeline step runner (harvest + content; lean forge-engine)
│   ├── dept-messaging/       ← DepartmentApp shell (registered with foundation block in workspace)
│   │
│   ├── forge-engine/         ← Agent orchestration + Mission (goals/planning) [WIRED]
│   ├── code-engine/          ← Code intelligence: parser, graph, BM25 [WIRED]
│   ├── harvest-engine/       ← Opportunity discovery + scoring [WIRED]
│   ├── content-engine/       ← Content creation + publishing [WIRED]
│   ├── flow-engine/          ← DAG workflow engine (petgraph) [WIRED]
│   ├── gtm-engine/           ← GoToMarket (CRM + outreach + ops) [WIRED]
│   ├── finance-engine/       ← Revenue, expenses, tax, runway, P&L [STUB]
│   ├── product-engine/       ← Roadmaps, pricing, feature prioritization [STUB]
│   ├── growth-engine/        ← Funnels, cohorts, KPIs, retention [STUB]
│   ├── distro-engine/        ← Marketplace, SEO, affiliates, partnerships [STUB]
│   ├── legal-engine/         ← Contracts, IP, compliance, licensing [STUB]
│   ├── support-engine/       ← Tickets, knowledge base, NPS, auto-triage [STUB]
│   ├── infra-engine/         ← CI/CD, deployments, monitoring, incidents [STUB]
│   │
│   ├── dept-forge/           ← DepartmentApp for Forge [NEW]
│   ├── dept-code/            ← DepartmentApp for Code [NEW]
│   ├── dept-content/         ← DepartmentApp for Content [NEW]
│   ├── dept-harvest/         ← DepartmentApp for Harvest [NEW]
│   ├── dept-flow/            ← DepartmentApp for Flow [NEW]
│   ├── dept-gtm/             ← DepartmentApp for GoToMarket [NEW]
│   ├── dept-finance/         ← DepartmentApp for Finance [NEW]
│   ├── dept-product/         ← DepartmentApp for Product [NEW]
│   ├── dept-growth/          ← DepartmentApp for Growth [NEW]
│   ├── dept-distro/          ← DepartmentApp for Distribution [NEW]
│   ├── dept-legal/           ← DepartmentApp for Legal [NEW]
│   ├── dept-support/         ← DepartmentApp for Support [NEW]
│   ├── dept-infra/           ← DepartmentApp for Infra [NEW]
│   │
│   ├── rusvel-api/           ← Axum HTTP: 153 route chains, 39 modules
│   ├── rusvel-cli/           ← 3-tier CLI: one-shot (Clap) + REPL (reedline) + dept subcommands
│   ├── rusvel-tui/           ← TUI dashboard (Ratatui) — wired via --tui flag
│   ├── rusvel-mcp/           ← MCP server (stdio JSON-RPC, 6 tools)
│   └── rusvel-app/           ← Binary entry point (composition root)
│
├── frontend/                 ← SvelteKit 5 + Tailwind 4 + shadcn/ui (oklch tokens)
├── Cargo.toml
└── CLAUDE.md
```

**55** workspace members — **28** `rusvel-*`, **13** `*-engine`, **14** `dept-*` (includes `dept-messaging` and `rusvel-pipeline` in the `rusvel-*` set). Thirteen engines are six wired (Forge, Code, Harvest, Content, Flow, GTM) plus seven stubs. See [`docs/status/current-state.md`](../status/current-state.md) §1 for canonical counts.

### AgentRuntime Streaming + Tool Loop

`AgentRuntime::run_streaming()` returns an `mpsc::Receiver<AgentEvent>` that emits:
- `AgentEvent::TextDelta { text }` — incremental LLM output
- `AgentEvent::ToolCall { name, input }` — tool invocation
- `AgentEvent::ToolResult { name, output }` — tool result
- `AgentEvent::Done { output }` — final output
- `AgentEvent::Error { message }` — error

The runtime manages a multi-turn tool loop: LLM generates tool calls, the runtime executes
them via `ScopedToolRegistry`, feeds results back, and continues until the LLM produces a
final text response or hits the iteration limit.

### ScopedToolRegistry + Deferred Loading

`ScopedToolRegistry` in `rusvel-tool` scopes tool visibility per department. Each department
declares which tools it contributes via `DepartmentManifest::tools`. Tools can be loaded
lazily (deferred) to avoid startup cost. The `tool_search` meta-tool allows the agent to
discover additional tools at runtime.

### ModelTier Routing + CostTracker

`rusvel-llm` supports `ModelTier` routing (e.g. `Quick`, `Standard`, `Premium`) to select
the appropriate model based on task complexity. `CostTracker` tracks token usage and
estimated costs per session.

### Three-Tier Terminal Interface

The CLI surface provides three ways to interact from the terminal:

1. **One-shot commands** (`rusvel <dept> <action>`) — Clap 4 with 11 department subcommands + session/forge. Pipe-friendly, scriptable.
2. **Interactive REPL** (`rusvel shell`) — reedline-powered prompt with Tab completion, history, department context switching (`use finance` → `rusvel:finance> `).
3. **TUI dashboard** (`rusvel --tui`) — Ratatui full-screen dashboard with Tasks, Goals, Pipeline, Events panels. Loads live data from storage.
