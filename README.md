# RUSVEL

> The Solo Builder's AI-Powered Virtual Agency
> Rust + SvelteKit | One binary, one human, infinite leverage

## What is RUSVEL?

A single Rust binary that replaces an entire agency. AI agents handle code, content, sales, finance, legal, support, and more — all orchestrated from one unified interface. Not a SaaS platform. Not for teams. A personal superpower.

## Quick Start

```bash
cargo build                    # Build all workspace members (see docs/status/current-state.md)
cargo run                      # Start web server on http://localhost:3000
```

## Three Ways to Use the Terminal

### One-shot commands
```bash
rusvel session create my-startup    # Create a session
rusvel forge mission today          # AI-generated daily plan
rusvel finance status               # Department status
rusvel growth list                  # List department items
rusvel harvest events               # Recent department events
```

**Tier 1 CLI** one-shot commands cover 12 departments: `forge`, `finance`, `growth`, `distro`, `legal`, `support`, `infra`, `product`, `code`, `harvest`, `content`, `gtm`. The API and web UI register **14** department apps (`flow`, `messaging` included).

### Interactive REPL shell
```bash
rusvel shell
```
Drop into a persistent prompt with autocomplete, history, and department context switching:
```
rusvel> use finance
rusvel:finance> status
rusvel:finance> list transactions
rusvel:finance> back
rusvel> status          # All departments overview
```

### TUI dashboard
```bash
rusvel --tui
```
Full-screen terminal dashboard with Tasks, Goals, Pipeline, and Events panels.

## Other Surfaces

```bash
rusvel                  # Web server (Axum + SvelteKit) on :3000
rusvel --mcp            # MCP server (stdio JSON-RPC) for Claude Code
```

## Architecture

Hexagonal (ports & adapters). **55** workspace members, **~69,456** lines Rust across **295** `*.rs` files under `crates/`, **13** domain engines plus **14** `dept-*` department apps (including `dept-messaging` and `dept-flow`), single binary. DepartmentApp pattern (ADR-014). Full metrics: [docs/status/current-state.md](docs/status/current-state.md).

```
SURFACES: CLI (Clap) | REPL (reedline) | TUI (Ratatui) | Web (Svelte) | MCP
    |
DEPARTMENTS: 14 dept-* crates (DepartmentApp pattern)
    |
ENGINES:  Forge | Code | Harvest | Content | GTM | Finance | Product
          Growth | Distro | Legal | Support | Infra | Flow
    |
FOUNDATION: rusvel-core (22 port traits in ports.rs, incl. five *Store + BrowserPort + RusvelBasePort) + adapter crates (DB, LLM, Agent, Events, Vector, Terminal, ...)
    |
TOOLS:    22+ tools (10 built-in incl. tool_search + 12 engine)
```

See [docs/design/architecture-v2.md](docs/design/architecture-v2.md) for full details.

## Stack

- **Backend:** Rust 2024, SQLite WAL, Axum, Clap 4, reedline, ratatui, tokio
- **Frontend:** SvelteKit 5, Tailwind CSS 4
- **AI:** Ollama (local), Claude API, Claude CLI, OpenAI — ModelTier routing (Haiku/Sonnet/Opus)
- **Agent:** AgentRuntime with streaming, ScopedToolRegistry, deferred tool loading

## Testing

```bash
cargo test              # All tests
cargo test -p rusvel-core
cargo test -p forge-engine
cargo test -p rusvel-db
cargo test -p rusvel-api
```

## Documentation

- [Vision](docs/design/vision.md) — What RUSVEL is
- [Architecture](docs/design/architecture-v2.md) — Hexagonal architecture, 14 department apps
- [Decisions](docs/design/decisions.md) — 14 ADRs with rationale
- [Current State](docs/status/current-state.md) — Live codebase metrics
- [Roadmap](docs/plans/roadmap-v2.md) — 5-phase plan

## License

MIT
