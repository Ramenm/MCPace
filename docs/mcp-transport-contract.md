# MCP transport contract

MCPace treats transport correctness as a release boundary, not only as a runtime detail.

`npm run check:mcp-transport` runs `scripts/mcp-transport-contract.mjs` and verifies the source tree still preserves these invariants:

- `mcpace stdio` delegates to the live MCP JSON-RPC server.
- stdio reads one newline-delimited JSON-RPC message at a time.
- stdio writes compact JSON-RPC responses to stdout followed by exactly one newline frame.
- runtime diagnostics are routed through protocol-safe stderr helpers.
- upstream stdio forwarding uses newline-delimited JSON-RPC writes.
- Streamable HTTP POST requests require `Accept` coverage for `application/json` and `text/event-stream`.
- Streamable HTTP POST requests require `Content-Type: application/json`.
- normal HTTP MCP operations require the initialize/session/initialized lifecycle.
- local HTTP Host/Origin checks are centralized before route handlers.
- dashboard responses keep security headers centralized.

This checker is intentionally static because it must still run in source-review and sandbox environments where the native Rust binary cannot be built. It does not replace live MCP conformance tests. The release-ready host must still run the native binary and exercise stdio and Streamable HTTP end-to-end.

The checker produces a machine-readable report:

```bash
npm run check:mcp-transport
```

The expected JSON schema is `mcpace.mcpTransportContract.v1`.
