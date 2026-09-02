
## What Are Agents?

Agents are AI personas with specific roles, instructions, tools, and budgets. Each department has a default agent, and you can create custom agents for specialized tasks.

Agents are powered by the **AgentRuntime** (in `rusvel-agent`), which wraps LLM access, tool execution, streaming, and memory into a single orchestration layer.

## Agent Profile Structure

Every agent has:

```
name            — Display name (e.g., "rust-engine")
role            — One-line role description
instructions    — System prompt guiding behavior
model           — Which LLM to use (provider + model name)
allowed_tools   — List of tools the agent can invoke
capabilities    — What the agent can do (code_analysis, content_creation, etc.)
budget_limit    — Optional spending cap per run
department      — Which department this agent belongs to
```

## Seeded Agents

RUSVEL ships with 5 pre-configured agents:

| Agent | Department | Role |
|-------|-----------|------|
| **rust-engine** | Code | Rust development -- writes, tests, and refactors Rust code |
| **svelte-ui** | Code | Frontend development -- SvelteKit components and styling |
| **test-writer** | Code | Writes unit and integration tests for existing code |
| **content-writer** | Content | Drafts blog posts, social media content, and documentation |
| **proposal-writer** | Harvest | Writes proposals for freelance opportunities |

## Creating Custom Agents

### Via the Web UI

1. Navigate to any department
2. Open the **Agents** tab in the department panel
3. Click **"Add Agent"**
4. Fill in name, role, instructions, and model
5. Save

### Via the API

```bash
curl -X POST http://localhost:3000/api/agents \
  -H "Content-Type: application/json" \
  -d '{
    "name": "seo-analyst",
    "role": "SEO and keyword research specialist",
    "instructions": "You analyze websites for SEO performance...",
    "model": "claude-sonnet-5",
    "department": "distro",
    "capabilities": ["seo"]
  }'
```

### Managing Agents

```bash
# List all agents
curl http://localhost:3000/api/agents

# Get a specific agent
curl http://localhost:3000/api/agents/<id>

# Update an agent
curl -X PUT http://localhost:3000/api/agents/<id> \
  -H "Content-Type: application/json" \
  -d '{"instructions": "Updated instructions..."}'

# Delete an agent
curl -X DELETE http://localhost:3000/api/agents/<id>
```

## Model Selection and ModelTier Routing

Agents can use any configured LLM provider:

| Provider | Example Models |
|----------|---------------|
| Ollama | `llama3.1`, `mistral`, `codellama` |
| Claude API | `claude-sonnet-5`, `claude-opus-5` |
| Claude CLI | Real streaming via Claude CLI binary |
| OpenAI | `gpt-4o`, `gpt-4o-mini` |

The model is set per agent and falls back to the department default, then the global default.

**ModelTier routing** automatically classifies models into three tiers -- Haiku (fast/cheap), Sonnet (balanced), Opus (powerful/expensive). The **CostTracker** in MetricStore logs token usage and costs per tier, enabling budget-aware model selection.

## Tools and ScopedToolRegistry

Agents have access to **22+ tools**:

- **10 built-in tools:** `read_file`, `write_file`, `edit_file`, `glob`, `grep`, `bash`, `git_status`, `git_diff`, `git_log`, and `tool_search` (meta-tool)
- **12 engine tools:** harvest (5: scan, score, propose, list, pipeline), content (5: draft, adapt, publish, list, approve), code (2: analyze, search)

The **ScopedToolRegistry** filters tools per department -- the Content department sees content tools, the Code department sees code tools. This prevents tool overload and keeps agents focused.

**Deferred tool loading** via the `tool_search` meta-tool saves ~85% of tool-description tokens. Instead of loading all tool schemas upfront, agents discover tools on-demand by searching for capabilities they need.

## Agent Budgets

Set a `budget_limit` to cap how much an agent can spend per run. This is useful for expensive operations like long code generation sessions.

When the budget is exceeded, the agent pauses and requests approval to continue.

## How Agents Execute

When you chat with a department, here is what happens:

1. Your message goes to the department's agent
2. The **AgentRuntime** loads the system prompt (department prompt + active rules via `load_rules_for_engine()`)
3. The **ScopedToolRegistry** provides department-specific tools
4. The LLM generates a response via `run_streaming()`, emitting **AgentEvents** (text chunks, tool calls, tool results)
5. Tool calls are executed and results fed back to the LLM in a tool-use loop
6. **LlmStreamEvent** provides character-by-character SSE streaming to the frontend
7. An event is logged with tokens used and tools called

Engines never call the LLM directly (ADR-009). They always go through the AgentPort, which handles prompt construction, tool routing, retries, and memory injection.

## Frontend Components

The chat interface includes:

- **ToolCallCard** -- renders tool invocations inline in the chat
- **ApprovalCard** -- shows approval requests for sensitive operations
- **ApprovalQueue** -- sidebar badge showing pending approvals across all departments
