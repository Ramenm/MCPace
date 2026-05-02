# MCPace runtime trace harness

Project: `mcpace` v`0.5.9`
Status: `ready-to-run`

## Steps

| step | status | evidence |
|---|---:|---|
| binary | pass | C:\Users\rmatv\Projects\mcpace\target\release\mcpace.exe |
| tiny-upstream-fixture | pass | tiny stdio MCP fixture for deterministic upstream_tools/upstream_call proof |
| serve-endpoint | manual | http://127.0.0.1:39022/mcp |
| initialize | manual | POST JSON-RPC initialize with Accept: application/json, text/event-stream |
| tools-list | manual | POST JSON-RPC tools/list through MCPace |
| upstream-call | manual | POST JSON-RPC tools/call -> upstream_call against tiny stdio server |

## Next commands

- `cargo build --release --locked`
- `node scripts/runtime-trace-harness.mjs --json --write reports/runtime-trace-latest.json --markdown reports/runtime-trace-latest.md`
- `./target/release/mcpace serve --port 39022`
- `Use MCP Inspector or a real client to run initialize -> tools/list -> tools/call -> upstream_call.`
