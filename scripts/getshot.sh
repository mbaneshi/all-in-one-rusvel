#!/usr/bin/env bash
# getshot.sh — pull a Mac screenshot to this Linux box so Claude can Read it.
#
# Usage:
#   getshot.sh                     newest screenshot from ~/Desktop or clipboard temp
#   getshot.sh <pattern>           newest file whose name contains <pattern>
#   getshot.sh /Users/bm/...png    pull a specific Mac-side absolute path
#   getshot.sh --list              list candidates without pulling
#
# Looks (on the Mac side):
#   ~/Desktop/Screenshot*.png            persistent (Cmd-Shift-3/4 default save)
#   /var/folders/*/T/TemporaryItems/NSIRD_screencaptureui_*/*.png
#                                         ephemeral (Cmd-Shift-4 -> clipboard / preview)
#
# Env:
#   GETSHOT_HOST    (default bm@m4)        Mac SSH target
#   GETSHOT_DIR     (default /tmp/shots)   Linux destination dir

set -euo pipefail

HOST="${GETSHOT_HOST:-bm@m4}"
DEST_DIR="${GETSHOT_DIR:-/tmp/shots}"
mkdir -p "$DEST_DIR"

# Run a bash script on the Mac with an optional pattern arg, using nullglob so
# unmatched globs vanish instead of erroring (zsh would otherwise raise nomatch).
remote_find() {
  local pat="${1:-}"
  ssh "$HOST" bash -s -- "$pat" <<'REMOTE_BASH'
shopt -s nullglob
pat="$1"
if [ -n "$pat" ]; then
  files=( ~/Desktop/*"$pat"*.png /var/folders/*/T/TemporaryItems/NSIRD_screencaptureui_*/*"$pat"*.png )
else
  files=( ~/Desktop/Screenshot*.png /var/folders/*/T/TemporaryItems/NSIRD_screencaptureui_*/*.png )
fi
[ ${#files[@]} -eq 0 ] && exit 1
# Sort by mtime desc and print all (newest first).
for f in "${files[@]}"; do
  stat -f '%m@@@%N' "$f" 2>/dev/null
done | sort -rn | sed 's/^[0-9]*@@@//'
REMOTE_BASH
}

arg="${1:-}"

case "$arg" in
  --list)
    remote_find "" | head -20
    exit 0
    ;;
  /*)
    remote_path="$arg"
    ;;
  "")
    remote_path=$(remote_find "" | head -1 || true)
    ;;
  *)
    remote_path=$(remote_find "$arg" | head -1 || true)
    ;;
esac

if [[ -z "${remote_path:-}" ]]; then
  echo "getshot.sh: no matching screenshot on $HOST" >&2
  exit 1
fi

# Build a Linux-safe basename.
fname=$(basename "$remote_path" | tr ' :' '__')
dest="$DEST_DIR/$fname"

# Stream the file content over ssh (handles spaces in the remote path).
ssh "$HOST" "cat $(printf '%q' "$remote_path")" > "$dest"

if [[ ! -s "$dest" ]]; then
  rm -f "$dest"
  echo "getshot.sh: empty file received for $remote_path" >&2
  exit 1
fi

echo "$dest"

size=$(stat -c '%s' "$dest")
echo "getshot.sh: pulled '$remote_path' -> $dest (${size} bytes)" >&2
