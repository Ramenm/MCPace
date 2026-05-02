# Module split and upstream smoke pass — v0.5.5

## Goal

Make MCPace easier to maintain and easier to use for BYO MCP onboarding without changing the runtime contract in a way that cannot be verified in this sandbox.

## What was inspected

- Large Rust roots from source-audit warnings.
- Client-facing endpoint text in `src/client/actions.rs`.
- Server command family under `src/server/`.
- Upstream probing path under `src/upstream.rs`.
- Source audit classification of extracted Rust tests.
- README/docs/memory-bank/report sync.

## Changes

### Safe module split

- Split `dashboard` HTTP helpers into child modules for boundary, headers, sessions, tool handling, runtime commands, overview, diagnostics, and response writing.
- Split MCP stdio tool-surface construction into `src/mcp_server/tool_surface.rs`.
- Moved extracted test modules to `src/*/tests.rs` and taught source audit to treat those as tests.
- Kept remaining big modules intact where a compile-only refactor would be riskier than useful without Cargo dependency access.

### Native upstream smoke

Added:

```bash
mcpace server test [<name>|--name <server>] [--timeout-ms <ms>] [--refresh] [--json]
```

The command calls `upstream::probe_servers`, so it uses the same registry and stdio probe path as runtime diagnostics. It is meant for this workflow:

```bash
mcpace server add my-server --command node --arg ./server.js
mcpace server test my-server --refresh --json
mcpace client install --client codex
```

### Hardcode cleanup

- User-facing client export/install guidance now uses resolved endpoint URLs from `runtimepaths`.
- Default endpoint literals remain in defaults, fixtures, tests, and docs examples only.

## Verification

- `cargo fmt --all -- --check` — PASS.
- `npm test` — PASS.
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json` — PASS.

## Known blockers

- `cargo check --all-targets --locked` cannot complete in this sandbox because crates.io DNS resolution fails while resolving `auto-launch`.
- Real runtime proof needs a host with a built binary and one real stdio MCP upstream.

## Next move

On a Rust host with dependency access, run:

```bash
cargo check --all-targets --locked
cargo test --all-targets --locked
cargo build --release --locked
```

Then record a real-client trace through `/mcp` and one stdio upstream. Future refactors should be behavior-driven rather than line-count-driven.
