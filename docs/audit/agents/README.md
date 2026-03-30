# Agent audit pack — RUSVEL

**Purpose:** Industry-style audits split into ten runs. Each numbered markdown file gives **context** (for the next agent), a **copy-paste prompt**, and a **report scaffold** (findings, fix proposals, improvement space).

**Where outputs live:** Edit the same file under **Report** after each run, or duplicate the file to `docs/audit/agents/runs/YYYY-MM-DD-NN-topic.md` if you want history without overwriting templates.

**Handoff rule for “next agents”:** Before starting audit `N+1`, read this README plus the **Handoff** subsection (if filled) in prior reports so assumptions stay consistent.

**Suggested severity:** Critical | High | Medium | Low | Info

**Evidence:** Always cite `path/to/file.rs:line` or paste command output snippets.

## Index

| # | File | Topic |
|---|------|--------|
| 01 | [01-architecture-adr-compliance.md](./01-architecture-adr-compliance.md) | Hexagonal + ADRs |
| 02 | [02-boundaries-dependencies.md](./02-boundaries-dependencies.md) | Composition root, deps, crate size |
| 03 | [03-security-api-surface.md](./03-security-api-surface.md) | Auth, tools, webhooks, MCP |
| 04 | [04-data-privacy.md](./04-data-privacy.md) | DB, migrations, PII |
| 05 | [05-jobs-async-reliability.md](./05-jobs-async-reliability.md) | Queue, workers, spawn_blocking |
| 06 | [06-llm-tools-abuse-surface.md](./06-llm-tools-abuse-surface.md) | Agent, tools, cost, injection |
| 07 | [07-performance-scalability.md](./07-performance-scalability.md) | Hot paths, DB, vectors |
| 08 | [08-testing-ci-quality.md](./08-testing-ci-quality.md) | Tests, gates, gaps |
| 09 | [09-frontend-sveltekit.md](./09-frontend-sveltekit.md) | UI, API contract, a11y |
| 10 | [10-operations-supply-chain.md](./10-operations-supply-chain.md) | Config, logging, deps audit |

## After all ten

Merge **Findings** and **Fix proposals** into a single roadmap (dedupe by theme: security, architecture, performance, reliability, tests, ops). Sort by severity × impact × effort.
