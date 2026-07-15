# Supported clients

MCPace keeps one local MCP endpoint and patches client configs only when a supported local config or installed CLI is detected.

| Client mode | Behavior |
| --- | --- |
| `--client auto` | Default. Patch detected clients only. |
| `--client cursor-local` | Patch Cursor local config when present. |
| `--client vscode-workspace` | Patch VS Code MCP config using the `servers` root expected by `mcp.json`. |
| `--client all` | Patch every supported detected client. |
| `--client none` | Start MCPace without touching client config. |

The updater preserves unrelated server entries and skips MCPace self-references to avoid routing loops.

Notes:

- VS Code uses `servers` in `mcp.json`; many other clients use `mcpServers`, so MCPace keeps this shape client-specific.
- Prefer the local Streamable HTTP endpoint for clients that support it. The `mcpace stdio` launch surface now uses the live MCP JSON-RPC stdio server; the legacy `stdio-shim` name remains only as a compatibility alias for older client configs.

## Migrating `stdio-shim` client entries

For an existing command-based client entry, replace only the MCPace subcommand:

```text
mcpace stdio-shim [existing flags]
```

becomes:

```text
mcpace stdio [the same existing flags]
```

For a managed client, preview the exact patch before applying it:

```bash
mcpace client install <client> --dry-run --diff
mcpace client install <client>
mcpace client list --json
```

Restart the client and run one MCP `tools/list` request (or the client's normal MCP connection check). Real client writes create a rollback backup; use `mcpace client restore <client> --backup latest` if needed.

MCPace no longer writes `stdio-shim` into new configs. The alias will not be removed before an announced major release (no earlier than `1.0.0`), and the removal release must repeat this migration in `CHANGELOG.md`.
