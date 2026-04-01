# Design Brief: The New System

> Everything we know. One document. Start here.
>
> Written 2026-04-01 after a full-day session analyzing: RUSVEL codebase (73K lines),
> Throne Protocol taxonomy, Claude Code internals, community repos (superpowers, hermes,
> claude-mem, openclaw), Manus/OpenHands/OpenClaw architectures, market research on
> freelancer APIs, content platforms, MCP ecosystem, and 20+ competing products.

---

## 1. Who This Is For

**Mehdi Baneshi.** Senior fullstack developer, AI agent builder, solo founder.
Lives in Claude Code 8+ hours/day. Needs income from freelancing. Needs personal brand.
Needs clients. Needs everything automated. Needs to never write the same prompt twice.

## 2. What This System Is

An **ambient intelligence layer** — always aware, always processing, always ready.

Not a chatbot. Not a dashboard. Not another SaaS tool.
A **personal AI operating system** that receives signals from everywhere,
processes them with AI intelligence, and acts through every available channel.

It runs alongside Claude Code (not replacing it), augments it via MCP and config generation,
and extends to web UI, messaging (Telegram/WhatsApp), cron, webhooks, and any future surface.

```
┌──────────────────────────────────────────────────────────────┐
│                     YOU (Claude Code, Web, Mobile, Voice)     │
└──────────────────────────┬───────────────────────────────────┘
                           │
┌──────────────────────────┴───────────────────────────────────┐
│                    THE SYSTEM                                 │
│                                                               │
│   INBOUND          PROCESS              OUTBOUND              │
│   23 categories    5 phases             7 modes               │
│   300+ sources     Intake→Understand    150+ channels         │
│   Always listening →Decide→Act→Learn   Always acting          │
│                                                               │
│   Events, webhooks  Agents, skills      APIs, email           │
│   RSS, MCP, email   Rules, workflows    Posts, proposals      │
│   Browser, cron     Memory, RAG         Notifications         │
│   Messaging apps    Approval gates      Bids, invoices        │
└──────────────────────────────────────────────────────────────┘
```

## 3. Design Principles

### From Throne Protocol
1. **INBOUND → PROCESS → OUTBOUND** — every feature follows this pattern
2. **Hexagonal architecture** — domain has zero external dependencies
3. **Compound growth** — system gets smarter with every interaction
4. **Hybrid execution** — AI agents AND human contractors can execute

### From RUSVEL
5. **Ports & adapters** — swap any component without touching the core
6. **Event sourcing** — every state change is an event, append-only
7. **Human approval gates** — agents propose, human decides (ADR-008)
8. **Department-scoped contexts** — different rules, tools, personas per domain

### From Claude Code
9. **Skills as markdown with frontmatter** — write once, use everywhere
10. **Agents as configurable specs** — not code, configuration
11. **MCP as the universal integration protocol** — one interface, any service
12. **Rules auto-loaded by context** — path-based, department-based

### From Manus
13. **KV-cache aware design** — append-only context, stable prefix, 10x cost savings
14. **todo.md task externalization** — agent never loses focus on long tasks
15. **Files as context** — intermediate data in files, not chat window
16. **Async background execution** — start task, get notified on completion

### From OpenHands
17. **Agent.step(state) → Action** — the entire agent is one clean method
18. **Condenser** — old events compressed to summaries, context never exhausts
19. **Per-tool risk rating** — SecurityAnalyzer + ConfirmationPolicy separation
20. **Stateless agents, stateful conversations** — agents are specs, state is in EventLog

### From OpenClaw
21. **Messaging as primary surface** — WhatsApp/Telegram, not just web
22. **Dynamic skill injection per turn** — semantic relevance, not static loading
23. **A2UI Canvas** — agents generate interactive UI, clicks become tool calls
24. **Session type = security boundary** — operator vs sandboxed per channel

## 4. The Five Layers

### Layer 1: Core Domain (zero dependencies)

```
Ports (traits/interfaces):
  InboundPort    — receive signals from any source
  ProcessPort    — transform signals through intelligence
  OutboundPort   — deliver results to any channel
  MemoryPort     — store/recall/search across sessions
  EventPort      — append-only event log
  StoragePort    — CRUD for domain objects
  AgentPort      — create/run/stop agents
  ToolPort       — register/call/search tools
  ChannelPort    — messaging (Telegram, WhatsApp, email, etc.)

Domain Types:
  Signal         — normalized inbound event (source, category, payload, urgency)
  Mission        — decomposed intent (goals, tasks, constraints, deadline)
  Action         — outbound operation (channel, payload, approval_status)
  Entity         — person, company, project, opportunity (relationship graph)
  Memory         — fact, decision, learning, outcome (vector + FTS indexed)
  AgentSpec      — stateless agent definition (model, tools, rules, persona)
  Skill          — markdown SOP with frontmatter (triggers, tools, context)
  Rule           — always-active constraint (scope: global, department, session)
  Event          — immutable log entry (kind: string, payload, timestamp)
```

### Layer 2: Intelligence Engine (the PROCESS plane)

The five phases from Throne Protocol, implemented as a pipeline:

```
INTAKE
  Receive (API listeners, polling, streams, file watchers, messaging bots)
  Normalize (unified schema, timestamps, dedup, validation)
  Classify (source category, urgency, sensitivity)
  Store (raw + processed, indexed, versioned)

UNDERSTAND
  Entity extraction (people, companies, money, dates, tech)
  Intent detection (asking, offering, informing — action required?)
  Context linking (match to known entities, thread conversations, associate projects)
  Relevance scoring (goal alignment, opportunity score, time sensitivity)
  Enrichment (company lookup, person lookup, financial data)

DECIDE
  Routing (which agent, which workflow, which queue)
  Prioritization (urgency × value × relationship × dependency)
  Decision rules (auto-respond, auto-archive, escalate, delegate)
  Resource allocation (agent availability, budget check, parallel vs sequential)

ACT
  Communication actions (draft email, message, social reply)
  Content actions (generate proposal, draft post, create report)
  Data actions (create/update/link entities, tag, archive)
  Workflow actions (create task, trigger automation, spawn sub-agent)
  External actions (API calls, webhooks, file ops, payments)

LEARN
  Outcome tracking (completion, conversion, quality)
  Feedback collection (explicit thumbs, implicit edits, outcome results)
  Pattern recognition (what works, what fails, when, where)
  Model improvement (scoring calibration, prompt refinement)
  Knowledge base updates (entity updates, relationship discovery, preference learning)
```

### Layer 3: Agent Runtime

Inspired by OpenHands V1 + Manus + Claude Code:

```rust
trait Agent {
    fn spec(&self) -> AgentSpec;           // stateless definition
    async fn step(&self, state: &ConversationState) -> Action;  // one decision
}

struct ConversationState {
    event_log: Vec<Event>,                 // append-only, full history
    condensed_context: Vec<Event>,         // compacted for LLM window
    task_state: TaskChecklist,             // todo.md pattern (Manus)
    active_tools: Vec<ToolDefinition>,     // dynamically scoped
    active_skills: Vec<Skill>,             // injected per turn (OpenClaw)
    active_rules: Vec<Rule>,              // loaded per department
    memory_context: Vec<Memory>,           // relevant memories for this turn
    security_policy: SecurityPolicy,       // per-session risk thresholds
}

// The agent loop (one function, universal):
async fn run_agent(agent: &dyn Agent, state: &mut ConversationState) -> Outcome {
    loop {
        // 1. Condense if context too large (OpenHands)
        if state.event_log.len() > threshold {
            state.condensed_context = condense(&state.event_log);
        }

        // 2. Inject relevant skills for THIS turn (OpenClaw)
        state.active_skills = select_skills_by_relevance(&state);

        // 3. Inject relevant memories for THIS turn
        state.memory_context = recall_relevant(&state);

        // 4. Update task checklist (Manus todo.md)
        state.task_state.refresh_from_events(&state.event_log);

        // 5. Agent decides next action
        let action = agent.step(&state).await;

        // 6. Risk-rate the action (OpenHands SecurityAnalyzer)
        let risk = rate_risk(&action);
        if risk > state.security_policy.auto_approve_threshold {
            wait_for_human_approval(&action).await;
        }

        // 7. Execute
        let observation = execute(&action).await;

        // 8. Append to event log
        state.event_log.push(Event::from(action, observation));

        // 9. Check completion
        if is_done(&observation) { return Outcome::from(state); }
    }
}
```

### Layer 4: Integration Layer (adapters)

```
INBOUND ADAPTERS                    OUTBOUND ADAPTERS
  MCP Server (Claude Code bridge)     MCP Client (external services)
  Messaging Gateway                   Messaging (Telegram, WhatsApp, email)
    Telegram bot                      Platform APIs (LinkedIn, Twitter, DEV.to)
    WhatsApp                          Freelancer API (bid submission)
    Slack                             Gmail, Calendar
    Discord                           Webhooks
  Webhook receiver                    Notifications (push, SMS)
  Cron scheduler                      File system
  RSS/API poller                      A2UI Canvas (interactive agent UI)
  Browser (CDP)
  File watcher
  Email (IMAP)

STORAGE ADAPTERS                    LLM ADAPTERS
  SQLite (local-first)                Claude API / CLI
  Vector store (LanceDB)              Ollama (local)
  File system                         OpenAI
                                      Gemini (Vertex AI)
                                      Router (cheap for text, premium for reasoning)
```

### Layer 5: Surfaces (where the human interfaces)

```
1. Claude Code (via MCP)
   - Memory tools (store, recall, search)
   - Department context loading
   - Skill execution
   - Automation triggers
   - Knowledge search

2. Web UI (SvelteKit)
   - Dashboard (goals, events, metrics, approvals)
   - Department chat (SSE streaming, tool calls, approval cards)
   - Content Studio (capture → draft → repurpose → publish)
   - Flow builder (visual DAG)
   - Knowledge base
   - Entity browser (people, companies, opportunities)
   - Extension marketplace
   - Reasoning panel (show agent decisions)

3. Messaging (Telegram/WhatsApp)
   - Inbound: messages route to agents
   - Outbound: notifications, briefs, approvals
   - Inline approval: "Reply YES to send this proposal"

4. CLI / REPL / TUI
   - One-shot commands
   - Interactive shell
   - Dashboard (ratatui)

5. A2UI Canvas
   - Agent-generated interactive HTML
   - User clicks → tool calls → updated UI
```

## 5. The "Never Write This Prompt Again" Stack

```
Layer 5: AUTOMATION (zero prompts)
  Cron: daily scan freelancer, generate brief
  Events: on push → arch review, on draft → adapt all platforms
  Triggers: pattern-match events → spawn agent
  Pipelines: harvest→score→propose→draft→schedule

Layer 4: AGENTS (one @mention)
  @researcher → deep investigation with scoped tools
  @content-writer → draft + adapt + schedule
  @arch-reviewer → boundary compliance check
  @harvester → scan + score + propose

Layer 3: SKILLS (one /slash)
  /review → full architecture review workflow
  /draft <topic> → content generation for all platforms
  /harvest → opportunity scan + score + propose
  /research <topic> → multi-source deep dive

Layer 2: RULES (always active, zero effort)
  Hexagonal boundaries enforced
  Content voice maintained
  Crate size limits checked
  Approval gates enforced

Layer 1: MEMORY (always there)
  "Last time you worked on auth, you decided X because Y"
  "Your client Sarah prefers Tuesday calls"
  "The Freelancer.com bid for DataFlow was $7,500"
```

## 6. Data Architecture

From Throne Protocol's 5 data domains:

```
IDENTITY STORE
  Who Mehdi is: bio, skills, positioning, voice
  Preferences, boundaries, goals
  → Powers content voice, proposal personalization, agent personas

RELATIONSHIP GRAPH
  People: clients, prospects, network contacts
  Companies: targets, partners, competitors
  Interactions: timestamped, scored
  → Powers CRM, outreach, referrals, enrichment

MISSION LEDGER
  Active missions and task state (todo.md pattern)
  Execution log (agent events, tool calls, outcomes)
  Learnings (what worked, what didn't, why)
  → Powers planning, daily briefs, outcome-based scoring improvement

INTELLIGENCE LAKE
  Market signals, opportunity feeds
  Competitive intelligence, trend detection
  Research results (last30days pattern)
  → Powers harvest scoring, content inspiration, strategic decisions

ASSET REGISTRY
  Content library (proposals, templates, posts, case studies)
  Code library (repos, snippets, analysis)
  Financial assets (invoices, payments, runway)
  Digital properties (domains, accounts, subscriptions)
  → Powers reuse, templating, financial tracking
```

Storage: SQLite WAL (local-first) + LanceDB vectors + file system. Cloud optional.

## 7. What to Carry from RUSVEL

| Keep | Why |
|------|-----|
| Hexagonal ports/adapters pattern | Proven, clean separation |
| Event sourcing + EventPort | Append-only log is universal |
| DepartmentApp + manifest system | Departments as configuration, not code |
| AgentRuntime streaming | Production-grade tool loop |
| ScopedToolRegistry | Per-department tool isolation |
| Job queue + approval gates | Background execution + human-in-the-loop |
| SQLite WAL + 5 sub-stores | Solid local-first persistence |
| rusvel-memory FTS5 | Session-scoped search works |
| Content engine adapters (just fixed) | LinkedIn, Twitter, DEV.to ready |
| MCP server (--mcp) | Claude Code bridge exists |
| SvelteKit frontend | Working UI foundation |

| Redesign | Why |
|----------|-----|
| 55 crates → fewer, denser modules | Too much indirection for one person |
| 7 stub engines → department configs | Departments are data, not code |
| Static rule injection → dynamic per-turn | OpenClaw's semantic relevance model |
| Context window management → Condenser | Long runs need compaction |
| Approval per job type → per tool call risk | OpenHands granularity |
| MCP client stdio-only → HTTP/SSE transport | Unlocks Pipedream, Google, Stripe |
| No messaging inbound → Gateway pattern | OpenClaw's ambient intelligence |
| No task externalization → todo.md pattern | Manus's focus across 50+ iterations |

| Drop | Why |
|------|-----|
| 7 stub engines (finance→infra code) | Replace with department configs |
| Separate dept-* crates (14 crates) | Departments become manifest data |
| Code duplication across engine tests | Shared test harness instead |
| RUSVEL-specific seed data | Generic starter templates |

## 8. Technology Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Primary language | **Rust** | Performance, single binary, RUSVEL's art carries over |
| Secondary | **TypeScript** (frontend only) | SvelteKit, proven |
| Database | **SQLite WAL** + **LanceDB** | Local-first, zero ops, proven in RUSVEL |
| LLM primary | **Claude API** | Best for coding + reasoning, KV-cache optimization |
| LLM local | **Ollama** | Private, offline, free |
| LLM routing | **Multi-provider** | Cheap for classification, premium for reasoning |
| Frontend | **SvelteKit 5** | Proven in RUSVEL, fast, small |
| Integration | **MCP** | Universal protocol, Claude Code native |
| Messaging | **Telegram first** | Bot API is excellent, ChannelPort exists |
| Automation | **Event triggers + cron** | Job queue + event bus proven |
| Packaging | **Single binary** (rust-embed frontend) | Zero ops, proven in RUSVEL |

## 9. Build Order

### Sprint 1: Core + Agent Loop (Week 1-2)
- Core domain types (Signal, Mission, Action, Entity, Memory, AgentSpec, Skill, Rule)
- Port traits (Inbound, Process, Outbound, Memory, Event, Storage, Agent, Tool, Channel)
- Agent runtime with: step(state)→Action, Condenser, todo.md, dynamic skill injection, per-tool risk rating
- SQLite adapter (events, objects, sessions, jobs, metrics)
- Basic LLM adapter (Claude API with KV-cache aware prompting)

### Sprint 2: MCP Bridge + Skills (Week 3-4)
- MCP server with 15 tools (memory, knowledge, dept context, goals, events, skills, automation)
- Skill system: load from .claude/skills/, frontmatter parsing, {{input}} substitution
- Rule system: load from .claude/rules/, path-based auto-loading
- Agent definitions: load from .claude/agents/, tool scoping
- `sync-config` command: generate .claude/ files from registry
- `install` command: download community skills from GitHub

### Sprint 3: INBOUND + OUTBOUND (Week 5-6)
- Messaging gateway (Telegram inbound → agent → Telegram outbound)
- Cron scheduler (daily freelancer scan, daily brief)
- RSS/API poller (Freelancer.com API, PPH/Guru RSS)
- Content publishing (LinkedIn REST, Twitter OAuth, DEV.to)
- Notification on job completion (Telegram channel)

### Sprint 4: Web UI + Intelligence (Week 7-8)
- Dashboard (goals, events, approvals, metrics)
- Content Studio (capture → draft → repurpose → schedule → publish)
- Entity browser (people, companies, opportunities — relationship graph)
- Reasoning panel (show agent decision chain during tool loops)
- Knowledge base (ingest, search, relate)

### Sprint 5: Automation + Learning (Week 9-10)
- Event triggers → agent runs (on push → arch review)
- Cross-department pipelines (harvest→score→propose→draft→schedule)
- Outcome tracking (won/lost → scoring improvement)
- Pattern recognition (what works when, where)
- Community extension marketplace

## 10. Success Metric

**Day 1:** You use the MCP server in Claude Code and it remembers what you decided yesterday.

**Week 2:** Skills and agents deploy to any project with one command. You never explain your conventions again.

**Week 4:** Telegram sends you scored freelancer gigs every morning. You approve proposals by replying YES.

**Week 6:** Content publishes to LinkedIn/DEV.to weekly from your captures. No manual steps.

**Week 8:** The dashboard shows your pipeline, your content calendar, your entity relationships, and your agent decisions — all in one view.

**Week 10:** The system is smarter than it was on day 1. It scores better, writes better proposals, knows your preferences, and catches things you'd miss.

---

*This brief combines: Throne Protocol (taxonomy), RUSVEL (architecture), Claude Code (runtime),
Manus (context engineering), OpenHands (agent abstraction + security), OpenClaw (ambient intelligence),
superpowers/hermes/claude-mem (community patterns), and market research on platforms and competitors.*

*Owner: Mehdi Baneshi*
*Date: 2026-04-01*
