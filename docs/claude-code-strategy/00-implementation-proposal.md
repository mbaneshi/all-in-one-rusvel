# Implementation Proposal: Bringing Claude Code Strategy into RUSVEL

> The best path from 8 strategy reports → working system.
> With reasoning for every decision.

---

## Why This Order Matters

The 8 reports propose **~70 actions** across Immediate/Short-term/Medium-term. Doing them randomly wastes effort. The right order follows three principles:

1. **Unblock Phase 0 first** — nothing else matters until the foundation is done
2. **Wire infrastructure before features** — settings, rules, hooks, subagents are force-multipliers that make ALL future work faster
3. **Close the self-building loop** — once RUSVEL can manage itself via MCP, development velocity compounds

---

## The Three Waves

```
Wave 1: FOUNDATION (Days 1-3)     → Unblock Phase 0 + wire dev infrastructure
Wave 2: ACCELERATION (Days 4-10)  → Subagents, skills, hooks, second engine
Wave 3: CONVERGENCE (Days 11-20)  → MCP self-loop, CI/CD, approval flow, frontend
```

---

## Wave 1: Foundation (Days 1-3)

**Goal:** Fix the two biggest blockers and set up the developer toolchain that makes everything else faster.

### Day 1: Fix LLM + Wire MCP

**Action 1.1 — Wire MultiProvider with Ollama default**
- File: `crates/rusvel-app/src/main.rs`, `crates/rusvel-llm/src/lib.rs`
- Change: Replace `ClaudeCliProvider::max_subscription()` with `MultiProvider` that tries Ollama first (qwen3:14b), falls back to Claude CLI
- Why first: Every engine depends on LLM. If this is broken or unreliable, nothing else works. Ollama is local, fast, free, and always available. Claude CLI is rate-limited and can fail.
- Effort: 2-3 hours

**Action 1.2 — Wire `--mcp` flag dispatch**
- File: `crates/rusvel-app/src/main.rs`
- Change: One if-branch: `if args.mcp { rusvel_mcp::serve(state).await?; return Ok(()); }`
- Why now: This is literally 3 lines of code that unlocks the entire self-building loop. The MCP server is already built with 6 tools. Not wiring it is leaving money on the table.
- Effort: 10 minutes

**Action 1.3 — Add `--bare --no-session-persistence --output-format json` to ClaudeCliProvider**
- File: `crates/rusvel-llm/src/lib.rs` (ClaudeCliProvider)
- Change: Append flags to the `claude -p` command
- Why now: These flags make every LLM call faster (skip CLAUDE.md discovery), cleaner (JSON instead of markdown), and stateless (no session accumulation). Every LLM call from this point forward benefits.
- Effort: 30 minutes

**Reasoning:** Day 1 fixes the engine (LLM) and opens the door (MCP). Everything downstream depends on reliable LLM calls.

### Day 2: Developer Infrastructure

**Action 2.1 — Create `.claude/settings.json`**
```json
{
  "permissions": {
    "allow": [
      "Bash(cargo build*)", "Bash(cargo test*)", "Bash(cargo run*)",
      "Bash(cargo clippy*)", "Bash(pnpm *)", "Bash(cd frontend*)",
      "Bash(git log*)", "Bash(git status*)", "Bash(git diff*)",
      "Read", "Glob", "Grep", "WebSearch"
    ],
    "deny": [
      "Bash(rm -rf*)", "Bash(git push --force*)",
      "Bash(git reset --hard*)", "Bash(cargo publish*)"
    ]
  }
}
```
- Why: Stops permission prompts for safe operations. Every session currently wastes time approving `cargo test`. This is a one-time setup that saves minutes per hour.
- Effort: 15 minutes

**Action 2.2 — Create `crates/CLAUDE.md` + `frontend/CLAUDE.md`**
- Content: Rust conventions (from report 03) and Svelte/design-system conventions
- Why: These load automatically when Claude works in those directories. Instead of Claude re-learning "don't import adapters in engines" every session, it's loaded once. Prevents architecture violations before they happen.
- Effort: 30 minutes

**Action 2.3 — Create 6 rules files in `.claude/rules/`**
```
rust-ports.md       → "when modifying port traits, update ALL implementations"
rust-engines.md     → "NEVER import adapter crates, use AgentPort"
svelte-components.md → "use $props(), design tokens, cn(), barrel exports"
api-endpoints.md    → "follow existing pattern, run /security-review"
migrations.md       → "SQLite WAL, nullable columns, name convention"
test-files.md       → "mock ports for engines, in-memory SQLite for DB"
```
- Why: Rules are conditional — they load ONLY when relevant files are touched. This keeps context lean while guaranteeing guidance is there when needed. The Svelte rules alone prevent the design system from being bypassed.
- Effort: 45 minutes

**Action 2.4 — Add `@imports` to CLAUDE.md**
```markdown
@docs/design/decisions.md
@docs/status/current-state.md
```
- Why: Instead of pasting ADRs or status into every conversation, Claude auto-loads them. The 10 ADRs are the law — they should always be in context. Current state prevents Claude from building things that already exist.
- Effort: 5 minutes

**Reasoning:** Day 2 builds the "institutional memory" layer. From this point forward, every Claude session starts smarter — with architecture rules, conventions, permissions, and current status pre-loaded. This is a force multiplier for all subsequent work.

### Day 3: Hooks (Automated Quality)

**Action 3.1 — PostToolUse: Auto-format**
- Hook: After any Write/Edit to `.rs` files → run `rustfmt`
- Hook: After any Write/Edit to `.svelte`/`.ts` → run `prettier`
- Why: Eliminates all formatting discussions. Code is always clean. No more "fix indentation" commits.

**Action 3.2 — Stop: Desktop notification**
- Hook: When Claude finishes → macOS notification
- Why: Solo builder walks away during long operations. Notification brings them back instantly instead of checking every few minutes.

**Action 3.3 — SessionStart: Status check**
- Hook: On session start → run `cargo test --quiet | tail -1` + show last commit
- Why: Every session starts with truth: "222 tests passing, last commit: batch LlmPort + hybrid RAG". Prevents working on a broken branch unknowingly.

**Reasoning:** Day 3 closes the automation loop. Format, notify, verify — all automatic. The developer's cognitive load drops significantly.

---

## Wave 2: Acceleration (Days 4-10)

**Goal:** Create the AI team that builds RUSVEL faster, expose a second engine, prove the pattern scales.

### Days 4-5: Subagents + Skills

**Action 4.1 — Create 5 subagent files in `.claude/agents/`**

| Agent | Model | Scope | Why This Agent |
|-------|-------|-------|---------------|
| `rust-engine` | opus | `crates/` | Enforces hexagonal rules by construction. Can't touch frontend. System prompt bakes in ADRs. |
| `svelte-ui` | sonnet | `frontend/` | Knows the design system. Can't break Rust code. Cheaper model for UI work. |
| `test-writer` | sonnet | full | Focused on test coverage. Cheaper model because tests are structured. |
| `api-builder` | sonnet | `rusvel-cli/`, `rusvel-api/`, `rusvel-mcp/` | Knows the surface layer pattern. Can scaffold CLI+API+MCP in parallel. |
| `security-auditor` | opus | full, read-only | No write access. Uses expensive model because security requires deep reasoning. |

- Why 5 and not 3 or 10: These map exactly to RUSVEL's architectural layers. Backend / Frontend / Tests / Surfaces / Security. Each has a distinct skill set and constraint set. Fewer would force one agent to context-switch. More would add overhead without clear benefit.
- Why now: From this point forward, every feature can be parallelized. "Build harvest pipeline page" becomes 3 agents working simultaneously.
- Effort: 1 hour

**Action 4.2 — Create 5 custom skills in `.claude/skills/`**

| Skill | Trigger | Why |
|-------|---------|-----|
| `/wire-engine` | Scaffold CLI+API+MCP for any engine | The #1 repeated task in RUSVEL — expose engine functionality. One command instead of manual copy-paste. |
| `/new-component` | Create design system component | Guarantees every component follows the pattern (cn(), $props(), tokens, barrel export). |
| `/new-engine` | Scaffold entire engine crate | Ensures correct Cargo.toml deps, workspace membership, composition root wiring. |
| `/daily-review` | End-of-day summary | Tracks velocity. Git log + test status + gap check. Takes 30 seconds, saves 15 minutes of manual checking. |
| `/deploy-check` | Pre-deployment verification | Release build + tests + clippy + frontend + security. Catches issues before they become rollbacks. |

- Why: Skills are reusable workflows. RUSVEL will scaffold dozens of engines, components, and endpoints. Doing this manually each time is error-prone and slow.
- Effort: 1.5 hours

### Days 6-8: Second Engine Exposure

**Action 5.1 — Run `/wire-engine harvest`**
- This tests the skill we just created
- Adds: `rusvel harvest scan`, `rusvel harvest pipeline`, `GET /api/harvest/opportunities`
- Why harvest first: It has the most tests (12), the most complete implementation (scoring pipeline, proposal generation), and the most interesting demo value.

**Action 5.2 — Run `/wire-engine content`**
- Adds: `rusvel content draft "topic"`, `rusvel content schedule`, `GET /api/content/items`
- Why content second: It has the approval gate already in the domain model. Exposing it proves the approval flow architecture.

- Why both: Phase 0 requires "prove architecture works beyond one engine." Two engines proved = architecture validated. If `/wire-engine` works for both without modification, the skill is correct and all remaining engines can be wired mechanically.
- Effort: 2-3 hours per engine (including tests)

### Days 9-10: Job Queue + Architecture Guard

**Action 6.1 — Wire job queue worker loop**
- File: `crates/rusvel-app/src/main.rs`
- Pattern: `tokio::spawn` a loop that calls `job_port.dequeue()` every 5 seconds, dispatches to the right engine
- Why now: Jobs are the glue between engines. Content publishing, opportunity scanning, outreach sequences — all need async processing. Without a worker, jobs accumulate forever.
- Effort: 3-4 hours

**Action 6.2 — Add architecture guard hook**
- Hook: PreToolUse on Write/Edit → check that engine files don't import adapter crates
- Why: The #1 architectural risk is an engine importing `rusvel-db` directly. This hook makes it mechanically impossible. Prevention > detection.
- Effort: 1 hour

**Action 6.3 — Add PostCompact hook**
- Hook: After context compaction → re-inject `current-state.md`
- Why: Long sessions lose context about what's built. Re-injecting current state after compaction prevents Claude from rebuilding existing features.
- Effort: 15 minutes

**Reasoning for Wave 2:** By the end of Day 10, RUSVEL has:
- 5 specialized agents that can work in parallel
- 5 reusable skills that eliminate repetitive scaffolding
- 3 engines exposed (forge + harvest + content)
- A working job queue
- Architecture guards that prevent violations automatically
- This is the inflection point — development velocity roughly doubles from here.

---

## Wave 3: Convergence (Days 11-20)

**Goal:** Close the self-building loop, ship the frontend, set up CI/CD, and complete Phase 0.

### Days 11-13: Self-Building Loop

**Action 7.1 — Register RUSVEL as MCP server**
- Create `.mcp.json`:
```json
{
  "mcpServers": {
    "rusvel": {
      "command": "cargo",
      "args": ["run", "--", "--mcp"],
      "cwd": "/Users/bm/rusvel"
    }
  }
}
```
- Why: This is the convergence moment. Claude Code can now call `rusvel.session_list`, `rusvel.mission_today`, `rusvel.mission_add_goal` natively. The development tool manages the product. The product manages the development tool.
- Effort: 15 minutes (MCP server already built)

**Action 7.2 — Wire RUSVEL event emission hook**
- Hook: PostToolUse on Write/Edit → `curl -X POST http://localhost:3000/api/events`
- Why: Every file Claude edits gets logged in RUSVEL's event bus. RUSVEL now tracks its own development. The self-awareness loop closes.
- Effort: 30 minutes

**Action 7.3 — Add effort-level routing to ClaudeCliProvider**
```rust
match task.complexity {
    Trivial => ("--effort", "low"),    // classification, tagging
    Normal  => ("--effort", "high"),   // generation, analysis
    Critical => ("--effort", "max"),   // security, architecture
}
```
- Why: Cost optimization. Low-effort calls are 3-5x faster and cheaper. Not every LLM call needs deep reasoning.
- Effort: 1 hour

### Days 14-17: Frontend + Approval Flow

**Action 8.1 — Build real dashboard page**
- Route: `/` (already exists, needs real data)
- Components: Card, Badge, ProgressBar, Heading, Stack from design system
- Data: Sessions list, goals overview, daily plan, recent events
- Why: The dashboard is the single pane of glass for the solo builder. Without it, everything is CLI-only. The design system we built exists exactly for this.
- Effort: 4-5 hours

**Action 8.2 — Build approval flow**
- API: `POST /api/approvals/:id/approve`, `POST /api/approvals/:id/reject`
- Frontend: Approval queue page with pending items, approve/reject buttons
- Wire: content-engine and gtm-engine check approval before publishing/sending
- Why: ADR-008 mandates human approval gates. This is architecturally required, not optional. Content publishing and outreach without approval is a liability.
- Effort: 4-5 hours

**Action 8.3 — Build harvest pipeline page**
- Route: `/harvest`
- Displays: Opportunity pipeline (Cold → Contacted → Qualified → Proposal → Won/Lost)
- Components: Card, Badge, Tabs, ProgressBar, EmptyState
- Why: This is RUSVEL's revenue generation page. Opportunities → proposals → revenue. It's the most valuable page after the dashboard.
- Effort: 3-4 hours

**Action 8.4 — Build content calendar page**
- Route: `/content`
- Displays: Content items by status (draft → review → scheduled → published)
- Components: Card, Badge, Tabs, Calendar-like view
- Why: Content is RUSVEL's visibility engine. Blog posts, tweets, LinkedIn articles — all managed here.
- Effort: 3-4 hours

### Days 18-20: CI/CD + Security + Embed

**Action 9.1 — Create GitHub Actions CI**
```yaml
# .github/workflows/ci.yml
- cargo clippy -- -D warnings
- cargo test
- cargo build --release
- cd frontend && pnpm install --frozen-lockfile && pnpm check && pnpm build
```
- Why: Without CI, regressions go unnoticed until manual testing. With CI, every push is verified. One-time setup, permanent value.
- Effort: 1 hour

**Action 9.2 — Run `/security-review` on all API endpoints**
- Scope: `rusvel-api/src/lib.rs`, all handlers
- Check: Input validation, auth, injection, rate limiting
- Why: Before opening any API to the frontend, security review is mandatory. Especially the approval endpoints.
- Effort: 1 hour (Claude does the work)

**Action 9.3 — Embed frontend via rust-embed**
- File: `rusvel-app/Cargo.toml` (add rust-embed), `rusvel-api/src/lib.rs` (serve embedded files)
- Why: Single binary distribution. `./rusvel-app` serves both API and frontend. No separate `pnpm dev`. True single-binary experience.
- Effort: 2 hours

**Action 9.4 — Create pre-commit git hook**
- Content: `cargo clippy && cargo test --quiet && cd frontend && pnpm check`
- Why: Local quality gate before commits even reach CI. Fast feedback loop.
- Effort: 15 minutes

---

## Why Not a Different Order?

### "Why not build frontend first?"
Frontend without reliable LLM backend shows empty states. Fix the engine before building the dashboard. Also, the design system (Wave 0 — already done) makes frontend work fast once we get there.

### "Why not CI/CD first?"
CI/CD guards code that exists. We need to write the code first (expose engines, wire MCP, build approval flow). CI/CD in Week 3 guards everything we built in Weeks 1-2.

### "Why not skip subagents and just code?"
A solo builder working **55** workspace members cannot hold all conventions in their head (see [`docs/status/current-state.md`](../status/current-state.md)). Subagents enforce conventions by construction. The `rust-engine` subagent literally cannot import adapter crates because its system prompt forbids it. This prevents the #1 architectural risk.

### "Why skills before second engine?"
Because the `/wire-engine` skill is used to expose the second engine. Building the skill first means the second AND third AND fourth engine all benefit. Tool before task.

### "Why MCP before frontend?"
MCP is 15 minutes (one `.mcp.json` file). Frontend is 15+ hours. MCP unlocks self-building immediately — Claude can manage RUSVEL sessions from the terminal. Frontend can be built gradually after.

### "Why hooks before more features?"
Hooks prevent quality decay. Without auto-format, code gets messy. Without architecture guards, engines accumulate adapter imports. Without notifications, the developer wastes time checking terminal. Each hook is 5-15 minutes to set up but saves hours over the project lifetime.

---

## Success Criteria

### Phase 0 Complete (Day 20)

| Criterion | Verification |
|-----------|-------------|
| Single binary < 50MB | `ls -lh target/release/rusvel-app` |
| `rusvel forge mission today` works | `cargo run -- forge mission today` |
| `rusvel harvest scan` works | `cargo run -- harvest scan` |
| Approval flow works | Click approve in UI → content publishes |
| Job queue processes jobs | Check job status after queuing work |
| Frontend embedded | `cargo run` → open http://localhost:3000 |
| 150+ tests pass | `cargo test` |
| CI/CD active | GitHub Actions green |
| MCP self-loop works | Claude calls `rusvel.mission_goals` via MCP |

### Developer Velocity (Ongoing)

| Metric | Target |
|--------|--------|
| Permission prompts per session | < 3 (settings.json handles the rest) |
| Architecture violations caught | 0 (hooks prevent them) |
| Time to expose new engine | < 1 hour (using `/wire-engine` skill) |
| Time to create new component | < 10 minutes (using `/new-component` skill) |
| Format issues | 0 (auto-format hooks) |
| Regressions caught in CI | 100% |

---

## Total Effort Estimate

| Wave | Days | Hours (est.) | What You Get |
|------|------|-------------|-------------|
| Wave 1: Foundation | 1-3 | ~8 hours | Reliable LLM, MCP wired, dev infrastructure, auto-format, notifications |
| Wave 2: Acceleration | 4-10 | ~15 hours | 5 agents, 5 skills, 3 engines exposed, job queue, architecture guards |
| Wave 3: Convergence | 11-20 | ~25 hours | Self-building loop, frontend pages, approval flow, CI/CD, single binary |
| **Total** | **20 days** | **~48 hours** | **Phase 0 complete + self-building RUSVEL** |

At 3-4 hours/day of focused work, this is achievable in 2-3 weeks.

---

## What Comes After (Phase 1 Preview)

Once Phase 0 is done:
- Port safety patterns (CircuitBreaker, RateLimiter, CostTracker) from old/forge-project
- Wire code-engine and gtm-engine via `/wire-engine`
- Add real platform adapters (DEV.to, Twitter, LinkedIn)
- Multi-provider LLM routing (Ollama → Claude CLI → Claude API)
- Agent Teams for parallel development
- TUI surface
- Advanced agent workflows (Pipeline, Best-of-N, Concurrent)

The foundation supports all of this. Phase 0's job is to prove the foundation works. The 8 strategy reports are the playbook for Phase 1+.
