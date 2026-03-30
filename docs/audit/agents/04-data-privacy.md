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

- _TBD_

### Findings

| Severity | Topic | Evidence | Notes |
|----------|-------|----------|-------|
| | | | |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| | | S / M / L | |

### Space for improvement

- _TBD_

### Handoff (for audit 07)

- _Hot tables / query patterns_
