# 07 — Performance & scalability

## Context (for next agents)

- **Hot paths:** Chat SSE, embeddings + `rusvel-vector` (LanceDB), SQLite access, flow engine, harvest/content pipelines.
- **Inputs:** Handoffs from 04 (data), 05 (jobs/async), 06 (LLM cost).

---

## Agent prompt (copy below)

```
Performance audit for RUSVEL.

Identify: N+1 or unbounded queries; memory buffers; blocking in async runtimes; LLM call amplification; vector index misuse.

Propose a measurement plan (tracing/metrics/benches) and top 5 optimizations with rationale.

Fill docs/audit/agents/07-performance-scalability.md Report including fix proposals and improvement backlog.

Handoff: frontend concerns (bundle size, SSE client) for audit 09.
```

---

## Report

### Executive summary

Hot paths (chat SSE, department chat, object store, agent loop, RAG) are functionally correct but several patterns scale linearly with **total stored rows** or **agent iterations**, not with “work needed for this request.” The largest wins are: **scoped queries for chat objects** (avoid loading every `chat_message`), **default caps on `ObjectStore::list` call sites**, and **tighter defaults for agent iteration + context compaction** to reduce LLM amplification. SQLite access is correctly offloaded via `spawn_blocking`, but a **single mutexed connection** and an unbounded blocking pool under load remain operational risks. Vector search uses `.limit(n)` appropriately; Lance **per-row upsert** (delete + add) is fine for small corpora but becomes a throughput bottleneck for bulk ingestion.

### Findings

| Severity | Topic | Evidence | Notes |
|----------|-------|----------|-------|
| High | Unbounded / full-table object reads | `load_history` lists all `chat_message` then filters by `conversation_id` in memory (`crates/rusvel-api/src/chat.rs`); department chat uses the same pattern for namespaced messages; `dept_conversations` lists entire namespace (`crates/rusvel-api/src/department.rs`) | Classic “read the world, filter in RAM.” Cost grows with **global** message count, not conversation length. |
| High | `ObjectFilter::default()` without `limit` | Widespread `.list(..., ObjectFilter::default())` for agents, rules, skills, hooks, workflows, MCP servers, etc. (`crates/rusvel-api/src/*.rs`, hooks, cron, webhooks) | `ObjectStore` supports `limit`/`offset` (`rusvel-core` `ObjectFilter`), but most callers omit them. |
| Medium | SQLite `OFFSET` without `limit` uses huge scan cap | `rusvel-db` object `list`: if `offset` set without `limit`, adds `LIMIT ?` with `i64::MAX` (`crates/rusvel-db/src/store.rs`) | Prevents invalid SQL but can still read an enormous row set into memory. |
| Medium | LLM call amplification (agent loop) | `DEFAULT_MAX_ITERATIONS = 50` (`crates/rusvel-agent/src/lib.rs`); department chat passes `max_iterations: None` → default applies (`crates/rusvel-api/src/department.rs`) | Each iteration can include tools + full context; worst case is dozens of model calls per user message. |
| Medium | Extra LLM calls from context compaction | `compact_messages` runs when `messages.len() > 30`, issuing a **summarization** `llm.generate` (tier `fast`) (`crates/rusvel-agent/src/lib.rs`) | Reduces tokens long-term but adds latency and **another** billed call whenever the threshold is crossed during the loop. |
| Medium | RAG on every department chat turn | `embed_one` + `vector_store.search(..., 5)` before run (`crates/rusvel-api/src/department.rs`) | Expected for quality; still doubles embedding work vs non-RAG paths and should be budgeted/traced. |
| Low | Lance vector write pattern | `upsert` = `delete` + `add` per id (`crates/rusvel-vector/src/lib.rs`) | Correct semantics; poor bulk-ingest throughput vs batched append + dedupe strategy. |
| Low | Blocking thread pool pressure | Heavy `tokio::task::spawn_blocking` usage in `rusvel-db` for all SQLite ops | Correct vs blocking the async runtime; under concurrency, default blocking pool size can become the bottleneck (queueing). |
| Low | In-memory aggregation | `load_rules_for_engine`: list all rules, filter enabled + engine in memory (`crates/rusvel-api/src/rules.rs`) | Acceptable at small N; same class as unbounded list if rules grow large. |

### Measurement plan

1. **Tracing (structured spans)**  
   - Span around `ObjectStore::list` with attributes: `kind`, `limit`, `row_count` (after query), `duration_ms`.  
   - Spans for chat path: `load_history`, `store_message`, `run_streaming`, per-iteration `agent.iteration`, `compact_messages`, `llm.stream` / `llm.generate`.  
   - Spans for RAG: `embed_one`, `vector_store.search` with `limit` and result count.  
   - Optional: `spawn_blocking` wrapper span in DB adapter to correlate async wait vs SQLite work.

2. **Metrics (counters / histograms)**  
   - Histogram: `rusvel_object_list_rows`, `rusvel_object_list_duration_seconds` by `kind`.  
   - Counter: `rusvel_agent_iterations_total`, `rusvel_llm_requests_total` by `department_id` / `route`.  
   - Histogram: SSE `time_to_first_byte`, stream duration (API layer).  
   - Gauge or histogram: Tokio blocking pool metrics if exposed (or log queue depth under load test).

3. **Benchmarks / load scripts**  
   - Extend Criterion-style benches (e.g. `cargo bench -p rusvel-app --bench boot`) or add a small bench that seeds N `chat_message` objects and times `load_history`-equivalent list+filter vs a capped/scoped query (before/after fix).  
   - Scripted k6/oha against `POST /api/chat` and `POST /api/dept/:id/chat` with concurrent clients to observe p95 latency and blocking pool saturation.

4. **Static guards**  
   - Clippy custom lint or simple `scripts/` grep in CI: flag new `ObjectFilter::default()` in `list(` without adjacent `limit` (allowlist test fixtures).

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| P1 | Add **conversation-scoped** object queries: extend `ObjectFilter` with optional JSON path predicates (e.g. `conversation_id`), or store chat messages under ids prefixed by `conversation_id` and list by key range; migrate `load_history` / namespaced loaders to never full-scan `chat_message`. | L | `rusvel-core` ports + `rusvel-db` + `rusvel-api` chat/department |
| P2 | Apply **safe default limits** on all hot `list` call sites (e.g. 500–2000 cap per kind for admin UI, lower for chat-derived paths); return pagination metadata where the API is REST list. | M | `rusvel-api`, optional frontend query params |
| P3 | Reduce **default `max_iterations`** (e.g. 12–20) and/or set department chat `max_iterations` from dept config with a conservative default; document “high” mode for power users. | S | `rusvel-agent`, `rusvel-api` department chat |
| P4 | **Compaction policy**: before calling `compact_messages`, try cheap truncation of old tool transcripts; or raise threshold / debounce compaction to once per run; emit metric `context_compaction_llm_calls`. | M | `rusvel-agent` |
| P5 | **Bulk vector ingest**: batch multiple embeddings into one Lance `add` (and optional periodic optimize) for harvest/knowledge backfill; keep single-doc upsert for interactive path. | M | `rusvel-vector`, callers in harvest/knowledge |
| P6 | Cap **arbitrary SQL** / table browser rows at the API (max rows + max cell size) if not already enforced everywhere `execute_sql` is exposed. | S | `rusvel-api` db routes, `rusvel-db` |
| P7 | Optional: **read connection pool** or `sqlx`/`deadpool` for SQLite read replicas is out of scope short term; instead document max concurrent blocking tasks and tune `tokio` blocking pool for deployment. | L | `rusvel-app` runtime config |

**Top 5 optimizations (ranked) — rationale**

1. **Scoped chat history queries (P1)** — Removes the dominant O(global messages) term on every god-agent and department chat request; largest latency and memory win at scale.  
2. **Default/paginated `ObjectFilter.limit` on lists (P2)** — Low-risk containment for agents, rules, hooks, workflows; prevents accidental multi-megabyte JSON decodes.  
3. **Tighter agent iteration default + config wiring (P3)** — Directly caps worst-case LLM spend and tail latency per chat without changing architecture.  
4. **Compaction / summarization policy (P4)** — Cuts “hidden” extra LLM calls that compete with user-visible streaming latency.  
5. **Trace + metric instrumentation for list/agent/RAG (measurement plan §1–2)** — Makes regressions visible before production; validates P1–P4 in CI or staging.

### Space for improvement

- **Object store indexing**: Today filtering is `kind` + optional `session_id` JSON extract; richer metadata indexes (or normalized tables for high-cardinality kinds) would avoid full kind scans.  
- **Hook dispatch**: `load_matching_hooks` lists all hooks every time (`crates/rusvel-api/src/hook_dispatch.rs`); with many hooks, add engine/event index or in-memory cache invalidated on hook CRUD.  
- **Flow engine**: Review checkpoint frequency and payload size for large `node_outputs` (memory + SQLite write amplification).  
- **Harvest**: `index_outcome_vector` per outcome is correct but N embeddings + N Lance upserts under load; batch when processing pipeline chunks.  
- **Department `@agent` resolution**: Full `agents` list per message (`crates/rusvel-api/src/department.rs`); cache by department or resolve by id when mention format allows.

### Handoff (for audit 09)

- **Frontend bundle**: Vite/SvelteKit production bundle analysis (route-based code splitting, heavy deps on initial load, duplicate chunks); compare against embedded static serving in `rusvel-app`.  
- **SSE client**: Browser `EventSource` / fetch-stream usage for chat and department streams — reconnection backoff, idle timeouts, tab visibility, memory growth from long-lived connections, and whether multiple concurrent streams are opened.  
- **API coupling**: Which chat/history endpoints the UI calls and whether it passes pagination params once backend adds limits (coordinate with P2).  
- **Visual/E2E load**: Playwright suites that keep sessions open; ensure they do not mask SSE leaks or runaway polling in devtools performance profiles.
