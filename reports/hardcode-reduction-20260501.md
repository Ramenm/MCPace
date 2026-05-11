# Hardcode reduction pass — 2026-05-01

## Closed in v0.5.5

- Client install/export no longer has to advertise a compiled-in `http://127.0.0.1:39022/mcp` URL. It uses `runtimepaths::resolve_serve_endpoint` and supports `serve.publicUrl` / `MCPACE_PUBLIC_MCP_URL`.
- `mcpace serve start/status` resolves host/port defaults from project config/env before falling back to localhost `39022`.
- Upstream server loading uses `mcp_sources::load_mcp_server_registry` instead of reading only root `mcp_settings.json`.
- MCP settings can be extended with `mcpSettings.includePaths` and `MCPACE_MCP_SETTINGS`.
- HTTP `initialize` responses now advertise `Mcp-Session-Id` and `MCP-Protocol-Version` headers.
- HTTP upstream lease context now recognizes more client/chat/session/project-root headers.
- Added `tests/node/configurable-mcp-connectivity-contract.test.js` to pin these source contracts.

## Still intentionally hardcoded

- Default local endpoint remains `127.0.0.1:39022/mcp` as a compatibility default.
- `/mcp` and `/healthz` route names are still product defaults; `mcpPath` affects advertised endpoint and serve status, but full custom HTTP routing needs Rust/runtime validation before broadening.
- Built-in client target catalog remains compiled into Rust, though external catalog paths already exist.
- Remote HTTP upstream calling is not implemented as a production lane.

## Verification

Passed in this sandbox:

```bash
npm test
```

Not confirmed in this sandbox:

```bash
cargo fmt --all -- --check
cargo test --all-targets --locked
npm run verify:rust-quality
real-client runtime trace
```


## Additional finding: router path alignment

The first endpoint pass moved client-facing URLs into config/env, but the router still matched only `/mcp`. This pass adds configured `serve.mcpPath` acceptance in unified serve and updates setup probes to use the resolved path and Streamable HTTP Accept header.
