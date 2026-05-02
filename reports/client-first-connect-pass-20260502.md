# Client-first connect pass — 2026-05-02

## Goal

Look at MCPace from the user's first-run perspective instead of from internal module boundaries. A user needs one native answer before editing client config:

```text
Which endpoint should my client use?
Which client target is supported?
Which upstream MCP servers are configured?
What blocks runtime readiness?
What exact command should I run next?
```

## Implemented

Added `mcpace connect` as a read-only top-down guide.

```bash
mcpace connect
mcpace connect codex
mcpace connect cursor-local --server filesystem --json
```

The command composes existing read paths instead of inventing a new source of truth:

- endpoint: `runtimepaths::resolve_serve_endpoint`;
- upstream settings sources: `mcp_sources::load_mcp_source_report`;
- effective server records: `server::load_server_records`;
- client targets: `client_catalog::load_registry`;
- readiness blockers: `verify::collect_readiness`.

It emits human-readable output or JSON with schema `mcpace.connectReport.v1`.

## Guardrails

`connect` is intentionally read-only. Contract coverage checks that it does not call MCP settings write/remove/toggle helpers and does not directly write files or create directories.

## Verified in this sandbox

- `cargo fmt --all -- --check` — PASS.
- `node --test tests/node/configurable-mcp-connectivity-contract.test.js` — PASS.
- `node --test tests/node/command-coverage-contract.test.js` — PASS.
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json` — PASS.

## Not verified

- `cargo check --all-targets --locked`, `cargo test`, and `cargo build --release` remain blocked by dependency access/cache in this environment.
- A real external client trace through `/mcp` and a real upstream stdio tool call is still not captured.
- Remote Streamable HTTP upstream forwarding remains inventory-only.
