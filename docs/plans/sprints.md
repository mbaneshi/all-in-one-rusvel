# RUSVEL — Consolidated Sprint Plan

> Single source of truth. All other plan docs are reference material.
> Date: 2026-03-25 | Status: Active | Updated: 2026-03-26

**Parallel initiative:** [program-harvest-agency.md](program-harvest-agency.md) — native harvest / autonomous agency (waves, priorities, AG-1…7 slices, Cursor todo map). Orthogonal to Sprint 2–5 below.

---

## Where We Are (verified 2026-03-27)

**Build:** `cargo build` clean (0 errors). `cargo test` 0 failures in full local run.

**Metrics glossary:** [`docs/status/current-state.md`](../status/current-state.md) — **55** workspace members, **~73,058** LOC in `crates/*.rs`, **301** Rust files under `crates/`, **~645** tests (workspace sum), **~102** test targets from `cargo test --no-run`, **153** `.route(` chains in `rusvel-api/src/lib.rs`, **39** API handler modules (excluding `lib.rs`). Older docs used different counts; use **§1** in `current-state` only.

**Git:** See working tree; not tracked here.

**What shipped since original plan:**

| Commit | What | Sprint Task |
|--------|------|-------------|
| `7fadf75` | AgentRuntime wired to chat, 21 tools, ScopedToolRegistry, ToolCallCard + ApprovalCard | Pre-sprint (Phases 1-4) |
| `452e730` | LLM text streaming, DEV.to real adapter | Pre-sprint (Phases 5-6) |
| `7108933` | ADR-014 DepartmentApp architecture | Sprint 1 #1-5 |
| `0bb4dfe` | dept-content complete (tools, events, jobs) | Sprint 1 #1 |
| `94a7eb1` | dept-forge complete (manifest, mission, safety) | Sprint 1 #2 |
| `119af14` | Remove EngineKind, use department ID strings | Sprint 1 #6 |
| `4c387f1` | Deferred tool loading (`tool_search` meta-tool) | Sprint 1 #7 |
| `2ba3872` | ModelTier routing + MetricStore cost tracking | Sprint 1 #8 |
| `17e82ac` | Approval queue UI, sidebar badge, approve/reject API | Sprint 1 #9 |
| `856b75d` | TerminalPort + rusvel-terminal Phase 1 | Sprint 1 #10 |
| `087e414` | Frontend manifest-aligned departments (nav + tabs) | Sprint 2 #15 |
| `b1df03f` | Flow REST client + /flows page | Sprint 2 (partial) |

**Summary: Sprint 0 done. Sprint 1 done. Sprint 2 partially done.**

---

## What's Left

### Sprint 2 Remaining (~8 days)

> Theme: Agent intelligence + Terminal web bridge

| # | Task | Effort | Status | Depends On |
|---|------|--------|--------|------------|
| 11 | **Context Compaction** — auto-summarize at 30 messages, keep recent 10 | 2d | Not started | — |
| 12 | **Memory Tools** — `memory_read/write/search/delete` as agent tools, auto-inject top 5 | 2d | Not started | — |
| 13 | **Batch API** — `submit_batch()/poll_batch()` on LlmPort, 50% discount on async jobs | 3d | Not started | #8 ✅ (done) |
| 14 | **Hybrid RAG** — fuse FTS5 + LanceDB via RRF, rerank top-N with Haiku | 3d | Not started | — |
| ~~15~~ | ~~Frontend manifest alignment~~ | — | ✅ Done | — |
| 16 | **Terminal Phase 2: Web Bridge** — WebSocket route, xterm.js, `/terminal` page | 3d | Not started | #10 ✅ (done) |
| 17 | **Terminal Phase 3: Dept Integration** — "Terminal" tab in dept panel, lazy panes | 2d | Not started | #10 ✅, #15 ✅ |

**Parallelizable:** 11+12 parallel, 13+14 parallel, 16+17 sequential. **~8 working days.**

---

### Sprint 3: Orchestration + Flow Engine Evolution (~14 days)

> Theme: Agents controlling agents, flow engine becomes production-grade.
> ADRs: ADR-015 (Node Extension), ADR-018 (Expression Language)

| # | Task | Effort | Status | Depends On |
|---|------|--------|--------|------------|
| 18 | **`delegate_agent` tool** — spawn sub-agent with persona, tools, budget cap, max depth 3 | 2d | Not started | — |
| 19 | **`invoke_flow` tool** — agents trigger FlowEngine DAGs, get results | 1d | Not started | — |
| 20 | **PreToolUse/PostToolUse hooks** — `HookPoint` enum, `ToolHook` trait, wire into agent loop | 3d | Not started | ADR-014 ✅ |
| 21 | **Tool permissions** — `ToolPermissionMode` per-dept (auto/supervised/locked) | 2d | Not started | ADR-014 ✅ |
| 22 | **Event triggers** — `EventTrigger` subscribes to patterns, auto-starts agents/flows | 2d | Not started | #18, #19 |
| 23 | **Self-correction loops** — `VerificationStep` trait, per-tool critique, auto-fix rules | 4d | Not started | #8 ✅ (done) |
| 24 | **Terminal Phase 4: Agent Visibility** — `DelegationTerminal.svelte`, `terminal_open/watch` tools | 3d | Not started | #16, #18 |
| 34 | **Flow node Tier 1** — `LoopNode`, `DelayNode`, `HttpRequestNode`, `ToolCallNode` (ADR-015) | 3d | Not started | — |
| 35 | **MiniJinja expressions** — template resolution in node parameters (ADR-018) | 2d | Not started | — |
| 36 | **Claude Code hooks** — 6 hooks in `.claude/hooks/` + settings.json wiring (ADR-019) | 1d | Not started | — |

**Sprint 3 total:** ~14 days (18-21+34-36 parallel, 22-23 after deps, 24 after 16+18)

---

### Sprint 4: Channels + Cost Tracking (~14 days)

> Theme: Multi-channel communication, spend visibility, workflow depth.
> ADRs: ADR-016 (Multi-Channel), ADR-017 (Cost Tracking)

| # | Task | Effort | Status | Depends On |
|---|------|--------|--------|------------|
| 25 | **Durable Execution** — checkpoint/resume for FlowEngine, retry per node | 5d | Not started | — |
| 26 | **Playbooks** — predefined multi-step JSON pipelines, UI at `/playbooks`, run/history | 5d | Not started | #18, #19, #22 |
| 27 | **AG-UI Protocol** — map SSE events to AG-UI schema, add RUN_STARTED/STATE_DELTA | 4d | Not started | — |
| 28 | **Terminal Phase 5: Flow/Playbook Visibility** — node-per-pane in DAG execution | 3d | Not started | #16, #25, #26 |
| 37 | **ChannelPort expansion** — expand trait in `rusvel-core`, add domain types (ADR-016) | 2d | Not started | — |
| 38 | **Discord adapter** — rich embeds, threads, reactions, webhook inbound (ADR-016) | 3d | Not started | #37 |
| 39 | **ChannelRouter** — pattern-based routing in `rusvel-channel`, dept→channel mapping | 2d | Not started | #37 |
| 40 | **Cost tracking** — `CostEvent` in core, record in `CostTrackingLlm` + executor (ADR-017) | 2d | Not started | — |
| 41 | **Cost API + dashboard** — `GET /api/analytics/costs`, frontend spend-by-dept chart | 2d | Not started | #40 |
| 42 | **Flow node Tier 2** — `SwitchNode`, `MergeNode`, `SubFlowNode`, `NotifyNode` | 3d | Not started | #34, #39 |

**Sprint 4 total:** ~14 days (25+27+37+40 parallel, 38+39 after #37, 26+41+42 after deps, 28 last)

---

### Sprint 5: Product Features + Channel Depth (~14 days)

> Theme: Framework → Product. Multi-channel depth, self-improvement.

| # | Task | Effort | Status | Depends On |
|---|------|--------|--------|------------|
| 29 | **Executive Brief** — enhanced `forge mission today` + delegate per dept + cost summary | 3d | Not started | #18, #40 |
| 30 | **Starter Kits** — pre-built dept bundles via `!build` + Capability Engine | 3d | Not started | ADR-014 ✅ |
| 31 | **Self-Improving KB** — auto-index engine outputs, cross-dept insights | 3d | Not started | #14 |
| 32 | **Streamable HTTP MCP** — Axum handlers, session mgmt, OAuth, keep stdio fallback | 5d | Not started | — |
| 33 | **Terminal Phase 6-7: CDP + TUI** — browser panes, ratatui panel | 4d | Not started | #16, CDP bridge |
| 43 | **Slack adapter** — threads, interactive messages, app mentions (ADR-016) | 3d | Not started | #37, #39 |
| 44 | **Email adapter** — SMTP outbound + retry queue, HTML templates (ADR-016) | 2d | Not started | #37 |
| 45 | **`/learn` command + provenance** — pattern extraction, `.provenance.json`, learned skills (ADR-019) | 2d | Not started | #36 |
| 46 | **Session persistence** — session-end hook saves state, session-start loads it (ADR-019) | 1d | Not started | #36 |

**Sprint 5 total:** ~14 days (29-31+43-46 parallel, 32 independent, 33 after CDP)

---

### Sprint 6: Scale + Polish (~12 days)

> Theme: Production hardening, design system, advanced flows.

| # | Task | Effort | Status | Depends On |
|---|------|--------|--------|------------|
| 47 | **Webhook channel** — generic HTTP POST outbound, configurable templates, retry (ADR-016) | 2d | Not started | #37 |
| 48 | **Inbound channel routing** — webhook receivers → EventPort → flow triggers (ADR-016) | 3d | Not started | #39, #47 |
| 49 | **Message queue + batching** — debounce rapid notifications, batch flush (ADR-016) | 2d | Not started | #39 |
| 50 | **Design tokens + component variants** — CSS custom properties, `tailwind-variants` in Svelte 5 | 3d | Not started | — |
| 51 | **Credential encryption** — AES-256 at rest in SQLite, decrypt per-execution | 3d | Not started | — |
| 52 | **Flow versioning** — version counter on `FlowDef`, execution links to version, diff view | 2d | Not started | — |
| 53 | **Flow templates** — predefined starter flows, `POST /api/flows/from-template` | 2d | Not started | #52 |
| 54 | **Continuous learning pipeline** — observe hook + evaluate-session hook + `/evolve` command | 3d | Not started | #45, #46 |

**Sprint 6 total:** ~12 days (47+50+51+52 parallel, 48+49 after #47, 53 after #52, 54 after #45)

---

## Backlog (unscheduled)

| Task | Effort | Depends On | Source Doc |
|------|--------|------------|-----------|
| CDP Browser Bridge (phases 1-6) | 10d | ADR-014 ✅ | cdp-browser-bridge.md |
| AI SDK 6 Frontend | 8d | AG-UI (#27) | next-level-proposals.md |
| Leveling/Progression system | 4d | ADR-014 ✅ | next-level-inspiration.md |
| Roundtable/Strategy Review UI | 5d | #18 (delegate_agent) | next-level-inspiration.md |
| Agent Workforce dogfooding | Ongoing | #18, ADR-014 ✅ | agent-workforce.md |
| A2UI Component Registry | 4d | AG-UI (#27) | next-level-proposals.md |
| Inter-department messaging | 3d | ADR-014 ✅ | agent-orchestration.md |
| In-Process MCP Bridge | 3d | — | agent-sdk-features.md |
| Progress Docs (session continuity) | 2d | #12 (memory tools) | agent-sdk-features.md |
| Agentic Search (prompt adjustment) | 1d | #7 ✅ (deferred tools) | agent-sdk-features.md |
| WhatsApp/Signal adapters | 5d | #37, #39 | reference-repos-minibook.md |
| Plugin system trait | 5d | — | reference-repos-minibook.md |
| Media pipeline (image/audio/video) | 4d | #37 | reference-repos-minibook.md |
| Desktop distribution (Tauri) | 10d | — | reference-repos-minibook.md |
| Collaborative editing (realtime) | 8d | — | reference-repos-minibook.md |

---

## Completed Work Log

### Pre-Sprint (Phases 1-6, session 2026-03-25)
- ✅ AgentRuntime::run_streaming() with AgentEvent channel
- ✅ 9 built-in tools registered (file ops, shell, git)
- ✅ 12 engine tools (harvest 5, content 5, code 2)
- ✅ ScopedToolRegistry for per-department tool filtering
- ✅ Chat handlers rewired from ClaudeCliStreamer → AgentRuntime
- ✅ LLM text streaming (LlmStreamEvent + stream() on LlmPort)
- ✅ Frontend ToolCallCard + ApprovalCard components
- ✅ DEV.to real HTTP adapter (LinkedIn + Twitter already existed)

### Sprint 0: Unbreak ✅
- ✅ Build compiles clean
- ✅ All tests pass
- ✅ `searchable` field added to all ToolDefinition sites

### Sprint 1: Foundation ✅
- ✅ **#1** dept-content DepartmentApp (routes, tools, personas, skills, handlers, events, jobs)
- ✅ **#2** dept-forge DepartmentApp (mission tools, 10 personas, safety)
- ✅ **#3** Boot sequence — main.rs iterates installed_departments()
- ✅ **#4** Code, Harvest, Flow → dept-* crates
- ✅ **#5** 8 stub depts → dept-* crates (all 12 departments now DepartmentApp)
- ✅ **#6** EngineKind removed, string IDs everywhere
- ✅ **#7** Deferred Tool Loading — `tool_search` meta-tool, 85% token savings
- ✅ **#8** LLM Cost Intelligence — ModelTier routing + CostTracker
- ✅ **#9** Approval UI — ApprovalQueue, inline chat approve/reject, sidebar badge
- ✅ **#10** Terminal Phase 1 — TerminalPort trait, rusvel-terminal crate

### Sprint 2: Partial ✅
- ✅ **#15** Frontend manifest alignment — dept manifest drives UI tabs/routes
- ✅ Flows page — REST client + /flows route

---

## Terminal Multiplexer Threading

```
Sprint 0   Sprint 1          Sprint 2            Sprint 3              Sprint 4             Sprint 5
--------   ----------        ----------          ----------            ----------           ----------
✅ done    ✅ TerminalPort   ⬜ Web Bridge       ⬜ Agent Visibility   ⬜ Flow Panes        ⬜ CDP + TUI
           ✅ PTY core       ⬜ xterm.js         ⬜ DelegationTerm    ⬜ Playbook dash     ⬜ Browser panes
           ✅ TerminalMgr    ⬜ /terminal route  ⬜ terminal_open     ⬜ Checkpoint UI     ⬜ ratatui panel
                             ⬜ Dept tab         ⬜ /runs/:id/panes   ⬜ /flows/:id/panes
```

---

## Critical Path (updated 2026-03-30)

```
✅ DONE                 Sprint 2          Sprint 3              Sprint 4              Sprint 5          Sprint 6
---------               ---------         ---------             ---------             ---------         ---------
✅ ADR-014 ────────────────────────→ Hooks + Perms
                                         |
✅ Cost routing ──→ Batch API            +──→ Self-correction
                                         |
✅ Terminal PTY ──→ Terminal Web ───→ Terminal Agent ───→ Terminal Flow ──→ Terminal CDP
                                         |
                    Memory + RAG         |
                    Compaction       delegate_agent ──→ Playbooks ──→ Executive Brief
                                         |                  |
                                    Event triggers          |
                                         |            Durable Exec ──→ Flow Version ──→ Templates
                                    invoke_flow
                                                      AG-UI ──────→ (AI SDK 6)

NEW TRACKS (ADRs 015-019):
                                    Flow nodes T1 ──→ Flow nodes T2 ──→ SubFlowNode
                                         |                  |
                                    Expressions         MergeNode
                                         |
                                    ChannelPort ──→ Discord ──→ Slack ──→ Email
                                         |              |
                                    ChanRouter     Inbound ──→ MsgQueue
                                         |
                                    Cost Track ──→ Cost API ──→ Exec Brief w/ cost
                                         |
                                    Claude hooks ──→ /learn ──→ Session persist ──→ Learning pipeline
                                                                                        |
                                                                              Design tokens + variants
                                                                              Credential encryption
```

**Three unlock points:**
1. **delegate_agent (Sprint 3 #18)** — unlocks playbooks, executive brief, roundtable, workforce dogfooding
2. **Terminal Web Bridge (Sprint 2 #16)** — unlocks all agent visibility features
3. **ChannelPort expansion (Sprint 4 #37)** — unlocks all multi-channel features (Discord, Slack, Email, Webhook, inbound)

---

## Metrics

| Sprint | Tasks | Effort | Status | Cumulative |
|--------|-------|--------|--------|------------|
| 0 | 5 | ~1d | ✅ Done | 1d |
| 1 | 10 | ~10d | ✅ Done | 11d |
| 2 | 6 remaining | ~8d | **In Progress** | 19d |
| 3 | 10 | ~14d | Planned | 33d |
| 4 | 10 | ~14d | Planned | 47d |
| 5 | 9 | ~14d | Planned | 61d |
| 6 | 8 | ~12d | Planned | 73d |
| **Total** | **43 tasks remaining** | **~62 working days** | | **~12.5 weeks** |

Backlog: 15 items, ~70 additional days if all scheduled.

**Progress: 17/60 tasks done (28%). ~21 working days completed. ~62 remaining.**

**New tasks from reference repo analysis (ADRs 015-019):**
- Sprint 3: #34 (flow nodes), #35 (expressions), #36 (Claude hooks)
- Sprint 4: #37-42 (channels, cost, more flow nodes)
- Sprint 5: #43-46 (Slack, email, learning, session persistence)
- Sprint 6: #47-54 (webhook, inbound, batching, design, credentials, versioning, templates, learning pipeline)

---

## Codebase Snapshot (2026-03-26)

| Metric | Value |
|--------|-------|
| Crates | 49 (16 foundation + 13 engines + 15 dept-* + 5 surfaces) |
| Rust source files | 185 |
| Rust lines | ~43,276 |
| API handler functions | ~115 |
| Test suites passing | 98 (0 failures) |
| Department crates | 15 (all 12 + dept-code, dept-flow, dept-harvest) |
| Registered tools | 21+ (9 built-in + 12 engine + tool_search) |
| Frontend routes | 16+ |

---

## Reference Documents

| Doc | What It Contains | Relevant Sprints |
|-----|-----------------|-----------------|
| `plans/agent-delegation-handbook.md` | Parallel agent briefs, GLOBAL RULES (G), deps, waves, one-line commands, report template | Sprints 2–5 (execution) |
| `design/department-as-app.md` | DepartmentApp trait, manifest types, migration steps | ✅ Sprint 1 (done) |
| `plans/native-terminal-multiplexer.md` | TerminalPort, PTY, xterm.js, pane sources | Sprints 2-5 |
| `plans/agent-sdk-features.md` | 9 features from Anthropic SDK + cross-references | Sprints 2-3 |
| `plans/agent-orchestration.md` | delegate_agent, invoke_flow, triggers, templates | Sprint 3-4 |
| `plans/next-level-proposals.md` | 12 proposals (P1-P12), dependency graph | All |
| `plans/next-level-inspiration-2026-03-25.md` | GenAICircle-inspired product features | Sprint 5 + Backlog |
| `plans/cdp-browser-bridge.md` | BrowserPort, CDP, platform extractors | Sprint 5 + Backlog |
| `design/agent-workforce.md` | 14 builder agents for self-building | Backlog |
| `plans/flow-engine.md` | DAG executor design, node types, triggers | Sprint 4 |
| `proposals/reference-repos-minibook.md` | Deep analysis of 6 reference repos, pattern catalog | Sprints 3-6 |
| `plans/pattern-extraction-design.md` | Code samples + implementation design for ADR-015/016/017 | Sprints 3-6 |
| `plans/sprint-6-pattern-extraction.md` | Standalone sprint plan for pattern extraction (18 tasks) | Sprint 6 alt |
| `design/decisions.md` | 19 ADRs (ADR-001 through ADR-019) | Reference |
| `design/architecture-v2.md` | Hexagonal architecture, port traits | Reference |

---

## Superseded Documents

- `plans/sprint-current.md` → superseded by this document
- `plans/phase-0-foundation.md` → superseded by `phase-0-foundation-v2.md`
- `plans/roadmap.md` → superseded by `roadmap-v2.md`
- `plans/refactor-2026-03-23.md` → work completed
- `plans/ui-overhaul-2026-03-23.md` → work completed
- `plans/ui-enhancements-2026-03-23.md` → work completed
- `plans/refactor-and-enhance-2026-03-23.md` → work completed
