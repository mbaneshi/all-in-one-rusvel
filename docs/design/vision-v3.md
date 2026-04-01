# VISION v3 — From Here to There

> One human. One system. Every department. Every workflow. Every signal.
> Built properly, once.

---

## Where We Start

**Today, April 2026.**

Mehdi is a senior fullstack developer and AI agent builder. He lives in Claude Code 8+ hours a day. He has:

- **RUSVEL** — 73K lines of Rust, 55 crates, hexagonal architecture, 6 working engines, agent runtime, event bus, job queue, approval gates. A solid sketch that proved the architecture. Not yet a product.
- **Throne Protocol** — A complete taxonomy: 23 inbound categories (300+ sources), 5 processing phases (100+ functions), 7 outbound modes (150+ channels). The blueprint for what the system should handle.
- **Claude Code knowledge** — Reverse-engineered source, community repos (superpowers, claude-mem, hermes-agent, openclaw), understanding of skills/agents/rules/hooks/MCP patterns.
- **Platform research** — Deep understanding of Supabase, n8n, NocoDB, Plane, Appwrite, Composio, Flowise, Manus, OpenHands, OpenClaw. How real platforms are built.
- **Financial pressure** — Needs income. Needs clients. Needs visibility. Needs this system to work, not just compile.

He writes the same prompts daily. Loses context between sessions. Manages skills, rules, and configs manually. Has no automation running. His content, CRM, and freelancer pipeline are manual. He's invisible online.

---

## What We're Building

A **personal AI operating system** that grows into a platform.

Not another chatbot. Not another dashboard. Not another SaaS wrapper.

A system that:
- **Receives** every signal that matters — opportunities, messages, events, code changes, market shifts
- **Processes** with AI intelligence — scores, classifies, drafts, decides, routes
- **Acts** through every channel — publishes, bids, emails, notifies, deploys
- **Learns** from every outcome — what works, what fails, what to do differently
- **Serves the developer first** — enhances Claude Code, automates repetitive work, remembers everything

### The Core Metaphor

Think of it as **Supabase meets n8n meets Claude Code**:

- **Supabase** gives you a database with instant API, auth, realtime — you build on top
- **n8n** gives you a workflow engine with 500+ integrations — you automate on top
- **Claude Code** gives you an AI coding assistant with tools and MCP — you work inside it

Our system combines all three:
- A **data foundation** (Postgres + pgvector) with auto-generated API and views (NocoDB pattern)
- A **workflow engine** (n8n-inspired) with triggers, automations, and AI agent nodes
- An **AI brain** (Claude Code-compatible) with skills, agents, rules, memory, MCP
- **Department-scoped contexts** (unique contribution) that organize everything
- A **control plane dashboard** (Next.js) that shows the full picture

---

## Architecture in One Picture

```
┌─────────────────────────────────────────────────────────────────┐
│                        SURFACES                                  │
│                                                                  │
│   Claude Code ──MCP──┐                                          │
│   Web Dashboard ─────┤                                          │
│   Telegram Bot ──────┤──▶  API Server (Bun + Hono)             │
│   CLI ───────────────┤                                          │
│   Webhooks ──────────┘                                          │
│                                                                  │
├──────────────────────────┬──────────────────────────────────────┤
│     INTELLIGENCE         │        DATA FOUNDATION               │
│                          │                                       │
│   Agent Runtime          │   PostgreSQL                          │
│     step(state)→Action   │     entities (people, companies)     │
│     Condenser            │     signals (inbound events)         │
│     todo.md pattern      │     missions (goals, tasks)          │
│     dynamic skills       │     memories (facts, decisions)      │
│     per-tool risk        │     content (drafts, published)      │
│                          │     workflows (DAGs, executions)     │
│   Skill Engine           │     departments (config, manifest)   │
│   Rule Engine            │     skills, rules, agents (registry) │
│   Tool Registry          │     jobs (queue, approvals)          │
│   Memory (RAG)           │     events (append-only log)         │
│                          │                                       │
│   LLM Router             │   pgvector                           │
│     Claude API           │     memory embeddings                │
│     Ollama               │     knowledge embeddings             │
│     OpenAI               │                                       │
│     Gemini               │   Redis                              │
│                          │     BullMQ job queues                │
│                          │     pub/sub events                   │
│                          │     session cache                    │
├──────────────────────────┴──────────────────────────────────────┤
│                      AUTOMATION                                  │
│                                                                  │
│   Worker Process (Bun)                                          │
│     Inbound:  RSS poller, API poller, email, webhooks           │
│     Process:  Agent runs, scoring, enrichment, classification   │
│     Outbound: Publish, email, bid, notify, deploy               │
│     Learn:    Outcome tracking, scoring calibration             │
│     Cron:     Daily brief, freelancer scan, content schedule    │
│                                                                  │
├─────────────────────────────────────────────────────────────────┤
│                      INTEGRATIONS (MCP + Direct)                 │
│                                                                  │
│   MCP Servers: GitHub, Notion, Slack, Stripe, Google, Pipedream │
│   Direct APIs: Freelancer.com, LinkedIn, Twitter, DEV.to        │
│   Messaging:   Telegram, WhatsApp, Discord, Email               │
│   Browser:     CDP for scraping when no API exists              │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## The Journey: 5 Phases

### Phase 1 — The Brain (Weeks 1-4)

**What it does:** Claude Code becomes 10x smarter. You never lose context again.

```
You (in Claude Code)
  │
  ├── "Remember: I chose SQLite WAL because..."  → memory_store → PostgreSQL
  ├── "What did I decide about auth?"            → memory_recall → instant answer
  ├── "Load my code department context"          → dept_context → rules + skills injected
  ├── "/review this PR"                          → skill runs with arch rules
  └── "Search my knowledge base for..."          → knowledge_search → pgvector RAG
```

**Stack:** Bun API + MCP server (15 tools) + PostgreSQL + pgvector + Redis + Docker Compose

**Delivers:** Persistent memory, department-scoped context, skill/rule/agent management, knowledge search. You use it every day in Claude Code.

### Phase 2 — The Dashboard (Weeks 5-8)

**What it does:** You see everything in one place. The control plane.

```
┌──────────────────────────────────────────────────┐
│  Dashboard                                        │
│                                                    │
│  ┌─────────┐ ┌──────────┐ ┌───────────┐         │
│  │ Goals   │ │ Pipeline │ │ Content   │          │
│  │ 3 active│ │ 5 leads  │ │ 4 drafts  │          │
│  │ 1 due   │ │ 2 scored │ │ 2 scheduled│         │
│  └─────────┘ └──────────┘ └───────────┘          │
│                                                    │
│  Recent Agent Activity                             │
│  ● arch-reviewer checked PR #42 — 2 violations   │
│  ● content-writer drafted LinkedIn post           │
│  ● harvester scored 3 new opportunities           │
│                                                    │
│  Approvals (2 pending)                            │
│  ○ Proposal for "AI Pipeline Builder" — $7,500   │
│  ○ LinkedIn post: "How I built..."               │
└──────────────────────────────────────────────────┘
```

**Stack adds:** Next.js 15 + React 19 + shadcn/ui + SSE streaming

**Delivers:** Entity browser (people, companies, opportunities), content studio, approval queue, agent activity log, department views. NocoDB-style dynamic views over your data.

### Phase 3 — The Automation Engine (Weeks 9-12)

**What it does:** Things happen without you prompting.

```
6:00 AM  — Cron fires → Freelancer.com API polled → 12 new gigs found
6:01 AM  — Agent scores all 12 → 3 score above threshold
6:02 AM  — Agent drafts proposals for top 3
6:03 AM  — Telegram: "3 new opportunities scored. Proposals ready."
7:30 AM  — You reply "approve 1 and 3"
7:31 AM  — Proposal 1 submitted via Freelancer API

9:00 AM  — Daily brief generated from all departments
8:00 PM  — Content published to LinkedIn (approved yesterday)
8:01 PM  — Cross-posted to DEV.to with canonical URL
```

**Stack adds:** BullMQ workers, cron scheduler, Telegram bot gateway, RSS/API pollers

**Delivers:** The INBOUND → PROCESS → OUTBOUND loop running continuously.

### Phase 4 — The Workflow Builder (Weeks 13-16)

**What it does:** You compose custom automations visually.

```
[Git Push] ──▶ [Code Analyze] ──▶ [Draft Post]
                                   │
                         ┌─────────┴──────────┐
                         ▼                    ▼
                   [LinkedIn]           [DEV.to]
                         │                    │
                         └────────┬───────────┘
                                  ▼
                         [Notify Telegram]
```

**Stack adds:** n8n-inspired flow engine with visual builder in React

**Delivers:** Custom workflows with drag-and-drop. Trigger, action, condition, agent, code nodes. Templates for common solo-builder flows.

### Phase 5 — The Platform (Weeks 17+)

**What it does:** Other solo builders use it too.

**Stack adds:** Multi-tenant auth, workspace isolation, marketplace, billing

**Delivers:** What you built for yourself, available to others. The taxonomy is universal. The AI intelligence and Claude Code integration are the differentiators.

---

## Data Model

Inspired by ERPNext (cross-department), NocoDB (meta-layer), Plane (workspace scoping):

```
Workspace
  └── Department (config: skills, rules, agents, tools, persona)
       └── Entity (person | company | project | opportunity)
       └── Signal (inbound event: source, category, urgency, payload)
       └── Mission (goal + tasks + outcomes)
       └── Content (draft | published | scheduled, per platform)
       └── Workflow (DAG definition + executions + checkpoints)
       └── Conversation (agent chat history, tool calls, events)

Cross-cutting:
  Memory    — fact | decision | learning | outcome (vector indexed)
  Event     — append-only log, every state change
  Job       — queued | running | completed | awaiting_approval
  Skill     — markdown + frontmatter, loaded per turn
  Rule      — always active, scoped per department
  AgentSpec — model + tools + persona + permissions
```

---

## Patterns We Adopt

| From | Pattern | Why |
|------|---------|-----|
| **Supabase** | JWT-as-session-context, RLS, composable services sharing one DB | Auth + authorization that scales |
| **n8n** | INodeExecutionData arrays with lineage, queue-mode separation, credential encryption | Workflow engine that's debuggable and scalable |
| **NocoDB** | Dual-database (meta vs data), views-as-config, dynamic API from schema | Flexible data layer without code generation |
| **Plane** | State.group abstraction, workspace→project scoping, Y.js CRDT for realtime | Project management patterns, status semantics |
| **Appwrite** | Per-queue workers, document/collection over SQL, service registry | Modular backend services |
| **Manus** | todo.md task externalization, KV-cache aware prompting, async background tasks | Agent focus + cost optimization |
| **OpenHands** | step(state)→Action, Condenser, SecurityAnalyzer + ConfirmationPolicy | Clean agent abstraction + security |
| **OpenClaw** | Messaging gateway, dynamic skill injection, A2UI Canvas, session-as-security | Ambient intelligence + interactive agent UI |
| **Claude Code** | Skills as markdown, agents as specs, MCP as integration, rules by context | Developer-first AI patterns |
| **RUSVEL** | Hexagonal ports/adapters, DepartmentApp manifests, event sourcing, approval gates | Proven architecture patterns |

---

## What Makes This Different

| Every other tool | This system |
|---|---|
| One use case (CRM, or automation, or agent) | All departments, one data model |
| Cloud-only or local-only | Local-first, cloud-ready |
| Separate from your coding workflow | **Lives inside Claude Code** via MCP |
| You go to the tool | **The tool comes to you** (Telegram, notifications) |
| Static integrations | **AI agents** that think, score, draft, decide |
| Manual everything | **Automation-first** with human approval gates |
| Generic AI responses | **Your voice, your rules, your memory** |
| Data in 10 SaaS tools | **One database, one knowledge graph** |

---

## Tech Stack

| Layer | Technology | Why |
|---|---|---|
| Runtime | **Bun** | Fast, TS-native |
| Language | **TypeScript** (strict) | Matches Claude Code, MCP SDK, community |
| API | **Hono** (on Bun) | Lightweight, fast, middleware |
| Web | **Next.js 15** (App Router) | Server components, scale, ecosystem |
| UI | **React 19** + **Tailwind** + **shadcn/ui** | Production components |
| Database | **PostgreSQL 16** + **pgvector** | Relational + vectors, scales to millions |
| ORM | **Drizzle** | Type-safe, lightweight |
| Queue | **BullMQ** + **Redis** | Jobs, pub/sub, caching |
| Auth | **Better Auth** | Multi-tenant, self-hostable |
| MCP | **@modelcontextprotocol/sdk** | Claude Code bridge |
| LLM | **@anthropic-ai/sdk** + **Ollama** + **OpenAI** | Multi-provider |
| Messaging | **Telegram Bot API** → expand | Gateway pattern |
| Containers | **Docker Compose** | Dev + prod parity |
| Monorepo | **Turborepo** | Build caching, task orchestration |
| Testing | **Vitest** | Fast, TS-native |

---

## Where We Land

**12 months from now.**

Mehdi wakes up. His phone has 3 Telegram notifications:
- "2 high-scoring gigs overnight. Proposals drafted. Reply APPROVE to submit."
- "LinkedIn post: 2,400 impressions, 31 comments. 3 new CTO connections."
- "Daily brief: $12K invoiced this month. 2 deals negotiating. 1 contract expiring Friday."

He opens Claude Code. Context is loaded from last session. Skills, rules, and agents are synced. He types `/review` and the arch-reviewer checks the PR against his standards automatically.

He opens the dashboard during lunch. The entity graph shows his network growing. Content calendar full for two weeks. Pipeline has 4 active deals. Custom "Code → Content → Publish" workflow ran successfully this morning.

A client messages on Telegram. The system recognizes the entity, loads relationship history, drafts a response. He edits one line and sends.

He never wrote a prompt to make any of this happen. He built it once, properly. Now it compounds.

**Then he opens it up.** Other solo builders sign up. Same taxonomy — every solo builder has the same 23 inbound categories, same 7 outbound modes. The AI intelligence, the Claude Code integration, the "build once, never repeat" architecture — that's the moat.

---

## How to Start

**Next session:**
1. Create the new repo
2. Docker Compose: PostgreSQL + Redis + Bun API
3. Core domain types in `packages/core`
4. PostgreSQL schema in `packages/db` (Drizzle)
5. MCP server with `memory_store` + `memory_recall` + `memory_search`
6. Wire into Claude Code settings
7. Use it.

**Everything after that is driven by usage.**

---

*Owner: Mehdi Baneshi*
*Written: 2026-04-01*
*Status: Ready to build*
