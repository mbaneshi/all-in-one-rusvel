# 08 — Testing, CI & quality gates

## Context (for next agents)

- **Stack:** `cargo test` workspace-wide, `rusvel-api` integration tests, engine tests (often mock ports), `frontend` Playwright/visual (`pnpm`).
- **Inputs:** Security audit 03 listed routes/tools needing tests; reconcile here.

---

## Agent prompt (copy below)

```
Testing and quality-gates audit for RUSVEL.

Map test pyramid by area; list critical untested paths (auth, jobs, webhooks, MCP, tools).

Recommend CI gates: fmt, clippy, cargo deny (if applicable), pnpm check, minimal e2e slice.

Write docs/audit/agents/08-testing-ci-quality.md Report with a coverage-gap matrix, fix proposals (tests + CI), and improvement ideas.

Handoff: flaky or slow tests affecting ops (audit 10).
```

---

## Report

### Executive summary

- _TBD_

### Coverage / gap matrix

| Area | Covered? | Gap | Suggested test |
|------|----------|-----|----------------|
| | Y/N/Partial | | |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| | | S / M / L | |

### Space for improvement

- _TBD_

### Handoff (for audit 10)

- _Flaky/slow tests, CI runtime_
