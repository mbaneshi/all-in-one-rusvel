# Claude Code Tooling — Project Configuration

> Custom subagents, skills, path-based rules, PR review workflow, and task runner.
> Added 2026-03-28, adapted from [claude-code-mastery](https://github.com/mbaneshi/claude-code-mastery).

---

## Subagents (`.claude/agents/`)

Custom AI agents with scoped tools, models, and system prompts. Invoke with `@name` or let Claude auto-delegate based on the description.

### researcher

**Purpose:** Deep investigation before implementation — crate research, architecture patterns, LLM integrations, competitive tools.

| Field | Value |
|-------|-------|
| Tools | Read, Grep, Glob, WebSearch, WebFetch |
| Model | Sonnet |

Searches existing codebase first, then the web. Returns structured findings with trade-offs and crate recommendations. Considers RUSVEL constraints (single binary, SQLite WAL, tokio, 55-crate workspace).

### arch-reviewer

**Purpose:** Review code for hexagonal architecture compliance.

| Field | Value |
|-------|-------|
| Tools | Read, Grep, Glob |
| Model | Sonnet |

Checks enforced:
1. Engines never import adapter crates
2. Engines use AgentPort, not LlmPort (ADR-009)
3. Domain types have `metadata: serde_json::Value` (ADR-007)
4. Event.kind is String, not enum (ADR-005)
5. Each crate < 2000 lines
6. Human approval gates on publishing (ADR-008)
7. DepartmentApp pattern (ADR-014)

### dept-auditor

**Purpose:** Audit department wiring across all layers.

| Field | Value |
|-------|-------|
| Tools | Read, Grep, Glob |
| Model | Sonnet |

Verifies each department has: engine crate, dept-* wrapper with DepartmentApp, registry entry, API routes, CLI subcommand, frontend route.

---

## Skills (`.claude/skills/`)

Reusable workflows invoked with `/skill-name`. Each skill is a `SKILL.md` file with YAML frontmatter and instructions.

### /dept-scaffold

**Purpose:** Scaffold a new department crate pair.

```
/dept-scaffold analytics
```

Creates:
- `crates/analytics-engine/` — engine with domain logic skeleton
- `crates/dept-analytics/` — DepartmentApp wrapper

Updates:
- Root `Cargo.toml` workspace members
- `crates/rusvel-app/Cargo.toml` dependency
- `crates/rusvel-app/src/main.rs` boot registration

Uses `dept-forge` + `forge-engine` as reference implementation.

### /engine-check

**Purpose:** Verify an engine crate follows architecture rules.

```
/engine-check harvest-engine
```

Runs in a subagent (Explore). Checks:
- No adapter imports in `Cargo.toml`
- Only `rusvel-core` trait imports
- No direct `LlmPort` usage
- `metadata` field on domain structs
- String event kinds
- Crate size under 2000 lines

### /crate-audit

**Purpose:** Health check for one or all crates.

```
/crate-audit rusvel-api
/crate-audit              # all crates
```

Runs in a subagent (Explore). Checks:
- Line count (< 2000 threshold)
- Dependency hygiene
- Public API surface
- Error handling patterns
- Test coverage presence
- Documentation on public items

---

## Path-based Rules (`.claude/rules/`)

Rules auto-load when Claude works on files matching the path globs. This saves context by only injecting relevant rules.

| File | Paths | Key Rules |
|------|-------|-----------|
| `engines.md` | `crates/*-engine/**` | Port-only deps, AgentPort not LlmPort, metadata field, String event kinds, < 2000 lines |
| `api.md` | `crates/rusvel-api/**` | `/api/dept/{id}/*` pattern, shared SSE helper, spawn_blocking, CORS restrictions |
| `frontend.md` | `frontend/**` | pnpm only, Svelte 5 runes, Tailwind 4, TypeScript strict, dynamic dept routes |
| `docs.md` | `docs/**/*.md` | ATX headers, language-tagged code blocks, valid JSON/YAML, relative links |
| `dept-crates.md` | `crates/dept-*/**` | DepartmentApp trait, DepartmentManifest, thin wrappers, engine delegation |

---

## GitHub Workflow (`.github/workflows/claude-review.yml`)

Automated AI code review on every PR using `anthropics/claude-code-action@v1`.

**Triggers:** `pull_request` (opened, synchronize)

**Reviews for:**
1. Architecture compliance (engine/adapter boundaries)
2. ADR-007 metadata fields
3. ADR-005 String event kinds
4. Crate size limits
5. Code quality and bugs
6. Security concerns
7. Test coverage

**Requires:** `ANTHROPIC_API_KEY` secret in repository settings.

---

## Task Runner (`justfile`)

[just](https://github.com/casey/just) recipes for common workflows. Run `just` to see all recipes.

### Build & Test

| Recipe | Description |
|--------|-------------|
| `just build` | Build all workspace crates |
| `just test` | Run all tests |
| `just test-crate <name>` | Test a single crate |
| `just check` | Check Rust + frontend types |
| `just ci` | Full CI: fmt-check + lint + test |
| `just coverage` | Run coverage report |

### Development

| Recipe | Description |
|--------|-------------|
| `just serve` | Start API server on :3000 |
| `just dev-frontend` | Start dev frontend on :5173 |
| `just build-frontend` | Build frontend for embedding |
| `just shell` | Launch REPL |
| `just tui` | Launch TUI dashboard |
| `just mcp` | Start MCP server |

### Code Quality

| Recipe | Description |
|--------|-------------|
| `just fmt` | Format Rust code |
| `just fmt-check` | Check formatting |
| `just lint` | Run clippy |

### Architecture Checks

| Recipe | Description |
|--------|-------------|
| `just crate-lines` | Flag crates over 2000 lines |
| `just check-boundaries` | Detect engine→adapter imports |
| `just stats` | Workspace statistics |
