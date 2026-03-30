# 07 — Performance & scalability

## Context (for next agents)

- **Hot paths:** Chat SSE, embeddings + `rusvel-vector` (LanceDB), SQLite access, flow engine, harvest/content pipelines.
- **Inputs:** Handoffs from 04 (data), 05 (jobs/async), 06 (LLM cost).

---

## Agent prompt (copy below)

```
Performance audit for RUSVEL.

Identify: N+1 or unbounded queries; memory buffers; blocking in async runtimes; LLM call amplification; vector index misuse.

Propose a measurement plan (tracing/metrics/benches) and top 5 optimizations with rationale.

Fill docs/audit/agents/07-performance-scalability.md Report including fix proposals and improvement backlog.

Handoff: frontend concerns (bundle size, SSE client) for audit 09.
```

---

## Report

### Executive summary

- _TBD_

### Findings

| Severity | Topic | Evidence | Notes |
|----------|-------|----------|-------|
| | | | |

### Measurement plan

- _TBD_

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| | | S / M / L | |

### Space for improvement

- _TBD_

### Handoff (for audit 09)

- _TBD_
