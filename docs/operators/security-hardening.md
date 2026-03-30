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

## RusvelBase SQL console

| Variable | Default | Purpose |
|----------|---------|---------|
| `RUSVEL_DB_SQL_WRITE` | (unset = writes allowed when client sends `read_only: false`) | Set to `0`, `false`, or `off` to **force** read-only SQL execution on `POST /api/db/sql` (server-side), regardless of JSON body. |

Use this on shared or production-adjacent hosts so the DB browser cannot mutate data even with an admin bearer token.

## MCP HTTP

| Variable | Purpose |
|----------|---------|
| `RUSVEL_MCP_HTTP_AUTH` | `1` / `true` to require bearer auth on `/mcp`. |
| `RUSVEL_MCP_HTTP_TOKEN` | Shared secret for MCP HTTP bearer. |

If auth is disabled and the server binds outside loopback, startup logs a `rusvel_security` warning.
