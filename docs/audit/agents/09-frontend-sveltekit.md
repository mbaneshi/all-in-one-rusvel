# 09 — Frontend (SvelteKit) engineering & product

## Context (for next agents)

- **Stack:** `frontend/` — SvelteKit 5, Tailwind 4, **pnpm only** (not npm). API base aligns with `rusvel-api`.
- **Inputs:** Handoffs from 06–07 (SSE, perf); security themes from 03 (tokens in client, XSS).

---

## Agent prompt (copy below)

```
Audit RUSVEL frontend under frontend/.

Check: API contract vs rusvel-api; error handling; auth header usage; SSE client robustness; basic a11y; XSS sinks; secrets in client bundle; Playwright/visual stability.

Document in docs/audit/agents/09-frontend-sveltekit.md Report with separate bullets for UX debt vs engineering debt.

Fix proposals + improvement space. Handoff: env/build concerns for audit 10.
```

---

## Report

### Executive summary

The SvelteKit client is largely aligned with `rusvel-api` paths and shapes via a single `frontend/src/lib/api.ts` surface, with **no `Authorization: Bearer` wiring**—so enabling `RUSVEL_API_TOKEN` / `RUSVEL_API_READ_TOKEN` breaks the UI unless extended. Error handling is **inconsistent**: shared `request()` throws on non-OK, but many `DELETE` helpers and `getProfile`/`updateProfile` ignore failures or parse JSON blindly. **SSE** is centralized in `parseSSE()` with simple line splitting; there is no abort/reconnect and malformed lines are dropped silently. **A11y** is partial (shell components, command palette, some dept pages use roles/labels) but not systematic. **XSS**: assistant markdown flows through `svelte-streamdown` (`Streamdown`)—treat as HTML-capable output from the model; only one static `{@html}` use in onboarding styles. **Secrets**: no `VITE_*` usage found in the repo; GitHub PAT goes to the API over HTTPS/same-origin, not embedded in the bundle. **Playwright** correctly uses **`pnpm dev`** in `playwright.config.ts`; visual coverage **omits** `flow` and `messaging` departments; test `localStorage` key `rusvel-active-session` is **not read by the app**, so session selection in tests is brittle (depends on API ordering / single session).

### Findings

| Severity | Topic | Evidence | Notes |
|----------|-------|----------|-------|
| High | Auth header gap vs API | `rusvel-api` `auth.rs`: bearer required on `/api/*` except `/api/health` and webhook POST receive; `frontend/src/lib/api.ts`: no `Authorization` on `fetch`/`request` | Enabling API tokens bricks the SPA until the client sends `Bearer` (env-injected build-time token is a product/security tradeoff; cookie/session is future per ADR docs). |
| Medium | Error handling / contract | `deleteAgent`, `deleteSkill`, `deleteRule`, `deleteMcpServer`, `deleteHook`, `deleteWorkflow`, `deleteKnowledge`, `deleteFlow`: `fetch` without `res.ok` check; `getProfile`/`updateProfile`: `res.json()` without status check | Silent failures in UI; error JSON may be parsed as profile. |
| Medium | SSE robustness | `parseSSE` in `api.ts`: ignores `event:` lines; splits only on `\n` (no `\r\n` normalization); JSON parse errors swallowed; no `AbortSignal`; stream end without `run_completed` leaves UI ambiguous | Long streams / proxies / partial chunks risk stuck or empty states. |
| Low | Dev API base vs Vite proxy | `BASE = import.meta.env.DEV ? 'http://localhost:3000' : ''`; `vite.config.ts` proxies `/api` to 3000 | Dev bypasses same-origin proxy (CORS must allow); prod embedded SPA uses relative URLs—correct for rust-embed. |
| Low | API contract drift risk | Large hand-maintained path list in `api.ts` vs ~140+ routes in `rusvel-api` | No codegen/OpenAPI client; renames or new routes can desync until runtime failure. |
| Low | XSS / HTML sinks | `DepartmentChat.svelte`, `ChatSidebar.svelte`, `chat/+page.svelte`: `<Streamdown content={...} />`; `ProductTour.svelte`: static `{@html \`<style>...\`}` | LLM markdown is a trust boundary; verify streamdown sanitization policy. Static `@html` is fixed string—low risk. |
| Info | Secrets in bundle | Grep: no `VITE_` in `frontend/` | Do not introduce `VITE_PUBLIC_*` tokens; keep secrets server-side. |
| Low | Playwright / visual | `package.json`: `packageManager: pnpm@9.15.4`; `playwright.config.ts`: `pnpm dev`, `cargo run` | `networkidle` + fixed 500 ms wait can flake on slow CI; `routes.visual.ts` `DEPARTMENTS` missing `flow`, `messaging`. |
| Low | Test fixture mismatch | `tests/fixtures.ts` sets `rusvel-active-session`; app only sets `activeSession` from `getSessions()` (defaults `list[0]` in `TopBar.svelte`) | Misleading comment; tests rely on single session or sort order. |

### UX debt

- Errors often surface as generic “API error N: …” strings or swallowed failures (e.g. deletes), so users get little actionable guidance or false success.
- No in-app indication when API is token-protected and returns 401 (would look like broken/empty data).
- Session choice is not persisted across reloads (`activeSession` always reverts to first session on load)—power users re-select each visit.
- Visual/regression coverage does not include every department shell route (`flow`, `messaging`), so nav or manifest regressions can ship unseen.

### Engineering debt

- **Single mega-module** `api.ts` (~1.6k lines) mixes transport, types, SSE parsing, and domain calls—hard to test and diff against Rust routes.
- **Duplicate fetch paths**: some endpoints use `request()`, others raw `fetch`, with different header/error behavior.
- **SSE parser** is minimal and not spec-complete (events, CRLF, multi-line `data:`).
- **No OpenAPI/type sharing** with `rusvel-api`; contract enforced only by manual maintenance and E2E.
- **Playwright fixtures** document behavior the app does not implement (`rusvel-active-session`).

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| F1 | Add optional `Authorization: Bearer <token>` from `import.meta.env.VITE_RUSVEL_API_TOKEN` (or runtime config endpoint) and merge into all `fetch`/`request` headers when set; document that embedding read-only token is optional and risk-traded | M | `api.ts` + build docs |
| F2 | Centralize `apiFetch()` that always checks `res.ok`, use for DELETE/profile; unify JSON parse error handling | S | `api.ts` |
| F3 | Extend `parseSSE`: strip `\r`, optional `AbortSignal`, call `onError` on reader failure; document expected event types vs server | M | `api.ts` + chat handlers |
| F4 | Persist `activeSession` id to `localStorage` on change and hydrate in `TopBar` if id still in `getSessions()` list; align Playwright fixture with real key | M | `TopBar.svelte` / `stores.ts` |
| F5 | Add `flow` and `messaging` to `routes.visual.ts` `DEPARTMENTS` (or drive list from same manifest as production) | S | `tests/` |
| F6 | Run `pnpm exec eslint-plugin-jsx-a11y` or periodic axe pass on critical routes (chat, settings, dept layout) | M | frontend CI |
| F7 | Document/trust model for `svelte-streamdown` (sanitization, allowed HTML); restrict raw `{@html}` | S | security note + chat |

### Space for improvement

- Generate a thin TypeScript client from an OpenAPI description exported or mirrored from Rust handlers, or share JSON Schemas for high-churn DTOs.
- Replace `waitUntil: 'networkidle'` in visual tests with deterministic waits (selector, `expect(locator).toBeVisible()`, or API stubs) to reduce CI flake.
- Consider `EventSource` or structured SSE library only if server emits standard `event:` discrimination; otherwise extend current parser with tests fed by real backend chunks.
- Split `api.ts` by domain (`sessions.ts`, `chat.ts`, …) behind a barrel file to keep files under team size limits.

### Handoff (for audit 10)

- **Build / embed:** Production UI is built with `pnpm build` → `frontend/build/` embedded by `rusvel-app` (rust-embed); API calls use `BASE === ''` so all `/api/*` requests are same-origin to the binary—no client-side API host config in prod.
- **Env:** Server uses `RUSVEL_API_TOKEN` / `RUSVEL_API_READ_TOKEN`; frontend currently has **no** corresponding mechanism—audit 10 should cover whether tokens ever belong in the browser, SSR proxies, or cookie-based auth.
- **Dev:** `vite` proxies `/api` to 3000, but `api.ts` hardcodes `http://localhost:3000` in DEV—audit 10 can recommend converging on relative `/api` in dev to match proxy and simplify CORS.
- **CI:** Playwright assumes repo root `cargo run` + `pnpm dev`; timeout/reuse flags affect local vs CI; document required ports 3000/5173 and Ollama/LLM dependencies for tests that hit real chat if any.
