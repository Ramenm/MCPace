# MCP settings fragments

`mcpace server add` writes one JSON fragment per upstream MCP server here by default.

Example:

```bash
mcpace server add filesystem --command npx --arg @modelcontextprotocol/server-filesystem --arg .
```

MCPace loads `mcp_settings.json`, then `mcp_settings.d/*.json`, then additional configured or environment-provided sources. Later duplicate server names override earlier entries.
