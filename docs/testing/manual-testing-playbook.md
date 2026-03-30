# RUSVEL — Manual Testing Playbook

> Complete feature-by-feature testing guide with expected behavior.
> Last updated: 2026-03-28

---

## Table of Contents

1. [Prerequisites & Setup](#1-prerequisites--setup)
2. [Surface 1: CLI One-Shot Commands](#2-surface-1-cli-one-shot-commands)
3. [Surface 2: Interactive REPL Shell](#3-surface-2-interactive-repl-shell)
4. [Surface 3: TUI Dashboard](#4-surface-3-tui-dashboard)
5. [Surface 4: Web API (curl)](#5-surface-4-web-api-curl)
6. [Surface 5: Web Frontend (Browser)](#6-surface-5-web-frontend-browser)
7. [Surface 6: MCP Server](#7-surface-6-mcp-server)
8. [Cross-Cutting: Agent & Chat](#8-cross-cutting-agent--chat)
9. [Cross-Cutting: Job Queue & Approvals](#9-cross-cutting-job-queue--approvals)
10. [Cross-Cutting: Hooks & Events](#10-cross-cutting-hooks--events)
11. [Engine-Specific Tests](#11-engine-specific-tests)
12. [Database Browser](#12-database-browser)
13. [Knowledge / RAG](#13-knowledge--rag)
14. [Flow Engine (DAG Workflows)](#14-flow-engine-dag-workflows)
15. [Capability Engine (!build)](#15-capability-engine-build)
16. [Terminal & Browser Ports](#16-terminal--browser-ports)
17. [Configuration & Settings](#17-configuration--settings)
18. [Webhook & Cron](#18-webhook--cron)
19. [Automated Test Suite (cargo test)](#19-automated-test-suite-cargo-test)
20. [Visual Regression Tests](#20-visual-regression-tests)
21. [Smoke Test Checklist](#21-smoke-test-checklist)

---

## 1. Prerequisites & Setup

### 1.1 Build the binary

```bash
cargo build
```

**Expected:** Compiles 54 workspace members. Zero errors. Warnings are acceptable.

### 1.2 Run automated tests first

```bash
cargo test
```

**Expected:** ~635 tests pass (workspace sum), 0 failures. Some tests may be ignored (PTY tests in sandboxed environments).

### 1.3 Required services

| Service | How to start | Required for |
|---------|-------------|--------------|
| Ollama | `ollama serve` | LLM-powered features (chat, mission, draft, etc.) |
| Ollama model | `ollama pull llama3.2` | Must have at least one model pulled |

### 1.4 Optional environment variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `RUST_LOG` | Tracing level | `info` |
| `RUSVEL_API_TOKEN` | Bearer token for API auth | None (open) |
| `RUSVEL_RATE_LIMIT` | API rate limit (req/sec) | 100 |
| `RUSVEL_SMTP_HOST` | SMTP for outreach emails | Mock adapter |
| `RUSVEL_SMTP_PORT` | SMTP port | — |
| `RUSVEL_SMTP_USER` | SMTP username | — |
| `RUSVEL_SMTP_PASSWORD` | SMTP password | — |
| `RUSVEL_TELEGRAM_BOT_TOKEN` | Telegram notifications | Disabled |
| `RUSVEL_FLOW_PARALLEL_EVALUATE` | Enable parallel flow nodes | `0` |
| `ANTHROPIC_API_KEY` | Claude API provider | Claude CLI fallback |
| `OPENAI_API_KEY` | OpenAI provider | — |

### 1.5 Data directory

RUSVEL stores data in `~/.rusvel/`:
- `rusvel.db` — SQLite database
- `config.toml` — configuration
- `shell_history.txt` — REPL history
- `lance/` — vector store data

To **reset to clean state**: `rm -rf ~/.rusvel/` (destructive — removes all data).

### 1.6 Docker alternative

```bash
docker compose up
```

**Expected:** Starts RUSVEL on `:3000` + Ollama on `:11434`. No local install needed.

---

## 2. Surface 1: CLI One-Shot Commands

### 2.1 Help

```bash
cargo run -- --help
```

**Expected:** Shows usage, subcommands (session, forge, shell, brief, browser, finance, growth, distro, legal, support, infra, product, code, harvest, content, gtm), and flags (--mcp, --mcp-http, --tui).

### 2.2 Session Management

```bash
# Create a session
cargo run -- session create "test-project"
```

**Expected:** Prints session ID (UUID). Session is now active.

```bash
# List sessions
cargo run -- session list
```

**Expected:** Table showing session ID, name, kind, created_at. Should include "test-project".

```bash
# Switch session
cargo run -- session switch <session-id>
```

**Expected:** Confirms switch to the specified session.

### 2.3 Forge / Mission

```bash
# Daily mission plan (requires Ollama)
cargo run -- forge mission today
```

**Expected:** AI-generated daily plan with prioritized tasks. Streams text to stdout. May take 5-30s depending on model.

```bash
# List goals
cargo run -- forge mission goals
```

**Expected:** Table of goals (may be empty on fresh install).

```bash
# Add a goal
cargo run -- forge mission goal add "Launch MVP" --description "Ship v1 to production" --timeframe month
```

**Expected:** Prints goal ID. Goal appears in subsequent `goals` listing.

```bash
# Periodic review
cargo run -- forge mission review --period week
```

**Expected:** AI-generated weekly review (requires Ollama).

### 2.4 Executive Brief

```bash
cargo run -- brief
```

**Expected:** Generates an executive daily digest summarizing activity across departments.

### 2.5 Department Commands (Generic)

Every department supports `status`, `list`, and `events`:

```bash
# Status
cargo run -- finance status
cargo run -- growth status
cargo run -- code status
cargo run -- content status
cargo run -- harvest status
cargo run -- gtm status
cargo run -- product status
cargo run -- distro status
cargo run -- legal status
cargo run -- support status
cargo run -- infra status
```

**Expected:** Each prints a status summary. May show "No active items" on fresh install.

```bash
# List with kind filter
cargo run -- finance list --kind transactions
cargo run -- support list --kind tickets
cargo run -- legal list --kind contracts
cargo run -- growth list --kind funnel_stages
cargo run -- product list --kind features
```

**Expected:** Table of domain objects. Empty on fresh install, but no error.

```bash
# Events
cargo run -- finance events --limit 5
```

**Expected:** Recent events for that department. Empty on fresh install.

### 2.6 Engine-Specific Commands

```bash
# Code: analyze current directory
cargo run -- code analyze .
```

**Expected:** Prints analysis results (file count, complexity metrics, dependency info). Works without Ollama.

```bash
# Code: search symbols
cargo run -- code search "DepartmentApp"
```

**Expected:** BM25 search results showing matching symbols/definitions.

```bash
# Content: draft a topic (requires Ollama)
cargo run -- content draft "Why Rust is great for CLI tools"
```

**Expected:** AI-generated markdown draft. Streams to stdout.

```bash
# Harvest: pipeline stats
cargo run -- harvest pipeline
```

**Expected:** Pipeline stage counts (Cold, Contacted, Qualified, etc.). All zeros on fresh install.

### 2.7 Browser Commands

```bash
# Check browser status (no Chrome needed)
cargo run -- browser status
```

**Expected:** Shows "Not connected" or connection info.

```bash
# Connect to Chrome (requires Chrome with --remote-debugging-port=9222)
cargo run -- browser connect
```

**Expected:** Connects to Chrome DevTools Protocol endpoint.

---

## 3. Surface 2: Interactive REPL Shell

```bash
cargo run -- shell
```

**Expected:** Launches interactive prompt: `rusvel> `

### 3.1 REPL Commands

| Command | Expected Behavior |
|---------|-------------------|
| `help` | Lists all available commands |
| `status` | Overview across all departments |
| `use finance` | Switches context → prompt becomes `rusvel:finance> ` |
| `status` (in dept context) | Shows finance-specific status |
| `list transactions` | Lists finance transactions |
| `events` | Shows finance events |
| `back` | Returns to top-level `rusvel> ` |
| `use code` | Switches to code department |
| `session list` | Lists all sessions |
| `session create foo` | Creates a new session |
| `exit` or Ctrl+D | Exits the REPL |

### 3.2 REPL Features

| Feature | How to test | Expected |
|---------|------------|----------|
| Tab completion | Type `us` then press Tab | Completes to `use` |
| Department completion | Type `use ` then Tab | Shows all 14 departments |
| History | Type a command, exit, re-enter shell | Up arrow recalls previous commands |
| History search | Press Ctrl+R, type partial command | Finds matching history entry |

---

## 4. Surface 3: TUI Dashboard

```bash
cargo run -- --tui
```

**Expected:** Full-screen terminal dashboard with 4 panels.

### 4.1 TUI Panels

| Panel | Content | Expected |
|-------|---------|----------|
| Tasks (top-left) | Active tasks with priority markers | Shows tasks or "No active tasks" |
| Goals (top-right) | Goals with progress bars | Shows goals or "No goals" |
| Pipeline (bottom-left) | Opportunity counts by stage | Shows stages with counts |
| Events (bottom-right) | Recent system events | Shows recent events or empty |

### 4.2 TUI Keybindings

| Key | Expected |
|-----|----------|
| `q` or `Esc` | Exits TUI cleanly, returns to terminal |
| `t` | Toggles terminal pane focus |
| Arrow keys (in terminal mode) | Navigate between terminal panes |

### 4.3 TUI Verification

- **No crash on empty data** — Fresh install should render all panels without panic
- **Resize handling** — Resize terminal window; panels should reflow
- **Clean exit** — Terminal should restore to normal state after `q`

---

## 5. Surface 4: Web API (curl)

Start the server first:

```bash
cargo run
```

**Expected:** Server starts on `http://localhost:3000`. Logs show boot sequence: database init, department registration (14 departments), tool registration, job worker started.

### 5.1 Health & System

```bash
# Health check
curl -s http://localhost:3000/api/health | jq .
```

**Expected:** `{"status": "ok"}` or similar health response.

```bash
# System status
curl -s http://localhost:3000/api/system/status | jq .
```

**Expected:** JSON with system info (uptime, departments, sessions, etc.).

### 5.2 Sessions

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

### 5.3 Departments

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

### 5.4 Agents CRUD

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
# Get agent
curl -s http://localhost:3000/api/agents/<id> | jq .
```

**Expected:** Full agent object.

```bash
# Update agent
curl -s -X PUT http://localhost:3000/api/agents/<id> \
  -H 'Content-Type: application/json' \
  -d '{"name": "updated-agent", "instructions": "Updated instructions"}' | jq .
```

**Expected:** Returns updated agent.

```bash
# Delete agent
curl -s -X DELETE http://localhost:3000/api/agents/<id>
```

**Expected:** 200 OK or 204 No Content.

### 5.5 Skills CRUD

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

```bash
# Get / Update / Delete — same pattern as agents
curl -s http://localhost:3000/api/skills/<id> | jq .
curl -s -X PUT http://localhost:3000/api/skills/<id> -H 'Content-Type: application/json' -d '{"name": "renamed"}' | jq .
curl -s -X DELETE http://localhost:3000/api/skills/<id>
```

### 5.6 Rules CRUD

```bash
curl -s http://localhost:3000/api/rules | jq .
```

**Expected:** Array with seeded rules (architecture boundaries, crate size, etc.).

```bash
# Create / Get / Update / Delete — same CRUD pattern
curl -s -X POST http://localhost:3000/api/rules \
  -H 'Content-Type: application/json' \
  -d '{"name": "test-rule", "description": "Test", "content": "Always test"}' | jq .
```

### 5.7 Hooks CRUD

```bash
curl -s http://localhost:3000/api/hooks | jq .
```

**Expected:** Array of hooks (may be empty on fresh install).

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

**Expected:** Returns created hook with `id`.

```bash
# List hook events (what events can hooks fire on)
curl -s http://localhost:3000/api/hooks/events | jq .
```

**Expected:** Array of event kind strings.

### 5.8 MCP Servers CRUD

```bash
curl -s http://localhost:3000/api/mcp-servers | jq .
```

**Expected:** Array of registered MCP servers.

```bash
# Register external MCP server
curl -s -X POST http://localhost:3000/api/mcp-servers \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "test-mcp",
    "command": "node",
    "args": ["./mcp-server.js"],
    "description": "Test MCP server"
  }' | jq .
```

### 5.9 Workflows CRUD + Execution

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
    "description": "A test workflow",
    "steps": [
      {"name": "step1", "action": "echo step 1"},
      {"name": "step2", "action": "echo step 2"}
    ]
  }' | jq .
```

**Expected:** Returns workflow with `id`.

```bash
# Run workflow (requires Ollama for agent steps)
curl -s -X POST http://localhost:3000/api/workflows/<id>/run | jq .
```

**Expected:** Returns run status. Job is enqueued.

### 5.10 Chat (God Agent) — SSE Streaming

```bash
# Chat with the god agent (requires Ollama)
curl -N http://localhost:3000/api/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "Hello, what can you do?"}'
```

**Expected:** Server-Sent Events stream:
```
data: {"type":"text_delta","text":"I"}
data: {"type":"text_delta","text":" can"}
data: {"type":"text_delta","text":" help"}
...
data: {"type":"done","output":"I can help you with..."}
```

**Verify:** Stream starts within 2-5s. Text deltas arrive incrementally. Stream ends with a `done` event.

### 5.11 Department Chat — SSE Streaming

```bash
# Chat with a specific department (requires Ollama)
curl -N http://localhost:3000/api/dept/code/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "What files are in this project?"}'
```

**Expected:** SSE stream with department-scoped response. The agent should use code-related tools (read_file, glob, grep).

```bash
# List conversations
curl -s http://localhost:3000/api/dept/code/chat/conversations | jq .
```

**Expected:** Array of conversation summaries with IDs.

```bash
# Get conversation history
curl -s http://localhost:3000/api/dept/code/chat/conversations/<id> | jq .
```

**Expected:** Full message history for that conversation.

### 5.12 Mission / Forge API

```bash
# Daily plan (requires Ollama + active session)
curl -s http://localhost:3000/api/sessions/<session-id>/mission/today | jq .
```

**Expected:** AI-generated daily plan.

```bash
# List goals
curl -s http://localhost:3000/api/sessions/<session-id>/mission/goals | jq .
```

**Expected:** Array of goals.

```bash
# Create goal
curl -s -X POST http://localhost:3000/api/sessions/<session-id>/mission/goals \
  -H 'Content-Type: application/json' \
  -d '{"title": "API test goal", "description": "Created via API", "timeframe": "week"}' | jq .
```

**Expected:** Returns goal with `id`.

```bash
# Generate executive brief
curl -s -X POST http://localhost:3000/api/brief/generate | jq .
```

**Expected:** Returns generated brief.

```bash
# Get latest brief
curl -s http://localhost:3000/api/brief/latest | jq .
```

**Expected:** Most recent brief or 404 if none generated.

### 5.13 Config & Models

```bash
# Get config
curl -s http://localhost:3000/api/config | jq .
```

**Expected:** Current configuration object.

```bash
# List available models
curl -s http://localhost:3000/api/config/models | jq .
```

**Expected:** Array of available LLM models (from Ollama + any configured API keys).

```bash
# List available tools
curl -s http://localhost:3000/api/config/tools | jq .
```

**Expected:** Array of 22+ tool definitions with names, descriptions, and JSON schemas.

```bash
# Update config
curl -s -X PUT http://localhost:3000/api/config \
  -H 'Content-Type: application/json' \
  -d '{"default_model": "llama3.2"}' | jq .
```

**Expected:** Returns updated config.

### 5.14 Analytics

```bash
curl -s http://localhost:3000/api/analytics | jq .
```

**Expected:** Analytics summary (events, sessions, jobs, etc.).

```bash
curl -s http://localhost:3000/api/analytics/dashboard | jq .
```

**Expected:** Dashboard data with department-level metrics.

```bash
curl -s http://localhost:3000/api/analytics/spend | jq .
```

**Expected:** LLM cost tracking data (tokens used, estimated cost).

### 5.15 Jobs & Approvals

```bash
# List jobs
curl -s http://localhost:3000/api/jobs | jq .
```

**Expected:** Array of jobs with status (Queued, Running, Succeeded, Failed, AwaitingApproval).

```bash
# List pending approvals
curl -s http://localhost:3000/api/approvals | jq .
```

**Expected:** Array of jobs awaiting human approval (content publish, outreach, etc.).

```bash
# Approve a job
curl -s -X POST http://localhost:3000/api/approvals/<job-id>/approve | jq .
```

**Expected:** Job status changes to Running or Succeeded.

```bash
# Reject a job
curl -s -X POST http://localhost:3000/api/approvals/<job-id>/reject | jq .
```

**Expected:** Job status changes to Cancelled.

### 5.16 User Profile

```bash
curl -s http://localhost:3000/api/profile | jq .
```

**Expected:** User profile object (name, skills, etc.). May be empty on fresh install.

```bash
curl -s -X PUT http://localhost:3000/api/profile \
  -H 'Content-Type: application/json' \
  -d '{"name": "Mehdi", "skills": {"primary": ["rust", "sveltekit"]}}' | jq .
```

**Expected:** Updated profile.

### 5.17 Help (AI-Powered)

```bash
curl -s -X POST http://localhost:3000/api/help \
  -H 'Content-Type: application/json' \
  -d '{"question": "How do I create a content draft?"}' | jq .
```

**Expected:** AI-generated help response explaining how to use the content draft feature.

---

## 6. Surface 5: Web Frontend (Browser)

Start the server (`cargo run`) and open `http://localhost:3000` in a browser.

### 6.1 Pages to Check

| URL | What to verify |
|-----|---------------|
| `/` | Dashboard loads. Shows department cards, system status, recent activity |
| `/chat` | Chat interface loads. Can type messages, SSE stream works, messages render |
| `/dept/forge` | Forge overview page. Shows mission, goals, tasks |
| `/dept/code` | Code department. Shows analysis tools |
| `/dept/content` | Content department. Shows drafting UI |
| `/dept/harvest` | Harvest department. Shows pipeline |
| `/dept/gtm` | GTM department. Shows CRM |
| `/dept/finance` | Finance department. Shows ledger |
| `/dept/[id]/chat` | Department-scoped chat works for each department |
| `/dept/[id]/agents` | Agent CRUD UI — list, create, edit, delete |
| `/dept/[id]/skills` | Skills CRUD UI |
| `/dept/[id]/rules` | Rules CRUD UI |
| `/dept/[id]/hooks` | Hooks CRUD UI |
| `/dept/[id]/mcp` | MCP server management |
| `/dept/[id]/workflows` | Workflow management |
| `/dept/[id]/config` | Department configuration |
| `/dept/[id]/events` | Event log |
| `/dept/[id]/actions` | Quick actions |
| `/dept/[id]/engine` | Engine-specific UI |
| `/dept/[id]/terminal` | Terminal pane |
| `/dept/content/calendar` | Content calendar view |
| `/dept/harvest/pipeline` | Opportunity pipeline view |
| `/dept/gtm/contacts` | CRM contacts |
| `/dept/gtm/deals` | CRM deals |
| `/dept/gtm/outreach` | Outreach sequences |
| `/dept/gtm/invoices` | Invoicing |
| `/flows` | Flow builder — create/edit DAG workflows |
| `/knowledge` | Knowledge base — ingest, search |
| `/approvals` | Approval queue — pending human approvals (sidebar badge) |
| `/settings` | Global settings |
| `/settings/spend` | Cost analytics |
| `/terminal` | Terminal view |
| `/database/tables` | Database browser — list tables |
| `/database/schema` | Schema viewer |
| `/database/sql` | SQL runner |

### 6.2 Frontend Checklist

| Feature | How to test | Expected |
|---------|------------|----------|
| Navigation | Click each sidebar item | Page loads without error |
| Department switching | Click different departments | URL updates, content refreshes |
| Dark mode | Toggle theme (if available) | CSS variables swap, no broken colors |
| Chat streaming | Send a message in `/chat` | Text appears character by character |
| Tool call cards | Chat with code dept, ask about files | ToolCallCard renders with tool name + result |
| Approval cards | Create content draft that needs approval | ApprovalCard appears in sidebar badge |
| Department colors | Visit each department | Each has its own oklch color accent |
| Responsive | Resize browser window | Layout adapts, no overflow |
| Error handling | Visit `/dept/nonexistent` | Error page or redirect, no crash |
| Command palette | Keyboard shortcut (Cmd+K or similar) | CommandPalette opens |
| Onboarding | Fresh install, first visit | OnboardingChecklist or ProductTour appears |

---

## 7. Surface 6: MCP Server

### 7.1 Stdio Transport

```bash
# Start MCP server
cargo run -- --mcp
```

**Expected:** Server enters stdio JSON-RPC mode. No HTTP server starts. Reads JSON from stdin, writes to stdout.

```bash
# Test with echo (in a separate terminal or pipe)
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}' | cargo run -- --mcp
```

**Expected:** JSON response with server capabilities and tool list.

### 7.2 HTTP Transport

```bash
cargo run -- --mcp-http
```

**Expected:** MCP server starts on HTTP with `/mcp` (POST) and `/mcp/sse` (GET) endpoints.

### 7.3 MCP Tools

| Tool | Input | Expected |
|------|-------|----------|
| `session_list` | `{}` | Array of sessions |
| `session_create` | `{"name": "mcp-test"}` | New session with ID |
| `mission_today` | `{"session_id": "<id>"}` | Daily plan text |
| `mission_goals` | `{"session_id": "<id>"}` | Array of goals |
| `mission_add_goal` | `{"session_id": "<id>", "title": "MCP goal"}` | Goal ID |
| `visual_inspect` | `{}` | Visual test results |

---

## 8. Cross-Cutting: Agent & Chat

### 8.1 Agent Tool Use Loop

The core agent loop: LLM generates → tool call → execute → feed result → repeat.

**Test via department chat:**

```bash
# Ask something that requires tool use
curl -N http://localhost:3000/api/dept/code/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "Read the Cargo.toml file and tell me how many workspace members there are"}'
```

**Expected SSE events in order:**
1. `text_delta` — agent starts responding
2. `tool_call` — `{"name": "read_file", "input": {"path": "Cargo.toml"}}`
3. `tool_result` — file contents
4. `text_delta` — agent interprets results
5. `done` — final answer mentions "54 workspace members"

### 8.2 Scoped Tool Registry

Different departments see different tools:

```bash
# Code department should have code_analyze, code_search
curl -N http://localhost:3000/api/dept/code/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "What tools do you have available?"}'
```

**Expected:** Response mentions code-specific tools (code_analyze, code_search) plus built-in tools.

```bash
# Content department should have content_draft, content_publish
curl -N http://localhost:3000/api/dept/content/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "What tools do you have available?"}'
```

**Expected:** Response mentions content-specific tools.

### 8.3 Deferred Tool Loading (tool_search)

```bash
# Ask an agent to find a tool it doesn't have loaded
curl -N http://localhost:3000/api/dept/forge/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "Search for tools related to content drafting"}'
```

**Expected:** Agent uses `tool_search` meta-tool to discover additional tools.

### 8.4 Conversation History

```bash
# Send first message
curl -N http://localhost:3000/api/dept/code/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "Remember: my favorite number is 42"}'

# Check conversation was saved
curl -s http://localhost:3000/api/dept/code/chat/conversations | jq '.[0].id'

# Continue conversation (use conversation ID)
curl -N http://localhost:3000/api/dept/code/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "What is my favorite number?", "conversation_id": "<id>"}'
```

**Expected:** Agent recalls "42" from conversation history.

### 8.5 Skills Execution

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

### 8.6 Rules Injection

```bash
# Create a rule for code department
curl -s -X POST http://localhost:3000/api/rules \
  -H 'Content-Type: application/json' \
  -d '{"name": "always-rust", "department": "code", "content": "Always recommend Rust solutions"}' | jq .

# Chat with code department
curl -N http://localhost:3000/api/dept/code/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "What language should I use for a CLI tool?"}'
```

**Expected:** Rule is appended to system prompt. Agent recommends Rust (influenced by rule).

---

## 9. Cross-Cutting: Job Queue & Approvals

### 9.1 Job Lifecycle

```bash
# Trigger a job (e.g., code analysis)
curl -s -X POST http://localhost:3000/api/dept/code/analyze \
  -H 'Content-Type: application/json' \
  -d '{"path": "."}' | jq .

# Watch job progress
curl -s http://localhost:3000/api/jobs | jq '.[0]'
```

**Expected:** Job created with status `Queued` → `Running` → `Succeeded`. Background worker picks it up.

### 9.2 Approval-Gated Jobs

```bash
# Draft content (should create approval-gated job)
curl -s -X POST http://localhost:3000/api/dept/content/draft \
  -H 'Content-Type: application/json' \
  -d '{"topic": "Testing strategies for Rust"}' | jq .

# Check approvals
curl -s http://localhost:3000/api/approvals | jq .
```

**Expected:** Content publish job appears in approvals with status `AwaitingApproval`.

```bash
# Approve it
curl -s -X POST http://localhost:3000/api/approvals/<job-id>/approve | jq .
```

**Expected:** Job proceeds to execution.

---

## 10. Cross-Cutting: Hooks & Events

### 10.1 Command Hook

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

### 10.2 HTTP Hook

```bash
# Create HTTP hook (posts to any endpoint)
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

### 10.3 Event Query

```bash
# Query events by department
curl -s http://localhost:3000/api/dept/forge/events | jq .

# Query session events
curl -s http://localhost:3000/api/sessions/<id>/events | jq .
```

**Expected:** Array of events with `kind`, `source`, `payload`, `created_at`.

---

## 11. Engine-Specific Tests

### 11.1 Code Engine

```bash
# Analyze codebase
curl -s -X POST http://localhost:3000/api/dept/code/analyze \
  -H 'Content-Type: application/json' \
  -d '{"path": "."}' | jq .
```

**Expected:** Returns analysis with file count, LOC, complexity metrics.

```bash
# Search symbols
curl -s "http://localhost:3000/api/dept/code/search?query=DepartmentApp" | jq .
```

**Expected:** BM25 ranked results with file paths and line numbers.

### 11.2 Content Engine

```bash
# Draft content
curl -s -X POST http://localhost:3000/api/dept/content/draft \
  -H 'Content-Type: application/json' \
  -d '{"topic": "Building CLI tools in Rust"}' | jq .
```

**Expected:** Returns markdown draft with title, body, metadata.

```bash
# Code-to-content pipeline
curl -s -X POST http://localhost:3000/api/dept/content/from-code \
  -H 'Content-Type: application/json' \
  -d '{"path": ".", "topic": "architecture"}' | jq .
```

**Expected:** Analyzes code first, then generates content based on analysis.

```bash
# List content
curl -s http://localhost:3000/api/dept/content/list | jq .
```

**Expected:** Array of content items with status (Draft, Adapted, Scheduled, Published).

```bash
# Approve content
curl -s -X PATCH http://localhost:3000/api/dept/content/<id>/approve | jq .
```

**Expected:** Content status changes. Approval status updated.

```bash
# Schedule content
curl -s -X POST http://localhost:3000/api/dept/content/schedule \
  -H 'Content-Type: application/json' \
  -d '{"content_id": "<id>", "scheduled_at": "2026-04-01T09:00:00Z"}' | jq .
```

**Expected:** Content marked as Scheduled with timestamp.

### 11.3 Harvest Engine

```bash
# Scan for opportunities (requires Ollama)
curl -s -X POST http://localhost:3000/api/dept/harvest/scan | jq .
```

**Expected:** Scans configured sources, returns discovered opportunities.

```bash
# Score an opportunity
curl -s -X POST http://localhost:3000/api/dept/harvest/score \
  -H 'Content-Type: application/json' \
  -d '{"opportunity_id": "<id>"}' | jq .
```

**Expected:** Returns score (0.0–1.0) with scoring rationale.

```bash
# Generate proposal
curl -s -X POST http://localhost:3000/api/dept/harvest/proposal \
  -H 'Content-Type: application/json' \
  -d '{"opportunity_id": "<id>"}' | jq .
```

**Expected:** AI-generated proposal draft.

```bash
# Pipeline stats
curl -s http://localhost:3000/api/dept/harvest/pipeline | jq .
```

**Expected:** Counts by stage (Cold, Contacted, Qualified, ProposalSent, Won, Lost).

```bash
# Advance opportunity
curl -s -X POST http://localhost:3000/api/dept/harvest/advance \
  -H 'Content-Type: application/json' \
  -d '{"opportunity_id": "<id>", "stage": "Contacted"}' | jq .
```

**Expected:** Opportunity stage updated.

```bash
# Record outcome
curl -s -X POST http://localhost:3000/api/dept/harvest/outcome \
  -H 'Content-Type: application/json' \
  -d '{"opportunity_id": "<id>", "outcome": "Won", "value": 5000.0}' | jq .
```

**Expected:** Outcome recorded with value.

### 11.4 GTM Engine

```bash
# Create contact
curl -s -X POST http://localhost:3000/api/dept/gtm/contacts \
  -H 'Content-Type: application/json' \
  -d '{"name": "Alice", "emails": ["alice@example.com"], "company": "Acme"}' | jq .
```

**Expected:** Contact created with ID.

```bash
# List contacts
curl -s http://localhost:3000/api/dept/gtm/contacts | jq .
```

```bash
# List deals
curl -s http://localhost:3000/api/dept/gtm/deals | jq .
```

```bash
# Create outreach sequence
curl -s -X POST http://localhost:3000/api/dept/gtm/outreach/sequences \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "cold-intro",
    "steps": [
      {"kind": "email", "template": "Hi {{name}}, ..."},
      {"kind": "follow_up", "delay_days": 3, "template": "Following up..."}
    ]
  }' | jq .
```

**Expected:** Sequence created.

```bash
# Create invoice
curl -s -X POST http://localhost:3000/api/dept/gtm/invoices \
  -H 'Content-Type: application/json' \
  -d '{
    "contact_id": "<id>",
    "amount": 5000.00,
    "description": "Consulting services",
    "due_date": "2026-04-15"
  }' | jq .
```

**Expected:** Invoice created with status (draft/sent/paid).

```bash
# Get invoice
curl -s http://localhost:3000/api/dept/gtm/invoices/<id> | jq .
```

```bash
# Update invoice status
curl -s -X POST http://localhost:3000/api/dept/gtm/invoices/<id>/status \
  -H 'Content-Type: application/json' \
  -d '{"status": "sent"}' | jq .
```

---

## 12. Database Browser

```bash
# List tables
curl -s http://localhost:3000/api/db/tables | jq .
```

**Expected:** Array of table names from SQLite database.

```bash
# Get table schema
curl -s http://localhost:3000/api/db/tables/events/schema | jq .
```

**Expected:** Column definitions (name, type, nullable, primary key).

```bash
# Get table rows
curl -s "http://localhost:3000/api/db/tables/events/rows?limit=10" | jq .
```

**Expected:** Array of row objects (up to 10).

```bash
# Execute SQL
curl -s -X POST http://localhost:3000/api/db/sql \
  -H 'Content-Type: application/json' \
  -d '{"sql": "SELECT COUNT(*) as count FROM events"}' | jq .
```

**Expected:** `[{"count": N}]` where N is the event count.

**Security check:** Try SQL injection:
```bash
curl -s -X POST http://localhost:3000/api/db/sql \
  -H 'Content-Type: application/json' \
  -d '{"sql": "DROP TABLE events"}' | jq .
```

**Expected:** Should be rejected or run in read-only mode. Destructive SQL should not execute.

---

## 13. Knowledge / RAG

```bash
# Ingest knowledge
curl -s -X POST http://localhost:3000/api/knowledge/ingest \
  -H 'Content-Type: application/json' \
  -d '{"content": "Rust is a systems programming language focused on safety and performance.", "source": "manual"}' | jq .
```

**Expected:** Returns knowledge entry ID. Content is embedded and stored in vector DB.

```bash
# Search knowledge
curl -s -X POST http://localhost:3000/api/knowledge/search \
  -H 'Content-Type: application/json' \
  -d '{"query": "programming language safety"}' | jq .
```

**Expected:** Array of results ranked by semantic similarity.

```bash
# Hybrid search (keyword + semantic)
curl -s -X POST http://localhost:3000/api/knowledge/hybrid-search \
  -H 'Content-Type: application/json' \
  -d '{"query": "Rust safety"}' | jq .
```

**Expected:** Results combining BM25 keyword and vector similarity scores.

```bash
# Knowledge stats
curl -s http://localhost:3000/api/knowledge/stats | jq .
```

**Expected:** Count of entries, index status.

```bash
# Related knowledge
curl -s "http://localhost:3000/api/knowledge/related?id=<id>" | jq .
```

**Expected:** Semantically similar entries.

```bash
# Delete knowledge
curl -s -X DELETE http://localhost:3000/api/knowledge/<id>
```

**Expected:** Entry removed from store and vector index.

---

## 14. Flow Engine (DAG Workflows)

### 14.1 CRUD

```bash
# Create a flow
curl -s -X POST http://localhost:3000/api/flows \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "test-flow",
    "description": "A test DAG workflow",
    "nodes": [
      {"id": "start", "type": "code", "config": {"command": "echo start"}},
      {"id": "check", "type": "condition", "config": {"expression": "true"}},
      {"id": "end", "type": "code", "config": {"command": "echo done"}}
    ],
    "edges": [
      {"from": "start", "to": "check"},
      {"from": "check", "to": "end"}
    ]
  }' | jq .
```

**Expected:** Flow created with ID. DAG is validated (no cycles).

```bash
# List flows
curl -s http://localhost:3000/api/flows | jq .
```

```bash
# Get flow
curl -s http://localhost:3000/api/flows/<id> | jq .
```

```bash
# Update flow
curl -s -X PUT http://localhost:3000/api/flows/<id> \
  -H 'Content-Type: application/json' \
  -d '{"name": "updated-flow"}' | jq .
```

```bash
# Delete flow
curl -s -X DELETE http://localhost:3000/api/flows/<id>
```

### 14.2 Execution

```bash
# Run a flow
curl -s -X POST http://localhost:3000/api/flows/<id>/run | jq .
```

**Expected:** Returns execution ID. Nodes execute in topological order.

```bash
# List executions
curl -s http://localhost:3000/api/flows/<id>/executions | jq .
```

**Expected:** Array of executions with status.

```bash
# Get execution details
curl -s http://localhost:3000/api/flows/executions/<exec-id> | jq .
```

**Expected:** Node-by-node status, outputs, timing.

```bash
# Get checkpoint
curl -s http://localhost:3000/api/flows/executions/<exec-id>/checkpoint | jq .
```

**Expected:** Serialized execution state for resumption.

```bash
# Resume suspended execution
curl -s -X POST http://localhost:3000/api/flows/executions/<exec-id>/resume | jq .
```

```bash
# Retry a failed node
curl -s -X POST http://localhost:3000/api/flows/executions/<exec-id>/retry/<node-id> | jq .
```

### 14.3 Node Types

```bash
curl -s http://localhost:3000/api/flows/node-types | jq .
```

**Expected:** `["code", "condition", "agent"]`. If `RUSVEL_FLOW_PARALLEL_EVALUATE=1`, also includes `parallel_evaluate`.

### 14.4 Templates

```bash
curl -s http://localhost:3000/api/flows/templates/cross-engine-handoff | jq .
```

**Expected:** Pre-built flow template showing cross-engine data handoff.

---

## 15. Capability Engine (!build)

### 15.1 Via API

```bash
curl -s -X POST http://localhost:3000/api/capability/build \
  -H 'Content-Type: application/json' \
  -d '{"description": "A GitHub PR review agent that checks code style"}' | jq .
```

**Expected:** Returns a bundle of created entities:
- Agent with instructions for PR review
- Skills for common review actions
- Rules for code style checks
- Possibly MCP server config for GitHub
- Possibly hooks for PR events

### 15.2 Via Chat

```bash
curl -N http://localhost:3000/api/dept/forge/chat \
  -H 'Content-Type: application/json' \
  -d '{"message": "!build a daily standup summary agent"}'
```

**Expected:** Agent detects `!build` prefix, invokes capability engine, creates entities, reports what was installed.

---

## 16. Terminal & Browser Ports

### 16.1 Terminal Panes

```bash
# WebSocket terminal connection
websocat ws://localhost:3000/api/terminal/ws
```

**Expected:** WebSocket connection established. Can send commands and receive output.

```bash
# Get department terminal pane
curl -s http://localhost:3000/api/terminal/dept/code | jq .
```

**Expected:** Terminal pane info for code department.

### 16.2 Browser (CDP)

Requires Chrome running with `--remote-debugging-port=9222`.

```bash
# Check status
curl -s http://localhost:3000/api/browser/status | jq .
```

**Expected:** Connection status (connected/disconnected).

```bash
# Connect
curl -s -X POST http://localhost:3000/api/browser/connect \
  -H 'Content-Type: application/json' \
  -d '{"endpoint": "http://localhost:9222"}' | jq .
```

**Expected:** Connection established.

```bash
# List tabs
curl -s http://localhost:3000/api/browser/tabs | jq .
```

**Expected:** Array of open Chrome tabs.

```bash
# Observe tab (screenshot + DOM)
curl -s -X POST http://localhost:3000/api/browser/observe/0 | jq .
```

**Expected:** Screenshot data and DOM snapshot.

```bash
# Perform action
curl -s -X POST http://localhost:3000/api/browser/act \
  -H 'Content-Type: application/json' \
  -d '{"action": "click", "selector": "#submit-button"}' | jq .
```

**Expected:** Action performed on the page.

---

## 17. Configuration & Settings

### 17.1 Config File

```bash
cat ~/.rusvel/config.toml
```

**Expected:** TOML config with sections for LLM, departments, sessions.

### 17.2 Department Config Cascade

```bash
# Get department config (inherits from global)
curl -s http://localhost:3000/api/dept/code/config | jq .

# Update department-specific config
curl -s -X PUT http://localhost:3000/api/dept/code/config \
  -H 'Content-Type: application/json' \
  -d '{"model": "codellama"}' | jq .
```

**Expected:** Department config overrides global. Session config overrides department.

---

## 18. Webhook & Cron

### 18.1 Webhooks

```bash
# Create webhook
curl -s -X POST http://localhost:3000/api/webhooks \
  -H 'Content-Type: application/json' \
  -d '{"event_kind": "forge.pipeline.requested", "url": "http://localhost:3000/api/forge/pipeline"}' | jq .
```

**Expected:** Webhook registered with ID.

```bash
# List webhooks
curl -s http://localhost:3000/api/webhooks | jq .
```

```bash
# Trigger webhook (POST to its endpoint)
curl -s -X POST http://localhost:3000/api/webhooks/<id> \
  -H 'Content-Type: application/json' \
  -d '{"session_id": "<session-id>"}' | jq .
```

**Expected:** Webhook fires, enqueues job based on event_kind.

### 18.2 Cron Scheduling

```bash
# Create cron schedule
curl -s -X POST http://localhost:3000/api/cron \
  -H 'Content-Type: application/json' \
  -d '{"name": "daily-brief", "cron": "0 9 * * *", "job_kind": "Custom", "payload": {"action": "brief"}}' | jq .
```

**Expected:** Schedule created with ID.

```bash
# List schedules
curl -s http://localhost:3000/api/cron | jq .
```

```bash
# Manual tick (trigger due schedules now)
curl -s -X POST http://localhost:3000/api/cron/tick | jq .
```

**Expected:** Any due schedules fire immediately.

```bash
# Get / Update / Delete schedule
curl -s http://localhost:3000/api/cron/<id> | jq .
curl -s -X PUT http://localhost:3000/api/cron/<id> -H 'Content-Type: application/json' -d '{"cron": "0 10 * * *"}' | jq .
curl -s -X DELETE http://localhost:3000/api/cron/<id>
```

---

## 19. Automated Test Suite (cargo test)

```bash
# Full workspace (from repo root)
cargo test
```

**Expected:** ~635 tests, 0 failures.

```bash
# Individual crates
cargo test -p rusvel-core       # Core types + traits
cargo test -p rusvel-db         # Database stores
cargo test -p rusvel-api        # API routes
cargo test -p rusvel-llm        # LLM providers
cargo test -p forge-engine      # Forge engine (15 tests)
cargo test -p content-engine    # Content engine (7 tests)
cargo test -p harvest-engine    # Harvest engine (12 tests)
cargo test -p code-engine       # Code engine
cargo test -p flow-engine       # Flow engine
cargo test -p rusvel-tool       # Tool registry
cargo test -p rusvel-agent      # Agent runtime
cargo test -p rusvel-memory     # FTS5 memory
cargo test -p rusvel-event      # Event bus
```

### Line Coverage

```bash
# Requires: rustup component add llvm-tools-preview && cargo install cargo-llvm-cov
./scripts/coverage.sh
```

**Expected:** HTML report generated. Check `docs/testing/coverage-strategy.md` for targets.

### Benchmark

```bash
cargo bench -p rusvel-app --bench boot
```

**Expected:** Criterion boot-time benchmark (SQLite init + department registry).

---

## 20. Visual Regression Tests

```bash
cd frontend
pnpm install
pnpm test:visual
```

**Expected:** Playwright takes screenshots and compares against baselines.

```bash
# Update baselines
pnpm test:e2e:update
```

```bash
# AI-powered diff analysis
pnpm test:analyze
```

**Expected:** Claude Vision analyzes visual diffs and reports issues.

```bash
# Via API
curl -s -X POST http://localhost:3000/api/system/visual-test | jq .
```

```bash
# Self-correction loop
curl -s -X POST http://localhost:3000/api/system/visual-report/self-correct | jq .
```

**Expected:** Auto-generates fix skills/rules based on visual diff analysis.

---

## 21. Smoke Test Checklist

Quick pass to verify everything works after a build. Run these in order:

### Phase 1: Build & Boot (no Ollama needed)

| # | Command | Pass if |
|---|---------|---------|
| 1 | `cargo build` | Compiles without errors |
| 2 | `cargo test` | ~635 tests pass |
| 3 | `cargo run -- --help` | Shows help text with all subcommands |
| 4 | `cargo run -- session create smoke-test` | Prints session UUID |
| 5 | `cargo run -- session list` | Shows the smoke-test session |
| 6 | `cargo run -- finance status` | Prints status (no crash) |
| 7 | `cargo run -- code analyze .` | Prints analysis results |
| 8 | `cargo run -- harvest pipeline` | Prints pipeline (all zeros OK) |

### Phase 2: Web Server (no Ollama needed)

Start `cargo run` in background, then:

| # | Command | Pass if |
|---|---------|---------|
| 9 | `curl -s localhost:3000/api/health` | Returns 200 |
| 10 | `curl -s localhost:3000/api/departments \| jq length` | Returns 14 |
| 11 | `curl -s localhost:3000/api/agents \| jq length` | Returns ≥ 7 (seeded agents) |
| 12 | `curl -s localhost:3000/api/skills \| jq length` | Returns ≥ 5 (seeded skills) |
| 13 | `curl -s localhost:3000/api/rules \| jq length` | Returns ≥ 5 (seeded rules) |
| 14 | `curl -s localhost:3000/api/config/tools \| jq length` | Returns ≥ 22 |
| 15 | `curl -s localhost:3000/api/db/tables \| jq length` | Returns ≥ 5 |
| 16 | Open `http://localhost:3000` in browser | Dashboard renders |

### Phase 3: LLM Features (requires Ollama)

| # | Command | Pass if |
|---|---------|---------|
| 17 | `cargo run -- forge mission today` | Streams daily plan |
| 18 | `curl -N localhost:3000/api/chat -H 'Content-Type: application/json' -d '{"message":"hi"}'` | SSE stream with text deltas |
| 19 | `curl -N localhost:3000/api/dept/code/chat -H 'Content-Type: application/json' -d '{"message":"list files"}'` | SSE stream with tool calls |
| 20 | `cargo run -- content draft "test"` | Streams content draft |

### Phase 4: Interactive Surfaces

| # | Action | Pass if |
|---|--------|---------|
| 21 | `cargo run -- shell` → type `help` → type `exit` | REPL starts, shows help, exits cleanly |
| 22 | `cargo run -- shell` → `use finance` → `status` → `back` | Context switching works |
| 23 | `cargo run -- --tui` → press `q` | TUI renders 4 panels, exits cleanly |

### Phase 5: CRUD Cycle

Pick any entity (agents, skills, rules, hooks, workflows) and verify the full cycle:

| # | Action | Pass if |
|---|--------|---------|
| 24 | POST create | Returns 200 with ID |
| 25 | GET by ID | Returns created entity |
| 26 | GET list | Entity appears in list |
| 27 | PUT update | Returns updated entity |
| 28 | GET by ID again | Shows updated fields |
| 29 | DELETE | Returns 200/204 |
| 30 | GET by ID again | Returns 404 |

---

## Appendix A: Useful Environment Combos

### Minimal (no AI)
```bash
# Test all non-LLM features
cargo run
# Then use curl for CRUD, DB browser, config, etc.
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

## Appendix B: Testing Tools

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

## Appendix C: Playbooks (Multi-Step Pipelines)

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

# Run playbook
curl -s -X POST http://localhost:3000/api/playbooks/<id>/run | jq .

# Check run status
curl -s http://localhost:3000/api/playbooks/runs | jq .
curl -s http://localhost:3000/api/playbooks/runs/<run-id> | jq .
```

## Appendix D: Starter Kits

```bash
# List available kits
curl -s http://localhost:3000/api/kits | jq .

# Get kit details
curl -s http://localhost:3000/api/kits/<id> | jq .

# Install kit (creates agents, skills, rules, etc.)
curl -s -X POST http://localhost:3000/api/kits/<id>/install | jq .
```

## Appendix E: Notification System

```bash
# Send a notification (requires RUSVEL_TELEGRAM_BOT_TOKEN)
curl -s -X POST http://localhost:3000/api/system/notify \
  -H 'Content-Type: application/json' \
  -d '{"message": "Test notification from RUSVEL"}' | jq .
```

**Expected with token:** Message sent via Telegram. Returns success.
**Expected without token:** Returns error or no-op.

## Appendix F: Forge Pipeline (Webhook-Triggered)

```bash
# Register webhook for pipeline
curl -s -X POST http://localhost:3000/api/webhooks \
  -H 'Content-Type: application/json' \
  -d '{"event_kind": "forge.pipeline.requested"}' | jq .

# Trigger pipeline via webhook
curl -s -X POST http://localhost:3000/api/webhooks/<id> \
  -H 'Content-Type: application/json' \
  -d '{"session_id": "<session-id>"}' | jq .

# Or directly
curl -s -X POST http://localhost:3000/api/forge/pipeline \
  -H 'Content-Type: application/json' \
  -d '{"session_id": "<session-id>"}' | jq .
```

**Expected:** Enqueues `forge.pipeline` job. Worker runs `ForgeEngine::orchestrate_pipeline`.
