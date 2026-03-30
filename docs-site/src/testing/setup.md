# Setup & Prerequisites

## Build the Binary

```bash
cargo build
```

**Expected:** Compiles 55 workspace members. Zero errors. Warnings are acceptable.

## Run Automated Tests First

```bash
cargo test
```

**Expected:** ~636 tests pass (workspace sum), 0 failures. Some tests may be ignored (PTY tests in sandboxed environments).

## Required Services

| Service | How to start | Required for |
|---------|-------------|--------------|
| Ollama | `ollama serve` | LLM-powered features (chat, mission, draft, etc.) |
| Ollama model | `ollama pull llama3.2` | Must have at least one model pulled |

## Optional Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `RUST_LOG` | Tracing level | `info` |
| `RUSVEL_API_TOKEN` | Bearer token for API auth | None (open) |
| `RUSVEL_RATE_LIMIT` | API rate limit (req/sec) | 100 |
| `RUSVEL_SMTP_HOST` | SMTP for outreach emails | Mock adapter |
| `RUSVEL_SMTP_PORT` | SMTP port | -- |
| `RUSVEL_SMTP_USER` | SMTP username | -- |
| `RUSVEL_SMTP_PASSWORD` | SMTP password | -- |
| `RUSVEL_TELEGRAM_BOT_TOKEN` | Telegram notifications | Disabled |
| `RUSVEL_FLOW_PARALLEL_EVALUATE` | Enable parallel flow nodes | `0` |
| `ANTHROPIC_API_KEY` | Claude API provider | Claude CLI fallback |
| `OPENAI_API_KEY` | OpenAI provider | -- |

## Data Directory

RUSVEL stores data in `~/.rusvel/`:
- `rusvel.db` -- SQLite database
- `config.toml` -- configuration
- `shell_history.txt` -- REPL history
- `lance/` -- vector store data

To **reset to clean state**: `rm -rf ~/.rusvel/` (destructive -- removes all data).

## Docker Alternative

```bash
docker compose up
```

**Expected:** Starts RUSVEL on `:3000` + Ollama on `:11434`. No local install needed.

## Environment Combos

### Minimal (no AI)
```bash
cargo run
# Test all non-LLM features: CRUD, DB browser, config, etc.
```

### Full Local
```bash
ollama serve &
ollama pull llama3.2
cargo run
# All features available
```

### With Claude API
```bash
export ANTHROPIC_API_KEY=sk-ant-...
cargo run
# Uses Claude for higher-quality responses
```

### With Notifications
```bash
export RUSVEL_TELEGRAM_BOT_TOKEN=...
cargo run
# POST /api/system/notify sends Telegram messages
```

## Testing Tools

| Tool | Use for | Install |
|------|---------|---------|
| `curl` | API testing | Pre-installed on macOS |
| `jq` | JSON formatting | `brew install jq` |
| `websocat` | WebSocket testing | `brew install websocat` |
| `httpie` | Friendlier API calls | `brew install httpie` |
| `just` | Task runner recipes | `brew install just` |
| `watchexec` | File-watching re-runs | `cargo install watchexec-cli` |

### httpie Examples (alternative to curl)

```bash
# GET
http :3000/api/health
http :3000/api/departments

# POST
http POST :3000/api/sessions name=test kind=General

# SSE streaming
http --stream POST :3000/api/chat message="hello"
```
