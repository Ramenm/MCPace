# v0.5.9 preset simplification pass

## Goal

Make useful MCP setup simpler from a client/user perspective without turning MCPace into a Rust-hardcoded third-party package catalog.

## Changes

- Extended `src/mcp_presets.rs` so preset catalogs are merged from `mcpace.config.json` `mcpPresets.includePaths`, the default `presets/mcp-servers.json`, and `MCPACE_MCP_PRESETS`.
- Added catalog `sources` and `warnings` to preset JSON output.
- Added install-time `--arg` and `--env` passthrough for preset installs.
- Added `repository-flag` path mode for git repository presets.
- Expanded the packaged preset catalog to `filesystem`, `context7`, `git`, and `playwright` while keeping the default starter pack conservative (`filesystem` only).
- Moved preset rendering into `src/server/preset_render.rs` so `src/server/render.rs` stays focused on generic configured-server output.
- Updated `mcpace.config.json` and `schemas/mcpace-config.schema.json` with `mcpPresets.includePaths`.
- Updated connect/onboarding docs so the first path is `connect -> presets/starter -> server test -> client install/export`.

## Verification

- `cargo fmt --all -- --check` — PASS.
- `npm run lint:npm` — PASS.
- `node --test tests/node/configurable-mcp-connectivity-contract.test.js tests/node/source-quality-contract.test.js tests/node/command-coverage-contract.test.js` — PASS (`33/33`).
- `npm run test:npm` — PASS (`3/3` npm CLI test files).
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json` — PASS (`critical: []`, `warnings: []`, `largeModules: 0`, `productionUnwraps: 0`).
- `node scripts/verify-npm-pack.mjs --json` — PASS for `@mcpace/cli@0.5.9`.
- Full `npm run test:repo` reached the final test file before the sandbox timeout; the final three repo test files were then run directly and passed (`7/7`).

## Blocked / not verified

- `cargo check --all-targets --locked` is blocked by crates.io DNS/dependency access in this environment.
- Full `cargo test` and release build are still not confirmed here.
- Real-client runtime trace is still not confirmed.
- Remote Streamable HTTP upstream connector is still future work; the local HTTP session lifecycle is now implemented in-process, with cross-process/relay persistence still future hardening.
