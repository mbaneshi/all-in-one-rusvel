# 04 — Data layer, migrations & privacy

## Context (for next agents)

- **Stack:** SQLite WAL via `rusvel-db`, migrations, session-scoped data per product docs.
- **Concern:** SQL safety, PII in DB/logs, backup/restore story, future multi-tenant boundaries.
- **Handoff:** Schema or store patterns that affect security (03) and performance (07).

---

## Agent prompt (copy below)

```
Audit RUSVEL data layer: rusvel-db, rusvel-schema, store traits usage, migrations.

Check: parameterized queries vs string concat; migration discipline; session isolation; sensitive fields in events/logs; WAL/backup assumptions.

Output in docs/audit/agents/04-data-privacy.md Report:
- Executive summary.
- Findings table with severity and path:line.
- Fix proposals (S/M/L).
- Space for improvement (retention, encryption stance, least-privilege).
- Handoff: tables/queries perf-sensitive for audit 07.
```

---

## Report

### Executive summary

- **Storage model:** One SQLite file per deployment (`rusvel-db::Database`), opened with `PRAGMA journal_mode=WAL` and `foreign_keys=ON` on init. All five `StoragePort` sub-stores (`events`, `objects`, `sessions`, `jobs`, `metrics`) plus `cost_events` share this file. Migrations are embedded Rust constants, applied in version order via `user_version` (`crates/rusvel-db/src/migrations.rs`).
- **SQL safety (core paths):** Inserts/updates/selects in `store.rs` use `rusqlite` placeholders (`?1`, `params![]`). Dynamic filters build **SQL structure** with numeric placeholders only; filter **values** are bound (e.g. `EventStore::query`, `JobStore::list`, `MetricStore::query`, `JobStore::dequeue` `IN (...)` lists). Identifier interpolation for PRAGMA/table browsing goes through `validate_identifier` (ASCII alphanumeric + `_`) or `rusvel_schema::SchemaIntrospector` checks before ending up in SQL strings.
- **Residual risks:** (1) **RusvelBase** exposes `POST /api/db/sql` with optional `read_only` (disables `PRAGMA query_only` when false). With `RUSVEL_API_TOKEN` / `RUSVEL_API_READ_TOKEN` unset, middleware allows all API traffic — effectively **remote arbitrary SQL** including writes. (2) **Session isolation** is not enforced by SQLite (no RLS); it depends on handlers passing `session_id` into filters. (3) **Privacy:** `events.payload`, `threads.messages`, `jobs.payload`/`error`, and JSON `metadata` columns can hold prompts, tool output, and secrets **in plaintext** on disk. (4) **Backups:** Code assumes normal SQLite WAL semantics; there is no in-app checkpoint/backup API — operators must copy `-wal`/`-shm` consistently or use SQLite backup APIs.
- **Overall:** Strong parameter discipline on the main store implementation; highest practical concerns are **operational auth defaults**, **admin SQL console power**, **plaintext sensitive blobs**, and **application-level session scoping** rather than classic value-concatenation SQLi in CRUD.

### Findings

| Severity | Topic | Evidence | Notes |
|----------|-------|----------|-------|
| **High** | Unauthenticated DB admin surface when auth env unset | `crates/rusvel-api/src/auth.rs` (no token → pass-through); `crates/rusvel-api/src/lib.rs` (`/api/db/*` on same router as `bearer_auth`); `crates/rusvel-api/src/db_routes.rs` `post_sql` | Any client can list tables, read rows, and (with `read_only: false` on POST) mutate schema/data if no bearer tokens configured. Aligns with phased auth in product docs but is a **data-exfil / wipe** risk for default local “open” installs exposed beyond localhost. |
| **High** | Arbitrary SQL execution (read + optional write) | `db_routes.rs:119–157` `run_sql`; `db_routes.rs:251–280` `post_sql` | Even with auth, a holder of the **admin** token can run any statement `read_only` allows toggling. Expected for a DB console; document and restrict in hardened deployments. |
| **Medium** | No database-enforced multi-session isolation | `migrations.rs` (single shared tables); `store.rs` `ObjectStore::list` optional `json_extract(data, '$.session_id')` | Cross-session reads are possible wherever handlers omit `session_id` or use global `list`. `objects` keys are `(kind, id)` — session is not a first-class row key. |
| **Medium** | Sensitive content at rest (no field-level encryption) | `events` / `threads` / `jobs` schemas in `migrations.rs`; `store.rs` `EventStore::append`, `SessionStore::put_thread`, `JobStore::enqueue` | Full chat and job payloads persist as JSON text. Logs/traces elsewhere may echo errors containing DB or path info (see Low). |
| **Medium** | `ObjectStore` session filter uses `json_extract` without index | `store.rs:795–798` | Session-scoped object listing can degrade to table scans as `objects` grows (perf + privacy of “who scanned what” in query plans). |
| **Low** | `ORDER BY` built from partially validated string | `store.rs:416–428` `get_table_rows` | Each comma-separated segment’s **first** token is validated; the full `order` string is appended. Safer pattern is in `db_routes.rs:201–211` (known column + `ASC`/`DESC` only). Low practical impact given identifier rules but inconsistent hardening. |
| **Low** | Health endpoint may surface storage error text | `crates/rusvel-api/src/routes.rs:44–47` | `database.execute_sql("SELECT 1")` failure string returned in JSON `checks.database` — minor information leak. |
| **Info** | Migration discipline (forward-only) | `migrations.rs:8–9`, `156–168` | Ordered versions, `tracing::info!` on apply; no down migrations, no content checksums, no `IF NOT EXISTS` guard redundancy beyond individual statements. Acceptable for current scale; formalize review for prod. |
| **Info** | WAL operational assumptions | `store.rs:216–218` | WAL enabled; no `PRAGMA synchronous` tuning or documented backup procedure in crate. Restore/clone needs WAL awareness. |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| F1 | Document in operator runbook: set `RUSVEL_API_TOKEN` (and optionally `RUSVEL_API_READ_TOKEN`) for any network-exposed API; never expose `:3000` without token. | **S** | Docs + `auth.rs` crate doc |
| F2 | Add env flag to **disable** `POST /api/db/sql` or force `read_only=true` server-side in non-dev builds (`RUSVEL_DB_SQL_WRITE=0`). | **S** | `rusvel-api` / `db_routes` |
| F3 | Align `Database::get_table_rows` `order` handling with `db_routes::parse_order` (whitelist column + direction only). | **S** | `rusvel-db` |
| F4 | Add composite or generated-column index strategy for `objects(kind, json_extract(...))` **or** store `session_id` as a real column for hot kinds. | **M** | `rusvel-db` schema + migration |
| F5 | Redact or map DB errors in `/api/health` to `"error"` without internal detail when `RUSVEL_ENV=production`. | **S** | `rusvel-api` `routes.rs` |
| F6 | **Session / tenancy roadmap:** optional per-session DB files, or strict middleware that injects `session_id` into all storage calls + audit handlers for global lists. | **L** | `rusvel-app`, `rusvel-api`, ADR |
| F7 | **Encryption stance:** document “filesystem/OS volume encryption only” vs future SQLCipher or envelope encryption for selected JSON fields. | **M** | Security docs |

### Space for improvement

- **Retention:** No TTL or archival in `events`, `metrics`, or `cost_events` — long-running instances grow without bound. Consider caps, periodic prune jobs, or export-and-delete policies per session.
- **Encryption:** At-rest protection is currently whatever the host provides; `metadata` and payloads may contain API keys if users paste them into chat. Prefer env/secret stores for credentials; if required, field-level encryption would touch `Event`, `Thread`, `Job` write paths and key management (KMS or local keyfile).
- **Least-privilege:** Split tokens (`RUSVEL_API_READ_TOKEN`) already block mutating HTTP methods but **not** `POST /api/db/sql` with `read_only: true` (reads entire DB). Consider a dedicated “schema browse” scope or disabling SQL console for read-only tokens.
- **Logging:** `tracing` on migration apply is benign; ensure HTTP/trace layers do not log full SQL bodies or chat payloads in production (separate from DB, but affects privacy story).

### Handoff (for audit 07)

- **Hot tables / growth:** `events` (append-only, `ORDER BY created_at` in `EventStore::query`), `metrics` (`recorded_at` range scans), `cost_events` (analytics aggregates + `MetricStore::query_costs`), `objects` (large JSON blobs, `json_extract` session filter).
- **Job queue shape:** `jobs` — `dequeue` uses `WHERE status = ? AND (scheduled_at …) [AND kind IN (...)] ORDER BY rowid ASC LIMIT 1` then `UPDATE` to claim (`store.rs:1286–1342`). Contention and index use on `(status, scheduled_at, kind)` matter under load.
- **Full-table or heavy scans:** `cost_events_spend_snapshot_inner` runs multiple `GROUP BY` queries (`store.rs:22–100`); `list_tables` + per-table `COUNT(*)` in schema browser (`store.rs:256–274`, `rusvel-schema` `list_tables`).
- **API entry points worth profiling:** `GET /api/db/tables/{table}/rows` (paged `SELECT *` with bound `LIMIT`/`OFFSET`), `POST /api/db/sql` (arbitrary query plans), `GET /api/sessions/{id}/events` (session-filtered event stream).
