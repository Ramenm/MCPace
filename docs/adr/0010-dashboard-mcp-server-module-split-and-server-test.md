# ADR 0010 — Dashboard/MCP-server module split and native upstream smoke command

## Context

`src/dashboard.rs`, `src/upstream.rs`, `src/adapter.rs`, `src/client/actions.rs`, and `src/mcp_server.rs` were the largest Rust files in the tree. Large root modules made review harder and hid whether code growth was production logic or tests. At the same time, BYO MCP onboarding had `server add`, `server remove`, and `server sources`, but no native command to smoke-test one configured upstream before wiring a client.

## Decision

Split low-risk cohesive child modules first, without changing the public runtime contract:

- `src/dashboard.rs` remains the route/orchestration root.
- Dashboard HTTP boundary/header/session/tool/runtime/overview/diagnostic/response helpers live under `src/dashboard/` child modules.
- `src/mcp_server.rs` keeps the stdio protocol root while tool-surface construction lives in `src/mcp_server/tool_surface.rs`.
- Rust test modules for dashboard/upstream/adapter/mcp_server live in child `tests.rs` files.
- `scripts/audit-source.mjs` treats `src/**/tests.rs` as test code so extracted tests do not count as production Rust debt.
- Add `mcpace server test [<name>|--name <server>] [--timeout-ms <ms>] [--refresh] [--json]` as a native tools/list smoke path over the same upstream probe implementation used by runtime diagnostics.

## Consequences

- Source audit large-module warnings were reduced in the first split and are now zero after the follow-up extraction of upstream, adapter discovery, and client action helper modules.
- The next refactor should be behavior-driven rather than line-count-driven: adapter discovery/projection, client install/update writers, durable session storage, and remote HTTP upstream forwarding remain the main boundaries.
- Users can now add a BYO stdio MCP server, inspect sources, and test the upstream tools/list path before connecting an external client.
- This pass intentionally does not implement remote HTTP upstream forwarding or durable HTTP sessions; those remain separate runtime features.

## Verification

- `cargo fmt --all -- --check`
- `npm test`
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json`

Full Rust check/test/build still require crates.io dependency access or a populated Cargo cache.

## 2026-05-02 follow-up split

The second split pass moved additional cohesive upstream helpers out of `src/upstream.rs`:

- `src/upstream/policy_audit.rs` owns tool annotation/name-signal policy audit classification.
- `src/upstream/policy_suggestions.rs` owns dry-run policy suggestion report construction.
- `src/upstream/process_config.rs` owns child process template/path helpers.
- `src/upstream/source_type.rs` owns stdio/http source-type normalization.
- `src/upstream/inventory.rs` owns inventory/catalog/probe/audit public report surfaces.

This keeps `src/upstream.rs` as the runtime orchestration root while reducing review surface. The follow-up pass also extracted lease runtime, session pool, server config, stdio runtime, tool cache, adapter discovery, and client action helper modules. Full Cargo and real-client runtime proof are still required before larger behavioral rewrites.
