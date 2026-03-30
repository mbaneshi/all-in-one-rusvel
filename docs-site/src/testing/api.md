# API Testing

Start the server first:

```bash
cargo run
```

**Expected:** Server starts on `http://localhost:3000`. Logs show boot sequence: database init, department registration (14 departments), tool registration, job worker started.

## Health & System

```bash
curl -s http://localhost:3000/api/health | jq .
```

**Expected:** `{"status": "ok"}` or similar health response.

```bash
curl -s http://localhost:3000/api/system/status | jq .
```

**Expected:** JSON with system info (uptime, departments, sessions, etc.).

## Sessions

```bash
# Create session
curl -s -X POST http://localhost:3000/api/sessions \
  -H 'Content-Type: application/json' \
  -d '{"name": "api-test", "kind": "General"}' | jq .
```

**Expected:** Returns JSON with `id` (UUID), `name`, `kind`, `created_at`.

```bash
# List sessions
curl -s http://localhost:3000/api/sessions | jq .
```

**Expected:** Array of session objects.

```bash
# Get single session
curl -s http://localhost:3000/api/sessions/<id> | jq .
```

**Expected:** Single session object with full details.

## Departments

```bash
# List all departments
curl -s http://localhost:3000/api/departments | jq .
```

**Expected:** Array of 14 department objects, each with `id`, `name`, `description`, `icon`, `color`, `capabilities`, `quick_actions`.

```bash
# Department config
curl -s http://localhost:3000/api/dept/code/config | jq .
```

**Expected:** Code department configuration (system prompt, model, etc.).

```bash
# Department events
curl -s http://localhost:3000/api/dept/forge/events | jq .
```

**Expected:** Array of events (may be empty).

## Agents CRUD

```bash
# List agents
curl -s http://localhost:3000/api/agents | jq .
```

**Expected:** Array with seeded agents (`rust-engine`, `svelte-ui`, `test-writer`, `arch-reviewer`, etc.).

```bash
# Create agent
curl -s -X POST http://localhost:3000/api/agents \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "test-agent",
    "role": "Test",
    "instructions": "You are a test agent",
    "model": "llama3.2"
  }' | jq .
```

**Expected:** Returns created agent with `id`.

```bash
# Get / Update / Delete
curl -s http://localhost:3000/api/agents/<id> | jq .

curl -s -X PUT http://localhost:3000/api/agents/<id> \
  -H 'Content-Type: application/json' \
  -d '{"name": "updated-agent", "instructions": "Updated"}' | jq .

curl -s -X DELETE http://localhost:3000/api/agents/<id>
```

**Expected:** Standard CRUD responses (200 with entity, 204 on delete).

## Skills CRUD

```bash
# List skills
curl -s http://localhost:3000/api/skills | jq .
```

**Expected:** Array with seeded skills (`/analyze-architecture`, `/check-crate-sizes`, etc.).

```bash
# Create skill
curl -s -X POST http://localhost:3000/api/skills \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "test-skill",
    "description": "A test skill",
    "action": "echo {{input}}"
  }' | jq .
```

**Expected:** Returns created skill with `id`. The `{{input}}` placeholder is preserved.

## Rules CRUD

```bash
curl -s http://localhost:3000/api/rules | jq .
```

**Expected:** Array with seeded rules (architecture boundaries, crate size, etc.).

```bash
curl -s -X POST http://localhost:3000/api/rules \
  -H 'Content-Type: application/json' \
  -d '{"name": "test-rule", "description": "Test", "content": "Always test"}' | jq .
```

## Hooks CRUD

```bash
curl -s http://localhost:3000/api/hooks | jq .
```

```bash
# Create a command hook
curl -s -X POST http://localhost:3000/api/hooks \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "test-hook",
    "event_kind": "code.chat.completed",
    "hook_type": "command",
    "action": "echo hook fired"
  }' | jq .
```

```bash
# List hook events (what events can hooks fire on)
curl -s http://localhost:3000/api/hooks/events | jq .
```

## MCP Servers CRUD

```bash
curl -s http://localhost:3000/api/mcp-servers | jq .

curl -s -X POST http://localhost:3000/api/mcp-servers \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "test-mcp",
    "command": "node",
    "args": ["./mcp-server.js"],
    "description": "Test MCP server"
  }' | jq .
```

## Workflows CRUD + Execution

```bash
# List workflows
curl -s http://localhost:3000/api/workflows | jq .
```

**Expected:** Array with seeded self-improvement workflow.

```bash
# Create workflow
curl -s -X POST http://localhost:3000/api/workflows \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "test-workflow",
    "steps": [
      {"name": "step1", "action": "echo step 1"},
      {"name": "step2", "action": "echo step 2"}
    ]
  }' | jq .
```

```bash
# Run workflow (requires Ollama for agent steps)
curl -s -X POST http://localhost:3000/api/workflows/<id>/run | jq .
```

**Expected:** Returns run status. Job is enqueued.

## Chat (God Agent) -- SSE Streaming

```bash
curl -N http://localhost:3000/api/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "Hello, what can you do?"}'
```

**Expected SSE events:**
```text
data: {"type":"text_delta","text":"I"}
data: {"type":"text_delta","text":" can"}
data: {"type":"text_delta","text":" help"}
...
data: {"type":"done","output":"I can help you with..."}
```

**Verify:** Stream starts within 2-5s. Text deltas arrive incrementally. Stream ends with a `done` event.

## Department Chat -- SSE Streaming

```bash
curl -N http://localhost:3000/api/dept/code/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "What files are in this project?"}'
```

**Expected:** SSE stream with department-scoped response. The agent uses code-related tools.

```bash
# List conversations
curl -s http://localhost:3000/api/dept/code/chat/conversations | jq .

# Get conversation history
curl -s http://localhost:3000/api/dept/code/chat/conversations/<id> | jq .
```

## Agent Tool-Use Loop

The core loop: LLM generates -> tool call -> execute -> feed result -> repeat.

```bash
curl -N http://localhost:3000/api/dept/code/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "Read the Cargo.toml and count workspace members"}'
```

**Expected SSE events in order:**
1. `text_delta` -- agent starts responding
2. `tool_call` -- `{"name": "read_file", "input": {"path": "Cargo.toml"}}`
3. `tool_result` -- file contents
4. `text_delta` -- agent interprets results
5. `done` -- final answer mentions "55 workspace members" (or `docs/status/current-state.md`)

## Scoped Tool Registry

Different departments see different tools:

```bash
# Code dept should have code_analyze, code_search
curl -N http://localhost:3000/api/dept/code/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "What tools do you have available?"}'

# Content dept should have content_draft, content_publish
curl -N http://localhost:3000/api/dept/content/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "What tools do you have available?"}'
```

## Conversation History

```bash
# Send first message
curl -N http://localhost:3000/api/dept/code/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "Remember: my favorite number is 42"}'

# Get conversation ID
curl -s http://localhost:3000/api/dept/code/chat/conversations | jq '.[0].id'

# Continue conversation
curl -N http://localhost:3000/api/dept/code/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "What is my favorite number?", "conversation_id": "<id>"}'
```

**Expected:** Agent recalls "42" from conversation history.

## Skills Execution

```bash
# Create a skill with input interpolation
curl -s -X POST http://localhost:3000/api/skills \
  -H 'Content-Type: application/json' \
  -d '{"name": "greet", "action": "Say hello to {{input}}"}' | jq .

# Execute via chat
curl -N http://localhost:3000/api/dept/forge/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "/greet World"}'
```

**Expected:** Agent resolves skill, interpolates "World" into `{{input}}`, executes.

## Rules Injection

```bash
# Create a rule for code department
curl -s -X POST http://localhost:3000/api/rules \
  -H 'Content-Type: application/json' \
  -d '{"name": "always-rust", "department": "code", "content": "Always recommend Rust"}' | jq .

# Chat with code department
curl -N http://localhost:3000/api/dept/code/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "What language should I use for a CLI tool?"}'
```

**Expected:** Rule is appended to system prompt. Agent recommends Rust.

## Mission / Forge API

```bash
# Daily plan (requires Ollama + active session)
curl -s http://localhost:3000/api/sessions/<session-id>/mission/today | jq .

# List goals
curl -s http://localhost:3000/api/sessions/<session-id>/mission/goals | jq .

# Create goal
curl -s -X POST http://localhost:3000/api/sessions/<session-id>/mission/goals \
  -H 'Content-Type: application/json' \
  -d '{"title": "API test goal", "timeframe": "week"}' | jq .

# Generate executive brief
curl -s -X POST http://localhost:3000/api/brief/generate | jq .

# Get latest brief
curl -s http://localhost:3000/api/brief/latest | jq .
```

## Config & Models

```bash
curl -s http://localhost:3000/api/config | jq .
curl -s http://localhost:3000/api/config/models | jq .
curl -s http://localhost:3000/api/config/tools | jq .

curl -s -X PUT http://localhost:3000/api/config \
  -H 'Content-Type: application/json' \
  -d '{"default_model": "llama3.2"}' | jq .
```

## Analytics

```bash
curl -s http://localhost:3000/api/analytics | jq .
curl -s http://localhost:3000/api/analytics/dashboard | jq .
curl -s http://localhost:3000/api/analytics/spend | jq .
```

## Jobs & Approvals

```bash
# List jobs
curl -s http://localhost:3000/api/jobs | jq .

# List pending approvals
curl -s http://localhost:3000/api/approvals | jq .

# Approve / reject a job
curl -s -X POST http://localhost:3000/api/approvals/<job-id>/approve | jq .
curl -s -X POST http://localhost:3000/api/approvals/<job-id>/reject | jq .
```

## Hooks & Events

### Command Hook

```bash
# Create hook that fires on chat completion
curl -s -X POST http://localhost:3000/api/hooks \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "log-chat",
    "event_kind": "forge.chat.completed",
    "hook_type": "command",
    "action": "echo HOOK_FIRED >> /tmp/rusvel-hook.log"
  }' | jq .

# Trigger by chatting with forge
curl -N http://localhost:3000/api/dept/forge/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "hello"}'

# Verify hook fired
cat /tmp/rusvel-hook.log
```

**Expected:** File contains "HOOK_FIRED".

### HTTP Hook

```bash
curl -s -X POST http://localhost:3000/api/hooks \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "notify-hook",
    "event_kind": "content.published",
    "hook_type": "http",
    "action": "http://localhost:8080/webhook"
  }' | jq .
```

**Expected:** When content is published, POST request sent to the URL with event payload.

## User Profile

```bash
curl -s http://localhost:3000/api/profile | jq .

curl -s -X PUT http://localhost:3000/api/profile \
  -H 'Content-Type: application/json' \
  -d '{"name": "Mehdi", "skills": {"primary": ["rust", "sveltekit"]}}' | jq .
```

## Help (AI-Powered)

```bash
curl -s -X POST http://localhost:3000/api/help \
  -H 'Content-Type: application/json' \
  -d '{"question": "How do I create a content draft?"}' | jq .
```

## Database Browser

```bash
# List tables
curl -s http://localhost:3000/api/db/tables | jq .

# Get table schema
curl -s http://localhost:3000/api/db/tables/events/schema | jq .

# Get table rows
curl -s "http://localhost:3000/api/db/tables/events/rows?limit=10" | jq .

# Execute SQL
curl -s -X POST http://localhost:3000/api/db/sql \
  -H 'Content-Type: application/json' \
  -d '{"sql": "SELECT COUNT(*) as count FROM events"}' | jq .
```

**Security check:** Destructive SQL should be rejected or run in read-only mode:
```bash
curl -s -X POST http://localhost:3000/api/db/sql \
  -H 'Content-Type: application/json' \
  -d '{"sql": "DROP TABLE events"}' | jq .
```

## Knowledge / RAG

```bash
# Ingest knowledge
curl -s -X POST http://localhost:3000/api/knowledge/ingest \
  -H 'Content-Type: application/json' \
  -d '{"content": "Rust is a systems programming language.", "source": "manual"}' | jq .

# Search knowledge
curl -s -X POST http://localhost:3000/api/knowledge/search \
  -H 'Content-Type: application/json' \
  -d '{"query": "programming language safety"}' | jq .

# Hybrid search (keyword + semantic)
curl -s -X POST http://localhost:3000/api/knowledge/hybrid-search \
  -H 'Content-Type: application/json' \
  -d '{"query": "Rust safety"}' | jq .

# Stats, related, delete
curl -s http://localhost:3000/api/knowledge/stats | jq .
curl -s "http://localhost:3000/api/knowledge/related?id=<id>" | jq .
curl -s -X DELETE http://localhost:3000/api/knowledge/<id>
```

## Webhook & Cron

### Webhooks

```bash
# Create webhook
curl -s -X POST http://localhost:3000/api/webhooks \
  -H 'Content-Type: application/json' \
  -d '{"event_kind": "forge.pipeline.requested"}' | jq .

# List / trigger
curl -s http://localhost:3000/api/webhooks | jq .
curl -s -X POST http://localhost:3000/api/webhooks/<id> \
  -H 'Content-Type: application/json' \
  -d '{"session_id": "<session-id>"}' | jq .
```

### Cron Scheduling

```bash
# Create schedule
curl -s -X POST http://localhost:3000/api/cron \
  -H 'Content-Type: application/json' \
  -d '{"name": "daily-brief", "cron": "0 9 * * *", "job_kind": "Custom", "payload": {"action": "brief"}}' | jq .

# List / tick / get / update / delete
curl -s http://localhost:3000/api/cron | jq .
curl -s -X POST http://localhost:3000/api/cron/tick | jq .
curl -s http://localhost:3000/api/cron/<id> | jq .
curl -s -X PUT http://localhost:3000/api/cron/<id> -H 'Content-Type: application/json' -d '{"cron": "0 10 * * *"}' | jq .
curl -s -X DELETE http://localhost:3000/api/cron/<id>
```

## Playbooks

```bash
# List playbooks
curl -s http://localhost:3000/api/playbooks | jq .

# Create playbook
curl -s -X POST http://localhost:3000/api/playbooks \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "code-to-blog",
    "steps": [
      {"engine": "code", "action": "analyze", "params": {"path": "."}},
      {"engine": "content", "action": "from-code", "params": {"topic": "architecture"}}
    ]
  }' | jq .

# Run / check status
curl -s -X POST http://localhost:3000/api/playbooks/<id>/run | jq .
curl -s http://localhost:3000/api/playbooks/runs | jq .
curl -s http://localhost:3000/api/playbooks/runs/<run-id> | jq .
```

## Starter Kits

```bash
curl -s http://localhost:3000/api/kits | jq .
curl -s http://localhost:3000/api/kits/<id> | jq .
curl -s -X POST http://localhost:3000/api/kits/<id>/install | jq .
```

## Terminal & Browser Ports

### Terminal

```bash
# WebSocket terminal connection
websocat ws://localhost:3000/api/terminal/ws

# Get department terminal pane
curl -s http://localhost:3000/api/terminal/dept/code | jq .
```

### Browser (CDP)

Requires Chrome running with `--remote-debugging-port=9222`.

```bash
curl -s http://localhost:3000/api/browser/status | jq .

curl -s -X POST http://localhost:3000/api/browser/connect \
  -H 'Content-Type: application/json' \
  -d '{"endpoint": "http://localhost:9222"}' | jq .

curl -s http://localhost:3000/api/browser/tabs | jq .

curl -s -X POST http://localhost:3000/api/browser/observe/0 | jq .

curl -s -X POST http://localhost:3000/api/browser/act \
  -H 'Content-Type: application/json' \
  -d '{"action": "click", "selector": "#submit-button"}' | jq .
```

## Notification System

```bash
# Requires RUSVEL_TELEGRAM_BOT_TOKEN
curl -s -X POST http://localhost:3000/api/system/notify \
  -H 'Content-Type: application/json' \
  -d '{"message": "Test notification from RUSVEL"}' | jq .
```

## MCP Server

### Stdio Transport

```bash
cargo run -- --mcp
```

**Expected:** Server enters stdio JSON-RPC mode. Reads JSON from stdin, writes to stdout.

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}' \
  | cargo run -- --mcp
```

### HTTP Transport

```bash
cargo run -- --mcp-http
```

**Expected:** MCP server starts on HTTP with `/mcp` (POST) and `/mcp/sse` (GET) endpoints.

### MCP Tools

| Tool | Input | Expected |
|------|-------|----------|
| `session_list` | `{}` | Array of sessions |
| `session_create` | `{"name": "mcp-test"}` | New session with ID |
| `mission_today` | `{"session_id": "<id>"}` | Daily plan text |
| `mission_goals` | `{"session_id": "<id>"}` | Array of goals |
| `mission_add_goal` | `{"session_id": "<id>", "title": "MCP goal"}` | Goal ID |
| `visual_inspect` | `{}` | Visual test results |
