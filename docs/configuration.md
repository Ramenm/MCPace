# Configuration

Common files:

- `mcpace.config.json` — runtime defaults and include paths.
- `mcp_settings.json` — root MCP server settings.
- `mcp_settings.d/*.json` — per-server fragments written by `mcpace install` or `mcpace up` import.

## Config-first import

```bash
mcpace server import ./mcp.json --dry-run
mcpace server import ./mcp.json --force
mcpace server sources
mcpace up
```

Supported input shapes:

```json
{ "mcpServers": { "local": { "command": "npx", "args": ["-y", "pkg"] } } }
```

```json
{ "servers": { "remote": { "serverUrl": "https://example.com/mcp" } } }
```

## Install examples

```bash
mcpace install npm:@modelcontextprotocol/server-memory --as memory --dry-run
mcpace install pypi:mcp-server-demo --as demo --dry-run
mcpace install https://example.com/mcp --as remote-example --dry-run
mcpace install . --as filesystem --dry-run
mcpace install -- npx -y @modelcontextprotocol/server-memory
```

## Options

- `--as <name>` sets the server name.
- `--path <path>` appends path arguments for servers that need explicit scopes.
- `--env KEY=VALUE` adds environment variables.
- `--header KEY=VALUE` adds HTTP headers for remote servers.
- `--settings <path>` writes to a specific MCP settings file.
- `--dry-run` previews without writing.
- `--force` replaces an existing fragment.
- `--disabled` writes the server as disabled.
