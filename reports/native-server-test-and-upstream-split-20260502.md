# Native server smoke and upstream split pass — 2026-05-02

## Goal

Make BYO MCP onboarding more native and reduce large-file debt without changing the external runtime contract.

## Changes

- Added `mcpace server test [<name>|--name <server>] [--timeout-ms <ms>] [--refresh] [--json]`.
  - It uses the same upstream probe path as runtime diagnostics.
  - It lets users verify a configured stdio MCP server reaches initialize/tools-list before installing or exporting a client configuration.
- Split MCP settings write/remove logic out of `src/mcp_sources.rs` into `src/mcp_sources/write.rs`.
- Split upstream policy suggestion logic into `src/upstream/policy_suggestions.rs`.
- Split upstream tool-policy audit/classification logic into `src/upstream/policy_audit.rs`.
- Kept `src/upstream.rs` as the orchestration root; callable runtime behavior is unchanged.
- Centralized the public relay placeholder through `runtimepaths::PUBLIC_MCP_RELAY_PLACEHOLDER_URL` / `public_mcp_url_or_placeholder` so client export guidance does not carry duplicate URL literals in production code.

## Verification

Confirmed in this sandbox:

- `cargo fmt --all -- --check` — PASS.
- `npm test` — PASS.
- `node --test tests/node/configurable-mcp-connectivity-contract.test.js` — PASS.
- `node scripts/audit-source.mjs --json --fail-on-critical` — PASS, `critical: []`.

Blocked / not confirmed:

- `cargo check --all-targets --locked --offline` — BLOCKED because `getrandom` is not in the local Cargo cache.
- `cargo check --all-targets --locked` — BLOCKED because the sandbox cannot resolve `index.crates.io`; the failing dependency fetch was `auto-launch`.
- Full Rust tests/build and a real MCP client → MCPace → upstream stdio runtime trace remain NOT CONFIRMED.

## Current large-module state

After this pass, source audit warnings are narrowed to three Rust roots:

- `src/upstream.rs` — still large but reduced to roughly 3.2k lines; more extraction remains possible.
- `src/adapter.rs` — still large; should be split after behavior is frozen.
- `src/client/actions.rs` — still large; should be split after install/export behavior is frozen.

## Why this is the right next step

The new `server test` command closes the biggest UX gap after `server add/remove/sources`: a user can now verify one configured upstream before wiring a real client. The module split follows Rust's normal file-module pattern and is intentionally cohesive: source registry writes, policy suggestions, and policy auditing are now separate from the upstream orchestration root.
