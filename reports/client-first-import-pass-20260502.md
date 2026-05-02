# Client-first import pass — 2026-05-02 / v0.5.5

## Goal

Improve the product from the viewpoint of a user who already has MCP servers configured somewhere else and wants MCPace to become the one local broker without hand-editing JSON.

## Changes

- Added `mcpace server import --from <mcp-settings.json>`.
- Added `src/mcp_sources/import.rs` to preserve existing `mcpServers` entries while writing MCPace-managed fragments.
- Added `src/server/import.rs` and native help/rendering for import results.
- Updated `mcpace connect` next steps to recommend import when no upstreams are configured.
- Split `mcpace client list` into `src/client/actions/list.rs`, keeping read-only list rendering away from install/export mutation paths.
- Split client install backup/restore helpers into `src/client/actions/backup.rs` so backup support no longer expands the client action dispatcher root.
- Added contract tests for the import flow and the client list boundary.

## Verification performed in this environment

- `cargo fmt --all -- --check` — PASS.
- `npm run lint:npm` — PASS.
- `node --test tests/node/configurable-mcp-connectivity-contract.test.js` — PASS, 16/16.
- `node --test tests/node/source-quality-contract.test.js` — PASS, 13/13.
- `cargo check --all-targets --locked --offline` — BLOCKED by missing cached `getrandom` crate/dependency index, not by a confirmed code error.

## Remaining runtime blockers

- Full Cargo check/test/build with dependency access.
- Real client-to-MCPace-to-upstream trace.
- Durable Streamable HTTP session store.
- Remote Streamable HTTP upstream connector.
