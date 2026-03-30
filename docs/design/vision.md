# RUSVEL — Vision Document

> The Solo Builder's AI-Powered Virtual Agency
> One binary. One human. Infinite leverage.

---

## The Problem

A solo founder wears every hat: developer, marketer, sales, ops, recruiter, content creator, project manager. The tools exist to help — but they're scattered across dozens of SaaS platforms, languages, and workflows. Each tool solves one problem but creates integration overhead.

The result: a solo founder spends more time managing tools than doing work.

## The Solution

RUSVEL is a single Rust binary that replaces an entire agency. It combines AI agents, automation, and domain-specific intelligence into one system that a solo founder controls from a unified interface.

**Not a platform for teams. Not enterprise software. A personal superpower.**

## Core Philosophy

1. **One binary, zero ops** — `cargo install rusvel` or download a release. No Docker, no cloud, no configuration required. SQLite for storage, Ollama for local AI. Cloud services are optional adapters.

2. **Ports & adapters** — The core is pure Rust traits with zero framework dependencies. Everything pluggable. Swap Claude for Ollama. Swap SQLite for Supabase. Swap Twitter for LinkedIn. The core doesn't care.

3. **Agents do the work** — The founder decides what to do. AI agents figure out how and execute. The system orchestrates multiple agents working in parallel across domains.

4. **Local-first, cloud-optional** — Everything works offline. When online, cloud services enhance capabilities (better LLMs, more sources, broader publishing).

5. **Small crates, deep modules** — Each crate has a small interface and significant implementation. Under 2000 lines. Single responsibility. The workspace is the composition layer.

## Architecture

```
┌──────────────────────── SURFACES ─────────────────────────┐
│  CLI (Clap)  │  REPL (reedline)  │  TUI (Ratatui)        │
│  Web (Svelte)│  MCP Server                                │
└──────────────────────────┬────────────────────────────────┘
                       │
┌──────────────────────┴─────────────────────────┐
│       DOMAIN ENGINES (13) + dept-* crates       │
│                                                 │
│  Core: Forge │ Code │ Harvest │ Content │ GTM   │
│  Extended: Finance│Product│Growth│Distro│Legal  │
│            Support│ Infra │ Flow                │
└──────────────────────┬─────────────────────────┘
                       │ (depends on)
┌──────────────────────┴─────────────────────────┐
│              FOUNDATION (Ports + Adapters)       │
│                                                 │
│  rusvel-core   (19 traits, zero deps)           │
│  rusvel-llm    (Claude/OpenAI/Ollama)           │
│  rusvel-agent  (agent runtime + workflows)      │
│  rusvel-db     (SQLite WAL + migrations)        │
│  rusvel-event  (event bus + persistence)        │
│  rusvel-memory (context + semantic search)      │
│  rusvel-tool   (tool registry + execution)      │
│  rusvel-builtin-tools (9 built-in agent tools)  │
│  rusvel-jobs   (central job queue)              │
│  rusvel-vector (LanceDB vector store)           │
│  rusvel-embed  (text embeddings)                │
│  rusvel-schema (DB schema introspection)        │
│  rusvel-mcp-client (external MCP connections)   │
│  rusvel-deploy (deployment adapter)             │
│  rusvel-auth   (API keys from env)              │
│  rusvel-config (settings + workspace config)    │
└─────────────────────────────────────────────────┘
```

## The 19 Port Traits (14 Port + 5 Store) in rusvel-core

| Port | Responsibility |
|------|---------------|
| `LlmPort` | Raw model access: generate, stream (LlmStreamEvent) |
| `AgentPort` | Agent orchestration: create, run, stop, status |
| `ToolPort` | Tool registry + execution (ScopedToolRegistry) |
| `EventPort` | System-wide typed event bus |
| `StoragePort` | 5 canonical sub-stores (see architecture-v2) |
| `EventStore` | Append-only event log |
| `ObjectStore` | CRUD for domain objects |
| `SessionStore` | Session/Run/Thread hierarchy |
| `JobStore` | Job queue persistence |
| `MetricStore` | Time-series metrics |
| `MemoryPort` | Context, knowledge, semantic search |
| `JobPort` | Central job queue (replaces AutomationPort + SchedulePort) |
| `SessionPort` | Session management |
| `AuthPort` | Credentials (opaque handles) |
| `ConfigPort` | Settings, per-session overrides |
| `EmbeddingPort` | Text → dense vectors |
| `VectorStorePort` | Similarity search |
| `DeployPort` | Deployment operations |
| `TerminalPort` | Terminal interaction, shell commands |

**Plus:** `Engine` trait (name, capabilities, health) — implemented by all 13 engines. Not counted as a port trait.

> **Removed from v1:** `AutomationPort`, `SchedulePort`, `HarvestPort`, `PublishPort` — consolidated or moved to engine-internal traits (see ADR-003, ADR-006).

## The 13 Domain Engines

### Core Engines (5 — fully wired)

**Forge Engine** — Agent Orchestration (meta-engine)
- 10 agent personas, mission planning, goal orchestration
- `rusvel forge mission today` → AI daily brief
- Safety: budget enforcement, approval gates

**Code Engine** — Code Intelligence
- Rust parsing, dependency graph, BM25 symbol search
- Complexity and coupling metrics
- Wired: `rusvel code analyze`, `rusvel code search`, CodeAnalyze jobs

**Harvest Engine** — Opportunity Discovery
- Source scanning, AI-powered scoring, proposal generation
- Pipeline: discover → score → qualify → propose → track
- Wired: `rusvel harvest pipeline`, HarvestScan jobs

**Content Engine** — Creation & Publishing
- Markdown-first authoring, AI adaptation
- Platform adapters: Twitter/X, LinkedIn
- Code-to-content pipeline (from code analysis to blog drafts)
- Wired: `rusvel content draft`, ContentPublish jobs, human approval gate

**Flow Engine** — DAG Workflow Execution
- Petgraph-based directed acyclic graph executor
- 3 node types: code, condition, agent
- 7 API routes at `/api/flows`

### Extended Engines (8 — domain stubs, chat works via generic agent)

- **GoToMarket** (`gtm-engine`) — CRM, outreach sequences, deal pipeline, invoicing
- **Finance** (`finance-engine`) — Ledger, runway, tax, P&L
- **Product** (`product-engine`) — Roadmaps, pricing, feedback
- **Growth** (`growth-engine`) — Funnels, cohorts, KPIs
- **Distribution** (`distro-engine`) — Marketplace, SEO, affiliates
- **Legal** (`legal-engine`) — Contracts, IP, compliance
- **Support** (`support-engine`) — Tickets, knowledge base, NPS
- **Infra** (`infra-engine`) — CI/CD, deployments, monitoring

> **v1 → v2 changes:** Ops + Connect merged → GoToMarket. Mission folded into Forge. 7 extended engines added for full business coverage. See ADR-001, ADR-002.

## Shared Domain Types (in rusvel-core)

```rust
// Universal content type (from adk-rust's Content/Part model)
Content { parts: Vec<Part> }
Part::Text | Part::Image | Part::Audio | Part::Video | Part::File

// Cross-engine types
Opportunity { source, title, score, status, metadata }
Contact { name, channels, relationship_score, last_interaction }
ContentPiece { body, format, platform_variants, schedule, metrics }
Goal { description, deadline, progress, sub_goals }
AgentTask { agent_id, workflow, status, cost, events }
Session { id, workspace, agents, memory_scope, created_at }
```

## Three-Tier CLI UX

### Tier 1 — One-shot Commands
```bash
# Start the web dashboard
$ rusvel

# Session management
$ rusvel session create my-startup
$ rusvel session list
$ rusvel session switch <id>

# Forge engine
$ rusvel forge mission today
$ rusvel forge mission goals
$ rusvel forge mission review --period week

# Department commands (all 11 departments)
$ rusvel finance status
$ rusvel finance list --kind transactions
$ rusvel growth events
$ rusvel harvest status
$ rusvel content list
$ rusvel infra status
$ rusvel legal events
$ rusvel support list --kind tickets
$ rusvel product status
$ rusvel distro list
$ rusvel gtm status
```

### Tier 2 — Interactive REPL Shell
```bash
$ rusvel shell
RUSVEL Interactive Shell
Type 'help' for commands, 'exit' to quit.

rusvel> use finance              # Switch to department context
rusvel:finance> status           # Department-scoped commands
rusvel:finance> list transactions
rusvel:finance> events
rusvel:finance> back             # Leave department context
rusvel> session list             # Session management
rusvel> status                   # All departments overview
rusvel> exit
```
- Tab autocomplete for commands and departments
- Persistent history (`~/.rusvel/shell_history.txt`)
- Ctrl+R for history search

### Tier 3 — TUI Dashboard
```bash
$ rusvel --tui                   # Full-screen terminal dashboard
```
- 4-panel layout: Tasks, Goals, Pipeline, Events
- Loads live data from storage
- Press `q` or `Esc` to exit

## Current State

**Canonical numbers and wiring** (workspace members, LOC, tests, routes, ports): [`docs/status/current-state.md`](../status/current-state.md).

At a glance: **55** workspace crates, **22** port traits in `rusvel-core`, **14** booted `DepartmentApp` departments (including `dept-messaging` last in boot order), **13** domain engines, **22+** registered agent tools, **AgentRuntime** streaming, **ModelTier** routing, embedded SvelteKit UI, **3-tier CLI** (one-shot + REPL + TUI), **6** MCP stdio tools.

## What RUSVEL Is NOT

- Not a SaaS platform (it's a personal tool)
- Not for teams (it's for one person)
- Not a framework (it's a product)
- Not cloud-dependent (local-first)
- Not another wrapper around ChatGPT (it's an agent system with domain intelligence)

## Name

**RUSVEL** = **Rus**t + S**vel**teKit

The stack IS the name. Simple. Memorable. Searchable.
