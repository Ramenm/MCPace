# Supported clients

MCPace keeps one local MCP endpoint and patches client configs only when a supported local config or installed CLI is detected.

| Client mode | Behavior |
|---|---|
| `--client auto` | Default. Patch detected clients only. |
| `--client cursor-local` | Patch Cursor local config when present. |
| `--client vscode-workspace` | Patch VS Code MCP config using the `servers` root expected by `mcp.json`. |
| `--client all` | Patch every supported detected client. |
| `--client none` | Start MCPace without touching client config. |

The updater preserves unrelated server entries and skips MCPace self-references to avoid routing loops.

Notes:

- VS Code uses `servers` in `mcp.json`; many other clients use `mcpServers`, so MCPace keeps this shape client-specific.
- Prefer the local Streamable HTTP endpoint for clients that support it. Stdio launcher ingress remains a preview path until live forwarding proof is complete.
