# 01 — Architecture & ADR compliance

## Context (for next agents)

- **App:** RUSVEL — Rust monorepo + SvelteKit frontend; hexagonal layout per `docs/design/architecture-v2.md` and ADRs in `docs/design/decisions.md`.
- **Non-negotiables to verify:** Engines depend only on `rusvel-core` traits; no engine→adapter imports; engines use `AgentPort`, not `LlmPort` directly (ADR-009); domain types carry `metadata: serde_json::Value` (ADR-007); `Event.kind` is `String` (ADR-005); single job queue (ADR-003); human approval gates where ADR-008 applies.
- **Prior art:** `docs/audit/audit-2026-03-28.md` may already list violations — reconcile, don’t duplicate without checking dates.
- **Handoff:** List any **rule → pass/fail** table and open ADR drift for downstream security/perf audits.

---

## Agent prompt (copy below)

```
You are auditing RUSVEL in-repo only unless an external cite is required.

Scope: docs/design/architecture-v2.md, docs/design/decisions.md, crates/*-engine/, rusvel-core/src/ports.rs, dept-* wiring.

Verify:
- No engine crate imports adapter crates (use ripgrep / cargo tree reasoning).
- No engine calls LlmPort directly; AgentPort only.
- ADR-007 metadata on domain types; ADR-005 event kinds; ADR-003 single queue; ADR-008 approval surfaces.

Deliver in the Report section of docs/audit/agents/01-architecture-adr-compliance.md:
- Executive summary (5 bullets max).
- Findings table: severity | rule/ADR | evidence (path:line) | notes.
- Fix proposals: id, proposal, estimated effort S/M/L, suggested crate/owner.
- Space for improvement: non-blocking enhancements.
- Handoff: what audits 02–03 must re-check because of your conclusions.

Use ripgrep; cite paths. If uncertain, say what command would confirm.
```

---

## Report

### Executive summary

- **ADR-010 (engines → adapters):** All 13 `crates/*-engine` manifests list only `rusvel-core` plus ecosystem crates (e.g. `reqwest`, `tree-sitter`, `petgraph`); `rg` finds no `use rusvel_(db|llm|agent|channel|…)` under `*-engine/**/*.rs`.
- **ADR-009 (`LlmPort` vs `AgentPort`):** Zero `LlmPort` references under `*-engine/**/*.rs` (`rg 'LlmPort' …` exits with no matches). Engines that need models take `Arc<dyn AgentPort>` (e.g. `forge-engine/src/lib.rs:40`, `content-engine/src/lib.rs:63`, `flow-engine/src/lib.rs:42`, `gtm-engine/src/lib.rs:41`). `code-engine` uses neither port (static analysis only) — consistent with “use AgentPort when you need an agent.”
- **ADR-005 / ADR-003 / ADR-008:** `Event.kind` is `String` (`rusvel-core/src/domain.rs:901-906`). Central `JobPort` is the shared queue in composition root (`rusvel-app/src/main.rs:796-797`); `rusvel-cron` enqueues via the same port (`rusvel-cron/src/lib.rs:281`). Approvals surface: `JobStatus::AwaitingApproval` (`domain.rs:635-636`), HTTP `GET/POST /api/approvals*` (`rusvel-api/src/lib.rs:466-468`).
- **ADR-007:** Many domain structs carry `metadata`; several serde structs still omit it while `domain.rs:3-4` claims every struct does — treat as **partial** compliance (examples below).
- **Reconciliation with `docs/audit/audit-2026-03-28.md`:** §1.1 “`ChannelPort` in adapter crate” is **stale** — trait is in `rusvel-core/src/ports.rs:558-562`; `rusvel-channel/src/lib.rs:5` re-exports it. §2.1 “9 structs missing metadata” is **stale** for the listed brief/code/vector/tab/flow-connection types — those now have `metadata` in `domain.rs` (e.g. `ExecutiveBrief:877`, `RepoRef:960`, `VectorSearchResult:1321`, `TabInfo:1354`, `FlowConnectionDef:1470`).

### Findings

| Severity | Rule / ADR | Evidence (`path:line`) | Notes |
|----------|------------|-------------------------|-------|
| Pass | ADR-010 | `crates/forge-engine/Cargo.toml:11`; `crates/harvest-engine/Cargo.toml:8`; (same pattern for all `*-engine/Cargo.toml`) | No `rusvel-db`, `rusvel-llm`, `rusvel-agent`, etc. in engine dependencies. |
| Pass | ADR-010 (imports) | Shell: `rg 'use rusvel_(db\|llm\|agent\|channel\|vector\|embed\|tool\|event\|memory\|jobs\|builtin)' crates --glob '*-engine/**/*.rs'` → no matches | Confirms no direct adapter crate paths in engine sources. |
| Pass | ADR-009 | Shell: `rg 'LlmPort' crates --glob '*-engine/**/*.rs'` → no matches | Engines do not name `LlmPort`. |
| Pass | ADR-009 | `crates/forge-engine/src/lib.rs:40`; `crates/content-engine/src/lib.rs:15,63`; `crates/harvest-engine/src/lib.rs:11,91`; `crates/flow-engine/src/lib.rs:12,42`; `crates/gtm-engine/src/lib.rs:9,41` | `AgentPort` injected where AI paths exist. |
| Pass | ADR-005 | `crates/rusvel-core/src/domain.rs:901-906` | `Event { … pub kind: String … }` with ADR comment at `891-899`. |
| Pass | ADR-003 | `crates/rusvel-app/src/main.rs:796-797`; `crates/rusvel-cron/src/lib.rs:55-59,281` | Single `JobPort` instance shared; cron enqueues through it, not a parallel queue implementation. |
| Pass | ADR-008 | `crates/rusvel-core/src/domain.rs:1007-1027,629-636`; `crates/rusvel-api/src/lib.rs:466-468`; `crates/rusvel-api/src/dashboard.rs:47-76` | Core types + REST list/approve/reject + dashboard pending approvals. |
| Info | Dept wiring (not ADR-010) | `crates/dept-forge/Cargo.toml:11-12`; `crates/dept-messaging/Cargo.toml:12-13` | `dept-*` may depend on engines and, for messaging shell, `rusvel-channel` — expected at wrapper layer, not in `*-engine`. |
| Low | ADR-007 | `crates/rusvel-core/src/domain.rs:3-4` vs e.g. `19-21` (`Content`), `424-431` (`StarterKit`), `520-526` (`SessionSummary`), `573-577` (`ThreadMessage`), `914-921` (`EventFilter`), `925-935` (`EventTrigger`), `1072-1076` (`ToolHookConfig`), `1527-1533` (`FlowNodeResult`), `1541-1555` (`FlowCheckpoint`), `1604-1613` (`PlaybookRun`), `1222-1228` (`UserProfile`) | Module doc asserts all structs carry `metadata`; many DTO/filter/playbook types do not. Either narrow the doc to “persisted domain records” or add `#[serde(default)] metadata` to remaining structs. |
| Info | Doc / metrics drift | `docs/design/architecture-v2.md:33` (“12 depts”) vs `decisions.md:111` / registry (14 departments) | Architectural doc undercounts departments; unrelated to engine boundaries but confuses audits. |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| FP-1 | Align ADR-007 with reality: either add `metadata: serde_json::Value` (with `#[serde(default)]`) to remaining `domain.rs` structs listed in Findings, or revise `domain.rs:3-4` to define which categories must carry metadata. | M | `rusvel-core` |
| FP-2 | Update `docs/design/architecture-v2.md` department count (12 → 14) and any stale “6 wired + 7 ext” wording if registry changed, so future audits do not re-open closed items. | S | `docs/design` |
| FP-3 | Add a short errata note to `docs/audit/audit-2026-03-28.md` §1.1 and §2.1 pointing at this report (ChannelPort moved to core; metadata added to formerly flagged structs) — optional archival hygiene. | S | `docs/audit` |

### Space for improvement

- **Constants for event kinds / dept ids** (`audit-2026-03-28.md` §2.2): still a maintainability win; not an ADR violation by itself.
- **`just check-boundaries` / CI:** If present, cite in future audits as the canonical engine→adapter guard; this run used `rg` + `Cargo.toml` inspection only.
- **Extended engines:** `content-engine` / `harvest-engine` use `reqwest` for HTTP — not rusvel adapters; keep distinguishing “third-party HTTP” from “hex boundary” in reviews.

### Handoff (for audits 02+)

- Treat **ADR-010/009 as satisfied for `*-engine`** unless new engine crates appear; re-verify with `rg` + `Cargo.toml` on any engine PR.
- **Security / perf audits:** ADR-008 approval HTTP surface is `rusvel-api/src/approvals.rs` + routes in `lib.rs:466-468`; job worker holds are in `rusvel-app` (e.g. `main.rs` around `enqueue` / outreach). ADR-003 worker concurrency is composition-root concern, not engine crates.
- **`audit-2026-03-28.md`:** Do not re-file §2.1 metadata bugs for `ExecutiveBrief`, `RepoRef`, `CodeAnalysisSummary`, `DeployedUrl`, `DeployStatus`, `VectorSearchResult`, `TabInfo`, `FlowConnectionDef` without re-reading `domain.rs` — fixed since that audit.
- **ADR-007 follow-up:** If FP-1 chooses “add metadata everywhere,” regression-check serde defaults for API/DB roundtrips; if “narrow doc,” update any consumer assumptions in OpenAPI or frontend types.
