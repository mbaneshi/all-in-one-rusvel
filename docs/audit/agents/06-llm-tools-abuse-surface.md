# 06 — LLM, tools & abuse surface

## Context (for next agents)

- **Stack:** `rusvel-agent`, `rusvel-tool` / `ScopedToolRegistry`, `rusvel-engine-tools`, MCP client, streaming LLM providers in `rusvel-llm`.
- **Risks:** Tool exfiltration, prompt injection via untrusted content, credential leakage in logs/events, cost blowups (ModelTier / streaming).

---

## Agent prompt (copy below)

```
Audit LLM + tool surface for RUSVEL.

Review: tool registration, scoping per department, MCP tool dispatch, logging of prompts/responses, human approval gates (tie to ADR-008 where relevant).

Produce in docs/audit/agents/06-llm-tools-abuse-surface.md Report:
- Executive summary.
- Findings: injection/abuse/cost/privacy — with evidence paths.
- Fix proposals (guardrails, allowlists, rate limits, redaction).
- Space for improvement.

Handoff: APIs/UI flows that must stay aligned for audit 09.
```

---

## Report

### Executive summary

- _TBD_

### Findings

| Severity | Category (injection / abuse / cost / privacy) | Evidence | Notes |
|----------|-----------------------------------------------|----------|-------|
| | | | |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| | | S / M / L | |

### Space for improvement

- _TBD_

### Handoff (for audit 09)

- _UI/API flows_
