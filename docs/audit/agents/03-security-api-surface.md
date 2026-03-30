# 03 — Security (application & API surface)

## Context (for next agents)

- **Surfaces:** `rusvel-api` (Axum), optional bearer auth from env, webhooks, hooks, MCP (`rusvel-mcp`), builtin tools (file, git, bash, browser).
- **Trust model:** Document phased auth in `docs/plans/` if referenced; treat “open by default” as a finding unless explicitly scoped.
- **Inputs:** Handoff from audit 02 (which modules own sensitive operations).

---

## Agent prompt (copy below)

```
Security audit of RUSVEL (in-repo).

Review: rusvel-auth, rusvel-api middleware/route protection, CORS, file path handling in rusvel-builtin-tools, terminal/shell execution paths, webhook + hook_dispatch, MCP HTTP/stdio.

Deliver:
- Threat model sketch: actor → entrypoint → asset (1 short section).
- Findings: severity | threat | evidence | current mitigation | gap.
- Fix proposals with effort S/M/L.
- Space for improvement (hardening backlog).

Record everything in docs/audit/agents/03-security-api-surface.md Report section.

Handoff: explicit list of routes or tools needing regression tests for audit 08.
```

---

## Report

### Executive summary

- API auth is **opt-in** (`RUSVEL_API_TOKEN` / `RUSVEL_API_READ_TOKEN`): with tokens unset, `/api/*` is effectively open to the network, while non-`/api` traffic (embedded SPA) bypasses bearer checks by design.
- **Defense in depth is thin** wherever the agent/runtime can invoke tools: `bash`, hook `command`, HTTP hooks, and PTYs inherit the server process identity and working directory; file tools sandbox to cwd via `canonicalize` + prefix check.
- **Webhooks** exempt bearer auth but verify **HMAC-SHA256** with constant-time hex compare (`rusvel-webhook`); **MCP HTTP** auth is separate and **off unless** `RUSVEL_MCP_HTTP_AUTH` is enabled; **stdio MCP** has no transport auth (parent process is the trust boundary).

### Threat model (sketch)

- **Actors:** Anonymous network clients; authenticated API clients (bearer); webhook callers with shared secret; operators with shell/MCP/CLI access; malicious or compromised LLM/tool invocations; insiders who can define hooks, skills, or agent prompts.
- **Entrypoints:** Axum HTTP (`rusvel-api`: REST, SSE, WebSocket terminal bridge); `POST /api/webhooks/{id}`; hook execution (`hook_dispatch`: `sh -c`, HTTP POST, `claude -p`); MCP JSON-RPC over **stdio** (`rusvel-mcp`) and optional **HTTP** `/mcp`; builtin tools (`read_file`/`write_file`/`edit_file`/`glob`/`grep`/`bash`/git/browser when wired).
- **Assets:** Session data, object store (hooks, agents, rules), job queue, DB contents, host filesystem under process cwd, outbound network from hooks and tools, browser/CDP sessions, provider API keys in env and `rusvel-auth` metadata, Telegram/notify channels.

### Findings

| Severity | Threat | Evidence | Mitigation today | Gap |
|----------|--------|----------|------------------|-----|
| High | Unauthenticated access to full API when token env vars unset | `bearer_check` returns `next.run` if both tokens are `None` (`crates/rusvel-api/src/auth.rs`) | Documented opt-in; read-only token option | No safe-by-default bind (e.g. localhost-only) or startup warning; operators must set tokens for any exposed deployment |
| High | Arbitrary command execution as server user via agent `bash` tool | `tokio::process::Command::new("bash").arg("-c").arg(command)` (`crates/rusvel-builtin-tools/src/shell.rs`) | Tool marked destructive in metadata; agent policies/approvals elsewhere | No syscall sandbox, argv allowlist, or separate OS user; compromise of chat/session with tools ⇒ host RCE |
| High | Server-side RCE / SSRF / data exfil via hooks when storage is writable | `execute_command_hook` runs `sh -c` with `hook.action`; `execute_http_hook` POSTs arbitrary URL (`crates/rusvel-api/src/hook_dispatch.rs`) | Hooks loaded from object store; API protected when bearer enabled | Any API client who can create hooks gets persistent triggers; no URL allowlist or hook signing |
| Medium | Webhook **receive** bypasses bearer (by design) | `webhook_receive_exempt` in `crates/rusvel-api/src/auth.rs` | HMAC verification on body (`crates/rusvel-webhook/src/lib.rs`, `ct_eq` on hex) | Idempotency/replay window not addressed here; rate limits are global, not per-webhook |
| Medium | MCP HTTP surface on same server may be unauthenticated | `McpAuth::from_env` defaults `enabled: false` (`crates/rusvel-mcp/src/http.rs`) | Optional `RUSVEL_MCP_HTTP_AUTH` + `RUSVEL_MCP_HTTP_TOKEN` | Easy to enable MCP HTTP without enabling auth; no mutual TLS |
| Medium | stdio MCP trusts any process with stdin/stdout | No auth in JSON-RPC loop (`crates/rusvel-mcp/src/lib.rs`) | Intended for local IDE attachment | Misconfigured service exposure (e.g. socket forwarding) equals full tool/session access |
| Medium | PTY / terminal API gives interactive shell on host cwd | `create_pane` uses `SHELL` and `current_dir()` (`crates/rusvel-api/src/terminal.rs`) | Same bearer middleware as other `/api/*` when tokens set | Same blast radius as `bash` if API is open or token leaks; WS abuse not separately throttled |
| Low | CORS allowlist is fixed dev origins | `localhost:5173`, `localhost:3000` only (`crates/rusvel-api/src/lib.rs`) | Reduces accidental credentialed cross-origin calls in dev | Production/staging origins need env-driven config; no `AllowOrigin::mirror` |
| Low | File tool sandbox is cwd-relative, not per-session root | `validate_path` uses `env::current_dir()` + `starts_with` (`crates/rusvel-builtin-tools/src/file_ops.rs`) | Blocks simple `..` escapes after canonicalize | Multi-tenant or symlink-heavy trees need stronger invariants; glob combines validated base with user pattern (review edge cases) |
| Low | Bearer compare is plain string equality | `provided == admin_tok` (`crates/rusvel-api/src/auth.rs`) | Tokens are high-entropy if generated well | Not constant-time; minor in practice vs timing attacks |
| Info | `rusvel-auth` in-memory store; `from_env` records credentials without retaining secret material | `RUSVEL_KEY_*` iteration uses `_v` — value discarded (`crates/rusvel-auth/src/lib.rs`) | Keys remain only in process environment as intended for many setups | `AuthPort` consumers must not assume secret bytes are in `Credential` |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| S-01 | Log one prominent **warn** at startup when listening on non-loopback and no `RUSVEL_API_TOKEN` | S | `rusvel-app` / `rusvel-api` boot |
| S-02 | Document in operator runbooks: set `RUSVEL_API_TOKEN`, enable `RUSVEL_MCP_HTTP_AUTH` when using MCP HTTP | S | docs / onboarding |
| S-03 | Add integration tests for CORS rejection of disallowed `Origin` and webhook HMAC failure paths | S | `rusvel-api` tests |
| M-01 | Env-configurable CORS origins (comma-separated) with safe default for production | M | `rusvel-api` `build_router` |
| M-02 | Hook hardening: optional allowlist for HTTP hook URLs (scheme/host), max payload size, deny private IPs if desired | M | `hook_dispatch` + config |
| M-03 | `bash` / hook command policy: configurable denylist, timeout caps, or require explicit `RUSVEL_DANGEROUS_TOOLS=1` | M | `rusvel-builtin-tools`, `hook_dispatch` |
| L-01 | Per-session or per-workspace roots for file tools + stronger path resolution policy | L | `rusvel-builtin-tools`, agent runtime |
| L-02 | Session-scoped API keys and RBAC (per ADR/auth phase docs) replacing single shared bearer | L | `rusvel-auth`, `rusvel-api`, stores |

### Space for improvement

- Separate **machine identity** (API) from **agent capability** (tool allowlists, approval gates for `browser_act` already partial).
- **Audit logging** for tool invocation, hook execution, and webhook enqueue (who/when/session), not only tracing.
- **Rate limit** sensitive exempt routes (webhook receive) independently of global limit.
- **MCP stdio**: document threat model (parent PID, stdio not forwarded over untrusted networks).
- Periodic **dependency and secret scanning** in CI; rotate `RUSVEL_API_TOKEN` on leak.

### Handoff (for audit 08 — tests)

- **HTTP / middleware:** `build_router` stack — CORS allowed vs forbidden origin; global rate limit returns 429; middleware order (bearer applied to `/api/*` only).
- **Auth:** `POST /api/webhooks/{id}` exempt vs `GET /api/webhooks` requires bearer when token set; read-only token blocks `POST`/`PUT`/`DELETE`; invalid bearer on `GET /api/terminal/dept/{id}` and terminal WebSocket upgrade.
- **Webhooks:** `receive_webhook` — missing header, bad hex, wrong HMAC, valid HMAC; `FORGE_PIPELINE_WEBHOOK_KIND` enqueue path with valid/invalid `session_id`.
- **MCP HTTP:** `/mcp` POST and `/mcp/sse` with auth disabled vs enabled (401 missing/wrong bearer).
- **Tools (integration or unit):** `validate_path` — escape attempts, symlink outside cwd; `glob` with `..` in pattern; `grep` ripgrep fallback; `bash` timeout and non-zero exit (already partially covered — extend for policy hooks if added).
- **Hooks:** `hook_dispatch` — with test storage, matching `event`/`matcher`, `command` invocation receives `HOOK_PAYLOAD` (mock `sh` or inject test binary path) — currently high value, low coverage risk.
- **Terminal:** pane create with invalid `session_id`; resize/WebSocket handshake when token required.
