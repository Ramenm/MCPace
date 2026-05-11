# Native preset install pass — 2026-05-02 / v0.5.6

## Goal

Reduce first-run friction for useful MCP server setup without adding a hardcoded
third-party package catalog to Rust source.

## Changes

- Added `presets/mcp-servers.json` and `presets/README.md`.
- Added `src/mcp_presets.rs` as a data-driven preset loader/installer.
- Added native server commands:
  - `mcpace server presets`
  - `mcpace server install <preset>`
  - `mcpace server starter`
- The starter pack installs only the `filesystem` preset by default, with explicit
  allowed paths.
- `playwright` is available as opt-in but not in the default starter pack.
- Updated release manifest, docs, command coverage, and source contracts.

## Verification

Run locally:

```bash
cargo fmt --all -- --check
node --test tests/node/configurable-mcp-connectivity-contract.test.js
node --test tests/node/command-coverage-contract.test.js
node scripts/audit-source.mjs --json
node scripts/verify-npm-pack.mjs --json
```

Full Cargo check/test/build still require dependency access.
