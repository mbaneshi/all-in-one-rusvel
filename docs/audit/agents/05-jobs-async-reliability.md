# 05 — Jobs, async & reliability

## Context (for next agents)

- **Single queue:** `rusvel-jobs` + worker in `rusvel-app` (ADR-003).
- **Risk areas:** Retries, idempotency, visibility on failure, approval-gated jobs (content publish, outreach).
- **API:** `spawn_blocking` patterns in `rusvel-api` — thread pool and error mapping.

---

## Agent prompt (copy below)

```
Reliability audit: rusvel-jobs, job worker wiring in rusvel-app, approval flows, spawn_blocking in rusvel-api.

Assess: duplicate execution, lost jobs, error surfacing to user/API, backpressure, observability (logs/metrics gaps).

Document in docs/audit/agents/05-jobs-async-reliability.md Report with findings, fix proposals, improvement ideas.

Handoff: which job kinds need load test or chaos notes for audit 07.
```

---

## Report

### Executive summary

Production uses **one `Database` (`Arc<Mutex<Connection>>`)** as `JobPort`; dequeue is **SELECT then UPDATE** without an explicit SQL transaction. **Within a single `rusvel` process** the connection mutex serializes claims, so two tasks cannot dequeue the same row. **Two processes** opening the same `rusvel.db` can still race between SELECT and UPDATE and **double-claim** the same queued job—treat multi-instance as unsupported or fix with `BEGIN IMMEDIATE` + single atomic statement.

There is **no lease / visibility timeout**: jobs left **`Running`** after crash, `kill -9`, panic, or a failed `complete`/`fail` after side effects are **stuck until manual repair**. **`max_retries` / `retries`** are stored on `Job` but **`JobPort::fail` never re-queues or increments retries**—retry policy is unimplemented.

The job worker is a **single sequential loop** with a **5s idle poll**; long jobs **head-of-line block** all other kinds. **Backpressure** is implicit (queue grows in SQLite) with **no depth limits, metrics, or HTTP rejection**.

**Approval flows** (`hold_for_approval` → `AwaitingApproval` → `approve` → `Queued`) are coherent for **`ProposalDraft`** and **`OutreachSend`** (second pass uses `metadata.approval_pending_result`). **`GET /api/approvals`** and **`GET /api/jobs`** surface status and errors; some worker paths mark jobs **`Succeeded`** while embedding validation errors in **`JobResult.output`**, which is easy to misread in UIs.

**`rusvel-api`** uses **`spawn_blocking`** only in **`db_routes.rs`** (schema introspection, table rows, SQL runner). Errors map to HTTP status via `map_err`; **`JoinError`** from a panicked blocking task surfaces as **500**. The **`rusvel-db`** `JobStore` path also uses **`spawn_blocking`** heavily—API traffic and the job worker **share** the same DB mutex, so heavy SQL UI work can **lengthen job dequeue latency** (not starvation of the async runtime, but **queue latency**).

---

### Findings

| Severity | Topic | Evidence | Notes |
|----------|-------|----------|-------|
| High | Stuck `Running` / “lost” completion | `JobStore::dequeue` sets `Running`; no timeout or sweeper. `complete`/`fail` require `Running`. | Process death, panic, or DB update failure after successful side effect → job never reaches terminal state; may confuse operators and block re-enqueue of same logical work. |
| High | Multi-process double execution | `store.rs`: `SELECT … LIMIT 1` then separate `UPDATE` (no `BEGIN IMMEDIATE` / atomic `UPDATE … RETURNING`). | Two `rusvel` binaries on one DB can claim the same row. Single process mitigated by one shared `Mutex<Connection>`. |
| Medium | Retries unused | `Job` has `retries`, `max_retries`; `JobPort::fail` only sets `Failed`. | Fields appear in API (`JobListItem`) but convey no real retry behavior. |
| Medium | Misleading success for bad payload | `main.rs` `ContentPublish`: invalid `content_id`/platform → `Ok(Some(JobResult { output: {"error": …} }))` → `complete`. | Status `Succeeded` while output describes failure; clients must parse payload, not status alone. |
| Medium | Unknown kinds “succeed” | `main.rs` default arm: `Ok(Some(JobResult { "action": "unknown", … }))` + `complete`. | Misconfiguration or new enum variant on wire may look like success. |
| Low | Dead branch in worker | `main.rs`: `if job.status == JobStatus::AwaitingApproval` after `dequeue`. | Dequeue transitions `Queued` → `Running`; this path does not run for normal DB-backed flow. |
| Low | `spawn_blocking` pool + shared DB lock | `db_routes.rs` + `JobStore` both `spawn_blocking` on same `Database`. | Under load, many concurrent DB routes extend time-to-dequeue for jobs. |
| Low | `rusvel-jobs::spawn_worker` swallows terminal errors | `spawn_worker`: `let _ = queue.complete/fail`. | Test/helper worker only; production uses `rusvel-app` loop (logs failures). |
| Info | Single worker, fixed poll | `main.rs`: one `tokio::spawn` loop, `sleep(5s)` between iterations when idle. | Simple and predictable; high latency for empty queue; no priority or fairness across kinds. |
| Info | Worker starts for all modes | Worker spawned before CLI/TUI/MCP/server branch. | Consistent for long-running server; one-shot CLI exit can orphan `Running` if a job was mid-flight. |

---

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| F01 | Wrap dequeue in **`BEGIN IMMEDIATE`**; use single **`UPDATE … WHERE status='Queued' … RETURNING`** (or equivalent) so claim is atomic across connections. | M | `rusvel-db` / `JobStore` |
| F02 | Add **stale-Running** policy: `started_at` + threshold → `Failed` or re-`Queued` with `retries += 1`, plus admin/API to list stuck jobs. | M | `rusvel-db`, `JobPort`, optional `rusvel-api` |
| F03 | Implement **`max_retries`** in `fail` (or dedicated `retry`): increment `retries`, if `< max_retries` reset to `Queued` with backoff metadata; else `Failed`. | M | `rusvel-jobs` (trait semantics), `rusvel-db`, worker |
| F04 | Treat validation errors as **`fail()`** (or a distinct terminal status) for `ContentPublish` / unknown kinds instead of `complete` with error JSON. | S | `rusvel-app` worker |
| F05 | Remove or repurpose **AwaitingApproval** check** post-dequeue; document that approval-only dequeue is unnecessary with current `JobPort`. | S | `rusvel-app` |
| F06 | **Observability:** structured fields for `duration_ms`, `queue_depth` (count by status), `running_age_seconds`; optional `MetricStore` counters/histograms. | M | `rusvel-app`, `rusvel-db` or metrics adapter |
| F07 | **Backpressure (optional):** max queued jobs per session/kind, or reject enqueue with **429/507** when depth exceeds cap. | L | `rusvel-api` enqueue sites, `JobPort` |
| F08 | Document or enforce **single writer** to `rusvel.db` for job processing; if multi-instance is required, ship F01 first. | S | docs / ops |

---

### Space for improvement

- **Concurrency:** Optional worker pool with **per-kind concurrency limits** (e.g. cap parallel `HarvestScan` / CDP) without unbounded `tokio::spawn`.
- **Idempotency:** Content publish and outreach send should use **stable idempotency keys** (e.g. in `metadata`) so retries do not double-post or double-email after partial failure.
- **Shutdown:** Watch shutdown channel inside the **job body** for long engines, or **graceful drain** with timeout before exit.
- **Approvals UX:** `GET /api/approvals` has no limit; align with **`JobFilter::limit`** for large queues.
- **Testing:** Integration tests for **approve → second dequeue** for `ProposalDraft` and `OutreachSend`; property test that **dequeue never returns two in-flight rows with same id** under concurrent callers once F01 lands.

---

### Handoff (for audit 07)

**Job kinds / async hotspots for load testing**

| Kind | Why load-test |
|------|----------------|
| `HarvestScan` | Browser/CDP paths, variable latency, resource-heavy; good for **concurrency + lock contention** on shared `Database`. |
| `ScheduledCron` | Bursts when many schedules align; mixes **event emit**, **forge** briefing, **harvest** auto-scan. |
| `Custom:forge.pipeline` | Multi-step orchestration across harvest + content (+ agent); long wall-clock, failure cascades. |
| `OutreachSend` | Approval gate + **SMTP** + optional **chained enqueue** for next step; tests **transactional** “send then enqueue next”. |
| `CodeAnalyze` | CPU/disk bound `spawn_blocking`-style work inside engines; stresses **single-worker head-of-line** behavior. |
| `ContentPublish` | External platform adapters; good for **timeout/retry** policy once implemented. |

**Chaos / failure-injection notes (audit 07)**

- **`kill -9`** while `Running`: expect stuck row; validate recovery after F02-style sweeper or manual SQL.
- **Two `rusvel` processes**, same `data_dir`: duplicate job execution until F01.
- **SMTP failure** after approval on `OutreachSend`: email API errors should land in **`Failed`** with clear `job.error`; ensure no “silent” partial state in CRM/run metadata.
- **SQLite busy/locked**: concurrent heavy `/api/db` SQL + job worker; measure p95 dequeue latency.
- **`complete`/`fail` storage error** after successful external side effect: document **at-least-once** risk and need for idempotency (F08 + idempotency keys).
