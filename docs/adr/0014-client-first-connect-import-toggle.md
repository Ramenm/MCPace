# ADR 0014 — Client-first connect guide, MCP settings import, and server toggles

## Context

The repository has moved from a static proof repo toward a local MCPace broker/control plane. From a user perspective, the hard part is not only whether `/mcp` exists; it is knowing what to do next without reading source files or hand-editing JSON.

Current user jobs:

- discover the MCPace endpoint that clients should use;
- migrate existing MCP client configs into MCPace-owned fragments;
- add, test, temporarily disable, re-enable, or remove BYO upstream MCP servers;
- see whether runtime prerequisites block the next step;
- preview client config patches before writing them.

## Decision

Add and keep the following native surfaces:

- `mcpace connect [<client>] [--server <name>] [--json]` as a read-only, top-down wiring report;
- `mcpace server import --from <mcp-settings.json>` for migrating existing `mcpServers` blocks;
- `mcpace server enable <name>` and `mcpace server disable <name>` for pausing/resuming a BYO MCP entry without deleting JSON;
- next-step hints after `server add` and `server enable|disable` so the CLI points users toward `server test`, `verify readiness`, and client install/export previews.

## Constraints and non-goals

- Do not ship a hardcoded upstream MCP server catalog.
- Do not make `connect` mutate settings or client configs.
- Do not claim remote HTTP upstream forwarding is implemented; URL entries stay inventory-only until the connector exists.
- Keep destructive operations reversible or previewable through `--dry-run`.

## Consequences

Positive:

- A user can start from `mcpace connect` and receive an exact, project-specific command sequence.
- Existing client MCP configs can be migrated without manual JSON editing.
- Broken/noisy upstreams can be disabled without losing their config.
- The CLI becomes more native for BYO MCP operations.

Tradeoffs:

- More command surface means command coverage and docs must stay synchronized.
- Toggling is file-based and not a durable runtime session policy; active processes may still require a runtime restart or refresh to observe config changes, depending on caller path.
- Remote HTTP entries remain blocked for callable fan-out.

## Verification

- `cargo fmt --all -- --check`
- `node --test tests/node/command-coverage-contract.test.js tests/node/configurable-mcp-connectivity-contract.test.js tests/node/source-quality-contract.test.js`
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json`
- `node scripts/verify-npm-pack.mjs --json`

Full `cargo check`, `cargo test`, and release build still require dependency access to crates.io or a populated Cargo cache.
