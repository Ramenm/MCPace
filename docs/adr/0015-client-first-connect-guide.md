# ADR 0015: Client-first connect guide

## Context

MCPace already had separate commands for server management (`server add`, `server import`, `server sources`, `server test`, `server enable` / `server disable`, `server remove`), client install/export, readiness checks, and the local `/mcp` endpoint. From a user's perspective, that still required knowing which command to run first and how the pieces fit together.

The product should feel native for a user who asks: "What MCPace URL do I paste into my client, which client target is supported, which upstream servers are configured, and what command should I run next?"

## Decision

Add `mcpace connect` as a read-only, client-first wiring guide.

`mcpace connect [<client>] [--server <name>] [--json] [--root <path>]` resolves:

- the configured MCPace endpoint through `runtimepaths::resolve_serve_endpoint`;
- the selected client target through `client_catalog::load_registry`;
- upstream source inventory through `mcp_sources::load_mcp_source_report` and `server::load_server_records`;
- readiness blockers through `verify::collect_readiness`;
- exact next commands for import/add, server smoke, serve, client export/install preview, and readiness.

The command must not mutate MCP settings or client configuration. Contract tests assert that it does not call server write/remove/toggle helpers, `fs::write`, or `create_dir_all`.

## Consequences

Positive:

- Users get one top-down entrypoint before touching JSON or client configs.
- Existing command families stay focused; `connect` composes their read paths instead of duplicating mutation behavior.
- Support/debugging gets a stable JSON report schema: `mcpace.connectReport.v1`.

Tradeoffs:

- `connect` can only report the current runtime truth. It does not replace real client traces or full Rust build/test gates.
- The selected client heuristic is intentionally simple: prefer local, Streamable HTTP-capable, installable, tier-1 targets.
- Remote HTTP upstreams remain inventory-only until the remote upstream connector exists.

## Verification

- `cargo fmt --all -- --check`
- `node --test tests/node/configurable-mcp-connectivity-contract.test.js`
- `node --test tests/node/command-coverage-contract.test.js`
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json`

Full Cargo check/test/build and real-client runtime trace remain separate gates.
