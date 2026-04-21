# Summary

## Package

- project: **mcpace**
- packaged version: **0.3.0**
- archive root pattern: **`<project-name>-v<version>-<ddmmyy-hhmmss>`**
- canonical archive builder: **`scripts/archive-release.mjs`**

## What is included

- Rust CLI source under `src/`
- npm launcher under `packages/npm/cli`
- clean release/archive tooling under `scripts/`
- configs and schemas needed for local validation
- examples and runtime evaluation fixtures
- integration and contract tests under `tests/`
- focused docs for setup, verification, architecture, recovery, and release
- root project-control docs (`TODO.md`, `STATE.md`, `DECISIONS.md`)
- prompt/agent eval governance files under `eval/`
- session persistence/context files under `memory-bank/`

## What is intentionally excluded

- `.git`
- `node_modules`
- `target`
- caches and temporary files
- OS/system junk
- old patch artifacts and extra packaging byproducts

## Quick check

```bash
npm test
npm run prove:report
npm run pack:npm:dry-run
npm run archive:release
cargo test
cargo build --release
```

## Current implemented native commands

`version`, `doctor`, `init`, `hub up/down/repair/status/logs`, `profile show`, `projects list`, `candidates`, `client list`, `client plan`, `lab list/matrix/coverage/gaps/report/show`, `server list/capabilities/candidates`, `verify doctor`, `verify readiness`, `repair`.

## Current project-control artifacts

- `TODO.md` — prioritized backlog with points, dependencies, DoD, risks, and ETA ranges
- `STATE.md` — verified current state, progress range, blockers, and assumptions
- `DECISIONS.md` — project decisions, alternatives, consequences, and review triggers
- `reports/verification-latest.json` — latest machine-generated verification snapshot for the current environment
- `eval/scenario-matrix.json` / `eval/scoring-rubric.json` / `eval/dataset-plan.json` — machine-readable eval governance
