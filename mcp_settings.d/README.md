# MCP settings fragments

`mcpace install` writes one JSON fragment per upstream MCP server here by default.

Example:

```bash
mcpace install @modelcontextprotocol/server-filesystem --as filesystem --path .
```

MCPace loads `mcp_settings.json`, then `mcp_settings.d/*.json`, then additional configured or environment-provided sources. Later duplicate server names override earlier entries.
