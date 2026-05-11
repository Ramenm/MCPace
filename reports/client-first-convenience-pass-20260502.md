# Client-first convenience pass — 2026-05-02 / v0.5.5

## Goal

Review MCPace from the perspective of a user who wants to connect a client and upstream MCP server without reading source files or manually editing JSON.

## What changed

- Added/kept `mcpace connect` as a read-only top-down wiring guide.
- Added/kept `mcpace server import --from <mcp-settings.json>` for migrating existing MCP client configs into MCPace fragments.
- Added `mcpace server enable <name>` and `mcpace server disable <name>` to pause/resume an upstream MCP entry without deleting it.
- Added next-step CLI hints after server add/toggle operations.
- Updated command coverage, docs, source inventory, and memory-bank state.

## Why this is the right layer

The current product gap is not only protocol plumbing. A user needs a native sequence:

```text
connect guide → import/add → sources → test → serve → client export/install preview
```

This pass improves that sequence while leaving the still-blocked remote HTTP upstream connector honest; the HTTP session store is now implemented in-process, while cross-process/relay persistence remains future hardening.

## Verification

Confirmed in this environment:

```bash
cargo fmt --all -- --check
node --test tests/node/command-coverage-contract.test.js tests/node/configurable-mcp-connectivity-contract.test.js tests/node/source-quality-contract.test.js
node scripts/audit-source.mjs --json --write reports/source-audit-latest.json
```

Partially confirmed / blocked:

```text
cargo check --all-targets --locked: blocked by crates.io DNS/dependency access.
full sequential Node repo run: progressed through many files, but rust-quality-contract hit the environment's long-running process limit in this sandbox; targeted changed tests pass.
```

## Still not done

- Full cargo check/test/build.
- Real client → MCPace `/mcp` → upstream stdio tool runtime trace.
- Durable HTTP session store and strict `Mcp-Session-Id` lifecycle.
- Remote Streamable HTTP upstream forwarding with auth, SSRF controls, retry/backoff, and SSE handling.
