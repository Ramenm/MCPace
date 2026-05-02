# ADR 0014 — client-first MCP import and client action boundary

## Status

Accepted for v0.5.5.

## Context

MCPace already supports BYO upstream MCP settings from `mcp_settings.json`, `mcp_settings.d/*.json`, include paths/directories, and environment-provided sources. From a client user's perspective, the remaining friction is that an existing MCP configuration from another client or project still had to be copied by hand into MCPace-managed fragments.

The client action root also still mixed read-only catalog listing with install/export/restore mutation paths. The file was below the source-audit large-module threshold, but the responsibilities were separable.

## Decision

Add a native server import command:

```bash
mcpace server import --from <mcp-settings.json> [--settings <target.json>] [--dry-run] [--force] [--json]
```

The import path reads an existing JSON object with `mcpServers`, preserves server entries as provided, and writes MCPace-managed fragments under `mcp_settings.d/` by default. When `--settings` is supplied, multiple imported entries can target one explicit settings file. Existing normalized server names require `--force`; `--dry-run` reports the plan without writing.

Also extract `mcpace client list` rendering into `src/client/actions/list.rs` so the client action root focuses on plan/export/install/restore orchestration.

## Consequences

- Users can migrate existing MCP configs without hand-editing JSON.
- MCPace keeps the BYO model and does not add a hardcoded upstream catalog.
- Importing remote HTTP MCP entries remains an inventory operation until the remote Streamable HTTP upstream connector is implemented.
- The import path is intentionally conservative: it requires a top-level `mcpServers` object and rejects replacement conflicts unless `--force` is explicit.

## Verification

- `cargo fmt --all -- --check`
- `node --test tests/node/configurable-mcp-connectivity-contract.test.js`
- `node --test tests/node/source-quality-contract.test.js`
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json`
