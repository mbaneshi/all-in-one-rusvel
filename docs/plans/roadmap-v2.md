# RUSVEL — Roadmap v2 (Post-Review)

> 13 engines (6 wired + 7 skeletons). 55 crates. DepartmentApp pattern. Central job queue. Session-centric everything.

---

## Phase 0 — Foundation ← YOU ARE HERE
> Prove the architecture with one vertical slice.

**Deliverable:** `rusvel forge mission today` via CLI, Web, MCP, REPL, TUI.

- rusvel-core (22 port traits in `ports.rs`: 15 primary + 5 Store + `ChannelPort` + `BrowserPort` + `RusvelBasePort`; domain types + DepartmentApp/Manifest + approval model — see `current-state.md`)
- Adapters: config, db (5 stores), schema, event, llm (4 providers + ModelTier + CostTracker), memory (FTS5), tool (ScopedToolRegistry), builtin-tools, engine-tools, mcp-client, jobs, embed, vector, deploy, auth, terminal
- forge-engine + code-engine + content-engine + harvest-engine + flow-engine + gtm-engine (all wired)
- 14 dept-* crates implementing DepartmentApp trait (ADR-014, EngineKind removed)
- AgentRuntime with run_streaming() + 22+ registered tools (10 built-in incl. tool_search + 12 engine)
- Surfaces: CLI (3-tier: one-shot + REPL + TUI), API (141 route chains across 36 modules), Web (SvelteKit, 12+ pages), MCP (6 tools)
- 7 skeleton engines (finance, product, growth, distro, legal, support, infra)

**Proves:** Ports + adapters + session hierarchy + job queue + events + streaming LLM.

---

## Phase 1 — Agent Runtime + Code Intelligence (largely complete)
> Deep agent capabilities + first analytical engine.

**Deliverable:** Forge orchestrates multi-agent workflows that analyze code.

- ~~rusvel-agent expanded: workflow patterns~~ — AgentRuntime with run_streaming() + multi-turn tool loop DONE
- ~~rusvel-llm expanded: Claude + OpenAI adapters~~ — 4 providers + ModelTier routing + CostTracker DONE
- ~~rusvel-memory expanded: vector embeddings for semantic search~~ — LanceDB + embeddings DONE
- ~~forge-engine: multi-agent orchestration, persona system, safety controls~~ — 10 personas DONE
- ~~code-engine v0: Rust parsing, symbol graph, BM25 search, complexity metrics~~ — DONE
- Git worktree isolation for agent safety
- Observability: tracing + run IDs + replay view

**Proves:** `rusvel forge run "analyze this Rust repo"` spawns code agents, streams results.

---

## Phase 2 — Revenue Engines
> Find work and build audience.

**Deliverable:** Discover gigs, create content, publish — with human approval gates.

- harvest-engine: CDP scraping (Upwork adapter), AI scoring, proposal generation
- content-engine: Markdown authoring, AI adaptation, DEV.to + Twitter adapters, scheduling
- rusvel-tui: Expand TUI dashboard with per-department tabs (pipeline + content calendar)
  - Note: Basic TUI dashboard already wired in Phase 0 via `--tui` flag
- Human approval workflow: agent proposes → human approves → system executes
- Inbox/Capture: funnel links, emails, docs into sessions

**Proves:** Find gig → score → draft proposal → approve → send. Write post → adapt → approve → publish.

---

## Phase 3 — GoToMarket Engine
> Complete the business stack.

**Deliverable:** Full CRM + outreach + invoicing.

- gtm-engine: contacts, deals, pipeline stages, invoicing, SOPs (substantially implemented — 1949 LOC, wired in Phase 0)
- Outreach sequences with follow-up scheduling
- Email + LinkedIn adapters
- AI spend tracking and budget management
- Knowledge base

**Proves:** End-to-end: discover → qualify → propose → win → invoice → get paid.

---

## Phase 4 — Cross-Engine Intelligence
> Make it compound.

- Cross-engine workflows: Forge orchestrates Harvest + Content + GoToMarket in sequences
- Learning from outcomes (won/lost proposals → improve scoring)
- Context packs: standardized agent context per session
- Advanced agent workflows (Graph agents, best-of-N selection)
- Documentation as first-class artifact (agents maintain mission.md, architecture.md per session)
- Daily autonomous briefings from ALL engine states

---

## Phase 5 — Ecosystem
> Open it up.

- Plugin system (community engines and adapters)
- A2A protocol (agent-to-agent communication)
- Browser extension for passive harvesting
- More platform adapters (LinkedIn, Medium, YouTube, Substack, Freelancer)
- More LLM adapters (Gemini, DeepSeek, Groq)
- Community persona marketplace

---

## Crate Count by Phase

| Phase | New Crates | Total |
|-------|-----------|-------|
| 0 | 55 (23 `rusvel-*` foundation incl. pipeline + 13 engines + 14 dept-* + 5 surfaces) | 55 |
| 1 | 0 (expand existing) | 55 |
| 2 | 0 (expand existing) | 55 |
| 3 | 0 (expand existing) | 55 |
| 4 | 0 (expand existing) | 55 |
| 5 | +N (plugins, new adapters) | 55+N |

**Note:** Crate count grew from original plan of 20 to **55**. Growth: 20 → 27 (early adapters) → 34 (schema, embed, vector, deploy, builtin-tools, mcp-client, flow-engine) → 48 (13 dept-* crates for DepartmentApp pattern + rusvel-engine-tools + rusvel-terminal) → 54 (webhook, cron, dept-messaging, cdp, channel) → **55** (`rusvel-pipeline`: forge pipeline runner wiring harvest + content, keeps `forge-engine` free of those deps). The architecture holds — growth stayed in foundation adapters, department encapsulation, and composition crates.
