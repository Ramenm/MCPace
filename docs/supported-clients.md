# Supported clients

MCPace exposes one local MCP URL:

```text
http://127.0.0.1:39022/mcp
```

Use `mcpace client list` to see patchers known to the installed build. Preview before writing:

```bash
mcpace client install cursor-local --dry-run --diff
mcpace client install claude-code --dry-run --diff
mcpace client export --json
```

`mcpace up --client auto` patches only detected local clients. Use `--client none` when you only want the endpoint, and use `mcpace connect` for manual wiring guidance when a client is not patchable yet.
