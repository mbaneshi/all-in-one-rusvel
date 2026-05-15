# Parallel Worktrees + Zellij Lead-Pane Pattern

> Pattern for working on multiple audit options (or any independent workstreams) in parallel — one git worktree per option, one zellij pane per worktree, one "lead" pane that drives the others via `zellij action`.

## Why

Serial work on independent workstreams is slow and discourages exploration. The 2026-05-15 audit (#4) split into four issues (#5–#8) that touch different crates and different concerns — they should run side-by-side, not one after another.

## Topology

```
┌──────────────────────────────────────────────────────────────┐
│ Lead pane (this is where you live)                           │
│  - Drives the other panes via `zellij action`                │
│  - Runs Claude Code, kicks off agents, opens PRs             │
├──────────────┬──────────────┬──────────────┬─────────────────┤
│ rusvel-cut   │ rusvel-      │ rusvel-      │ rusvel-evals    │
│ (Option A)   │ checkpoint   │ streaming    │ (Option E)      │
│              │ (Option B)   │ (Option D)   │                 │
│ cargo watch  │ cargo watch  │ cargo watch  │ cargo watch     │
│ + test logs  │ + test logs  │ + test logs  │ + test logs     │
└──────────────┴──────────────┴──────────────┴─────────────────┘
```

## Worktrees (already created)

| Worktree path | Branch | Issue | Audit option |
|---|---|---|---|
| `/home/shahab/rusvel` | `main` | — | lead pane |
| `/home/shahab/rusvel-cut` | `cut/skeleton-engines` | #5 | A — cut skeleton engines |
| `/home/shahab/rusvel-checkpoint` | `feat/job-checkpoints` | #6 | B — checkpointing |
| `/home/shahab/rusvel-streaming` | `feat/llm-real-streaming` | #7 | D — Ollama/OpenAI streaming |
| `/home/shahab/rusvel-evals` | `feat/evals-crate` | #8 | E — evals crate |

Re-create if missing:

```bash
git worktree add -b cut/skeleton-engines    ../rusvel-cut          origin/main
git worktree add -b feat/job-checkpoints    ../rusvel-checkpoint   origin/main
git worktree add -b feat/llm-real-streaming ../rusvel-streaming    origin/main
git worktree add -b feat/evals-crate        ../rusvel-evals        origin/main
```

Remove a worktree when its PR lands:

```bash
git worktree remove ../rusvel-cut
git branch -D cut/skeleton-engines   # only if the PR is merged
```

## Zellij layout

A reusable layout that lays the four worker panes out for you. Save as `~/.config/zellij/layouts/rusvel-audit.kdl`:

```kdl
layout {
    pane_template name="worker" {
        pane stacked=false
    }
    tab name="lead" {
        pane size="60%" {
            cwd "/home/shahab/rusvel"
        }
        pane split_direction="vertical" {
            pane name="cut"        cwd="/home/shahab/rusvel-cut"
            pane name="checkpoint" cwd="/home/shahab/rusvel-checkpoint"
            pane name="streaming"  cwd="/home/shahab/rusvel-streaming"
            pane name="evals"      cwd="/home/shahab/rusvel-evals"
        }
    }
}
```

Launch:

```bash
zellij --layout ~/.config/zellij/layouts/rusvel-audit.kdl
```

## Driving worker panes from the lead

`zellij action` runs from inside any pane and targets a named pane. From the lead pane:

```bash
# Focus a named worker pane
zellij action focus-pane -p cut

# Type a command into the focused pane (does NOT press Enter)
zellij action write-chars "cargo test -p rusvel-app"

# Send the Enter key (key 0x0D / "enter")
zellij action write 13

# One-shot: focus + type + enter
zellij action focus-pane -p streaming \
  && zellij action write-chars "cargo test -p rusvel-llm streaming" \
  && zellij action write 13
```

Reusable helper — put in your shell `.zshrc` / `.bashrc`:

```bash
# usage: zr <pane-name> <command...>
zr() {
  local pane="$1"; shift
  zellij action focus-pane -p "$pane" \
    && zellij action write-chars "$*" \
    && zellij action write 13
}
```

Then from the lead pane:

```bash
zr cut         "cargo check"
zr checkpoint  "cargo test -p rusvel-jobs"
zr streaming   "cargo test -p rusvel-llm"
zr evals       "cargo run -p rusvel-evals -- --suite forge"
```

## Lead-pane workflow

1. **Plan in the lead** — read issue body, draft the change in your head, identify which worker pane gets the work.
2. **Dispatch** — `zr <pane> "<initial command>"` to kick a build/test in the worker.
3. **Work in the worker pane** — edit files there (each worker is its own working tree, no conflicts).
4. **Watch from the lead** — `zr <pane> "cargo watch -x test"` if you want continuous feedback.
5. **PR from the worker** — `git push -u origin <branch> && gh pr create --base main --title "..." --body "Closes #N"`.

## Subagent orchestration

When using Claude Code (or any agent SDK), each worker pane can host its own agent session scoped to that worktree:

```bash
# From the lead, kick off four independent agents
zr cut         "claude"
zr checkpoint  "claude"
zr streaming   "claude"
zr evals       "claude"
```

Then prompt each agent with the corresponding issue (#5–#8). They cannot conflict because each worktree has its own filesystem view of the repo.

## Caveats

- **Shared `target/` directory.** Cargo's default `target/` is per-worktree, which doubles disk use. Set `CARGO_TARGET_DIR=/home/shahab/.cache/rusvel-target` (or use the project-level setting) to share artifacts — but be aware concurrent cargo invocations against a shared target serialize on a lock.
- **Cargo.lock divergence.** All four worktrees inherit the main `Cargo.lock`. If one worker bumps a dep, the others will rebase later. Coordinate dep changes via the lead pane.
- **Frontend build artifacts.** `frontend/build/` is gitignored. If a worker needs the embedded SPA to compile, run `pnpm build` in that worktree's `frontend/`.

## Related

- GitHub Project: [Audit 2026-05-15](https://github.com/users/mbaneshi/projects/6)
- Parent issue: [#4 Audit 2026-05-15](https://github.com/mbaneshi/rusvel/issues/4)
- Discussion for strategic questions: [§10 deferred — pitch / MCP / frontend / LLM focus](https://github.com/mbaneshi/rusvel/discussions/9)
- Audit doc: [`docs/audit/audit-2026-05-15.md`](../audit/audit-2026-05-15.md)
