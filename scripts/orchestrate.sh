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
  [[ $# -ge 3 ]] || die "spawn: need <tab-name> <branch> <prompt>"
  local name="$1" branch="$2" prompt="$3"
  require zellij claude
  zellij action new-tab --cwd "$REPO_DIR" --name "$name"
  sleep 0.5
  # Wrap prompt for safe shell-in-shell. Use printf %q on the prompt only.
  local q_prompt
  q_prompt=$(printf '%q' "$prompt")
  zellij action write-chars "claude --worktree ${branch} --permission-mode acceptEdits --name ${name} ${q_prompt}"
  zellij action write 13
  sleep 0.3
  return_to_lead
  echo "spawned tab '$name' on branch '$branch'"
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

cmd_help() {
  sed -n '2,30p' "$0"
}

main() {
  local sub="${1:-help}"; shift || true
  case "$sub" in
    spawn)  cmd_spawn  "$@" ;;
    status) cmd_status "$@" ;;
    watch)  cmd_watch  "$@" ;;
    prs)    cmd_prs    "$@" ;;
    dump)   cmd_dump   "$@" ;;
    nudge)  cmd_nudge  "$@" ;;
    tabs)   cmd_tabs   "$@" ;;
    close)  cmd_close  "$@" ;;
    help|-h|--help) cmd_help ;;
    *) die "unknown subcommand: $sub (try: help)" ;;
  esac
}

main "$@"
