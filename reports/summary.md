# MCPace v0.6.9 packaging summary

## What changed in this pass

This pass focuses on internal MCP behavior rather than simple HTTP route availability.

- Tightened JSON-RPC/MCP envelope validation:
  - reject `id: null`;
  - reject decimal/exponential numeric request IDs;
  - keep string IDs and integer numeric IDs distinct;
  - reject array-style `params` and require MCP object-form params when present.
- Added request ID reuse tracking for stdio MCP sessions and Streamable HTTP MCP sessions.
- Added lifecycle readiness gating: after `initialize`, normal operations must wait for `notifications/initialized`; `ping` remains allowed.
- Added notification/request separation: `notifications/*` messages with an ID are rejected as invalid requests.
- Changed unknown `tools/call` names to JSON-RPC protocol errors instead of tool-result `isError` payloads.
- Extended the local logic harness with a protocol guard session covering lifecycle, IDs, notifications, and params edge cases.
- Updated HTTP lifecycle regression tests for initialized notification and duplicate request IDs.
- Fixed Streamable HTTP `tools/list` to reuse the negotiated session protocol when a subsequent request omits `MCP-Protocol-Version`, instead of falling back too early to the compatibility default.

## Important files

- `src/mcp_protocol.rs`
- `src/mcp_server.rs`
- `src/dashboard/http_session.rs`
- `src/dashboard/mcp_http.rs`
- `src/dashboard/http_tools.rs`
- `src/dashboard/tests.rs`
- `scripts/logic-test-local.mjs`
- `reports/mcp-internal-logic.md`

## Verification performed in this sandbox

Passed:

- `npm run check` — passed, 21/21 Node tests.
- `node --check scripts/logic-test-local.mjs` — passed.
- `npm run pack:npm:dry-run` — passed.

Could not be completed in this sandbox after the Rust source changes:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `cargo build --release`
- `npm run test:logic -- --json`
- `npm run load:local`

Reason: `cargo`, `rustc`, and `rustup` are not available in the current container, and `apt-get update` timed out. `npm run test:logic` and load testing require a built MCPace binary.

## Required final validation on Rust-capable machine

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
npm run test:logic -- --binary ./target/release/mcpace --json
npm run load:local -- --binary ./target/release/mcpace --duration-ms 5000 --concurrency 64
npm run pack:npm:dry-run
```

## Package hygiene

The ZIP is a source bundle with one root directory. It excludes `.git`, `node_modules`, caches, temporary files, OS artifacts, runtime logs/data/backups, vendored platform binaries, Rust `target`, and other heavyweight build outputs.
