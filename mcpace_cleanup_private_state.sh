#!/usr/bin/env bash
set -Eeuo pipefail
ROOT="${1:-.}"
APPLY="${2:-}"
cd "$ROOT"
paths=(
  ".claude"
  ".codex"
  ".omc"
  "%SystemDrive%"
  "screenshot_test.png"
  "screenshot_test2.png"
)
if [[ "$APPLY" != "--apply" ]]; then
  echo "Dry run. Pass --apply as the second argument to delete."
  for p in "${paths[@]}"; do
    [[ -e "$p" ]] && echo "would remove: $p"
  done
  exit 0
fi
for p in "${paths[@]}"; do
  if [[ -e "$p" ]]; then
    rm -rf -- "$p"
    echo "removed: $p"
  fi
done
