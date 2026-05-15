# Orchestration Toolkit — Claude Code, Zellij, gh, Worktrees

> **Pattern:** one zellij tab per workstream, each tab runs an independent `claude` session, each session owns its worktree and ships its PR. Lead pane drives the others via `zellij action`. Coordination happens through GitHub (issues, PRs, project board), not through in-session subagents.
>
> **Why:** subagents spawned via the Agent tool are sandboxed to the parent's cwd and cannot reach sibling worktrees. Independent `claude` sessions don't share that limitation — each gets its own cwd and permission scope.

---

## 1. Quick-start: spawn N parallel workstreams

Use `scripts/orchestrate.sh spawn` — it bundles the right flags and the
autonomous-mode system prompt for you:

```bash
# Rename the current tab so we can return to it
zellij action rename-tab "lead"

# Default = autonomous: --dangerously-skip-permissions + GitHub-issue-as-channel
./scripts/orchestrate.sh spawn opt-D feat/llm-real-streaming \
  "Work on https://github.com/mbaneshi/rusvel/issues/7 — read it with gh, ship the PR autonomously. Post status updates on the issue."

# Supervised = acceptEdits + you babysit (Bash still prompts)
./scripts/orchestrate.sh spawn opt-D feat/llm-real-streaming \
  "...prompt..." --supervised
```

### Two modes

| Mode | Flag combo | When to use | Risk |
|---|---|---|---|
| **autonomous** (default) | `--dangerously-skip-permissions` + appended system prompt directing the spawned Claude to post status/blockers via `gh issue comment` | You want the tab to run hands-off and report back via durable GitHub artifacts. Trusted local dev only. | Full tool bypass — never use on a host with sensitive credentials or network access to prod. |
| **supervised** | `--permission-mode acceptEdits` | You plan to watch the tab and approve Bash. Slower, but a human sees every shell call. | Bash still prompts; you must stay in the loop. |

### Raw equivalent

If you'd rather skip the script, the raw command behind autonomous mode is:

```bash
zellij action new-tab --cwd /home/shahab/rusvel --name "opt-D"
zellij action write-chars 'claude --dangerously-skip-permissions --worktree feat/llm-real-streaming --name opt-D --append-system-prompt "You are operating autonomously in your own zellij tab and git worktree. Coordinate via GitHub: gh issue comment <N> on start, blockers, decisions, milestones. Under 5 lines, action-oriented. Trusted local dev." "Work on https://github.com/mbaneshi/rusvel/issues/7 — ship the PR autonomously."'
zellij action write 13
zellij action go-to-tab-name "lead"
```

**Critical flags:**

- `--worktree <name>` — Claude creates a git worktree under `~/.claude/worktrees/<name>`. No manual `git worktree add`.
- `--dangerously-skip-permissions` — Aliased by `--permission-mode bypassPermissions`. Fully autonomous. **Trusted local dev only.**
- `--permission-mode acceptEdits` — Edits/Writes auto-approved; Bash still prompts. Safer middle ground.
- `--name "<label>"` — Visible in `/resume` picker, terminal title, prompt box. Use as the tab label.
- `--append-system-prompt "<text>"` — Adds to the default system prompt without replacing it. Good place to inject the "report via gh issue comment" contract.

---

## 2. Reading what a tab is doing

The lead pane can inspect any other tab without disrupting it:

```bash
# List all tabs and confirm names
zellij action query-tab-names

# Switch to a tab, dump its visible content to a file, switch back, read
zellij action go-to-tab-name "opt-D"
zellij action dump-screen /tmp/opt-D-screen.txt
# Add --full to include scrollback (everything the pane has emitted)
zellij action dump-screen --full /tmp/opt-D-scrollback.txt
zellij action go-to-tab-name "lead"
cat /tmp/opt-D-screen.txt
```

`dump-screen` writes the **focused** pane's buffer to disk. There is no "dump tab by name" in one call — you must focus first, dump, then return. Focus changes are visible to a human watching the session, so use sparingly.

For passive snapshots without focus changes, see `zellij action pipe` (plugin-based, more setup).

---

## 3. Sending text / keys to another tab

```bash
zellij action go-to-tab-name "opt-D"
zellij action write-chars "additional instructions here"
zellij action write 13          # Enter (ASCII 13)
zellij action write 27          # Esc (ASCII 27)
zellij action write 3           # Ctrl-C (ASCII 3)
zellij action go-to-tab-name "lead"
```

- `write-chars <text>` — types literal characters (no shell escaping for the receiver — but the *sender* shell must escape `'`, `"`, `$` etc.).
- `write <byte>...` — sends raw byte codes. Useful for Enter, Esc, control keys.

**Coordinating via GitHub is usually better than write-chars** for non-trivial nudges (more durable, visible in PR history, agent can `gh issue view` to re-read).

---

## 4. Claude Code 2.1.x flags worth knowing

(`claude --help` output as of 2.1.142 — re-run after upgrades.)

| Flag | What it does | Use when |
|---|---|---|
| `--worktree [name]` / `-w` | Creates a git worktree for the session | Spawning a focused workstream |
| `--tmux` | Pairs with `--worktree` to open in tmux | If you prefer tmux over zellij |
| `--permission-mode <mode>` | `default`, `acceptEdits`, `bypassPermissions`, `plan`, `auto`, `dontAsk` | Tune autonomy per session |
| `--allow-dangerously-skip-permissions` | Makes `--dangerously-skip-permissions` available without enabling | Locked-down envs |
| `--dangerously-skip-permissions` | Full bypass — recommended only in sandboxed envs | Trusted automation |
| `--allowedTools <tools...>` | Allowlist tool patterns (e.g. `"Bash(git *) Edit"`) | Tighter scope than permission-mode |
| `--disallowedTools <tools...>` | Denylist | Block specific tool patterns |
| `--tools <tools...>` | Limit built-in tool set | `"Bash,Edit,Read"` etc. |
| `--name <label>` / `-n` | Session display name | Tab labels, `/resume` picker |
| `--continue` / `-c` | Continue most recent conversation in current dir | Resume after restart |
| `--resume [value]` / `-r` | Resume by session ID or picker | Reopen a specific session |
| `--from-pr [N\|URL]` | Resume the session linked to a PR | PR review workflows |
| `--fork-session` | New session ID when resuming | Branch off an existing transcript |
| `--remote-control [name]` | Externally controllable session | Programmatic driving |
| `--add-dir <dirs...>` | Additional dirs to allow tool access to | Multi-repo work |
| `--mcp-config <files...>` | Load MCP servers from JSON | Per-session MCP wiring |
| `--agents <json>` | Inline custom agent definitions | One-off agent personas |
| `--append-system-prompt <text>` | Add to system prompt | Per-session guidance |
| `--system-prompt <text>` | Replace system prompt | Headless / custom workflows |
| `--effort <level>` | `low\|medium\|high\|xhigh\|max` | Budget control |
| `--max-budget-usd <amt>` | Hard spend cap (only with `--print`) | Cost-limited batch jobs |
| `--model <model>` | Override default model | `sonnet`, `opus`, full id |
| `--fallback-model <model>` | Auto-fallback on overload (`--print` only) | Reliability for batch |
| `--print` / `-p` | Non-interactive, print and exit | Pipe into other tools |
| `--output-format <fmt>` | `text\|json\|stream-json` (with `--print`) | Machine-readable output |
| `--input-format <fmt>` | `text\|stream-json` (with `--print`) | Streaming input |
| `--json-schema <schema>` | Validate structured output | Type-safe responses |
| `--include-partial-messages` | Stream partial chunks (with `--print` + `stream-json`) | Real-time UIs |
| `--bare` | Minimal mode (skip hooks, auto-memory, CLAUDE.md autoload) | Deterministic, isolated runs |
| `--setting-sources <list>` | Comma-list of `user`, `project`, `local` | Control settings loading |
| `--ide` | Auto-connect to IDE if available | Editor integration |
| `--debug [filter]` / `-d` | Debug mode (e.g. `"api,hooks"`) | Troubleshooting |

**Subcommands:**

- `claude agents` — Manage background agents (built-in). Flags: `--cwd`, `--effort`, `--model`, `--permission-mode`.
- `claude mcp` — `add`, `list`, `get`, `remove`, `add-json`, `add-from-claude-desktop`, `reset-project-choices`, `serve`.
- `claude project purge [path]` — Wipe Claude Code state for a project.
- `claude ultrareview [target] [--json] [--timeout N]` — Cloud-hosted multi-agent code review of branch or PR.
- `claude doctor` — Health check.
- `claude setup-token` — Long-lived auth token.

---

## 5. Zellij action cheat sheet

(`zellij action --help` — re-run after zellij upgrades.)

| Action | What it does |
|---|---|
| `rename-tab <name>` | Rename current tab (use early so you can `go-to-tab-name` back) |
| `rename-pane <name>` | Rename focused pane |
| `rename-session <name>` | Rename the session |
| `new-tab --cwd <path> --name <name> [--layout <l>]` | Create + focus new tab |
| `new-pane [--cwd <path>] [-d right\|down] [-- <cmd>...]` | Create + focus new pane |
| `go-to-tab-name <name>` | Switch focus by tab name |
| `go-to-tab <index>` | Switch focus by 1-based index |
| `go-to-next-tab` / `go-to-previous-tab` | Tab navigation |
| `move-tab <right\|left>` | Reorder tabs |
| `query-tab-names` | List all tab names (no focus change) |
| `close-tab` / `close-pane` | Close current |
| `write-chars <text>` | Type characters into focused pane |
| `write <byte>...` | Send raw bytes (`13`=Enter, `27`=Esc, `3`=Ctrl-C) |
| `dump-screen [--full] <path>` | Write focused pane buffer to file |
| `dump-layout` | Print current layout to stdout |
| `edit-scrollback` | Open scrollback in `$EDITOR` |
| `clear` | Clear focused pane |
| `focus-next-pane` / `focus-previous-pane` | Within-tab pane focus |
| `move-focus <right\|left\|up\|down>` | Directional pane focus |
| `move-focus-or-tab <dir>` | Like move-focus but jumps tabs at edges |
| `move-pane <dir>` / `move-pane-backwards` | Reorder panes |
| `page-scroll-up` / `page-scroll-down` | Scroll focused pane |
| `half-page-scroll-up` / `half-page-scroll-down` | Half-page scroll |
| `list-clients` | Show connected clients + their pane + running command |
| `launch-plugin` / `launch-or-focus-plugin` | Plugin management |
| `pipe [--plugin url] [--name n] -- <payload>` | Send data to plugins (advanced) |

---

## 6. gh project for board coordination

```bash
# List items on Project #6 owned by mbaneshi
gh project item-list 6 --owner mbaneshi --format json | jq '.items[] | {n: .content.number, t: .content.title}'

# Get field IDs (Status has a single-select with options Todo / In Progress / Done)
gh project field-list 6 --owner mbaneshi --format json | jq '.fields[] | select(.name=="Status")'

# Move an item — needs item ID, project ID, field ID, single-select option ID
gh project item-edit \
  --id PVTI_xxx \
  --project-id PVT_xxx \
  --field-id PVTSSF_xxx \
  --single-select-option-id <OPTION_ID>
```

For spawning, the **item ID** is what links a tab to its card. Pass it into the spawn prompt so the spawned Claude can move its own card.

---

## 7. git worktree refresher

```bash
git worktree add -b <new-branch> <path> <commit-ish>   # create + check out
git worktree add <path> <existing-branch>              # check out existing
git worktree list                                       # all worktrees
git worktree remove [-f] <path>                         # delete (use -f if dirty)
git worktree prune                                      # tidy stale refs
git worktree lock <wt>                                  # prevent accidental removal
```

**Important:** Claude Code's `--worktree` flag puts its worktrees under `~/.claude/worktrees/`, NOT under `..`. To find one:

```bash
git -C ~/.claude/worktrees/<name> branch --show-current
```

---

## 8. The spawn pattern, end-to-end

```bash
# 1. Read the issue body via gh so the prompt can be terse
gh issue view 7 --repo mbaneshi/rusvel --json title,body

# 2. From the lead pane, spawn the workstream tab
zellij action new-tab --cwd /home/shahab/rusvel --name "opt-D"
zellij action write-chars 'claude --worktree feat/llm-real-streaming --permission-mode acceptEdits --name "opt-D" "Work on mbaneshi/rusvel#7. Read with gh, ship PR autonomously."'
zellij action write 13
zellij action go-to-tab-name "lead"

# 3. From the lead, monitor durable signals (these are cheap)
gh pr list --repo mbaneshi/rusvel
gh project item-list 6 --owner mbaneshi --format json
git ls-remote --heads origin 'feat/*' 'cut/*'

# 4. To peek at the tab itself
zellij action go-to-tab-name "opt-D"
zellij action dump-screen --full /tmp/opt-D-screen.txt
zellij action go-to-tab-name "lead"
less /tmp/opt-D-screen.txt

# 5. To nudge a tab via durable channel
gh issue comment 7 --repo mbaneshi/rusvel --body "Heads-up: ..."
# The spawned Claude can re-fetch with `gh issue view` to see updates.

# 6. To send instructions directly (for trivial nudges only)
zellij action go-to-tab-name "opt-D"
zellij action write-chars "look at frontend/build/ if rust-embed errors"
zellij action write 13
zellij action go-to-tab-name "lead"
```

---

## 9. Hard-won rules

1. **Spawned `claude` sessions are peers, not subagents.** They don't see this conversation's context. Give them enough in the bootstrap prompt (or rely on `gh issue view <N>` + `CLAUDE.md` autoload).
2. **Permission-mode matters.** Default mode stalls on every Bash/Edit prompt. Use `acceptEdits` for trusted workstreams; reserve `bypassPermissions` for fully sandboxed envs.
3. **Use `--worktree`, not manual `git worktree add`.** Subagents inside a `claude` session are sandboxed to the session's cwd; they can't reach sibling paths you created out-of-band.
4. **Coordinate via durable artifacts** (issues, PR comments, project board). In-session messaging doesn't survive a restart; GitHub state does.
5. **`dump-screen --full` is your watcher.** It's the cheapest way to see what another tab is doing without disturbing it.
6. **Re-read `--help` after every upgrade.** Claude Code ships flags often (`--from-pr`, `--remote-control`, `agents` subcommand all arrived in 2.1.x).
7. **One tab = one PR.** Don't share state across tabs. If two workstreams need to share, do it through a shared issue or a shared file in `main`.

---

## Related

- [Audit 2026-05-15](../audit/audit-2026-05-15.md) — the trigger for this multi-workstream setup
- [Parent issue #4](https://github.com/mbaneshi/rusvel/issues/4)
- [Project board "Audit 2026-05-15"](https://github.com/users/mbaneshi/projects/6)
- [Discussion #9 — §10 strategic questions](https://github.com/mbaneshi/rusvel/discussions/9)
