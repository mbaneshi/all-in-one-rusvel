# Strategy: IDE Integrations, Git Features & File Handling in RUSVEL

> How editor integration, version control, and multi-format file support accelerate RUSVEL development.

---

## Part A: IDE Integrations

### VS Code Extension — Primary Development Surface

**RUSVEL's workspace is complex:** 20 Rust crates + SvelteKit frontend + docs. VS Code + Claude Code extension provides:

**File/folder references via `@`:**
```
@crates/forge-engine/src/lib.rs — show me the mission_today implementation
@frontend/src/routes/chat/ — this page needs the design system components
@docs/design/decisions.md — check if this change violates any ADR
```

**Multiple conversations:**
- One conversation for Rust backend work
- One conversation for SvelteKit frontend work
- One for documentation and planning

**Terminal integration:**
- Run `cargo test` from VS Code terminal
- Claude sees terminal output and can react to failures
- Run `pnpm dev` to preview frontend changes

**Built-in IDE MCP server:**
- VS Code provides file navigation, diagnostics, and symbol search to Claude
- Claude can use VS Code's Rust-analyzer for go-to-definition
- CSS/Svelte language server for frontend work

### VS Code Workspace Setup for RUSVEL

**`.vscode/settings.json`:**
```json
{
  "rust-analyzer.linkedProjects": ["Cargo.toml"],
  "rust-analyzer.checkOnSave.command": "clippy",
  "claude.defaultModel": "opus",
  "claude.autoConnect": true,
  "files.exclude": {
    "target/": true,
    "frontend/node_modules/": true,
    "frontend/.svelte-kit/": true
  }
}
```

**Multi-root workspace** (optional):
```json
{
  "folders": [
    { "path": ".", "name": "RUSVEL Root" },
    { "path": "crates", "name": "Rust Crates" },
    { "path": "frontend", "name": "SvelteKit Frontend" },
    { "path": "docs", "name": "Documentation" }
  ]
}
```

### JetBrains (RustRover/WebStorm)

If using RustRover for Rust or WebStorm for Svelte:
- Claude Code plugin auto-detects IDE
- External terminal support for CLI sessions
- Same `@` file referencing from editor

---

## Part B: Git Integration

### Git-Aware Development

RUSVEL is a monorepo. Git integration matters for:

**Branch awareness:**
- Claude knows the current branch at session start
- Sessions are associated with branches in the session picker
- PR status (green/yellow/red) visible when resuming sessions

**Worktree for risky refactors:**
```bash
claude --worktree  # Isolated copy of repo
# → Refactor rusvel-core ports without risk
# → If tests pass, merge the worktree branch
# → If they fail, discard — main is untouched
```

**PR workflow:**
```bash
# Claude creates a PR
claude -p "Create a PR for the harvest-engine wiring"

# Later, when review comments come in
claude --from-pr 42  # Resume the exact session that created the PR
```

**Git-driven RUSVEL practices:**

| Practice | Git Feature | RUSVEL Application |
|----------|------------|-------------------|
| Feature branches | Branch awareness | One branch per engine feature |
| Commit conventions | Co-Authored-By | Track Claude's contributions |
| PR reviews | `/pr-comments` | Fetch and address review feedback |
| Worktree experiments | `--worktree` | Safe core refactors |
| Session resumption | `--from-pr` | Continue work after review |

### Commit Message Conventions

```
<type>(<scope>): <description>

Types: feat, fix, refactor, test, docs, chore
Scopes: forge, code, harvest, content, gtm, api, cli, mcp, ui, core, db

Examples:
feat(harvest): add opportunity scoring pipeline
fix(api): handle missing session ID in /mission/today
refactor(core): split StoragePort into typed sub-stores
test(content): add integration tests for DEV.to adapter
docs(design): update architecture-v2 with MCP surface
```

### Git Hooks (not Claude Code hooks — actual `.git/hooks/`)

**`pre-commit`:**
```bash
#!/bin/bash
cargo clippy -- -D warnings
cargo test --quiet
cd frontend && pnpm check
```

**`commit-msg`:**
```bash
#!/bin/bash
# Enforce conventional commits
if ! grep -qE "^(feat|fix|refactor|test|docs|chore)\(.*\): " "$1"; then
  echo "Error: Commit message must follow convention: type(scope): description"
  exit 1
fi
```

---

## Part C: File & Media Handling

### Multi-Format Support for RUSVEL

**Code files (all formats):**
- Rust (`.rs`) — 55 workspace members, primary language ([`docs/status/current-state.md`](../status/current-state.md))
- Svelte (`.svelte`) — frontend components and pages
- TypeScript (`.ts`) — frontend logic, stores, API client
- CSS (`.css`) — app.css with design tokens
- TOML (`.toml`) — Cargo.toml, config files
- SQL (`.sql`) — database migrations
- JSON (`.json`) — package.json, MCP config, settings

**Images — UI Development:**
```
Ctrl+V to paste a screenshot → Claude analyzes the UI
- "This card has too much padding, fix it"
- "The sidebar icon for Harvest doesn't match the design"
- "Here's a mockup — implement this page"
```

**RUSVEL-specific image workflows:**
- Paste Figma/mockup screenshots → Claude builds the Svelte page
- Paste browser screenshot → Claude debugs layout issues
- Paste error screenshots → Claude diagnoses and fixes
- Paste competitor UI → Claude implements similar patterns

**PDFs — Business Documents:**
- Client contracts (harvest-engine opportunity attachments)
- Project briefs (content-engine source material)
- Invoice templates (gtm-engine financial documents)
- API documentation (integration reference)

**Config files — Full ecosystem:**
```
Cargo.toml          → Rust workspace dependencies
package.json        → Frontend dependencies
tsconfig.json       → TypeScript configuration
svelte.config.js    → SvelteKit configuration
vite.config.ts      → Build configuration
.mcp.json           → MCP server configuration
settings.json       → Claude Code settings
profile.toml        → RUSVEL user profile
config.toml         → RUSVEL app configuration
```

### Jupyter Notebooks — Future Analytics

When RUSVEL starts collecting data:
- **harvest-engine:** Opportunity funnel analysis in notebooks
- **content-engine:** Engagement metrics visualization
- **gtm-engine:** Revenue analytics, deal pipeline charts
- **code-engine:** Code quality trends over time

Claude can read and edit notebooks natively with NotebookEdit.

---

## Concrete Actions

### Immediate

1. **Set up VS Code workspace settings** — rust-analyzer, file exclusions, Claude auto-connect
2. **Establish commit convention** — type(scope): description
3. **Start using `@` references** in VS Code Claude conversations

### Short-term

4. **Create pre-commit hook** — clippy + test + svelte-check
5. **Use worktree** for next rusvel-core refactor
6. **Start pasting UI screenshots** — Claude builds from visual reference

### Medium-term

7. **Set up PR workflow** — feature branches, PR reviews, `--from-pr` resumption
8. **Build Jupyter analytics** — opportunity funnel, content engagement
9. **Create multi-root VS Code workspace** — separate Rust/Svelte/Docs
