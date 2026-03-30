# API bind, auth, and DB console (operator notes)

## HTTP listen address

| Variable | Default | Purpose |
|----------|---------|---------|
| `RUSVEL_HTTP_ADDR` | `127.0.0.1:3000` | Socket address for the Axum server (e.g. `0.0.0.0:3000` for all interfaces). |

## Bearer tokens

| Variable | Purpose |
|----------|---------|
| `RUSVEL_API_TOKEN` | Admin token: full read/write API access. |
| `RUSVEL_API_READ_TOKEN` | Read-only: `GET`/`HEAD`/`OPTIONS` only on `/api/*`. |
| `RUSVEL_ALLOW_INSECURE_API` | Set to `1` or `true` to silence the **non-loopback + no tokens** warning (lab/trusted networks only). |

If **both** token env vars are unset, `/api/*` accepts requests without `Authorization`. On a **non-loopback** bind, startup logs a `rusvel_security` warning unless `RUSVEL_ALLOW_INSECURE_API=1`.

## Runtime snapshot, operator prefs, and shutdown

| Endpoint | Method | Auth (when tokens are set) | Purpose |
|----------|--------|------------------------------|---------|
| `/api/system/runtime` | GET | Admin or read token | In-process effective configuration (no secret values): process/HTTP summary, LLM wiring snapshot, integration booleans, subsystem registration, boot-time operator prefs, and a small **capabilities** list (labels + doc paths). |
| `/api/system/operator-prefs` | PUT | Admin only | Persist operator preferences (e.g. `force_claude_cli`) in object store. Changes apply after a **full process restart**; the running LLM stack is not hot-reloaded. |
| `/api/system/shutdown` | POST | Admin only | Triggers graceful shutdown (same signal path as Ctrl+C). Use with **systemd**, **Docker** restart policy, **launchd**, or a manual terminal restart. |

Settings UI: **Control center** at `/settings/control` (tab under Settings).

## RusvelBase SQL console

| Variable | Default | Purpose |
|----------|---------|---------|
| `RUSVEL_DB_SQL_WRITE` | (unset = writes allowed when client sends `read_only: false`) | Set to `0`, `false`, or `off` to **force** read-only SQL execution on `POST /api/db/sql` (server-side), regardless of JSON body. |

Use this on shared or production-adjacent hosts so the DB browser cannot mutate data even with an admin bearer token.

## Frontend (Vite / embedded SPA)

| Variable | Purpose |
|----------|---------|
| `VITE_RUSVEL_API_TOKEN` | Sent as `Authorization: Bearer` on all `fetch` calls from [`frontend/src/lib/api.ts`](../frontend/src/lib/api.ts). **Embedded in the built JS bundle** — prefer the **read-only** server token when possible; do not commit real secrets. |

## MCP HTTP

| Variable | Purpose |
|----------|---------|
| `RUSVEL_MCP_HTTP_AUTH` | `1` / `true` to require bearer auth on `/mcp`. |
| `RUSVEL_MCP_HTTP_TOKEN` | Shared secret for MCP HTTP bearer. |

If auth is disabled and the server binds outside loopback, startup logs a `rusvel_security` warning.

## LLM and outbound

| Variable | Purpose |
|----------|---------|
| `ANTHROPIC_API_KEY` | Claude HTTP API |
| `OPENAI_API_KEY` | OpenAI API |
| `OLLAMA_HOST` | Ollama base URL (default `http://localhost:11434`) |

## GTM / email (outreach)

| Variable | Purpose |
|----------|---------|
| `RUSVEL_SMTP_HOST` | SMTP host (unset = mock adapter) |
| `RUSVEL_SMTP_PORT` | Port (default 587) |
| `RUSVEL_SMTP_USER` | Auth username |
| `RUSVEL_SMTP_PASSWORD` | Auth password |
| `RUSVEL_SMTP_FROM` | From address |

## Common misconceptions (docs drift)

| Variable | Status |
|----------|--------|
| `RUSVEL_DB_PATH` | **Not read** by `rusvel-app` — database is `{data_dir}/rusvel.db`. |
| `RUSVEL_SEED_DEV` | **Not read** by the binary; seed flows in docs are aspirational until implemented. |

## Job worker

| Variable | Default | Purpose |
|----------|---------|---------|
| `RUSVEL_JOB_STALE_RUNNING_SECS` | `3600` | On worker startup, `Running` jobs older than this many seconds are marked `Failed`. Set to `0` to disable. |

## Config vs logging

| Variable / key | Purpose |
|----------------|---------|
| `RUST_LOG` | Tracing filter when set (wins over TOML). |
| `log.level` in `~/.rusvel/config.toml` | Used when `RUST_LOG` is unset. |
