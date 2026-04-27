# Summary

## Package

- project: **mcpace**
- packaged version: **0.3.6**
- archive root pattern: **`<project-name>-v<version>-<ddmmyy-hhmmss>`**
- canonical archive builder: **`scripts/archive-release.mjs`**

## Current public promise

**One local MCPace endpoint, simpler install on selected local clients, and honest diagnostics for what is configured versus actually usable.**

For this cycle, treat `serve` as the product, `hub` as lifecycle machinery, and `dashboard` as an optional state view. Proof-focus surfaces and install-capable local surfaces are both resolved from `src/client_catalog.rs` metadata so new clients can be promoted without rewriting the summary contract.

## Latest update in this package

- client metadata fallback now merges `_meta` context hints across root / `params` / `payload` / `payload.params` instead of stopping at the first hint object
- Rust source now includes depth-4 combinatorial precedence coverage for `resolve_string` and depth-4 permutation coverage for `best_matching_root`
- Rust source now includes depth-4 metadata hint precedence/aggregation coverage for client metadata loading
- npm packaging now has a machine-checked tarball contract via `scripts/verify-npm-pack.mjs`, including `LICENSE`, launcher files, and staged vendored-binary inclusion when present
- vendored current-target bundles are now smoke-verified for version parity plus `verify doctor` / `verify readiness` JSON contracts, not just `version` / `help`
- release engineering now includes `scripts/generate-checksums.mjs`, `scripts/build-release-artifacts.mjs`, a hosted `release-artifacts` workflow scaffold, and a dynamic-version Ubuntu full-work proof script that no longer hardcodes `0.3.0`
- canonical source bundles now emit one cleaned `dist/` set with the ZIP, verification snapshot, `SHA256SUMS.txt`, and `release-artifacts.json`, while keeping `reports/verification-latest.json` aligned during fresh proof runs
- release/platform delivery now has a single target manifest, generated npm platform package scaffolds, generated GitHub Actions native matrices, checksum-gated npm publish, dry-run release rehearsal, draft GitHub Release workflow, and safe `update check` guidance with no silent self-update

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
npm run verify:npm-pack
npm run verify:release-targets
npm run verify:platform-packages
npm run verify:platform-packages:packed
npm run verify:publish-readiness
npm run prove:report
npm run pack:npm:dry-run
npm run archive:release
npm run build:release-artifacts
node scripts/stage-vendored-binary.mjs --json
npm run verify:vendored-binary
npm run generate:checksums -- --output-dir dist
cargo test
cargo build --release
```

## Current public claim view

- `supported`: 12 capabilities
- `supported-local-only`: 2 capabilities
- `control-plane-only`: 4 capabilities
- `bootstrap-only`: 1 capability
- `connectable-preview`: 1 capability
- `planned`: 4 capabilities

Those claim tiers come from `eval/runtime-capabilities.json` and intentionally stay narrower than the north-star runtime story.

## Current implemented native commands

`version`, `doctor`, `dashboard`, `serve`, `serve start/stop/status`, `init`, `hub up/down/repair/status/logs`, `profile show`, `projects list`, `candidates`, `client list`, `client plan`, `client install`, `client export`, `mcp-server`, `stdio-shim`, `lab list/matrix/coverage/gaps/report/show`, `server list/capabilities/candidates`, `verify doctor`, `verify readiness`, `repair`, `update check`.

## Current project-control artifacts

- `TODO.md` — prioritized backlog with points, dependencies, DoD, risks, and ETA ranges
- `STATE.md` — verified current state, progress range, blockers, and assumptions
- `DECISIONS.md` — project decisions, alternatives, consequences, and review triggers
- `reports/verification-latest.json` — latest machine-generated verification snapshot for the current environment
- `eval/scenario-matrix.json` / `eval/scoring-rubric.json` / `eval/dataset-plan.json` — machine-readable eval governance
