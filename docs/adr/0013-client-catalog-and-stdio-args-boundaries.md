# ADR 0013 — Client catalog and stdio-args boundaries

## Context

The v0.5.x modularization pass removed the largest source-audit warnings, but two root modules still mixed independent responsibilities:

- `src/client_catalog.rs` stored both the static built-in client target defaults and the runtime registry loading/merge behavior.
- `src/mcp_server.rs` contained both JSON-RPC/MCP serving behavior and process argv parsing/help text.
- `scripts/lib/client-catalog.mjs` assumed the built-in catalog lived directly in `src/client_catalog.rs`, which would drift as soon as defaults moved to a focused module.

## Decision

Split without changing the public CLI/MCP contract:

- `src/client_catalog/builtin.rs` owns static `CLIENT_TARGETS` defaults.
- `src/client_catalog.rs` owns catalog types, external registry loading, selector resolution, and merge behavior.
- `scripts/lib/client-catalog.mjs` reads `src/client_catalog/builtin.rs` first and falls back to the old root location for transition tolerance.
- `src/mcp_server/args.rs` owns `ParsedArgs`, `parse_args`, and `write_help`.
- `src/mcp_server.rs` remains focused on JSON-RPC/MCP lifecycle and command bridge behavior.

## Non-goals

- No change to MCP tool behavior.
- No durable HTTP session store.
- No remote Streamable HTTP upstream forwarding.
- No client install behavior rewrite in this pass.

## Consequences

Positive:

- Static client defaults are no longer mixed with external registry parsing and merge logic.
- stdio process argument parsing no longer lives in the JSON-RPC serving loop.
- Source tooling is resilient to the new built-in catalog location.
- New Node source-quality contracts guard the split.

Trade-offs:

- There are more modules to navigate.
- Full Cargo check/test/build is still required to confirm all Rust visibility and compile behavior.

## Verification

Confirmed in this pass:

```bash
cargo fmt --all -- --check
node --test tests/node/source-quality-contract.test.js
```

Then the full available source lane should run:

```bash
npm test
node scripts/audit-source.mjs --json --write reports/source-audit-latest.json
node scripts/verify-npm-pack.mjs --json
node scripts/proof-report.mjs --json --write
node scripts/build-release-artifacts.mjs --json
```
