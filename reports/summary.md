# MCPace v0.6.9 source-bundle summary

## What changed in the consolidation passes

This source bundle keeps MCPace source-only and removes stale release/CI assumptions that were causing false failures, especially around Windows and packaging.

- Added the missing npm launcher shim at `packages/npm/cli/bin/mcpace.js`.
- Added `reports/load-test.md` as an explicit placeholder instead of referencing a missing report.
- Kept Windows process launching explicit: npm/npx are invoked as `.cmd` wrappers where Node-side tooling needs to spawn them on Windows.
- Replaced the release ZIP builder's external `zip`/`unzip` dependency with a Node-only ZIP writer/reader under `scripts/lib/zip-writer.mjs`.
- Added a small process helper under `scripts/lib/process.mjs` so npm/npx command resolution is not duplicated.
- Removed stale npm script references and consolidated GitHub workflows to commands that actually exist in this source bundle.
- Centralized repeated Rust helpers for runtime paths, environment parsing, CLI text formatting, platform aliases, notification-method detection, and Windows command-line quoting.
- Hardened the local `auto-launch` compatibility crate's Windows argument quoting to preserve backslashes before quotes/trailing boundaries.
- Normalized repository text line endings to LF to match `.editorconfig` and `.gitattributes`.
- Aligned GitHub labels across `.github/labels.yml`, Dependabot, issue templates, and release-note categories so automation uses declared labels only.
- Removed stale generated-code comments that pointed at missing local Node scripts.
- Fixed hub example/schema drift: bundled `examples/mcpace-hub.*.json` files now match the schema's intended safe-by-default empty manual profile behavior.
- Removed the retired `manager.settings.json` bridge from the source bundle; runtime profile selection is now config-first with `MCPACE_RUNTIME_PROFILE` as the only override.
- Removed unused hub `legacyManagerBridge` / `legacyScriptAliases` flags from bundled examples and schema.
- Removed the opt-in projected-tool top-level control bridge; projected controls now use `_mcpace` / `mcpace` objects only.
- Hardened `npm run load:local` binary discovery so it honors `MCPACE_BINARY_PATH` / `MCPACE_DEV_BINARY` and reports missing Rust binaries clearly.
- Guarded the npm launcher against accidentally running a consuming project's unrelated `target/release/mcpace` or `dist/mcpace` binary.
- Reused the centralized serve resource-argument helper in the public `serve start` path.
- Reused shared helpers for UNIX millisecond timestamps, nullable JSON strings/numbers, empty JSON objects, sorted/deduplicated string lists, and Windows extended-path prefix stripping.
- Added static guards so these cross-cutting Rust helper implementations stay centralized instead of silently drifting across runtime, dashboard, serve, client, and upstream code.

## Important files

- `scripts/build-release-artifacts.mjs`
- `scripts/lib/zip-writer.mjs`
- `scripts/lib/process.mjs`
- `scripts/check-node-syntax.mjs`
- `packages/npm/cli/bin/mcpace.js`
- `.github/workflows/ci.yml`
- `.github/workflows/release-dry-run.yml`
- `schemas/mcpace-hub.schema.json`
- `tests/node/project-hygiene.test.mjs`
- `tests/node/docs-and-package.test.mjs`

## Verification performed in this sandbox

Passed:

- `npm run check`
- `npm run pack:npm:dry-run`
- `npm run release:dry-run`
- `node scripts/build-release-artifacts.mjs --json --out-dir <tmp> --timestamp 230526-194200`
- `npm install --ignore-scripts`
- `node scripts/load-test-local.mjs --help`

Could not be completed in this sandbox:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `cargo build --release`
- `npm run load:local`

Reason: `cargo`, `rustc`, and `rustup` are not available in the current container. Runtime load testing requires a built MCPace binary. In this pass the load-test script itself was still exercised via `--help` and a missing-binary failure path.

## Required final validation on a Rust-capable machine

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
npm run check
npm run pack:npm:dry-run
npm run build:release-artifacts
npm run load:local -- --binary ./target/release/mcpace --duration-ms 5000 --concurrency 64
```

## Package hygiene

The ZIP is a source bundle with one root directory. It excludes `.git`, `node_modules`, caches, temporary files, OS artifacts, runtime logs/data/backups, vendored platform binaries, Rust `target`, and other heavyweight build outputs.
- Fixed the release ZIP writer so Unix executable bits for npm bin shims survive extraction, and added a regression check for `packages/npm/cli/bin/mcpace.js`.

