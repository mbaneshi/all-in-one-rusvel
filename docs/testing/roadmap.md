# Testing roadmap — reliability backlog

Canonical **coverage targets and llvm-cov floor** live in [coverage-strategy.md](coverage-strategy.md). This file tracks **what to add next** for stronger end-to-end and operational confidence.

## Current gates (CI)

- Rust: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build --workspace`, `cargo llvm-cov test` with **`--fail-under-lines 42`** (see coverage-strategy).
- Frontend: `pnpm check`, `pnpm build`.
- Playwright **behavioral** E2E (`*.e2e.ts`): separate CI job builds `rusvel` then runs `pnpm exec playwright test --project=e2e` (see [.github/workflows/ci.yml](../../.github/workflows/ci.yml)).
- `cargo audit`: advisory job (`continue-on-error`); tighten when allowlist is ready.

## P0 — Highest impact

1. **Expand E2E** — Add routes and flows beyond [frontend/tests/smoke.e2e.ts](../../frontend/tests/smoke.e2e.ts): chat/SSE smoke, one CRUD path, authenticated API if bearer is enabled in test env.
2. **Job queue + worker** — Integration tests for dequeue contention, stale-running sweep, retry-on-fail, invalid payload → `fail()` (extend `rusvel-api` / `rusvel-db` tests).
3. **Auth matrix** — Read vs write token, optional bearer, RusvelBase SQL with `RUSVEL_DB_SQL_WRITE`.

## P1 — Depth

4. **`rusvel-app` boot** — Temp-dir boot smoke or minimal harness for critical CLI flags without duplicating every department.
5. **Adapter contracts** — Migration upgrade tests, LLM mock contracts per provider surface.
6. **Department apps** — Shared `DepartmentApp` contract tests for depts that are mostly registration glue.

## P2 — Sustainability

7. **Raise llvm-cov floor** — When workspace line % is stable above ~45%, bump `--fail-under-lines` incrementally (per coverage-strategy).
8. **Staging smoke** — Periodic health + one authenticated API call outside Rust (monitoring).
9. **Blocking `cargo audit`** — After noise review, fail CI with a committed allowlist.
10. **Frontend unit tests (Vitest)** — Only for non-trivial `src/lib` helpers; not a substitute for Playwright.

## Local commands

```bash
cargo test --workspace
cd frontend && pnpm check && pnpm build
# E2E (API + Vite started by Playwright; CI uses prebuilt binary)
cd frontend && pnpm exec playwright test --project=e2e
# Match CI binary path
cargo build -p rusvel-app && cd frontend && CI=true pnpm exec playwright test --project=e2e
```

Visual regression: `pnpm test:visual` (not run in CI by default — OS/renderer baselines).
