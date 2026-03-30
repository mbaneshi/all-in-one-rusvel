# Audit fix proposals (code-verified)

**Date:** 2026-03-30  
**Source:** Agent reports `docs/audit/agents/01–10.md` cross-checked against the repo (spot reads + `rg`, not full formal review).

**Verification summary**

| Claim | Checked | Result |
|-------|---------|--------|
| Bearer middleware passes all `/api/*` when both tokens unset | `rusvel-api/src/auth.rs:52–55` | **Confirmed** |
| `rusvel-app` uses `ToolRegistry::new()`, not `ScopedToolRegistry` | `rusvel-app/src/main.rs:791` | **Confirmed** |
| Agent loop builds tool defs from `tools.list()`, not `AgentConfig.tools` | `rusvel-agent/src/lib.rs:681–682`; `department.rs` still sets `tools:` on `AgentConfig` | **Confirmed** — config field exists (`domain.rs:448`) but is unused in the loop |
| Job `dequeue` = `SELECT` then separate `UPDATE` (no `BEGIN IMMEDIATE`) | `rusvel-db/src/store.rs:1286–1348` | **Confirmed** — multi-process race possible |
| `fail()` does not increment `retries` or re-queue | `rusvel-db/src/store.rs:1639–1657` | **Confirmed** |
| Invalid `ContentPublish` payload → `Ok(JobResult { error: … })` → `complete` | `rusvel-app/src/main.rs:1078–1082`, `1349–1353` | **Confirmed** |
| `load_history` lists all `chat_message` then filters in memory | `rusvel-api/src/chat.rs:273–282` | **Confirmed** |
| Frontend `api.ts` has no `Authorization` header | `rg Authorization frontend/src/lib/api.ts` | **Confirmed** (no matches) |
| SMTP password env in code | `gtm-engine/src/email.rs:81–82` | **`RUSVEL_SMTP_PASSWORD`** (not `RUSVEL_SMTP_PASS`) |
| `RUSVEL_DB_PATH` in Rust sources | workspace `*.rs` | **Not referenced** — doc-only risk stands |
| `ChannelPort` in `rusvel-core` | `rusvel-core/src/ports.rs:558` | **Confirmed** — older audit §1.1 is stale |

---

## Priority legend

- **P0** — Security or severe correctness; address before any network-exposed deploy.  
- **P1** — Reliability / misleading semantics / major scale pain.  
- **P2** — Hardening, performance containment, CI/DX.  
- **P3** — Documentation, polish, structural refactors with long payoff.

Order within a band is suggested implementation sequence (dependencies first).

---

## P0 — Security & exposure

### P0-A — Operator warning when API is open on non-loopback

- **Context:** Deployment / `rusvel-app` boot.  
- **Verified:** `auth.rs` — if `token` and `read_token` are both `None`, every `/api/*` request is allowed (`:52–55`).  
- **Reason:** Matches “local dev default” but is catastrophic if `:3000` is reachable from untrusted networks (DB SQL UI, chat, tools, hooks).  
- **Proposal:** On HTTP bind address != loopback and both tokens unset, log **WARN** (or **ERROR**) once with explicit text; optional `RUSVEL_ALLOW_INSECURE_API=1` to suppress for intentional lab use. Extend to MCP HTTP if enabled without auth (`rusvel-mcp` per audit 03).

### P0-B — Document and narrow “read-only” token vs SQL console

- **Context:** Auth + RusvelBase.  
- **Verified:** Read token allows GET/HEAD/OPTIONS only (`auth.rs:81–87`); `POST /api/db/sql` is not a GET — need to confirm read token blocks it. Actually POST with read_token → forbidden_response() — good. But **admin** token can still run arbitrary SQL.  
- **Reason:** Audit 04 is right that **unset tokens** + DB routes = remote SQL; read-token story is weaker if operators assume “read-only API” means “no data exfil” — `GET` + list endpoints may still expose a lot.  
- **Proposal:** Operator runbook one-pager: tokens required for LAN/WAN; optional env `RUSVEL_DB_SQL=off` or force `read_only` server-side for non-dev builds (audit 04 F2).

### P0-C — Frontend bearer support (pair with token deployment)

- **Context:** `frontend/src/lib/api.ts` + embedded SPA.  
- **Verified:** No `Authorization` header in `api.ts`.  
- **Reason:** Enabling `RUSVEL_API_TOKEN` breaks the UI until the client sends a header; today “secure API” and “usable UI” are mutually exclusive without a reverse proxy injecting auth.  
- **Proposal:** Optional `VITE_RUSVEL_API_TOKEN` (or runtime `/api/system/public-config` returning “auth required” only — avoid putting secrets in static bundle when possible); document threat model (read-only embedded token vs cookie/session later).

---

## P1 — Jobs, tools, and API truthfulness

### P1-A — Atomic job claim (SQLite)

- **Context:** `rusvel-db` `JobStore::dequeue`.  
- **Verified:** `SELECT … LIMIT 1` then `UPDATE … WHERE id = ?` inside `spawn_blocking` but **not** wrapped in `BEGIN IMMEDIATE`; second process on same DB file can double-claim. Single-process mutex mitigates one binary.  
- **Reason:** Duplicate job execution (publish, email, scan).  
- **Proposal:** `BEGIN IMMEDIATE` for the claim transaction, or single statement `UPDATE … WHERE status='Queued' … RETURNING` (sqlite 3.35+) to claim atomically; document **single writer** until fixed.

### P1-B — Stuck `Running` jobs

- **Context:** Worker crash / `kill -9` / panic after side effects.  
- **Verified:** `dequeue` sets `Running`; `fail`/`complete` require `Running` (`store.rs:1647–1651`). No lease timeout in code path reviewed.  
- **Reason:** Ops confusion; blocked reprocessing.  
- **Proposal:** Sweeper: `started_at` older than threshold → `Failed` or re-`Queued` with `retries += 1`; admin list API for stuck jobs.

### P1-C — Do not `complete()` validation errors as success

- **Context:** `rusvel-app` worker `JobKind::ContentPublish` invalid payload.  
- **Verified:** `main.rs:1078–1082` returns `Ok(Some(JobResult { output: {"error": …} }))` → `complete` at `1351`. Same pattern for unknown kind `1340–1345`.  
- **Reason:** UIs and monitors see `Succeeded` while output describes failure.  
- **Proposal:** Call `job_port.fail(&job_id, msg)` for invalid payload / unknown kind, or introduce `JobStatus::CompletedWithWarning` if you must preserve partial artifacts.

### P1-D — Enforce department tool allowlist at runtime

- **Context:** `rusvel-app` composition + `rusvel-agent` loop.  
- **Verified:** `ToolRegistry::new()` at root; `run_streaming_loop` uses `tools.list()` minus `searchable` (`lib.rs:681–682`); `department.rs` passes `tools: resolved.allowed_tools` into `AgentConfig` but agent crate does not filter defs by that list.  
- **Reason:** Documented per-dept tools do not match actual LLM-visible tools; widens blast radius for abuse and accidents.  
- **Proposal:** Wrap registry with `ScopedToolRegistry` from `allowed_tools` / manifest, or filter `tool_defs` in `AgentRuntime` against `config.tools` (define semantics for empty list = defaults vs deny-all). Inject trusted `__department_id` for `ToolRegistry::check_permission` (`rusvel-tool/src/lib.rs:241`).

### P1-E — Align supervised tool approval with job approvals (or fail closed)

- **Context:** `ToolPermissionMode::Supervised` vs ADR-008 jobs.  
- **Verified:** Audit 06 — string `AWAITING_APPROVAL` on tool result vs real `/api/approvals` for jobs.  
- **Reason:** Model and UI can misread state; inconsistent security story.  
- **Proposal:** Either enqueue a `JobPort` approval for supervised tools, or return `success: false` + structured error code; unify naming in docs and frontend.

---

## P2 — Performance, data access, and CI

### P2-A — Scoped chat history query

- **Context:** `rusvel-api` `load_history`, department namespaced messages.  
- **Verified:** Full `list("chat_message", ObjectFilter::default())` then filter `conversation_id` in memory (`chat.rs:273–282`).  
- **Reason:** Latency and RAM grow with **global** message count.  
- **Proposal:** `ObjectFilter` JSON predicate on `conversation_id`, or key layout `conversation_id/msg_id`, or normalized table — as in audit 07 P1.

### P2-B — Default `limit` on hot `ObjectStore::list` call sites

- **Context:** Rules, hooks, agents, workflows, etc.  
- **Verified:** Pattern exists (audit 07); spot-check: `chat.rs:207` uses `ObjectFilter::default()` for conversations listing.  
- **Reason:** Accidental full-table reads and large JSON decodes.  
- **Proposal:** Cap defaults (e.g. 500–2000) per kind; pagination in REST where needed; CI grep guard for risky patterns (audit 07).

### P2-C — Implement or remove `max_retries` / `retries`

- **Context:** `Job` domain + `JobStore::fail`.  
- **Verified:** `fail` sets `Failed`, does not touch `retries`.  
- **Reason:** Fields are misleading for operators and API consumers.  
- **Proposal:** In `fail`, if `retries < max_retries`, reset to `Queued` with backoff metadata; else `Failed`.

### P2-D — Tighter default `max_iterations` and compaction policy

- **Context:** `rusvel-agent` — default 50, compaction extra LLM call.  
- **Verified:** `domain.rs:451–452`; loop in `lib.rs`.  
- **Reason:** Cost and tail latency.  
- **Proposal:** Lower default (e.g. 12–20), dept-configurable; debounce `compact_messages` (audit 07).

### P2-E — CI gates: `fmt`, `clippy -D warnings`, `pnpm check`

- **Context:** `.github/workflows` per audit 08.  
- **Verified:** Not re-read in this pass; audit 08 states they were missing — treat as **likely**; confirm when editing CI.  
- **Reason:** Drift and silent type errors (`pnpm build` alone may not catch all).  
- **Proposal:** Add steps per audit 08 T1/T2; keep llvm-cov floor.

### P2-F — Minimal `rusvel-mcp` stdio smoke test

- **Context:** MCP server crate.  
- **Reason:** No tests = regressions in JSON-RPC loop.  
- **Proposal:** Golden stdin/stdout test for `initialize` + one method (audit 08 T5).

---

## P3 — Documentation, domain polish, structure

### P3-A — Env var truth table

- **Context:** Docs vs code.  
- **Verified:** `RUSVEL_SMTP_PASSWORD` in `email.rs`; `RUSVEL_DB_PATH` absent from `*.rs`.  
- **Reason:** Operators set dead or wrong variables.  
- **Proposal:** Single canonical table in `docs-site` or `CLAUDE.md`; fix `RUSVEL_SMTP_PASS` typos in any doc; implement or delete `RUSVEL_DB_PATH` / `RUSVEL_SEED_DEV` references.

### P3-B — Wire `log.level` from `rusvel-config` or document `RUST_LOG` only

- **Context:** Audit 10.  
- **Reason:** Misleading config keys.  
- **Proposal:** Read `log.level` when `RUST_LOG` unset, or remove key from default TOML.

### P3-C — ADR-007: narrow `domain.rs` module doc or add `metadata` to remaining DTOs

- **Context:** Audit 01.  
- **Reason:** Doc claims “all structs” carry metadata; many DTOs do not.  
- **Proposal:** FP-1 from audit 01 — choose one strategy and enforce.

### P3-D — `docs/design/architecture-v2.md` department count

- **Context:** Audit 01.  
- **Proposal:** Update 12 → 14 (or dynamic wording) to match registry.

### P3-E — Split or slim `rusvel-api` (long-term)

- **Context:** Audit 02 — ~14.5k LOC, engines + `Arc<Database>` on `AppState`.  
- **Proposal:** Vertical slices or `RusvelBasePort` instead of concrete `Database` on state (F1/F2 audit 02); pipeline runners into forge/dept-forge.

### P3-F — Supply chain monitoring

- **Context:** `cargo audit` / RUSTSEC-2026-0002 (`lru` transitive), pnpm `cookie` advisory.  
- **Proposal:** CI `cargo audit` with writable cache; track upstream bumps; optional `deny.toml`.

---

## Suggested execution waves

| Wave | Items | Outcome |
|------|--------|---------|
| **1** | P0-A, P0-B, P3-A | Safer default posture + accurate ops docs |
| **2** | P0-C + token deployment | Secured API + working UI |
| **3** | P1-A, P1-B, P1-C | Correct job semantics under failure |
| **4** | P1-D, P1-E | Tool surface matches product promises |
| **5** | P2-A, P2-B, P2-C, P2-D | Scale and cost containment |
| **6** | P2-E, P2-F, P3-F | CI and MCP confidence |
| **7** | P3-B–P3-E | Polish and architecture paydown |

---

## References

- Agent reports: `docs/audit/agents/README.md`  
- Prior snapshot: `docs/audit/audit-2026-03-28.md` (partially stale; see audit 01 reconciliation)  
- Meta alignment notes: prior chat review of audits 01–10
