# Strategy: Memory System in RUSVEL

> How CLAUDE.md, rules, and auto-memory create persistent intelligence for a large Rust workspace. Live metrics: [`docs/status/current-state.md`](../status/current-state.md).

---

## The Three Memory Layers

Claude Code's memory system maps directly to RUSVEL's needs:

| Layer | RUSVEL Purpose | Scope |
|-------|---------------|-------|
| **CLAUDE.md** | Architecture rules, conventions, crate layout | Checked into repo, shared with all |
| **Rules (`.claude/rules/`)** | File-type-specific guidance | Checked into repo, conditional loading |
| **Auto Memory** | Strategy, preferences, decisions, user context | Machine-local, per-project |

---

## 1. CLAUDE.md — Architecture Guardrails

### Current State

RUSVEL already has a strong CLAUDE.md (96 lines) with:
- Quick commands (build, test, run)
- Architecture overview (hexagonal, ports & adapters)
- 7 key rules (engines never import adapters, use AgentPort not LlmPort, etc.)
- Full workspace layout (55 workspace members per `current-state`)
- Wired features list (MCP, TUI, frontend embedding — all wired)
- Testing commands per crate
- Stack description

### What to Improve

**Add `@imports` for depth without bloat:**
```markdown
# CLAUDE.md (keep under 500 lines)
@docs/design/decisions.md        # 10 ADRs — loaded when discussing architecture
@docs/status/current-state.md    # What's working — loaded to prevent outdated assumptions
```

**Add engine-specific conventions:**
```markdown
## Engine Conventions
- Every engine exposes: `new(ports) → Self`, domain methods, no I/O in constructors
- Every engine test uses MockPorts (see forge-engine tests for pattern)
- Engine methods return `Result<T, EngineError>` — never panic
- Event emission: engines call `event_port.emit()` after every state change
```

**Add frontend conventions:**
```markdown
## Frontend Conventions
- Import from `$lib/components` barrel — never deep-import component files
- Use design system tokens (--r-*) — never raw Tailwind gray/indigo
- All components use Svelte 5 patterns: $props(), snippets, $bindable()
- Pages follow pattern: load data in script, render with design system components
```

**Add LLM integration conventions:**
```markdown
## LLM Conventions
- All LLM calls go through AgentPort, never LlmPort directly (ADR-009)
- Use --effort low for classification, high for generation, max for reasoning
- Always request structured JSON output with --json-schema
- Budget cap: $0.50 per call unless explicitly overridden
```

### Subdirectory CLAUDE.md Files

Create per-area CLAUDE.md files that load only when working in that directory:

**`crates/CLAUDE.md`:**
```markdown
# Rust Crate Conventions
- Each crate < 2000 lines
- Engines depend ONLY on rusvel-core
- All types have metadata: serde_json::Value
- Event.kind is String, not enum
- Use #[cfg(test)] mod tests at bottom of lib.rs
```

**`frontend/CLAUDE.md`:**
```markdown
# Frontend Conventions
- SvelteKit 5 + Tailwind 4 + design system
- Import components from $lib/components
- Use semantic tokens (--r-bg-surface, --r-fg-default, etc.)
- No raw color classes (gray-900, indigo-600) — use design tokens
- All state via $state/$derived runes, not Svelte stores
```

---

## 2. Rules System — Conditional, File-Specific Guidance

### Why Rules Matter for RUSVEL

RUSVEL has **55** Rust workspace members + a SvelteKit frontend + docs (see [`docs/status/current-state.md`](../status/current-state.md)). Different files need different rules. Rules load **only when relevant files are in context**, keeping the context window lean.

### Rules to Create

**`.claude/rules/rust-ports.md`** (glob: `crates/rusvel-core/**`):
```markdown
When modifying port traits:
- Every port method must be async and return Result
- Adding a method to a port trait requires updating ALL implementations
- Check: rusvel-db, rusvel-llm, rusvel-agent, rusvel-event, rusvel-memory, rusvel-tool, rusvel-jobs, rusvel-auth, rusvel-config
- Run `cargo build` after — compiler errors show missing implementations
```

**`.claude/rules/rust-engines.md`** (glob: `crates/*-engine/**`):
```markdown
When modifying engines:
- NEVER import adapter crates (rusvel-db, rusvel-llm, etc.)
- ONLY depend on rusvel-core traits
- Use AgentPort for LLM calls, never LlmPort
- Emit events for all state changes
- Write tests with mock ports (see forge-engine/src/lib.rs for pattern)
```

**`.claude/rules/svelte-components.md`** (glob: `frontend/src/lib/components/**`):
```markdown
When creating or modifying components:
- Use $props() with full type annotation
- Accept `class` prop for style overrides: `class: className`
- Spread ...rest onto root element
- Use snippets (not slots) for content
- Use cn() from $lib/utils/cn for class merging
- Reference design tokens: var(--r-bg-surface), var(--r-fg-default)
- Add to barrel export in index.ts
```

**`.claude/rules/api-endpoints.md`** (glob: `crates/rusvel-api/**`):
```markdown
When adding API endpoints:
- Follow existing pattern in handlers module
- Always return JSON with consistent error format
- Add route to router in lib.rs
- Run /security-review for auth-related endpoints
- Document in CLAUDE.md if it's a new surface area
```

**`.claude/rules/migrations.md`** (glob: `crates/rusvel-db/migrations/**`):
```markdown
When writing migrations:
- SQLite WAL mode — no ALTER TABLE DROP COLUMN
- Always add columns as nullable or with DEFAULT
- Name: YYYYMMDD_HHMMSS_description.sql
- Test with: cargo test -p rusvel-db
```

**`.claude/rules/test-files.md`** (glob: `**/*test*`, `**/tests/**`):
```markdown
When writing tests:
- Use mock ports from rusvel-core (see forge-engine tests)
- Test the happy path + at least one error case
- No external dependencies (no network, no real DB for engine tests)
- DB tests use in-memory SQLite (:memory:)
- Name tests descriptively: test_mission_today_returns_tasks
```

---

## 3. Auto Memory — Strategic Context Across Sessions

### Current Auto Memory

Already stored:
- `project_rusvel.md` — vision; for **port/engine/dept counts** copy from [`docs/status/current-state.md`](../status/current-state.md) only
- `user_mehdi.md` — solo founder, Rust+SvelteKit, cross-validates with AIs
- `reference_claude_p_max.md` — env vars for Max subscription
- `project_4layer_architecture.md` — Kernel/Runtime/Shell/Network layers
- `feedback_architecture_separations.md` — keep Brand/Content/Distribution separate
- `project_real_strategy.md` — 3 products (Codeilus, GlassForge, ContentForge)
- `project_self_building.md` — RUSVEL develops itself via chat UI

### What to Add

**Architecture decisions that aren't in ADRs:**
- Why SQLite WAL over Postgres (single binary, zero ops, solo builder)
- Why Claude CLI over API (Max subscription, $0 cost)
- Why hexagonal (swap adapters without touching engines)

**Development preferences learned from sessions:**
- Mehdi prefers bundled PRs over many small ones for refactors
- Terse responses, no trailing summaries
- Cross-validate with Perplexity for architecture decisions
- Real tests over mocks when possible (except engine tests)

**Strategic context:**
- Phase 0 = prove vertical slice. Don't over-engineer.
- Three products to launch: Codeilus (code), GlassForge (glass art), ContentForge (content)
- RUSVEL is the marketing/sales engine behind all three

---

## 4. Memory-Driven Workflows

### Self-Building Pattern

RUSVEL's self-building capability means Claude Code sessions need persistent context:

```
Session 1: "Add harvest-engine CLI commands"
  → Claude reads CLAUDE.md (architecture rules)
  → Claude reads auto-memory (what's built, what's next)
  → Claude reads rules (engine-specific patterns)
  → Claude builds the feature with full context
  → Claude saves memory: "harvest CLI wired, 3 commands added"

Session 2: "Now expose harvest via API"
  → Claude recalls Session 1 memory
  → Knows harvest CLI is done, can reference its patterns
  → Builds API endpoints following same structure
```

### Cross-Session Architecture Coherence

Memory ensures architectural decisions persist:
- Session 5: Decided to use String event kinds → saved to memory
- Session 47: Claude still uses String kinds, not enums — because memory says so
- Session 102: If pattern changes, update memory — all future sessions follow new pattern

---

## 5. Concrete Actions

### Immediate

1. **Add `@imports` to CLAUDE.md** — link decisions.md and current-state.md
2. **Create `crates/CLAUDE.md`** — Rust-specific conventions
3. **Create `frontend/CLAUDE.md`** — Svelte + design system conventions

### Short-term

4. **Create 6 rules files** — ports, engines, components, API, migrations, tests
5. **Add engine/frontend conventions** to main CLAUDE.md
6. **Review and update auto-memory** — ensure strategy context is current

### Medium-term

7. **Establish memory hygiene** — review memories monthly, remove stale entries
8. **Add engine-specific CLAUDE.md** files in each engine crate directory
9. **Create a `~/.claude/rules/rust-general.md`** for Rust conventions shared across all projects
