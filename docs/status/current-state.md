# RUSVEL — Current State

> **Last verified:** 2026-03-30 (metrics + `cargo test --workspace` + `cargo build`; see [verification-log-2026-03-30.md](verification-log-2026-03-30.md))

> **⚠️ Update 2026-08-25 — several claims below (and in [audit-2026-05-15.md](../audit/audit-2026-05-15.md)) are now stale:**
>
> - **Department tool wiring is complete.** All 13 engine-backed departments register agent tools via the ADR-014 `ToolRegistrar` path (Sprint 1/2 commits wired 9; `dept-harvest` wired 2026-08-25 with 6 `harvest.*` tools). `dept-messaging` intentionally exposes no tools (no engine).
> - **Job queue is persistent.** `rusvel_jobs::PersistentJobQueue` wraps the SQLite `jobs` table; `rusvel-app` runs interrupted-job recovery + 7-day retention pruning at startup. The "volatile `Vec<Job>`" claim no longer applies to the composition root.
> - **Ollama and OpenAI stream natively** (SSE) — the batch-fallback gap is closed.
> - **Claude adapter migrated to Claude 5 models** (`claude-opus-5` / `claude-sonnet-5` / `claude-haiku-4-5`), with adaptive thinking + effort support, prompt caching (`cache_control: ephemeral` on system), and current pricing in both `rusvel-llm::cost_tracking` and `rusvel-core::domain`.
> - **`rusvel-evals` exists** — fixture-based eval crate (audit Option E) merged.
>
> **Update 2026-09-02:** department tools were registered (ADR-014 `ToolRegistrar`, 55 tools
> across 13 departments) but never transferred to the `ToolPort` implementation — the
> `DepartmentsBootArtifacts.tools` field was destructured with `..` and dropped in
> `rusvel-app/src/main.rs`, so nothing could call them. `rusvel_tool::register_department_tools()`
> now bridges them onto the shared `ToolRegistry`, and `rusvel-mcp`'s `tools/list`/`tools/call`
> were extended to expose the same registry. Live `rusvel --mcp` now serves **94** tools (9
> hand-written MCP-native + 85 on the `ToolPort` registry, including all 55 department tools).
> The "MCP stdio tools: 6" and "registered agent tools: 22+" figures below predate this and are
> now stale; see the table row updates.

---

## How to re-verify

Run from the repository root:

```bash
cargo build
cargo test
cargo metadata --format-version 1 --no-deps | python3 -c "import json,sys; print(len(json.load(sys.stdin)['workspace_members']))"
find crates -name '*.rs' | wc -l
wc -l $(find crates -name '*.rs') | tail -1
rg '\.route\(' crates/rusvel-api/src/lib.rs | wc -l
```

Use the same host environment as normal development. Run `cargo test` from the **repository root** so `rusvel-builtin-tools` file-op tests (temp dirs under `current_dir`) pass. Some integration tests (e.g. terminal PTY) may fail in sandboxed or headless CI unless configured.

---

## Metric definitions

| Term | Meaning |
|------|---------|
| **Workspace members** | Packages listed in `[workspace].members` in root `Cargo.toml` — `cargo metadata --no-deps` count. |
| **Rust LOC** | Total lines of `*.rs` under `crates/` only (excludes `frontend/`). |
| **Rust source files** | Count of `*.rs` files under `crates/`. |
| **Tests (count)** | Sum of `running N tests` lines from `cargo test --workspace` output (~645). |
| **Test targets** | Approximate count of compiled test executables from `cargo test --workspace --no-run` (count `^  Executable` lines in output; **~102** as of [verification-log-2026-03-30.md](verification-log-2026-03-30.md)). Older docs (~30) referred to `[[test]]` / crate-level counts only. |
| **HTTP route chains** | Lines with `.route(` in `crates/rusvel-api/src/lib.rs` main API router (**153**). One line can register multiple methods (`get().post()`). |
| **API modules** | `*.rs` files in `crates/rusvel-api/src/` excluding `lib.rs` (**39**). |
| **Port traits** | `pub trait` entries in `crates/rusvel-core/src/ports.rs` (**22**, including five `*Store` subtraits, `ChannelPort`, `BrowserPort`, `RusvelBasePort`). `DepartmentApp` lives under `department/`. |

---

## 1. Numbers at a Glance

| Metric | Count |
|--------|------:|
| Workspace members | 55 |
| Rust lines of code (crates/*.rs) | ~73,058 |
| Rust source files (crates/) | 301 |
| Tests (approx., `cargo test`) | ~645 (0 failures, full workspace from repo root; sum of `running N tests` lines) |
| Test targets (approx., `cargo test --no-run`) | ~102 |
| HTTP route chains (`lib.rs` `.route(`) | 153 |
| API handler modules (`rusvel-api/src/*.rs` excl. lib) | 39 |
| Port traits (`rusvel-core` `ports.rs`) | 22 |
| `pub struct` / `pub enum` in `domain.rs` | 114 |
| Departments (booted `DepartmentApp`) | 14 |
| Department crates (dept-*) | 14 |
| Engines | 13 (6 wired + 7 skeletons, all via `DepartmentApp`) |
| Registered agent tools (`ToolPort` registry) | 85 as of 2026-09-02 (built-in + `tool_search` + engine tools + all 55 ADR-014 department tools, now bridged in; optional memory, delegate, terminal, flow, browser) |
| MCP stdio tools (`rusvel-mcp`) | 94 as of 2026-09-02 (9 hand-written MCP-native + everything on the `ToolPort` registry) |

---

## 2. Build Health

| Check | Status |
|-------|--------|
| `cargo build` | Clean — 0 errors |
| `cargo test` | Full suite passes (see re-verify note above) |

---

## 3. What Works End-to-End

These features are wired from the binary entry [`crates/rusvel-app/src/main.rs`](../../crates/rusvel-app/src/main.rs) through adapters to the HTTP API and, where noted, the embedded frontend.

**API server startup** — Boots SQLite WAL, LLM with ModelTier routing + `MetricStore` cost tracking, `EventBus`, `MemoryPort`, `ScopedToolRegistry` (built-in + engine tools + optional tools), `JobQueue`, `AgentRuntime` with streaming, optional `EmbeddingPort`, `VectorStore`, `TerminalPort`, optional `rusvel_cdp::CdpClient` / `BrowserPort`; collects `DepartmentApp` instances from 14 `dept-*` crates; `DepartmentManifest` registration order; seeds default data; spawns job worker; Axum on `:3000` with graceful shutdown.

**LLM composition root** — [`llm_bootstrap::compose_llm_multi`](../../crates/rusvel-app/src/llm_bootstrap.rs): `ANTHROPIC_API_KEY` → [`ClaudeProvider`](../../crates/rusvel-llm/src/claude.rs) (Messages API + streaming + batch); otherwise `ClaudeCliProvider`. `OPENAI_API_KEY` → `OpenAiProvider`. Ollama always registered at `OLLAMA_HOST` (default `http://localhost:11434`). Cursor agent unchanged. Matches picker prefixes in [`GET /api/config/models`](../../crates/rusvel-api/src/config.rs).

**First-run wizard** — Interactive `cliclack` onboarding: detects Ollama, collects name/role, writes `profile.toml`, creates first session (copy explains API key vs CLI vs Ollama).

**Embedded frontend** — `rust-embed` compiles `frontend/build/` into the binary; filesystem fallbacks and temp extraction as implemented in app.

**Department chat (SSE)** — `POST /api/dept/{dept}/chat` streams via `AgentRuntime` (`AgentEvent` SSE). Includes config cascade, **`chat_mode`** (`discuss` \| `agentic`) in [`LayeredConfig`](../../crates/rusvel-core/src/config.rs), optional GitHub PAT context when `github_connector` is set, `@agent` mentions, `/skill` resolution, `!build`, rules from `ObjectStore`, per-dept MCP config, **hook dispatch** after `{engine}.chat.completed` ([`hook_dispatch.rs`](../../crates/rusvel-api/src/hook_dispatch.rs) from [`department.rs`](../../crates/rusvel-api/src/department.rs)), conversation persistence.

**Artifacts** — `GET/POST /api/artifacts`, `GET/DELETE /api/artifacts/{id}` ([`artifacts.rs`](../../crates/rusvel-api/src/artifacts.rs)); UI `/artifacts`.

**Active tasks dashboard** — `GET /api/dashboard/active` aggregates jobs + approvals + cron ([`dashboard.rs`](../../crates/rusvel-api/src/dashboard.rs)); UI `/tasks`.

**GitHub connector (PAT)** — `GET/POST/DELETE /api/connectors/github/pat` ([`connectors.rs`](../../crates/rusvel-api/src/connectors.rs)); Settings page.

**God agent chat** — `POST /api/chat` with SSE, history, profile context.

**Sessions, mission, goals** — Session CRUD; `GET .../mission/today`; goals CRUD.

**Events** — `GET /api/sessions/{id}/events`, `GET /api/dept/{dept}/events`.

**Entity CRUD** — Agents, skills, rules, MCP servers, hooks, workflows (REST as routed in [`lib.rs`](../../crates/rusvel-api/src/lib.rs)).

**Approvals (ADR-008)** — `GET /api/approvals`, approve/reject; job worker respects approval-gated jobs.

**Capability engine** — `POST /api/capability/build`; `!build` in chat.

**Workflows** — CRUD + `POST /api/workflows/{id}/run`.

**Hooks** — CRUD + `GET /api/hooks/events`; dispatch on events (command / http / prompt).

**Engines (core depth)** — `CodeEngine`, `ContentEngine`, `HarvestEngine`, `FlowEngine`, `GtmEngine` instantiated with real logic; engine routes under `/api/dept/code/*`, `/api/dept/content/*`, `/api/dept/harvest/*`, `/api/dept/gtm/*`, `/api/flows/*`, playbooks, kits, brief, etc.
**Content publishing** — `content-engine` includes **real HTTP adapters** for DEV.to, Twitter/X, LinkedIn ([`adapters/`](../../crates/content-engine/src/adapters/)); credentials via `ConfigPort` keys (e.g. `twitter_token`, `linkedin_token`).

**Job queue worker** — Polls jobs; handles `CodeAnalyze`, `ContentPublish`, `HarvestScan` with `session_id` scoping.

**RusvelBase** — `/api/db/*` routes; UI under `/database/*`.

**Knowledge/RAG** — Multiple `/api/knowledge/*` routes (list, ingest, search, hybrid-search, stats, related, delete).

**Code-to-content** — `POST /api/dept/content/from-code`.

**MCP server (stdio)** — `--mcp` → `rusvel_mcp` JSON-RPC; **9** hand-written tools in `tool_definitions()` plus every tool on the `ToolPort` registry (`RusvelMcp::all_tool_definitions()`), **94** total as of 2026-09-02 — closes #13.

**MCP client** — `rusvel-mcp-client` for external servers.

**CLI** — One-shot, `shell`, `--tui`.

**Terminal** — API routes for dept pane, run panes, WebSocket (`/api/terminal/*`).

**Browser (CDP)** — `/api/browser/*` when CDP client wired. Optional **`RUSVEL_BROWSER_ALLOWED_ORIGINS`** (comma-separated hosts) blocks `navigate` elsewhere; successful `navigate` / `evaluate_js` emit `browser.navigate` / `browser.evaluate_js` events ([`browser.rs`](../../crates/rusvel-api/src/browser.rs)).

**System / visual** — `/api/system/*`, visual report routes for regression testing.

**Seed data** — Default agents, skills, rules on first run.

### Sprint 6 — shipped vs partial

Aligned with [`openclaw-sprint-plan.md`](../plans/openclaw-sprint-plan.md) Sprint 6: **S-040** (webhooks), **S-041** (cron), **S-046** (tests) shipped; **S-042** core (`POST /api/forge/pipeline`, `ForgeEngine::orchestrate_pipeline`); **S-043** (`forge.daily_briefing` → `generate_brief`); **S-044** partial (`record_outcome`, scorer hints); **S-045** partial (`ContextPack` + dept chat assembly). Matrix: [`sprint-6-7-implementation.md`](../plans/sprint-6-7-implementation.md).

---

## 4. Built but Needs More Work

**Extended GTM / CRM depth** — **OutreachSend** is wired: `JobKind::OutreachSend` in [`main.rs`](../../crates/rusvel-app/src/main.rs) runs `gtm-engine` outreach (approval-gated send, SMTP/mock). Remaining depth: more CRM surfaces, templates, and provider-specific polish.

**Authentication/authorization** — `rusvel-auth` is in-memory from env; optional bearer env; phased API middleware (see sprint implementation doc).

**Seven “business” engines** — Finance, Product, Growth, Distro, Legal, Support, Infra are thinner than the 6 wired engines (Forge/Code/Content/Harvest/Flow/GTM); chat works via `DepartmentApp` + registry.

**Gap vs old monoliths** — See [`gap-analysis.md`](gap-analysis.md): historical comparison to `old/` repos; **not** a substitute for §3–4 here.

---

## 5. Test Breakdown (high level)

Tests are spread across crates; highest concentration in `rusvel-llm`, `forge-engine`, `rusvel-api`, `rusvel-db`, `harvest-engine`, `rusvel-core`, `rusvel-agent`, `content-engine`, etc.

Some surface crates (`rusvel-app`, `rusvel-cli`, `rusvel-mcp`, `rusvel-tui`, …) rely on integration or have minimal unit tests. **Not** all `dept-*` crates have dedicated test binaries; **stub** domain engines (finance, product, …) often have fewer tests than core engines.

---

## 6. Next steps (from gaps + sprint intent)

1. Deepen **GTM** / CRM (beyond OutreachSend worker path) and channel adapters per roadmap.
2. Add **auth middleware** and a clear model for API keys/sessions if exposing beyond localhost.
3. Continue **Sprint** themes in [`../plans/sprints.md`](../plans/sprints.md) (reference only).
4. Re-run **§ How to re-verify** monthly or after large merges; append rows to [`verification-log-2026-03-30.md`](verification-log-2026-03-30.md) or a new dated log.

---

## 7. Historical (archived metrics)

Older snapshots (e.g. 48 crates, 124 handlers, 222 tests, 30 test binaries) referred to earlier tree sizes and counting methods. **Do not use** for current planning; use **§1** and **Metric definitions** above.
