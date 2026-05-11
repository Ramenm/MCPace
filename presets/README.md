# MCP server presets

This directory contains editable MCP server presets used by:

```bash
mcpace server presets
mcpace server install <preset>
mcpace server starter
```

Presets are data, not Rust package hardcode. The default catalog is `presets/mcp-servers.json`. Extra catalogs can be added through `mcpace.config.json` under `mcpPresets.includePaths` or with the `MCPACE_MCP_PRESETS` environment variable.

Useful commands:

```bash
mcpace server presets
mcpace server starter --path . --dry-run
mcpace server starter --path .
mcpace server install filesystem --path . --dry-run
mcpace server install context7 --dry-run
mcpace server install git --path . --dry-run
mcpace server install playwright --arg --headless --dry-run
```

Preset notes:

- `filesystem` adds `npx -y @modelcontextprotocol/server-filesystem <path>`. Keep the path narrow.
- `context7` adds `npx -y @upstash/context7-mcp` for developer documentation lookup.
- `git` adds `uvx mcp-server-git --repository <path>` and requires `uvx`.
- `playwright` adds `npx -y @playwright/mcp@latest`; use only for trusted browser automation workflows.

The starter pack intentionally remains conservative and installs only `filesystem` by default.
