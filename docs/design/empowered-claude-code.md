# RUSVEL — Empowered Claude Code Architecture

> Write prompts once. Never repeat yourself. Build a system that gets smarter every day.
> One binary that makes Claude Code 10x more powerful.

---

## 1. The Problem

You spend 8+ hours/day in Claude Code. You:
- Write similar prompts repeatedly for common tasks
- Lose context between sessions (what you decided, why, what worked)
- Manually manage skills, rules, agents, MCP servers across projects
- Can't automate recurring workflows (daily brief, code review, deploy checks)
- Have dozens of community repos (superpowers, claude-mem, hermes patterns, last30days) but no unified way to use them
- Want Claude Chat/Cowork features (persistent threads, collaboration) without leaving your workflow

## 2. The Vision

RUSVEL is the **brain behind Claude Code**. It doesn't replace Claude Code — it empowers it:

```
┌─────────────────────────────────────────────────┐
│  Claude Code (your terminal, your agent loop)   │
│                                                  │
│  You type. Claude acts. Tools execute.           │
│  Skills fire. Agents spawn. Code ships.          │
└────────────┬────────────────────────────────────┘
             │  MCP + .claude/ configs
             │
┌────────────┴────────────────────────────────────┐
│  RUSVEL (the brain)                              │
│                                                  │
│  Persistent memory across all sessions           │
│  Department-scoped contexts (code, content, gtm) │
│  Skill/rule/agent registry & marketplace         │
│  Automation: hooks, triggers, scheduled tasks    │
│  Web UI: chat, flows, dashboard, knowledge       │
│  Community extensions: curated, versioned         │
└─────────────────────────────────────────────────┘
```

**Claude Code owns:** The agent loop, terminal, file tools, streaming, permissions, IDE integration.
**RUSVEL owns:** Persistence, departments, automation, community registry, web UI, cross-session intelligence.

## 3. Integration Architecture

### 3.1 RUSVEL as MCP Server (primary bridge)

Claude Code already supports MCP servers. RUSVEL already has `--mcp` mode. This is the natural bridge.

```
claude code  ──MCP──>  rusvel --mcp
                       │
                       ├── memory_store / memory_recall / memory_search
                       ├── dept_context (load department-scoped rules + context)
                       ├── knowledge_search (RAG across all sessions)
                       ├── skill_registry (list/install/run community skills)
                       ├── automation_trigger (fire a workflow or scheduled task)
                       └── session_context (goals, events, metrics for current work)
```

**What this gives Claude Code:**
- Persistent memory that survives across sessions and projects
- Department-scoped context packs (when working on code vs content vs GTM)
- Knowledge base search (semantic + FTS5 across everything you've captured)
- Access to RUSVEL's automation engine from within Claude Code

**Implementation:** Expand `rusvel-mcp` from 6 tools to ~15 tools covering memory, departments, knowledge, automation. RUSVEL already has the backends — just needs MCP tool wrappers.

### 3.2 RUSVEL as Config Manager (`.claude/` generation)

Claude Code loads skills from `.claude/skills/`, agents from `.claude/agents/`, rules from `.claude/rules/`, hooks and MCP from `settings.json`. RUSVEL can **generate and manage these files**.

```
rusvel sync-config
  │
  ├── .claude/skills/          ← generated from RUSVEL skill registry
  │   ├── code-review.md       ← community: superpowers
  │   ├── tdd.md               ← community: superpowers
  │   ├── content-draft.md     ← RUSVEL: content department
  │   └── harvest-scan.md      ← RUSVEL: harvest department
  │
  ├── .claude/agents/          ← generated from RUSVEL agent definitions
  │   ├── researcher.md        ← RUSVEL: forge department
  │   ├── arch-reviewer.md     ← RUSVEL: code department
  │   └── content-writer.md    ← RUSVEL: content department
  │
  ├── .claude/rules/           ← generated from RUSVEL rules registry
  │   ├── hexagonal.md         ← RUSVEL: code department
  │   └── voice.md             ← RUSVEL: content department
  │
  └── .claude/settings.json    ← MCP servers, hooks, permissions
      └── mcpServers: { "rusvel": { "command": "rusvel", "args": ["--mcp"] } }
```

**What this gives you:**
- One command to sync your entire Claude Code configuration
- Skills/agents/rules managed centrally in RUSVEL's database, deployed to any project
- Community extensions installed via `rusvel install superpowers` → generates skill files
- Per-department configurations: `rusvel sync-config --dept content` only syncs content-related skills

### 3.3 RUSVEL Web UI as Chat/Cowork Surface

RUSVEL already has a SvelteKit frontend with department chat, flows, knowledge, CRM. This is the Claude Chat/Cowork equivalent:

- **Persistent chat threads** per department (already working via SSE)
- **Visual workflow builder** (flow-engine DAG, already exists)
- **Knowledge base** with semantic search (already exists)
- **Dashboard** with goals, events, metrics (already exists)
- **Content Studio** (just built)
- **Approval queue** for human-in-the-loop (already exists)

The web UI is for **review, planning, and oversight** — the things that don't belong in a terminal.

## 4. The Skill/Extension Registry

### 4.1 Extension Types

| Type | Claude Code Native | RUSVEL Manages | Example |
|------|-------------------|----------------|---------|
| **Skill** | `.claude/skills/*.md` | Registry + install + sync | superpowers/tdd, content-draft |
| **Agent** | `.claude/agents/*.md` | Registry + install + sync | researcher, arch-reviewer |
| **Rule** | `.claude/rules/*.md` | Registry + enable/disable per dept | hexagonal, voice, crate-size |
| **Hook** | `settings.json` hooks | Registry + lifecycle management | pre-commit check, post-deploy notify |
| **MCP Server** | `settings.json` mcpServers | Registry + connect/disconnect | rusvel, github, slack, stripe |
| **Workflow** | N/A (RUSVEL-only) | Flow-engine DAGs | harvest→score→propose, code→content |

### 4.2 Community Extension Install

```bash
# Install a community skill pack
rusvel install superpowers
# → Downloads from registry/GitHub
# → Stores in RUSVEL's ObjectStore
# → `rusvel sync-config` deploys to .claude/skills/

# Install a specific skill
rusvel install last30days/research

# List installed extensions
rusvel extensions list

# Enable/disable per department
rusvel extensions enable tdd --dept code
rusvel extensions disable tdd --dept content
```

### 4.3 Extension Format (RUSVEL-native, CC-compatible)

RUSVEL stores extensions in its ObjectStore. On `sync-config`, it generates CC-compatible markdown files:

```markdown
---
name: tdd
description: Test-driven development workflow with RED-GREEN-REFACTOR
allowedTools: FileRead, FileWrite, FileEdit, Bash, Grep, Glob
model: sonnet
hooks:
  preToolUse:
    - matcher: Bash
      command: "echo 'Running tests first...'"
---

Follow RED-GREEN-REFACTOR cycle:
1. Write a failing test first
2. Write minimal code to make it pass
3. Refactor while keeping tests green

Never write implementation code without a failing test.
```

This is identical to what Claude Code expects. RUSVEL just manages the lifecycle.

## 5. The "Never Write This Prompt Again" System

### 5.1 Problem
You type the same kinds of instructions daily:
- "Review this code for hexagonal architecture compliance"
- "Draft a LinkedIn post about what I just built"
- "Find gigs matching my skills on Freelancer"
- "What did I decide last week about the auth approach?"

### 5.2 Solution: Skills + Rules + Memory + Automation

```
┌──────────────────────────────────────────┐
│  Layer 1: Rules (always active)          │
│  Injected into every prompt automatically│
│  "Engines never import adapter crates"   │
│  "Content voice: direct, technical"      │
└──────────────────────┬───────────────────┘
                       │
┌──────────────────────┴───────────────────┐
│  Layer 2: Skills (on-demand, /slash)     │
│  "/review" → full arch review workflow   │
│  "/draft" → content generation pipeline  │
│  "/harvest" → opportunity scan + score   │
└──────────────────────┬───────────────────┘
                       │
┌──────────────────────┴───────────────────┐
│  Layer 3: Agents (autonomous, delegated) │
│  @researcher → deep investigation        │
│  @content-writer → draft + adapt + post  │
│  @arch-reviewer → boundary compliance    │
└──────────────────────┬───────────────────┘
                       │
┌──────────────────────┴───────────────────┐
│  Layer 4: Automation (no prompt needed)  │
│  On push → run tests + arch check        │
│  Daily 9am → scan freelancer, brief      │
│  On content.drafted → adapt all platforms│
│  On harvest.scored → notify Telegram     │
└──────────────────────┬───────────────────┘
                       │
┌──────────────────────┴───────────────────┐
│  Layer 5: Memory (cross-session context) │
│  "Last time you worked on auth, you      │
│   decided X because Y. Here's the PR."   │
└──────────────────────────────────────────┘
```

Each layer eliminates a class of repetitive prompting:
- **Rules** → you never explain conventions again
- **Skills** → complex workflows become one slash command
- **Agents** → delegation becomes a mention, not a paragraph
- **Automation** → recurring tasks happen without you
- **Memory** → context is always there, you never re-explain

## 6. What RUSVEL Needs to Build (Prioritized)

### Phase 1: The MCP Bridge (make Claude Code smarter today)

**Goal:** `rusvel --mcp` becomes the most useful MCP server in your Claude Code setup.

| MCP Tool | What It Does | Backend Exists? |
|----------|-------------|-----------------|
| `memory_store` | Save a fact/decision/learning | Yes (rusvel-memory) |
| `memory_recall` | Search memory by query | Yes (FTS5) |
| `knowledge_search` | Semantic search across knowledge base | Yes (vector store, gated) |
| `dept_context` | Load department-scoped rules + context pack | Yes (DepartmentApp + config cascade) |
| `session_goals` | Get/set goals for current work session | Yes (forge-engine goals) |
| `recent_events` | What happened recently in this department | Yes (event bus) |
| `skill_run` | Execute a RUSVEL skill by name | Yes (skill resolution) |
| `automation_trigger` | Fire a workflow or scheduled task | Yes (job queue + flow engine) |

**Work needed:** Expand `rusvel-mcp` tool definitions from 6 to ~10-15. The backends all exist — just MCP wrappers.

### Phase 2: Config Sync (community extensions as first-class)

**Goal:** `rusvel sync-config` deploys your entire skill/agent/rule/hook configuration to `.claude/`.

| Feature | What It Does | Effort |
|---------|-------------|--------|
| `rusvel sync-config` CLI command | Generate .claude/ files from RUSVEL registry | Medium |
| Extension registry in ObjectStore | Store skills/agents/rules with metadata | Small (ObjectStore exists) |
| `rusvel install <source>` | Download from GitHub/registry → ObjectStore | Medium |
| Per-department scoping | Only deploy relevant extensions | Small (dept manifests exist) |
| Conflict resolution | Handle overlapping skill names | Small |

### Phase 3: Automation Layer (things happen without prompting)

**Goal:** RUSVEL runs workflows automatically based on events, schedules, or triggers.

| Feature | What It Does | Effort |
|---------|-------------|--------|
| Event triggers → agent runs | "On push, run arch-review agent" | Partial (TriggerManager exists) |
| Cron-scheduled workflows | "Daily 9am: scan freelancer, generate brief" | Partial (ScheduledCron job kind exists) |
| Hook dispatch on Claude Code events | "After chat.completed, extract memories" | Partial (hook_dispatch.rs exists) |
| Cross-department pipelines | harvest→score→propose→draft→schedule | Partial (forge pipeline exists) |

### Phase 4: Web UI as Control Plane

**Goal:** The browser is for oversight, planning, and visual work — not terminal tasks.

| Feature | Status |
|---------|--------|
| Department chat with streaming | Working |
| Content Studio (capture → draft → publish) | Just built |
| Flow builder (visual DAGs) | Working |
| Knowledge base (search, ingest) | Working |
| Approval queue | Working |
| Reasoning panel (show agent decisions) | Needs frontend work |
| Extension marketplace | Not started |

## 7. What NOT to Build

- **Don't rebuild Claude Code's agent loop** — it's better than anything we'd write
- **Don't rebuild file/bash/grep tools** — CC has them, RUSVEL exposes via MCP what CC doesn't have
- **Don't build another terminal** — CC is the terminal; RUSVEL is the brain
- **Don't try to be a general platform** — this is for one person (you), optimize ruthlessly for your workflow
- **Don't add more departments** — 7 stubs are fine empty; focus on code + content + harvest + forge

## 8. Build Order

```
Week 1-2: Phase 1 — MCP Bridge
  Expand rusvel --mcp to 10-15 tools
  Memory, knowledge, dept context, goals, events
  Test by using in your daily Claude Code workflow

Week 3-4: Phase 2 — Config Sync
  rusvel sync-config generates .claude/ files
  rusvel install <github-url> downloads community skills
  Install superpowers, integrate its TDD workflow

Week 5-6: Phase 3 — Automation
  Event triggers: on-push → arch-review
  Cron: daily briefing, freelancer scan
  Cross-dept: harvest→content pipeline on schedule

Ongoing: Phase 4 — Web UI polish
  Reasoning panel, extension marketplace
  Content Studio improvements from daily use
  Dashboard refinements
```

## 9. Success Criteria

After Phase 1, you should be able to say:
- "I never re-explain my project conventions — rules handle it"
- "I recall decisions from last month without searching git log"
- "My Claude Code sessions have department-scoped context automatically"

After Phase 2:
- "I installed superpowers with one command and it works in every project"
- "My skills, agents, and rules are managed in one place"
- "Switching between code/content/harvest contexts is one command"

After Phase 3:
- "Freelancer gigs are scored and waiting for me every morning"
- "Content drafts are generated from my captures without prompting"
- "Architecture violations are caught before I even look at the PR"

After Phase 4:
- "I review agent decisions visually before approving"
- "My content calendar fills itself from captures"
- "The dashboard shows what matters without me asking"

---

## 10. Technical Notes

### Claude Code Skill Format (from source)
```markdown
---
name: skill-name
description: What this skill does
allowedTools: FileRead, Bash, Grep
model: sonnet
hooks:
  preToolUse:
    - matcher: Bash
      command: "validation command"
---

Skill prompt content here. Supports {{input}} substitution.
```

### Claude Code Agent Format (from source)
```markdown
---
description: What this agent does
tools: [FileRead, FileWrite, Bash, Grep]
disallowedTools: [AgentTool]
model: sonnet
effort: medium
permissionMode: plan
maxTurns: 20
mcpServers: ["rusvel"]
memory: project
---

Agent system prompt here.
```

### RUSVEL MCP Server Config (for `.claude/settings.json`)
```json
{
  "mcpServers": {
    "rusvel": {
      "command": "rusvel",
      "args": ["--mcp"],
      "env": {}
    }
  }
}
```
