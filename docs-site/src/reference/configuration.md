
## Directory Structure

RUSVEL stores its configuration and data in `~/.rusvel/`:

```
~/.rusvel/
├── active_session       # UUID of the currently active session
├── config.toml          # Global configuration
├── profile.toml         # User profile (name, email, bio)
└── rusvel.db            # SQLite database (WAL mode)
```

## Config Hierarchy

Configuration follows a three-layer cascade. More specific layers override less specific ones:

```
Global config  →  Department config  →  Session config
```

For example, you can set a default model globally, override it for the Code department to use a more capable model, and further override it for a specific session.

## Global Configuration (config.toml)

```toml
# Default LLM model for all departments
model = "claude-sonnet-4-20250514"

# Default effort level: "low", "medium", "high"
effort = "medium"

# API server settings
[server]
host = "0.0.0.0"
port = 3000

# LLM provider configuration
[llm]
default_provider = "ollama"

[llm.ollama]
base_url = "http://localhost:11434"

[llm.openai]
# Key loaded from OPENAI_API_KEY env var
model = "gpt-4o"

[llm.claude]
# Key loaded from ANTHROPIC_API_KEY env var
model = "claude-sonnet-4-20250514"
```

## User Profile (profile.toml)

```toml
name = "Your Name"
email = "you@example.com"
bio = "Solo founder building products with AI"
company = "Your Company"
```

The profile is used by agents for context -- the Content department knows your name for bylines, the GTM department uses your company name in outreach, etc.

## Department Configuration

Department-level config is set via the API or the Settings page in the UI:

```bash
# Set Code department to use high effort and a specific model
curl -X PUT http://localhost:3000/api/dept/code/config \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "effort": "high",
    "permission_mode": "default",
    "add_dirs": ["."]
  }'
```

### Department Config Options

| Key | Type | Description |
|-----|------|------------|
| `model` | string | LLM model to use for this department |
| `effort` | string | Response depth: `low`, `medium`, `high` |
| `permission_mode` | string | Tool permissions: `default`, `restricted` |
| `add_dirs` | string[] | Working directories (for Code department) |
| `system_prompt` | string | Override the department system prompt |
| `max_turns` | number | Max conversation turns per request |

## Department Registry (DepartmentApp Pattern)

Since ADR-014, departments are no longer configured via TOML files. Each department is a self-contained `dept-*` crate implementing the `DepartmentApp` trait, which declares its capabilities via a `DepartmentManifest`:

```rust
pub struct DepartmentManifest {
    pub id: String,              // e.g., "code"
    pub name: String,            // e.g., "Code"
    pub title: String,           // e.g., "Code Intelligence"
    pub icon: String,            // e.g., "terminal"
    pub color: String,           // e.g., "emerald"
    pub system_prompt: String,   // Department-specific system prompt
    pub capabilities: Vec<String>,
    pub tabs: Vec<String>,
    pub quick_actions: Vec<QuickAction>,
}
```

The host (`rusvel-app`) collects manifests from all 14 `dept-*` crates at boot to generate the `DepartmentRegistry`. Adding a new department means adding a new `dept-*` crate -- zero changes to `rusvel-core`.

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `ANTHROPIC_API_KEY` | Claude API authentication |
| `OPENAI_API_KEY` | OpenAI API authentication |
| `RUSVEL_DB_PATH` | *Not implemented in Rust binary* — DB file is `{data_dir}/rusvel.db` (default `~/.rusvel/rusvel.db`). |
| `RUSVEL_CONFIG_DIR` | Override config directory (default: `~/.rusvel`) |
| `RUST_LOG` | Logging level (e.g., `info`, `debug`, `rusvel_api=debug`) |
| `RUSVEL_RATE_LIMIT` | API rate limit in requests/second (default: 100) |
| `RUSVEL_API_READ_TOKEN` | Read-only API bearer token (GET/HEAD/OPTIONS only) |

## Database

RUSVEL uses SQLite in WAL (Write-Ahead Logging) mode for concurrent reads. The database file is at `~/.rusvel/rusvel.db` by default.

The database stores: sessions, goals, events, conversations, agents, skills, rules, workflows, hooks, MCP server configs, and analytics data. Migrations run automatically on startup.
