# Test coverage strategy (Rust + frontend)

## What percentage “makes sense”

RUSVEL is a **hexagonal** Rust monorepo plus a **SvelteKit** UI. A single workspace line % mixes pure domain code with composition roots, HTTP, TUI, and thin registration glue—so **one number is a coarse health metric**, not a quality score.

| Layer | Sensible target (lines, llvm-cov) | Rationale |
|-------|-----------------------------------|-----------|
| **rusvel-core** (domain + ports) | **85–95%** | Small, pure types and registry logic; cheap to test well. |
| **Adapters** (db, llm, memory, tool, jobs) | **60–80%** | IO-heavy; use integration tests + mocks; 100% is rarely worth it. |
| **Engines** (forge, code, content, …) | **70–90%** | Business rules; already mock ports in unit tests. |
| **rusvel-api** | **50–70%** | Handler tests + a few integration tests against `Router`; full E2E is separate. |
| **rusvel-app** (binary) | **30–50%** | Mostly wiring; cover critical paths; rest via `cargo run` / API E2E. |
| **Workspace TOTAL** (llvm-cov) | **~55–65%** stretch over several quarters | Honest ceiling without gaming tests; many files are 0% until explicitly exercised. |
| **Frontend** | **Not line-% driven** | Prefer `pnpm check`, Playwright E2E/visual; optional Vitest later for lib code. |

**Current baseline (typical dev machine):** workspace line coverage **~44%**. Treat **42%** as a **regression floor** in CI until the team deliberately raises it after adding tests.

## How we improve coverage (phased)

1. **Protect the baseline** — CI runs `cargo llvm-cov test --workspace --fail-under-lines 42` so large drops fail the build.
2. **Raise the floor slowly** — When workspace line % is stable **≥45%**, bump `--fail-under-lines` to **44**, then **45**, etc. (avoid jumps that block unrelated PRs).
3. **Prioritize by risk**, not by file size:
   - Job queue, approvals, auth-adjacent paths, persistence.
   - Engines’ public APIs and error paths.
   - Pure helpers in `rusvel-core` before chasing `rusvel-app` `main.rs`.
4. **Keep integration tests narrow** — Prefer `rusvel-api` tests that hit real `Router` with in-memory or temp DB over duplicating logic in every crate.
5. **Frontend** — Add component/unit tests only for non-trivial `src/lib` helpers; route coverage via Playwright where it matters.

## Commands

```bash
# Local HTML report (opens browser where supported)
./scripts/coverage.sh

# Summary table only (fast feedback)
./scripts/coverage.sh --summary-only

# Match CI gate
./scripts/coverage.sh --summary-only --fail-under-lines 42
```

HTML output: `target/llvm-cov/html/index.html`.

### Boot benchmarks (Criterion, not coverage)

Cold-ish startup slice (SQLite WAL open + migrations; department registry load fallback). No CI gate.

```bash
cargo bench -p rusvel-app --bench boot
```

## CI

See `.github/workflows/ci.yml`: **llvm-tools-preview**, **cargo-llvm-cov**, same **protoc** install as build, then tests run under coverage with the floor above. Playwright behavioral E2E and prioritized backlog: [roadmap.md](roadmap.md).

