# 10 — Operations, configuration & supply chain

## Context (for next agents)

- **Binary:** `rusvel-app` embeds `frontend/build` via rust-embed; config via `rusvel-config` + env.
- **Supply chain:** Rust crates + pnpm lockfile; advisories (`cargo audit`, `pnpm audit` / OSV) as available in environment.
- **Inputs:** Flaky CI or test runtime from audit 08; logging/redaction themes from 03 and 06.

---

## Agent prompt (copy below)

```
Operations and supply-chain audit for RUSVEL.

Review: config loading and validation at boot; env var documentation vs usage; logging and secret redaction; release/build path (rust-embed); dependency advisories for Rust and Node.

Output in docs/audit/agents/10-operations-supply-chain.md Report:
- Executive summary.
- Findings with severity.
- Fix proposals (secrets management, SBOM/version policy, boot validation).
- Space for improvement suitable for a solo builder but industry-sane.

Final handoff: consolidated themes to merge with audits 01–09 for a single roadmap doc (suggest filename under docs/plans/).
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

### Handoff — consolidated roadmap suggestion

- **Suggested merge doc:** _e.g. `docs/plans/audit-roadmap-YYYY-MM-DD.md`_
- **Themes to roll up from 01–09:** _TBD_
