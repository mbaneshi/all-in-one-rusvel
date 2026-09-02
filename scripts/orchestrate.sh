#!/usr/bin/env bash
# orchestrate.sh — multi-tab Claude Code orchestration helpers
#
# Usage:
#   orchestrate.sh spawn <tab-name> <branch> <prompt>   spawn a Claude session in a new zellij tab
#   orchestrate.sh status                               PRs + project board + remote branches
#   orchestrate.sh watch [interval-seconds]             loop status every N seconds (default 30)
#   orchestrate.sh dump <tab-name> [--full]             dump-screen of a tab to /tmp/orch-<tab>.txt
#   orchestrate.sh nudge <tab-name> <text...>           type text + Enter into a tab
#   orchestrate.sh tabs                                 list all zellij tab names
#   orchestrate.sh close <tab-name>                     go to tab, close it, return to lead
#   orchestrate.sh prs                                  watch new PRs in a loop until Ctrl-C
#   orchestrate.sh comments [interval] [issues...]      watch issue comments; default 30s on #5-#8
#   orchestrate.sh help                                 this message
#
# Env overrides:
#   RUSVEL_REPO     (default mbaneshi/rusvel)
#   RUSVEL_OWNER    (default mbaneshi)
#   RUSVEL_PROJECT  (default 6)
#   RUSVEL_LEAD_TAB (default lead)
#   RUSVEL_REPO_DIR (default /home/shahab/rusvel)
#
# Preconditions:
#   - You're inside a zellij session.
#   - One tab is named "$RUSVEL_LEAD_TAB" (rename with: zellij action rename-tab "lead").
#   - `claude`, `gh`, `git`, `jq` on PATH.

set -euo pipefail

REPO="${RUSVEL_REPO:-mbaneshi/rusvel}"
OWNER="${RUSVEL_OWNER:-mbaneshi}"
PROJECT_NUM="${RUSVEL_PROJECT:-6}"
LEAD_TAB="${RUSVEL_LEAD_TAB:-lead}"
REPO_DIR="${RUSVEL_REPO_DIR:-/home/shahab/rusvel}"

die() { echo "orchestrate.sh: $*" >&2; exit 1; }

require() {
  for c in "$@"; do
    command -v "$c" >/dev/null 2>&1 || die "missing dependency: $c"
  done
}

return_to_lead() {
  zellij action go-to-tab-name "$LEAD_TAB" 2>/dev/null || true
}

cmd_spawn() {
  [[ $# -ge 3 ]] || die "spawn: need <tab-name> <branch> <prompt> [--supervised]"
  local name="$1" branch="$2" prompt="$3"
  local mode="${4:-}"   # pass --supervised to opt into acceptEdits + prompts
  require zellij claude
  # Avoid `new-tab --cwd ... --name ...` — zellij 0.41 requires --layout when
  # --cwd is set alongside --name. Skip --cwd and cd inside the spawned shell.
  zellij action new-tab --name "$name"
  sleep 0.5
  local perm_args sys_prompt
  if [[ "$mode" == "--supervised" ]]; then
    # Supervised mode: edits auto-accept, Bash still prompts. Use when you
    # plan to babysit the tab.
    perm_args="--permission-mode acceptEdits"
    sys_prompt=""
  else
    # Autonomous mode (default): full bypass + GitHub-issue-as-channel.
    # The spawned Claude posts blockers/milestones to its issue instead
    # of stalling on prompts. Trusted local dev only.
    perm_args="--dangerously-skip-permissions"
    sys_prompt='You are operating autonomously in your own zellij tab and git worktree. Coordinate with the human via GitHub: use `gh issue comment <N>` (or `gh pr comment <N>`) to post brief status updates on the relevant issue when you (a) start work, (b) hit a decision point or blocker that needs human input, (c) make a non-trivial architectural choice, (d) complete a milestone. Keep comments under 5 lines, action-oriented. Do not block waiting for permission prompts — proceed with the work and report results via GitHub. The host is a trusted local dev environment.'
  fi
  local q_prompt q_sys
  q_prompt=$(printf '%q' "$prompt")
  if [[ -n "$sys_prompt" ]]; then
    q_sys="--append-system-prompt $(printf '%q' "$sys_prompt") "
  else
    q_sys=""
  fi
  zellij action write-chars "cd $(printf '%q' "$REPO_DIR") && claude ${perm_args} --worktree ${branch} --name ${name} ${q_sys}${q_prompt}"
  zellij action write 13
  sleep 0.3
  return_to_lead
  echo "spawned tab '$name' on branch '$branch' (${mode:-autonomous})"
}

cmd_status() {
  require gh jq git
  echo "=== Open PRs in $REPO ==="
  gh pr list --repo "$REPO" --state open \
    --json number,title,headRefName,isDraft,createdAt 2>/dev/null \
    | jq -r '.[] | "  #\(.number) [\(if .isDraft then "draft" else "open" end)] \(.headRefName) — \(.title)"' \
    || echo "  (none)"
  echo
  echo "=== Project #$PROJECT_NUM items ==="
  gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json 2>/dev/null \
    | jq -r '.items[] | "  #\(.content.number) \(.content.title)"' \
    || echo "  (no project access)"
  echo
  echo "=== Audit-related branches on origin ==="
  (cd "$REPO_DIR" && git ls-remote --heads origin 'feat/*' 'cut/*' 2>/dev/null) \
    | awk '{print "  " $2}' \
    || echo "  (none)"
  echo
  echo "=== Active zellij tabs ==="
  zellij action query-tab-names 2>/dev/null | sed 's/^/  /' || echo "  (zellij action unavailable)"
}

cmd_watch() {
  local interval="${1:-30}"
  while true; do
    clear
    echo "$(date '+%F %T')  every ${interval}s  Ctrl-C to exit"
    echo
    cmd_status
    sleep "$interval"
  done
}

cmd_prs() {
  require gh jq
  local seen="/tmp/orch-pr-seen.txt"
  : > "$seen"
  echo "watching new PRs on $REPO (Ctrl-C to stop)..."
  while true; do
    gh pr list --repo "$REPO" --state all --limit 30 \
      --json number,title,headRefName,state \
      | jq -r '.[] | "\(.number)\t\(.state)\t\(.headRefName)\t\(.title)"' \
      | while IFS=$'\t' read -r num state branch title; do
          key="${num}-${state}"
          if ! grep -qx "$key" "$seen" 2>/dev/null; then
            echo "$(date '+%T')  #${num}  ${state}  ${branch}  —  ${title}"
            echo "$key" >> "$seen"
          fi
        done
    sleep 30
  done
}

cmd_dump() {
  [[ $# -ge 1 ]] || die "dump: need <tab-name> [--full]"
  local tab="$1"; shift || true
  local full_flag=""
  [[ "${1:-}" == "--full" ]] && full_flag="--full"
  require zellij
  local out="/tmp/orch-${tab}.txt"
  zellij action go-to-tab-name "$tab" || die "no such tab: $tab"
  sleep 0.2
  zellij action dump-screen $full_flag "$out"
  return_to_lead
  echo "dumped tab '$tab' -> $out ($(wc -l < "$out") lines)"
}

cmd_nudge() {
  [[ $# -ge 2 ]] || die "nudge: need <tab-name> <text...>"
  local tab="$1"; shift
  local text="$*"
  require zellij
  zellij action go-to-tab-name "$tab" || die "no such tab: $tab"
  sleep 0.2
  zellij action write-chars "$text"
  zellij action write 13
  return_to_lead
  echo "nudged tab '$tab'"
}

cmd_tabs() {
  require zellij
  zellij action query-tab-names
}

cmd_close() {
  [[ $# -ge 1 ]] || die "close: need <tab-name>"
  local tab="$1"
  require zellij
  zellij action go-to-tab-name "$tab" || die "no such tab: $tab"
  sleep 0.2
  zellij action close-tab
  return_to_lead
  echo "closed tab '$tab'"
}

cmd_comments() {
  # comments [interval] [issues...]   poll issue comments and emit one line per new one.
  # Default interval: 30s. Default issues: 5 6 7 8 (audit children).
  require gh jq
  local interval="${1:-30}"; shift || true
  local issues=("$@")
  if [[ ${#issues[@]} -eq 0 ]]; then
    issues=(5 6 7 8)
  fi
  local state="/tmp/orch-seen-comments.txt"
  : > "$state"
  # Prime state with currently existing comment ids so we don't re-emit history.
  for n in "${issues[@]}"; do
    gh api "/repos/$REPO/issues/$n/comments" --jq '.[].id' 2>/dev/null \
      | awk -v n="$n" '{print n"-"$1}' >> "$state" || true
  done
  echo "$(date '+%T') comments-watch armed: $REPO #${issues[*]} every ${interval}s (state: $state)"
  while true; do
    for n in "${issues[@]}"; do
      gh api "/repos/$REPO/issues/$n/comments" \
        --jq '.[] | "\(.id)\t\(.user.login)\t\(.created_at)\t" + ((.body // "") | gsub("[\r\n]+"; " | ") | .[0:160])' 2>/dev/null \
        | while IFS=$'\t' read -r cid user when body; do
            [[ -z "$cid" ]] && continue
            key="$n-$cid"
            if ! grep -qx -- "$key" "$state" 2>/dev/null; then
              printf '%s  #%s  @%s  %s  %s\n' "$(date '+%T')" "$n" "$user" "$when" "$body"
              echo "$key" >> "$state"
            fi
          done || true
    done
    sleep "$interval"
  done
}

cmd_help() {
  sed -n '2,30p' "$0"
}

main() {
  local sub="${1:-help}"; shift || true
  case "$sub" in
    spawn)    cmd_spawn    "$@" ;;
    status)   cmd_status   "$@" ;;
    watch)    cmd_watch    "$@" ;;
    prs)      cmd_prs      "$@" ;;
    comments) cmd_comments "$@" ;;
    dump)     cmd_dump     "$@" ;;
    nudge)    cmd_nudge    "$@" ;;
    tabs)     cmd_tabs     "$@" ;;
    close)    cmd_close    "$@" ;;
    help|-h|--help) cmd_help ;;
    *) die "unknown subcommand: $sub (try: help)" ;;
  esac
}

main "$@"
