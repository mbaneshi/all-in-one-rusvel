# 02 — Boundaries & dependency hygiene

## Context (for next agents)

- **Composition root:** `crates/rusvel-app` wires stores, engines, departments, API/MCP/TUI.
- **Pattern:** `dept-*` implements `DepartmentApp`; engines sit on `rusvel-core` ports only.
- **Convention:** Crate line budget ~2000 lines per `CLAUDE.md`.
- **Handoff:** Circular deps, “god modules,” and API handlers doing domain logic feed into audits 03 (security) and 07 (performance).

---

## Agent prompt (copy below)

```
Audit RUSVEL boundary hygiene.

Tasks:
1. Summarize how rusvel-app composes dept-* → engines → ports → adapters (short diagram or bullet tree).
2. Reason about cargo workspace cycles (cargo tree -i on suspicious edges if needed).
3. Flag crates near or over ~2000 lines; note responsibility blur.
4. Find API-layer logic that belongs in engines (rusvel-api handlers vs engine methods) — sample 3–5 cases if any.

Write results into docs/audit/agents/02-boundaries-dependencies.md under Report using the template tables.

Include Fix proposals and Space for improvement. Handoff: list modules the security auditor must re-trace.
```

---

## Report

### Executive summary

- _TBD_

### Findings

| Severity | Topic | Evidence (`path:line` or command) | Notes |
|----------|-------|----------------------------------|-------|
| | | | |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| | | S / M / L | |

### Space for improvement

- _TBD_

### Handoff (for audits 03+)

- _TBD_
