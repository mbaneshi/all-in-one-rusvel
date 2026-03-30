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

- _Job kinds / async hotspots_
