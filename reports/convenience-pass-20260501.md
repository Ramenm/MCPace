# Convenience pass — 2026-05-01

## Closed in v0.5.5

- Added `mcpace server sources` to inventory the exact MCP settings sources used by runtime routing.
- Added `mcpace server add` so users can add BYO MCP servers without hand-editing root JSON.
- Added default per-server fragments under `mcp_settings.d/*.json`.
- Added `mcpSettings.includeDirs` and `MCPACE_MCP_SETTINGS_DIRS` so whole directories can be included.
- Added `mcp_settings.d/README.md` and added the directory to `release-manifest.json`.
- Kept `src/server.rs` thin by moving implementation to `src/server/add.rs` and `src/server/sources.rs`.
- Made `scripts/run-node-test-files.mjs` compatible with Node versions that do not support `--test-force-exit`.

## What this improves

Users can now add upstream stdio MCP servers with a copyable command:

```bash
mcpace server add filesystem --command npx --arg @modelcontextprotocol/server-filesystem --arg .
mcpace server sources --json
```

The root packaged upstream catalog stays empty, but onboarding is easier and more native.

## Still not closed

- Remote Streamable HTTP upstream forwarding is not implemented; `--url` entries are registry/inventory-ready, not callable through the stdio bridge yet.
- Durable HTTP session storage is not implemented.
- Rust fmt/test/build were not executed in the sandbox because Cargo is unavailable.
