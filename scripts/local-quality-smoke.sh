#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
node scripts/local-quality-suite.mjs \
  --profile smoke \
  --json \
  --write reports/local-quality-smoke-latest.json \
  --markdown reports/local-quality-smoke-latest.md
