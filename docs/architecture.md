# Architecture

MCPace runs as a local MCP hub between AI clients and upstream MCP servers.

```text
AI client -> http://127.0.0.1:39022/mcp -> MCPace -> upstream MCP server
```

Responsibilities:

- expose one local MCP endpoint;
- load upstream entries from `mcp_settings.json` and `mcp_settings.d/*.json`;
- import existing `mcpServers` and VS Code-style `servers` config shapes;
- infer server transport from `command`, URL fields, path input, or package prefixes;
- start local stdio upstreams through explicit commands such as `npx`, `uvx`, Docker, or custom commands;
- connect to remote Streamable HTTP MCP URLs;
- keep upstream sessions isolated per client/project/server route.

User-facing commands stay small. Development, proof, and release commands remain in the source but are not part of the normal quickstart.
