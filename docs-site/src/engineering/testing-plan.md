# Testing Plan

RUSVEL targets **~1200+ tests** across Rust and frontend, with staged workspace coverage toward 55-65%. This page summarizes the strategy; the full checklist lives in [`docs/plans/comprehensive-testing-plan.md`](https://github.com/mbaneshi/rusvel/blob/main/docs/plans/comprehensive-testing-plan.md).

## Current state

| Area | Count | Notes |
|------|-------|-------|
| Rust tests (workspace sum) | ~645 | Sum of `running N tests` from `cargo test --workspace`; see `docs/status/current-state.md` |
| Rust integration tests | ~92 | Strong API smoke tests, engine round-trips |
| Benchmarks | 2 | Criterion: DB open + registry load |
| Frontend visual tests | 27 routes | Playwright screenshot comparison |
| Frontend unit tests | 0 | No Vitest setup yet |
| Property-based tests | 0 | No proptest yet |
| Fuzz tests | 0 | No cargo-fuzz targets yet |
| Snapshot tests | 0 | No insta snapshots yet |

**CI coverage floor:** 42% (cargo-llvm-cov, `--fail-under-lines`).

## Testing pyramid

```
                    /\
                   /  \          E2E (Playwright)
                  / 27  \        Visual regression + interaction flows
                 /--------\
                /          \     Integration Tests
               /   ~150     \    API handlers, engine pipelines, cross-crate
              /--------------\
             /                \   Unit Tests
            /     ~800+        \  Domain types, port impls, utils, components
           /--------------------\
          /                      \  Property + Fuzz
         /        ~50+            \  Parsers, serialization, scoring
        /__________________________\
```

Most tests at the unit level. Integration tests for cross-boundary flows. E2E for critical user journeys only.

## Sprint overview

| Sprint | Theme | New Tests |
|--------|-------|-----------|
| 0 | Baseline capture | 0 (measurement) |
| 1 | Rust unit tests (rusvel-core to 90%, all crates > 50%) | ~200 |
| 2 | Frontend Vitest setup + api.ts + stores | ~100 |
| 3 | API negative path tests (400/401/404/405 for all routes) | ~120 |
| 4 | Rust engine contract tests (13 engines) | ~80 |
| 5 | Snapshot tests (insta) + property-based tests (proptest) | ~70 |
| 6 | Frontend component tests (@testing-library/svelte) | ~60 |
| 7 | Benchmarks (Criterion) + CI hardening | ~15 |
| 8 | Fuzz testing (cargo-fuzz, nightly) | ~8 |

## Coverage targets by layer

These align with [`docs/testing/coverage-strategy.md`](https://github.com/mbaneshi/rusvel/blob/main/docs/testing/coverage-strategy.md):

| Layer | Current | Target |
|-------|---------|--------|
| rusvel-core | ~60% | 85-95% |
| Engines (13) | ~50% | 70-90% |
| Adapters | ~35% | 60-80% |
| rusvel-api | ~45% | 50-70% |
| rusvel-app | ~25% | 30-50% |
| Frontend (lib/) | 0% | 50% |
| **Workspace total** | **~42%** | **~55-65%** |

## Test infrastructure

### Rust

- **Test harness:** [`crates/rusvel-api/tests/common/mod.rs`](https://github.com/mbaneshi/rusvel/blob/main/crates/rusvel-api/tests/common/mod.rs) provides `TestHarness` with temp SQLite, stub LLM, mock platform adapters
- **Mock pattern:** Port-based mocking via `FakeStorage`, `RecordingEvents`, `StubLlm` structs
- **Framework:** `tokio` async runtime, `tempfile` for isolation, `criterion` for benchmarks
- **Coverage:** `cargo-llvm-cov` with HTML reports via `./scripts/coverage.sh`

### Frontend

- **Visual regression:** Playwright + Claude Vision analysis (27 routes)
- **Unit tests (planned):** Vitest + @testing-library/svelte
- **Test data:** Global setup seeds sessions and goals via API

## Key principles

1. **Port-based mocking** -- engines receive `Arc<dyn Trait>` mocks, never real adapters in unit tests
2. **No mocking the database** in integration tests -- use temp SQLite for realistic behavior
3. **Negative paths are required** -- every API endpoint needs 400, 401, 404, 405 tests
4. **Property tests for parsers** -- proptest on code-engine parser, harvest scoring, cron expressions
5. **Snapshots for serialization** -- insta catches accidental field renames in API responses

## Running tests

```bash
# Rust
cargo test                          # Full workspace
cargo test -p rusvel-core           # Single crate
cargo bench -p rusvel-app --bench boot  # Benchmarks
./scripts/coverage.sh               # HTML coverage report

# Frontend
cd frontend
pnpm test                           # Vitest unit tests (planned)
pnpm test:visual                    # Playwright visual regression
pnpm test:e2e                       # All Playwright tests
pnpm test:coverage                  # Vitest coverage (planned)
```
