# MCPace v0.5.5 readiness and native-UX pass — 2026-05-01

## Scope

This pass rechecked the current source tree after the BYO MCP convenience work and focused on areas that had not been changed as much in earlier passes: upstream runtime type normalization, server lifecycle UX, config schema coverage, and proof-lane readiness.

## Confirmed issue fixed

`src/upstream.rs` used `infer_source_type(...)` while the helper was not defined in that module. That was a Rust compile-risk drift not caught by Node-only contract tests. The helper now exists in `upstream.rs` and normalizes HTTP aliases (`streamable-http`, `streamable_http`, `remote-http`, `remote-sse`, `sse`, `url`) to the runtime diagnostic class `http`.

Impact: HTTP/Streamable HTTP registry entries are now consistently inventoried as blocked HTTP upstreams until the remote HTTP connector is implemented, instead of being able to fall through to a generic missing-command diagnostic.

## Native UX added

`mcpace server remove <name> [--settings <path>] [--dry-run] [--json]` was added. It deletes a BYO MCP server entry from the source where the registry found it, or from an explicit `--settings` file. This completes the minimal native add/list/source/remove loop:

```bash
mcpace server add filesystem --command npx --arg @modelcontextprotocol/server-filesystem --arg .
mcpace server sources --json
mcpace server remove filesystem --dry-run
```

## Guardrail added

`mcpace server add --url ...` now accepts only `http://` and `https://` endpoint strings and rejects whitespace/control characters. Remote HTTP forwarding is still not implemented, but this prevents arbitrary URI schemes from becoming part of the future remote connector contract.

## Schema coverage added

`schemas/mcpace-config.schema.json` now captures the current project-level config fields that matter to runtime routing and BYO MCP source discovery: `ports`, `serve`, `mcpSettings`, `servers`, and `clientCatalog` extension shape.

## Verification performed in this pass

- `cargo fmt --all -- --check`: PASS after formatting.
- `npm test`: PASS.
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json`: PASS.

## Still blocked / not proven

- `cargo check --all-targets --locked`: blocked in this sandbox by crates.io DNS resolution, not by a confirmed Rust compiler diagnostic.
- `cargo test --all-targets --locked`: not executed for the same dependency/network reason.
- Real MCP client runtime trace remains required before beta/runtime readiness claims.
