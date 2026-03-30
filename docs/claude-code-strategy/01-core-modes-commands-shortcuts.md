# Strategy: Core Modes, Slash Commands & Keyboard Shortcuts in RUSVEL

> How RUSVEL leverages Claude Code's core interaction patterns as its development backbone and self-building engine.

---

## The Big Picture

RUSVEL's vision is a **self-aware, self-building system**. Claude Code isn't just a dev tool — it's RUSVEL's **primary development interface**. Every mode, command, and shortcut maps to a concrete workflow in how we build, maintain, and operate RUSVEL.

---

## 1. Core Modes — How We Use Each

### Interactive REPL → Daily Development

**Current use:** Primary way we build RUSVEL features, debug, and iterate.

**RUSVEL-specific workflow:**
```bash
claude                              # Start session in /rusvel
# → CLAUDE.md loads automatically (architecture rules, conventions)
# → Auto-memory recalls Mehdi's preferences, strategy, prior decisions
# → Full codebase context available
```

**Actionable setup:**
- CLAUDE.md already enforces hexagonal rules (engines never import adapters, use AgentPort not LlmPort)
- Every session starts with architecture guardrails loaded

### One-Shot (`claude -p`) → RUSVEL's LLM Backend

**Current use:** `ClaudeCliProvider::max_subscription()` spawns `claude -p` for all LLM calls.

**This is critical:** Every `rusvel forge mission today` command calls Claude Code in print mode under the hood. RUSVEL literally runs on this mode.

**How to improve:**
- Use `--output-format json` + `--json-schema` for structured responses instead of parsing markdown
- Use `--effort low` for quick classification tasks (opportunity scoring, content tagging)
- Use `--effort max` for deep analysis (code reviews, architecture decisions, proposal writing)
- Use `--max-budget-usd` to cap per-call costs

**Example — structured mission output:**
```bash
claude -p "Generate today's plan for these goals: ..." \
  --output-format json \
  --json-schema '{"type":"object","properties":{"tasks":[...],"priorities":[...]}}'
```

### Pipe Mode → Batch Processing Pipelines

**Not yet used. Should be.**

**harvest-engine:** Pipe scraped opportunity descriptions through Claude for scoring:
```bash
cat opportunities.jsonl | claude -p "Score each opportunity 1-10" --output-format stream-json
```

**content-engine:** Pipe draft content through Claude for adaptation:
```bash
cat blog-post.md | claude -p "Adapt this for Twitter thread format" --output-format json
```

**code-engine:** Pipe code analysis results through Claude for narration:
```bash
cargo test --message-format json | claude -p "Summarize test results"
```

### Continue/Resume → Long-Running Development Sessions

**How to use:**
- `claude -c` after lunch break — resume exactly where you left off
- `claude -r "forge-engine-v2"` — jump back into a named feature session
- `--from-pr 42` — resume the session that created PR #42 when reviews come in

**RUSVEL workflow:**
- Name sessions by feature: `claude -n "wire-mcp-flag"`
- Resume when CI fails: `claude --from-pr 15` → see what broke

---

## 2. Slash Commands — Mapped to RUSVEL Workflows

### Session Commands → Feature Branching

| Command | RUSVEL Use |
|---------|-----------|
| `/branch` | Fork a conversation when exploring two approaches (e.g., "should harvest-engine use polling or webhooks?") |
| `/rename` | Name sessions by engine: "forge-safety-guard", "content-devto-adapter" |
| `/rewind` | Undo a bad refactor without git reset — restore conversation + code state |

### Context Commands → Managing a 20-Crate Workspace

| Command | RUSVEL Use |
|---------|-----------|
| `/compact` | Essential — RUSVEL is ~73k lines of Rust across 55 workspace members ([`docs/status/current-state.md`](../status/current-state.md)). Compact when context fills up mid-session |
| `/context` | Check how much room is left before starting a cross-crate refactor |
| `/diff` | Review changes across multiple crates before committing |
| `/export` | Save architecture discussion sessions for docs |

### Configuration Commands → Per-Engine Tuning

| Command | RUSVEL Use |
|---------|-----------|
| `/model opus` | Deep architecture work (new engine design, ADR writing) |
| `/model haiku` | Quick tasks (fix typo, add a test, update docs) |
| `/effort max` | Security review of auth/credentials code |
| `/effort low` | Boilerplate generation (new endpoint, new store method) |
| `/plan` | Explore code-engine's tree-sitter integration without changing anything |

### AI & Agent Commands → Multi-Engine Development

| Command | RUSVEL Use |
|---------|-----------|
| `/agents` | Create specialized subagents: "rust-engine-builder", "svelte-ui-builder", "test-writer" |
| `/skills` | Custom skills: `/wire-engine` (scaffold new engine exposure via CLI+API+MCP) |
| `/mcp` | Connect RUSVEL's own MCP server to Claude Code — so Claude can manage sessions/goals directly |
| `/memory` | Review what Claude remembers about architecture decisions, strategies |

### Development Commands → CI/CD Integration

| Command | RUSVEL Use |
|---------|-----------|
| `/security-review` | Run before exposing new API endpoints (especially auth, outreach) |
| `/pr-comments` | Fetch review feedback on RUSVEL PRs |
| `/install-github-app` | Auto-review PRs with Claude in CI |
| `/hooks` | Verify auto-format and test hooks are active |

### Task Commands → Parallel Development

| Command | RUSVEL Use |
|---------|-----------|
| `/tasks` | Track background agents building different engines simultaneously |
| `/loop 10m cargo test` | Continuous testing while doing multi-crate refactors |
| `/btw` | Quick question about Axum middleware without derailing engine work |
| `/fast` | Toggle when doing rapid iteration vs. deep design |

---

## 3. Keyboard Shortcuts — Developer Efficiency

### Daily Workflow Shortcuts

| Shortcut | When to Use in RUSVEL |
|----------|----------------------|
| `Ctrl+G` | Open prompt in editor when writing complex multi-crate refactor instructions |
| `Ctrl+V` | Paste screenshot of UI bug from frontend |
| `Ctrl+B` | Background a long `cargo test` run while continuing to code |
| `Ctrl+T` | Check status of parallel subagent tasks |
| `Esc+Esc` | Rewind when Claude went wrong direction on engine implementation |
| `Option+P` | Switch between opus (design) and haiku (implementation) |
| `Option+T` | Enable deep thinking for security-sensitive code (auth, outreach gates) |
| `Shift+Tab` | Switch to `acceptEdits` when doing rapid file generation |

### Multiline Input → Complex Prompts

Use `Option+Enter` for multi-line prompts when describing cross-engine features:
```
Build the approval flow for content-engine:
1. Add approve/reject endpoints to rusvel-api
2. Wire JobPort.approve() in the handler
3. Add approval UI in frontend /content route
4. Emit events on approve/reject
```

---

## 4. Concrete Actions for RUSVEL

### Immediate (wire today)

1. **Add `--output-format json --json-schema` to ClaudeCliProvider** — structured LLM responses instead of parsing markdown
2. **Create `.claude/skills/wire-engine.md`** — custom skill to scaffold engine exposure (CLI command + API endpoint + MCP tool)
3. **Name sessions** — start using `claude -n "feature-name"` for every feature branch

### Short-term (this week)

4. **Set up `/loop 5m cargo test`** during refactors — catch breakage instantly
5. **Create subagents** — "rust-backend" (restricted to `crates/`), "svelte-frontend" (restricted to `frontend/`)
6. **Use pipe mode** in harvest-engine for batch opportunity scoring

### Medium-term (this phase)

7. **Wire RUSVEL MCP server to Claude Code** — Claude manages sessions and goals natively
8. **Use `/security-review`** before every API endpoint that touches auth or outreach
9. **Use `--effort` levels** — low for boilerplate, high for engine logic, max for security
