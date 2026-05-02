# Modularization and hardcode cleanup pass — v0.5.5

## Goal

Reduce oversized runtime files without changing the product contract, and remove remaining misleading user-facing hardcoded wording where MCPace already resolves endpoints and MCP settings from configurable sources.

## What was inspected

- `src/dashboard.rs` and its HTTP/MCP child modules.
- `src/upstream.rs` and its existing child modules.
- `src/mcp_sources.rs` and MCP settings source-registry modules.
- Client-facing descriptions in `src/dashboard/http_tools.rs`, `src/mcp_server/tool_surface.rs`, `src/adapter.rs`, `src/hub/leases.rs`, and `src/upstream/inventory.rs`.
- Source audit, Node proof lane, npm thin-launcher package check, and Cargo availability.

## Changes

### Dashboard split

`src/dashboard.rs` is now below the source-audit large-module warning threshold. MCP HTTP specifics moved into focused modules:

- `src/dashboard/mcp_http.rs` — MCP POST/GET/DELETE route behavior, JSON-RPC dispatch, protocol headers, and tool-call response shaping.
- `src/dashboard/http_boundary.rs` — Origin and Accept checks.
- `src/dashboard/http_headers.rs` — standard MCP header/body agreement.
- `src/dashboard/http_session.rs` — visible-ASCII bounded session id normalization and generation.
- `src/dashboard/http_tools.rs` — MCPace HTTP tool definitions.
- `src/dashboard/tool_runtime.rs` — HTTP tool runtime execution and upstream lease context.
- Existing `overview.rs`, `diagnostics.rs`, `response.rs`, and `tests.rs` remain as separate boundaries.

### Upstream split checked

`src/upstream.rs` now delegates policy audit, policy suggestions, inventory/catalog/probe surfaces, source type inference, process config shaping, stdio runtime, server config loading, session pool behavior, tool-list cache, diagnostics, projection, lease/runtime mechanics, and tests into child modules. The root file is now below the large-module audit threshold.

### Hardcode wording cleanup

User-facing runtime descriptions now say “merged MCP settings registry” where the code actually reads multiple sources. The root `mcp_settings.json` filename remains as an intentional default/config file name, not as the only runtime source.

Updated wording in:

- `src/adapter.rs`
- `src/hub/leases.rs`
- `src/dashboard/http_tools.rs`
- `src/mcp_server/tool_surface.rs`
- `src/upstream/inventory.rs`

### Regression contract

Added a Node source-quality contract that keeps the dashboard HTTP boundary split visible and fails if `src/dashboard.rs` grows back past the split target.

## Verification

- `cargo fmt --all -- --check` — PASS.
- `npm test` — PASS after updating contracts to read extracted modules.
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json` — PASS, `ok: true`, `critical: []`.
- `node scripts/verify-npm-pack.mjs --json` — PASS for `@mcpace/cli@0.5.5` thin launcher.

## Blocked / not verified

- `cargo check --all-targets --locked` — blocked by crates.io DNS resolution while fetching `auto-launch`.
- `cargo check --all-targets --locked --offline` — blocked because `getrandom` is not present in the offline cache.
- Full Rust tests/build remain unverified in this sandbox.
- Real MCP client runtime trace remains unverified.
- Durable HTTP session store and remote Streamable HTTP upstream forwarding remain future P1 work.

## Remaining large modules

No production Rust module currently exceeds the source-audit large-module threshold. The most important remaining refactors should be behavior-driven, not line-count-driven: adapter discovery/projection boundaries, client install/update config writers, durable session storage, and remote HTTP upstream connector.
