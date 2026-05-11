#!/usr/bin/env bash
set -Eeuo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
profile="${1:-${MCPACE_LINUX_CHECK_PROFILE:-standard}}"
cd "$repo_root"
exec node scripts/linux-auto-check.mjs \
  --profile "$profile" \
  --json \
  --write reports/linux-auto-check-latest.json \
  --markdown reports/linux-auto-check-latest.md
