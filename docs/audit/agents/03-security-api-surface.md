# 03 — Security (application & API surface)

## Context (for next agents)

- **Surfaces:** `rusvel-api` (Axum), optional bearer auth from env, webhooks, hooks, MCP (`rusvel-mcp`), builtin tools (file, git, bash, browser).
- **Trust model:** Document phased auth in `docs/plans/` if referenced; treat “open by default” as a finding unless explicitly scoped.
- **Inputs:** Handoff from audit 02 (which modules own sensitive operations).

---

## Agent prompt (copy below)

```
Security audit of RUSVEL (in-repo).

Review: rusvel-auth, rusvel-api middleware/route protection, CORS, file path handling in rusvel-builtin-tools, terminal/shell execution paths, webhook + hook_dispatch, MCP HTTP/stdio.

Deliver:
- Threat model sketch: actor → entrypoint → asset (1 short section).
- Findings: severity | threat | evidence | current mitigation | gap.
- Fix proposals with effort S/M/L.
- Space for improvement (hardening backlog).

Record everything in docs/audit/agents/03-security-api-surface.md Report section.

Handoff: explicit list of routes or tools needing regression tests for audit 08.
```

---

## Report

### Executive summary

- _TBD_

### Threat model (sketch)

- **Actors:** _TBD_
- **Entrypoints:** _TBD_
- **Assets:** _TBD_

### Findings

| Severity | Threat | Evidence | Mitigation today | Gap |
|----------|--------|----------|------------------|-----|
| | | | | |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| | | S / M / L | |

### Space for improvement

- _TBD_

### Handoff (for audit 08 — tests)

- _Routes / tools / flows needing test coverage_
