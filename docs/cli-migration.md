# CLI migration to the compact surface

MCPace is pre-1.0, and the human CLI now has one canonical spelling for each operation. Removed commands and pseudo-long flags fail with exit code 2; they are not silently redirected.

## Common replacements

| Before | Now |
| --- | --- |
| `mcpace setup`, `quickstart`, or `bootstrap` | `mcpace up` |
| `mcpace up <server> ...` | `mcpace install <server> ...`, then `mcpace up` |
| `mcpace auto ...` | `mcpace advanced server auto ...` |
| `mcpace server ...` | `mcpace advanced server ...` |
| `mcpace server add ...` | `mcpace install ...` |
| `mcpace client ...` | `mcpace advanced client ...` |
| `mcpace autostart ...` or `mcpace service ...` | `mcpace advanced autostart ...` |
| `mcpace doctor ...` | `mcpace advanced doctor ...` |
| `mcpace verify readiness ...` | `mcpace advanced doctor readiness ...` |
| `mcpace dashboard ...` | `mcpace advanced runtime foreground ...` |
| `mcpace cleanup ...` | `mcpace advanced runtime cleanup ...` |
| `mcpace repair ...` | `mcpace advanced runtime repair ...` |
| `mcpace update ...` | `mcpace advanced update ...` |
| `mcpace lab`, `candidates`, `profile`, `projects`, `release`, or `init` | `mcpace advanced dev <name> ...` |

Use standard double-dash options such as `--json` and `--root`. The former pseudo-long spellings `-json`, `-root`, and similar aliases were removed. Standard `-h`, `--help`, `-v`, and `--version` remain available where applicable.

## Lifecycle meanings

- `up` creates or repairs the MCPace home, imports existing MCP settings, starts the endpoint, wires selected clients, and repairs user login startup. It never installs a new upstream server.
- `start`, `stop`, and `restart` control only the configured runtime.
- `status` is read-only.
- `install` adds one upstream MCP server; it does not install the MCPace package or login startup.
- `uninstall` removes local MCPace integration while preserving the package, durable configuration, upstream definitions, and backups.

## Installed compatibility entrypoints

Do not manually rewrite generated `mcpace stdio --root ...` client commands or installed `mcpace agent run --autostart ...` login commands. Hidden `stdio-shim`, `mcp-server`, managed `serve`, and internal `hub` routes remain callable only for installed/configuration compatibility and are intentionally absent from normal help.

For client-entry migration, see [supported-clients.md](supported-clients.md).
