# Smoke Test Checklist

Quick pass to verify everything works after a build. Run these in order -- takes ~15 minutes.

## Phase 1: Build & Boot (no Ollama needed)

| # | Command | Pass if |
|---|---------|---------|
| 1 | `cargo build` | Compiles without errors |
| 2 | `cargo test` | ~636 tests pass (workspace sum; see `docs/status/current-state.md`) |
| 3 | `cargo run -- --help` | Shows help with all subcommands |
| 4 | `cargo run -- session create smoke-test` | Prints session UUID |
| 5 | `cargo run -- session list` | Shows the smoke-test session |
| 6 | `cargo run -- finance status` | Prints status (no crash) |
| 7 | `cargo run -- code analyze .` | Prints analysis results |
| 8 | `cargo run -- harvest pipeline` | Prints pipeline (all zeros OK) |

## Phase 2: Web Server (no Ollama needed)

Start `cargo run` in a terminal, then in another:

| # | Command | Pass if |
|---|---------|---------|
| 9 | `curl -s localhost:3000/api/health` | Returns 200 |
| 10 | `curl -s localhost:3000/api/departments \| jq length` | Returns 14 |
| 11 | `curl -s localhost:3000/api/agents \| jq length` | Returns >= 7 (seeded) |
| 12 | `curl -s localhost:3000/api/skills \| jq length` | Returns >= 5 (seeded) |
| 13 | `curl -s localhost:3000/api/rules \| jq length` | Returns >= 5 (seeded) |
| 14 | `curl -s localhost:3000/api/config/tools \| jq length` | Returns >= 22 |
| 15 | `curl -s localhost:3000/api/db/tables \| jq length` | Returns >= 5 |
| 16 | Open `http://localhost:3000` in browser | Dashboard renders |

## Phase 3: LLM Features (requires Ollama)

| # | Command | Pass if |
|---|---------|---------|
| 17 | `cargo run -- forge mission today` | Streams daily plan |
| 18 | `curl -N localhost:3000/api/chat -H 'Content-Type: application/json' -d '{"message":"hi"}'` | SSE stream with text deltas |
| 19 | `curl -N localhost:3000/api/dept/code/chat -H 'Content-Type: application/json' -d '{"message":"list files"}'` | SSE stream with tool calls |
| 20 | `cargo run -- content draft "test"` | Streams content draft |

## Phase 4: Interactive Surfaces

| # | Action | Pass if |
|---|--------|---------|
| 21 | `cargo run -- shell` -> type `help` -> type `exit` | REPL starts, shows help, exits cleanly |
| 22 | `cargo run -- shell` -> `use finance` -> `status` -> `back` | Context switching works |
| 23 | `cargo run -- --tui` -> press `q` | TUI renders 4 panels, exits cleanly |

## Phase 5: CRUD Cycle

Pick any entity (agents, skills, rules, hooks, workflows) and verify the full lifecycle:

| # | Action | Pass if |
|---|--------|---------|
| 24 | `POST` create | Returns 200 with ID |
| 25 | `GET` by ID | Returns created entity |
| 26 | `GET` list | Entity appears in list |
| 27 | `PUT` update | Returns updated entity |
| 28 | `GET` by ID again | Shows updated fields |
| 29 | `DELETE` | Returns 200/204 |
| 30 | `GET` by ID again | Returns 404 |

### Example CRUD cycle (agents):

```bash
# Create
ID=$(curl -s -X POST localhost:3000/api/agents \
  -H 'Content-Type: application/json' \
  -d '{"name":"smoke-agent","role":"Test","instructions":"test"}' | jq -r '.id')
echo "Created: $ID"

# Read
curl -s localhost:3000/api/agents/$ID | jq .name
# Expected: "smoke-agent"

# Update
curl -s -X PUT localhost:3000/api/agents/$ID \
  -H 'Content-Type: application/json' \
  -d '{"name":"updated-agent","instructions":"updated"}' | jq .name
# Expected: "updated-agent"

# List (verify it's there)
curl -s localhost:3000/api/agents | jq '.[].name' | grep updated-agent
# Expected: "updated-agent"

# Delete
curl -s -X DELETE localhost:3000/api/agents/$ID -w "\n%{http_code}\n"
# Expected: 200 or 204

# Verify gone
curl -s -o /dev/null -w "%{http_code}" localhost:3000/api/agents/$ID
# Expected: 404
```

## Automated Test Suite

For reference, the full automated suite:

```bash
# Full workspace
cargo test

# Individual crates
cargo test -p rusvel-core
cargo test -p rusvel-db
cargo test -p rusvel-api
cargo test -p rusvel-llm
cargo test -p forge-engine
cargo test -p content-engine
cargo test -p harvest-engine
cargo test -p code-engine
cargo test -p flow-engine
cargo test -p rusvel-tool
cargo test -p rusvel-agent

# Line coverage
./scripts/coverage.sh

# Benchmark
cargo bench -p rusvel-app --bench boot
```
