# Maintaining RUSVEL documentation

Canonical written truth for **what the repo does today** lives in [`docs/status/current-state.md`](../status/current-state.md), indexed by [`docs/README.md`](../README.md). Claim-to-evidence audits use dated logs such as [`docs/status/verification-log-2026-03-30.md`](../status/verification-log-2026-03-30.md). When you edit metrics anywhere public (README, mdBook, `PROJECT_CONTEXT.md`), copy numbers from `current-state` §1 and its metric definitions table, or link there instead of guessing.

## Prompt for humans and agents

Use when updating docs:

> You are updating RUSVEL documentation. **Do not invent metrics.** Open [`docs/status/current-state.md`](../status/current-state.md) and copy numbers only from §1 and the metric definitions table. For anything that might drift (crate count, LOC, route count, test count), either quote `current-state` verbatim or link to it. If a document is historical (phase milestone, dated audit), add a banner: date + “not updated for current metrics; see `docs/status/current-state.md`.”

## Drift detection (search)

From the repository root:

```bash
# Stale crate / workspace wording
rg -n '48 crate|49 crate|50 workspace members|54 workspace members|54 crates\)' --glob '*.md' .

# Stale LOC / file counts
rg -n '43,670|52,560|185 source|215 Rust' --glob '*.md' .

# Stale API surface counts
rg -n '105.*\.route|124 handler|23 module|141.*\.route\(|141 route chains|36 handler modules' --glob '*.md' .
```

Re-verify commands (also in `current-state`):

```bash
cargo build
cargo test
cargo metadata --format-version 1 --no-deps | python3 -c "import json,sys; print(len(json.load(sys.stdin)['workspace_members']))"
find crates -name '*.rs' | wc -l
wc -l $(find crates -name '*.rs') | tail -1
rg '\.route\(' crates/rusvel-api/src/lib.rs | wc -l
```

## Published mdBook vs `docs/`

The site under [`docs-site/`](../../docs-site/) is a curated subset. Deep design, plans, and audits stay in [`docs/`](../README.md). If the book and `current-state` disagree, **refresh the book** from `current-state` or link to the GitHub raw file.
