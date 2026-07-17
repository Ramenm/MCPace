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
- Prefer the local Streamable HTTP endpoint for clients that support it. Generated command-based configs use the hidden `mcpace stdio` transport entrypoint. The legacy hidden `stdio-shim` entrypoint remains callable through 0.8.x only so existing configs continue to start.

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
mcpace advanced client install <client> --dry-run --diff
mcpace advanced client install <client>
mcpace advanced client list --json
```

Restart the client and run one MCP `tools/list` request (or the client's normal MCP connection check). Real client writes create a rollback backup; use `mcpace advanced client restore <client> --backup latest` if needed. `mcpace uninstall` removes only entries that still match MCPace's owned URL/managed markers and creates another rollback backup; unrelated client entries are preserved.

MCPace no longer writes `stdio-shim` into new configs. The alias will not be removed before an announced major release (no earlier than `1.0.0`), and the removal release must repeat this migration in `CHANGELOG.md`.
