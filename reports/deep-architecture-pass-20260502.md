# Deep architecture pass — 2026-05-02 / v0.5.5

## Goal

Continue the source-architecture hardening work with concrete, reversible changes that make MCPace easier to extend as a native local MCP broker, without changing the external runtime contract before full Cargo check/test/build and real-client runtime traces are available.

## Changes completed

### Built-in client catalog split

`src/client_catalog.rs` no longer stores the built-in static target list inline.

- `src/client_catalog/builtin.rs` owns `CLIENT_TARGETS`.
- `src/client_catalog.rs` owns types, external registry parsing, selectors, and merge behavior.
- `scripts/lib/client-catalog.mjs` reads `src/client_catalog/builtin.rs` first and falls back to the old location only for compatibility.

### stdio MCP args split

`src/mcp_server.rs` no longer owns argv parsing/help text.

- `src/mcp_server/args.rs` owns `ParsedArgs`, `parse_args`, and `write_help`.
- `src/mcp_server.rs` remains focused on JSON-RPC/MCP lifecycle and command bridge behavior.

### Source-quality contracts expanded

`tests/node/source-quality-contract.test.js` now guards:

- built-in client catalog isolation;
- catalog parser script fallback to `src/client_catalog/builtin.rs`;
- stdio MCP argv parsing staying outside the JSON-RPC serving loop;
- root/split files staying under focused line-count targets.

## Verification performed

- `cargo fmt --all -- --check` — PASS.
- `node --test tests/node/source-quality-contract.test.js` — PASS.

Full project verification is recorded in `reports/summary.md` and `reports/verification-latest.json`. The final archive was produced with `node scripts/archive-release.mjs --json --output-dir dist` because the full proof-report runner timed out inside the sanitized child npm lane in this sandbox.

## Blockers / not verified

- `node scripts/proof-report.mjs --json --write` timed out inside the sanitized child npm lane in this sandbox; direct source and package checks passed.
- Full Rust check/test/build requires dependency access or a populated Cargo cache.
- The real external-client trace through `/mcp` and a stdio upstream tool remains not confirmed in this environment.

## Risk assessment

This pass avoided large behavioral rewrites. The main risk is Rust module visibility drift, which can only be fully confirmed by Cargo check/test/build. The added source-quality contracts reduce but do not replace that proof.
