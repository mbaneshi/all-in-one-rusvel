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

- **Test pyramid (RUSVEL):** broad **unit / engine** base (`cargo test`, mock ports in `*-engine` + `rusvel-core`); **mid layer** is `rusvel-api` integration tests against a real `Router` + temp/in-memory DB (CRUD, smoke, targeted E2E files); **narrow top** is Playwright (`pnpm test:e2e`, visual subset) — **not run in GitHub CI today** (CI only `pnpm build`).
- **Strongest automated coverage:** bearer middleware (unit + `tests/auth_bearer.rs`), webhooks + cron + flow trigger (`webhooks_e2e.rs`, `webhook_cron_e2e.rs`), GTM outreach / approvals (`outreach_*`, `jobs_list.rs`), harvest → proposal job path (`harvest_to_proposal.rs`), MCP **server registry** CRUD (`mcp_servers_crud.rs`).
- **Largest quality holes vs risk:** **MCP stdio server** (`rusvel-mcp` / `--mcp`) has no crate tests in-tree; **full job worker** in `rusvel-app` is mostly indirect (mirrored in API tests, not the binary loop); **tool execution** paths are thinly covered at integration level (builtin/tool crates have some unit tests, not end-to-end through `AgentPort`); **CI** omits `fmt`, `clippy`, `pnpm check`, and any E2E slice — only `cargo build`, `llvm-cov` with **42%** line floor, and frontend build.

### Test pyramid (by layer)

| Layer | What runs | Role |
|-------|-----------|------|
| **Unit / crate** | `cargo test` per crate | Engines with mock ports; core types; adapter logic; `rusvel-api` `#[cfg(test)]` (e.g. `auth.rs`) |
| **Integration (API)** | `crates/rusvel-api/tests/*.rs` | Full stack slice: sessions, departments, chat, flows, webhooks, jobs list, forge pipeline, outreach |
| **System / E2E** | `frontend` Playwright | UI + (when configured) live API; visual regression separate project |
| **CI today** | Rust build + llvm-cov 42% floor; `pnpm install` + `pnpm build` | No fmt/clippy/deny; no `pnpm check`; no Playwright |

### Recommended CI gates

| Gate | Status in `.github/workflows/ci.yml` | Recommendation |
|------|--------------------------------------|----------------|
| `cargo fmt --all -- --check` | Not present | **Add** — matches `just fmt-check`; cheap, high signal |
| `cargo clippy --workspace -- -D warnings` | Not present | **Add** — matches `just lint`; catches API misuse / unused |
| **cargo-deny** | No `deny.toml` in repo | **Optional** — add `deny.toml` + `cargo deny check` if license/advisory policy is desired; skip until policy defined |
| `pnpm check` (Svelte/TS) | Not present (only `pnpm build`) | **Add** in `frontend` job — catches type errors build might not |
| **Minimal E2E** | Not present | **Add** optional job: install Playwright browsers, run **one** smoke spec (e.g. health / login shell) against `cargo run` or preview; keep **visual** tests manual or nightly to control flake/cost |
| **Keep** | `cargo llvm-cov test --fail-under-lines 42` | Retain; align bumps with `docs/testing/coverage-strategy.md` |

### Coverage / gap matrix

| Area | Covered? | Gap | Suggested test |
|------|----------|-----|----------------|
| **Auth — bearer middleware** | **Y** | Exempt paths / webhook receive edge cases could drift vs `EXEMPT_PATHS` | Golden tests when adding routes; one integration test per new exempt pattern |
| **Auth — full API with real router** | **Partial** | `auth_bearer.rs` covers harness; not every route under auth matrix | Spot-check high-risk `POST` routes with read-only token (403) |
| **Jobs — queue API** | **Partial** | `jobs_list.rs`, outreach/harvest flows | `JobKind::Custom("forge.pipeline")` enqueue + worker idempotency; `ContentPublish` / `CodeAnalyze` happy paths if not already |
| **Jobs — `rusvel-app` worker loop** | **Partial** | Logic mirrored in API tests (`common/outreach.rs`, harvest worker helpers) | Thin `rusvel-app` test or subprocess smoke calling one job kind |
| **Webhooks — register + HMAC ingest** | **Y** | Forged signatures, replay, oversized body | Fuzz / property-style negative tests on `POST /api/webhooks/{id}` |
| **Webhooks — dispatch → jobs / flows** | **Partial** | `webhook_cron_e2e.rs` covers cron + flow trigger | Explicit test for `forge.pipeline.requested` → job enqueue matches production wiring |
| **MCP — HTTP CRUD** | **Y** | Only persistence API | — |
| **MCP — stdio JSON-RPC server** | **N** | No `#[test]` in `rusvel-mcp` (grep) | Golden transcript test: stdin JSON → stdout responses for `initialize` + one tool list |
| **Tools — registry / validation** | **Partial** | `rusvel-tool` + `rusvel-builtin-tools` have some unit tests | Integration: agent loop with 1–2 builtin tools against mock `LlmPort` |
| **Tools — engine tools (`rusvel-engine-tools`)** | **Partial** | Exercised indirectly via engines | Contract tests per tool schema + error JSON |
| **Frontend** | **Partial** | CI does not run `pnpm check` or Playwright | Add `pnpm check` to CI; minimal Playwright project for regression |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| T1 | Add CI steps: `cargo fmt --check`, `cargo clippy -D warnings` before/after build | S | DevOps / `.github/workflows/ci.yml` |
| T2 | Add `pnpm check` to `frontend` job after `pnpm install` | S | CI + frontend |
| T3 | Introduce `deny.toml` + `cargo deny check` job (licenses + advisories) when policy ready | M | Platform |
| T4 | Minimal Playwright job: single smoke test, no visual snapshots in PR CI | M | Frontend + API |
| T5 | `rusvel-mcp`: stdin/stdout protocol smoke test (no network) | M | MCP crate |
| T6 | Integration test: webhook `forge.pipeline.requested` body → `JobKind::Custom` queued | S | `rusvel-api` tests |
| T7 | Expand read-token matrix on mutating department routes | M | `rusvel-api` tests |

### Space for improvement

- **Split Rust CI job** (fmt + clippy fast, then build+cov) for clearer failure signals and caching.
- **Track wall time** per `rusvel-api` integration file; mark `@slow` or separate job if runtime grows (see audit 10).
- **Document** which routes are auth-exempt next to `EXEMPT_PATHS` to avoid security regressions.
- **Raise llvm-cov floor** only when baseline is stable per `docs/testing/coverage-strategy.md` (avoid blocking unrelated PRs).

### Handoff (for audit 10)

- **Flaky / slow tests:** Flag any `rusvel-api/tests/*` that spawn full harness + LLM stubs or multi-step flows (`harvest_to_proposal`, `outreach_e2e`, `webhook_cron_e2e`, `forge_pipeline_api`) for runtime variance; Playwright visual tests are high flake/cost — keep out of default PR CI unless stabilized.
- **CI runtime:** Full workspace `llvm-cov` + `protoc` + future browsers will dominate minutes — audit 10 should propose **parallel jobs** and **nightly** vs **PR** split.
- **Ops noise:** If clippy/fmt are added, expect a one-time burst of fixes; schedule or use `allow` with tickets for noisy lints.
