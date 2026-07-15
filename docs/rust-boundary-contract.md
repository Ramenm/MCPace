# Rust boundary contract

`npm run check:rust-boundaries` is a static release gate for Rust subsystem boundaries that can be checked before a native Rust toolchain is available.

It does **not** replace `cargo check`, `cargo test`, `cargo fmt`, or `cargo clippy`. It catches regressions that previously made the project look cleaner than it was:

- restored `Result<_, String>` signatures in migrated subsystem files;
- missing typed error traits on migrated modules;
- new low-level HTTP/TCP ownership outside the known dashboard/server boundary;
- accidental drift away from newline-framed upstream stdio JSON-RPC writes;
- loss of the MCP source symlink rejection boundary.

## Current locked contract

The current typed-boundary modules are:

| Module | Error seam |
|---|---|
| `src/init.rs` | `InitError` |
| `src/projects.rs` | `ProjectRegistryError` |
| `src/profile.rs` | `RuntimeProfileError` |
| `src/mcp_sources.rs` | `McpSourceError` |
| `src/hub/runtime.rs` | `HubRuntimeError` |
| `src/upstream/tool_cache.rs` | `ToolListCacheError` |
| `src/upstream/stdio_runtime.rs` | `StdioRuntimeError` |
| `src/server/policy.rs` | `ServerPolicyError` |
| `src/upstream/inventory.rs` | `UpstreamInventoryError` |
| `src/upstream/session_pool.rs` | `UpstreamSessionPoolError` |

The current `stringly-errors` budget is 16. Any increase is a regression unless a deliberate migration note and budget change accompany it.

The current raw HTTP/TCP allowlist is:

```text
src/dashboard.rs
src/dashboard/mcp_http.rs
src/dashboard/response.rs
src/http_probe.rs
```

Outbound HTTP should keep moving toward a higher-level client after the Rust lockfile can be refreshed on a Rust-enabled host. Dashboard/server-side low-level TCP remains allowed only under the explicit MCP and dashboard security contracts.

## Commands

```bash
npm run check:rust-boundaries
npm run check:ci
npm run check:endgame
```

`check:ci` and `check:endgame` both include this contract. Release workflows should keep the stricter Rust live proof gates enabled as well.
