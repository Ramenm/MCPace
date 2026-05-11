# Module boundary and hardcode sweep — v0.5.5

## Goal

Make MCPace easier to maintain before deeper runtime/session work: remove misleading hardcoded assumptions, split large runtime files into coherent Rust modules, and lock the improvement with source contracts.

## What was inspected

- `src/dashboard.rs` and all `src/dashboard/*` route/session/header/tool files.
- `src/upstream.rs` and all `src/upstream/*` runtime, policy, diagnostics, config, cache, and test files.
- `src/adapter.rs` and `src/adapter/discovery.rs`.
- `src/client/actions.rs` and `src/client/actions/*`.
- MCP source registry and server command family.
- User-facing endpoint/source text in README, docs, reports, and memory-bank.
- Source audit and Node contract tests.

## Changes completed

- Kept `src/dashboard.rs` as route/socket orchestration and kept MCP HTTP behavior in focused children:
  - `dashboard/mcp_http.rs`
  - `dashboard/http_boundary.rs`
  - `dashboard/http_headers.rs`
  - `dashboard/http_session.rs`
  - `dashboard/http_tools.rs`
  - `dashboard/tool_runtime.rs`
  - `dashboard/index.html`
- Kept `src/upstream.rs` as orchestration and moved focused runtime/config/policy behavior into child modules:
  - `upstream/inventory.rs`
  - `upstream/server_config.rs`
  - `upstream/stdio_runtime.rs`
  - `upstream/lease_runtime.rs`
  - `upstream/session_pool.rs`
  - `upstream/tool_cache.rs`
  - `upstream/policy_audit.rs`
  - `upstream/policy_suggestions.rs`
  - `upstream/diagnostics.rs`
  - `upstream/projection.rs`
  - `upstream/process_config.rs`
  - `upstream/source_type.rs`
- Split adapter discovery helpers into `src/adapter/discovery.rs`.
- Split client action render models into `src/client/actions/render_models.rs`.
- Added a source-quality regression contract asserting:
  - source audit currently reports zero production large-module warnings;
  - `src/adapter.rs`, `src/client/actions.rs`, `src/dashboard.rs`, and `src/upstream.rs` stay below 1500 lines;
  - key extracted boundaries remain present.
- Updated ADR 0011 and code inventory for the new module layout.

## Hardcode sweep result

Production endpoint defaults are centralized in `src/runtimepaths.rs`:

- `DEFAULT_LOCAL_HOST`
- `DEFAULT_LOCAL_MCP_PORT`
- `DEFAULT_LOCAL_MCP_PATH`
- `PUBLIC_MCP_RELAY_PLACEHOLDER_URL`

Remaining `127.0.0.1`, `39022`, and `/mcp` literals are either:

- intentional defaults in `runtimepaths.rs`;
- tests/fixtures asserting local transport behavior;
- documentation examples;
- client catalog path examples owned by specific client ecosystems.

Root `mcp_settings.json` remains an intentional default source, but runtime language now treats it as one member of the merged MCP settings registry rather than the only source.

## Current source-audit state

- `critical`: `[]`.
- `warnings`: `[]`.
- `largeModules`: `0`.
- `productionUnwraps`: `0`.

Largest production Rust files after the split are below the 1500-line source-audit warning threshold. Remaining refactors should be behavior-driven rather than line-count-driven.

## Verification

Confirmed in this sandbox:

- `cargo fmt --all -- --check` — PASS.
- `npm test` — PASS.
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json` — PASS, `ok: true`, `critical: []`, `warnings: []`.
- `node scripts/verify-npm-pack.mjs --json` — PASS for `@mcpace/cli@0.5.5` thin launcher.

Blocked / not confirmed:

- `cargo clippy --all-targets --locked -- -D warnings` — blocked by crates.io DNS resolution while fetching `auto-launch`.
- `cargo check --all-targets --locked` — blocked by the same dependency index access.
- `cargo test --all-targets --locked` and `cargo build --release --locked` are not confirmed.
- Real external-client runtime trace remains not confirmed.

## Follow-up

Do not continue splitting purely to reduce line counts. The next highest-value work is:

1. run Cargo compile/test/build on a host with dependency access;
2. capture one real client -> MCPace -> stdio upstream tool call trace;
3. then implement durable HTTP session storage and remote Streamable HTTP upstream forwarding.
