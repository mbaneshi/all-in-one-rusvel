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

- _TBD_

### Findings

| Severity | Rule / ADR | Evidence (`path:line`) | Notes |
|----------|------------|-------------------------|-------|
| | | | |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| | | S / M / L | |

### Space for improvement

- _Non-blocking ideas, tech debt, doc gaps_

### Handoff (for audits 02+)

- _Dependencies, files, or assumptions the next agent should treat as inputs_
