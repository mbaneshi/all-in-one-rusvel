
## Start the Server

From the project root, run:

```bash
cargo run
```

RUSVEL starts the API server on port 3000. You will see:

```
RUSVEL API listening on 0.0.0.0:3000
```

Open your browser to [http://localhost:3000](http://localhost:3000).

## Create Your First Session

Sessions are RUSVEL's workspaces. Everything -- goals, events, conversations, agent runs -- lives inside a session.

### Via the Web UI

The frontend guides you through creating a session on first load. The onboarding checklist tracks your progress:

1. Create a session
2. Add a goal
3. Generate a daily plan
4. Chat with a department
5. Create an agent

### Via the CLI

```bash
# Create a session
cargo run -- session create "My Startup"

# Output:
# Session created: My Startup
#   ID: a1b2c3d4-...  (set as active session)
```

The session ID is saved to `~/.rusvel/active_session` and used by all subsequent CLI commands.

### List and Switch Sessions

```bash
# List all sessions
cargo run -- session list

# Switch active session
cargo run -- session switch <session-id>
```

## LLM Configuration

RUSVEL supports four LLM providers. On first run, it auto-detects Ollama if running locally.

| Provider | Setup | Best For |
|----------|-------|----------|
| **Ollama** | Auto-detected if running | Free local inference |
| **Claude API** | Set `ANTHROPIC_API_KEY` env var | High-quality reasoning |
| **Claude CLI** | Install `claude` CLI tool | Claude Max subscription |
| **OpenAI** | Set `OPENAI_API_KEY` env var | GPT models |

You can configure the default model per department in the Settings page or via the API:

```bash
curl -X PUT http://localhost:3000/api/config \
  -H "Content-Type: application/json" \
  -d '{"model": "claude-sonnet-5"}'
```

## The MCP Server

RUSVEL can also run as an MCP (Model Context Protocol) server for integration with Claude Desktop or other MCP clients:

```bash
cargo run -- --mcp
```

This starts a stdio JSON-RPC server instead of the web server.

## Directory Structure

After first run, RUSVEL creates:

```
~/.rusvel/
├── active_session    # UUID of the current session
├── config.toml       # Global configuration
└── rusvel.db         # SQLite database (WAL mode)
```

## Next Steps

With your session created, head to [First Mission](/getting-started/first-mission/) to set goals and generate your first daily plan.
