# Strategy: Built-in Tools & CLI Flags in RUSVEL

> How RUSVEL uses Claude Code's 25+ tools and 50+ flags as both a development accelerator and a runtime engine.

---

## The Dual Role

Claude Code tools serve RUSVEL in **two distinct ways**:
1. **Development-time** — building RUSVEL itself (file editing, testing, searching)
2. **Runtime** — RUSVEL spawns `claude -p` with specific flags for its AI backend

---

## 1. File Operation Tools → Full workspace management

### Read — Cross-Crate Understanding

**Challenge:** RUSVEL has **55** workspace members. Understanding a feature often requires reading 4-5 files across layers. Scale: [`docs/status/current-state.md`](../status/current-state.md).

**Pattern:** When working on a feature, read the full vertical slice:
```
1. rusvel-core/src/ports.rs    → port trait definition
2. rusvel-*/src/lib.rs          → adapter implementation
3. *-engine/src/lib.rs          → engine usage
4. rusvel-api/src/lib.rs        → API endpoint
5. frontend/src/routes/*.svelte → UI
```

**Image reading:** Paste screenshots of the SvelteKit frontend to debug layout issues — Claude reads images natively.

**PDF reading:** Read contracts, proposals, or client briefs that harvest-engine discovers — pipe through Claude for analysis.

### Write & Edit — Architecture-Safe Modifications

**Key constraint:** CLAUDE.md enforces "each crate < 2000 lines" and "engines never import adapter crates."

**Pattern:** Edit enforces targeted changes (no accidental full-file rewrites). Use Write only for new files.

**RUSVEL-specific rules for tools:**
- Edit `crates/*/src/lib.rs` — always check line count stays < 2000
- Never Write to `rusvel-core/` without checking port trait compatibility
- Edit `frontend/src/lib/components/` — always use design system tokens

### Glob & Grep → Codebase Navigation

**Common RUSVEL searches:**
```
Glob: crates/*/src/lib.rs           → find all crate entry points
Glob: frontend/src/routes/**/*.svelte → find all pages
Grep: "impl AgentPort"              → find where AgentPort is implemented
Grep: "async fn mission_today"      → trace the mission flow
Grep: "Event::new"                  → find all event emissions
Grep: "metadata:"                   → audit schema evolution fields
```

---

## 2. Code Execution Tools → Testing & Building

### Bash → Build/Test/Run Pipeline

**Core RUSVEL commands that Bash executes:**
```bash
cargo build                          # Build all workspace members
cargo test                           # Full suite from repo root (~645 tests summed; see current-state)
cargo test -p forge-engine           # Test single engine
cargo run                            # Start API on :3000
cargo run -- session create "demo"   # Test CLI
cargo run -- forge mission today     # Test full vertical slice
cd frontend && pnpm check            # Type-check Svelte
cd frontend && pnpm dev              # Start frontend dev server
```

**Background execution:** Run `cargo test` in background while editing — get notified when done.

### NotebookEdit → Data Analysis

**Future use:** When harvest-engine collects opportunity data, analyze trends in Jupyter notebooks. When content-engine tracks engagement metrics, visualize in notebooks.

### LSP → Code Intelligence

**For RUSVEL:** LSP provides go-to-definition, find-references across the Rust workspace. Critical for refactoring port traits that touch many crates.

---

## 3. Web Tools → External Intelligence

### WebFetch → Platform API Testing

**harvest-engine:** Fetch Upwork RSS feeds, DEV.to API responses, LinkedIn profiles
```
WebFetch https://dev.to/api/articles?tag=rust → test content-engine's DEV.to adapter
WebFetch https://api.github.com/repos/... → code-engine dependency analysis
```

### WebSearch → Market Research

**gtm-engine:** Search for competitor analysis, pricing research, market trends
**content-engine:** Research topics before generating content
**harvest-engine:** Find new opportunity sources

---

## 4. Task Management Tools → Parallel Engine Development

### TaskCreate/TaskList/TaskUpdate → Multi-Engine Work

**Pattern:** Work on multiple engines simultaneously:
```
Task 1: "Wire harvest-engine CLI commands" → in progress
Task 2: "Add content-engine API endpoints" → blocked (needs approval flow)
Task 3: "Build GTM pipeline UI" → not started
```

**Background tasks:** Long-running operations:
- `cargo test` across all crates (background)
- `pnpm build` for frontend (background)
- Code search across ~62k lines of Rust under `crates/` (background via agent; see [`docs/status/current-state.md`](../status/current-state.md))

---

## 5. Agent & Planning Tools → Architecture Decisions

### Agent Tool → Specialized Workers

**RUSVEL-specific subagents to create:**

| Agent | Scope | Purpose |
|-------|-------|---------|
| `rust-engine` | `crates/` only | Build engine logic, enforce port patterns |
| `svelte-ui` | `frontend/` only | Build UI with design system components |
| `test-writer` | `**/tests/`, `*_test.rs` | Write tests for new features |
| `api-builder` | `rusvel-api/`, `rusvel-cli/`, `rusvel-mcp/` | Expose engines via surfaces |
| `security-auditor` | Full access, read-only | Review auth, credentials, outreach gates |

### EnterPlanMode → Safe Architecture Exploration

**When to use:**
- Exploring how to wire a new engine (read all affected crates without changing anything)
- Analyzing code-engine's tree-sitter integration before modifying
- Reviewing harvest-engine's scoring algorithm before optimizing

### EnterWorktree → Isolated Experiments

**When to use:**
- Try a risky refactor of rusvel-core ports (could break the whole workspace)
- Experiment with a new LLM provider without affecting working Claude CLI setup
- Test database migration changes in isolation

---

## 6. CLI Flags → RUSVEL's LLM Backend Configuration

### Flags Used in ClaudeCliProvider Today

```rust
// Current: ClaudeCliProvider spawns:
claude -p "prompt" --output-format json
```

### Flags We Should Add

| Flag | RUSVEL Use | Engine |
|------|-----------|--------|
| `--json-schema` | Structured responses for mission plans, opportunity scores | forge, harvest |
| `--effort low` | Quick classification (tag content, score opportunities) | content, harvest |
| `--effort high` | Deep analysis (code review, proposal writing) | code, harvest |
| `--effort max` | Architecture decisions, security review | forge |
| `--max-budget-usd 0.50` | Cap per-call spending | all engines |
| `--max-turns 5` | Prevent runaway agent loops | forge (agent runtime) |
| `--model haiku` | Fast, cheap tasks (summarize, classify) | content, harvest |
| `--model opus` | Complex reasoning (strategy, proposals) | forge, gtm |
| `--system-prompt` | Per-persona system prompts (CodeWriter vs ContentWriter) | forge (personas) |
| `--no-session-persistence` | Stateless API calls | all engines |
| `--bare` | Skip CLAUDE.md discovery for faster API calls | all engines |

### Implementation in ClaudeCliProvider

```rust
// Proposed: Dynamic flag selection based on task
fn build_command(&self, task: &AgentTask) -> Command {
    let mut cmd = Command::new("claude");
    cmd.arg("-p").arg(&task.prompt);
    cmd.arg("--output-format").arg("json");
    cmd.arg("--bare");  // skip discovery for API calls
    cmd.arg("--no-session-persistence");  // stateless

    match task.effort {
        Effort::Low => cmd.arg("--effort").arg("low"),
        Effort::High => cmd.arg("--effort").arg("high"),
        Effort::Max => cmd.arg("--effort").arg("max"),
    };

    if let Some(schema) = &task.json_schema {
        cmd.arg("--json-schema").arg(schema);
    }

    if let Some(budget) = task.budget_usd {
        cmd.arg("--max-budget-usd").arg(budget.to_string());
    }

    cmd
}
```

---

## 7. Scheduling Tools → Automated Operations

### CronCreate → Recurring RUSVEL Tasks

**Content calendar:**
```
CronCreate: Every Monday 9am → "Generate weekly content plan"
CronCreate: Every day 8am → "Scan for new opportunities"
CronCreate: Every Friday 5pm → "Generate weekly review"
```

**Health monitoring:**
```
CronCreate: Every 30min → "cargo test" (catch regressions)
CronCreate: Every hour → "Check API health endpoint"
```

---

## 8. Concrete Actions

### Immediate

1. **Add `--bare --no-session-persistence` to all ClaudeCliProvider calls** — faster, stateless
2. **Add `--json-schema` support** — eliminate markdown parsing hacks
3. **Use `--effort` flag** — match effort to task complexity

### Short-term

4. **Create the 5 custom subagents** (rust-engine, svelte-ui, test-writer, api-builder, security-auditor)
5. **Use worktree isolation** for risky rusvel-core refactors
6. **Set up background test runs** during development

### Medium-term

7. **Implement dynamic flag selection** in ClaudeCliProvider based on persona/task
8. **Use CronCreate** for content calendar and opportunity scanning
9. **Build pipe-mode integration** in harvest-engine for batch scoring
