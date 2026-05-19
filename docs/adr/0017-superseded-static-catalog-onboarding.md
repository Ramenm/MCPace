# ADR 0017 — Superseded by automatic MCP install

Status: superseded.

This decision has been replaced by the automatic package/URL/command install flow. MCPace no longer depends on a packaged upstream-server catalog for useful MCP onboarding. Users provide an npm, PyPI, OCI, URL, or command spec; MCPace writes a reviewable `mcp_settings.d/*.json` fragment and the adaptive profiler classifies the server from source evidence plus later live probes.

Current commands:

```bash
mcpace server install npm:@modelcontextprotocol/server-filesystem --as filesystem --path . --dry-run
mcpace server install pypi:mcp-server-time --as time --dry-run
mcpace server install --url https://example.com/mcp --as remote-docs --dry-run
mcpace server install custom --command node --arg ./server.js --dry-run
```
