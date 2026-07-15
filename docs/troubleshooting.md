# Troubleshooting

Use the shortest check that proves the layer you are debugging.

| Symptom | Check |
|---|---|
| MCPace does not start | `mcpace doctor --json` |
| Client cannot connect | Confirm `http://127.0.0.1:39022/mcp` and run `mcpace serve status`. |
| Server was not imported | `mcpace server sources --json` |
| Upstream catalog is empty but servers exist elsewhere | Run `mcpace server sources --json`. If only root `mcp_settings.json` appears, run `mcpace serve restart` from an environment that can see `MCPACE_MCP_SETTINGS`/`MCPACE_MCP_SETTINGS_DIRS`; Windows autostart hydrates persistent MCPace env from the registry. |
| A terminal window appears when Windows logs in | Install a build/package that includes `mcpace-agent-launcher.exe` next to `mcpace.exe`, then run `mcpace autostart repair --json` and `mcpace autostart verify --json`. The Run entry should point at `mcpace-agent-launcher.exe`, not directly at `mcpace.exe`. |
| Wrong concurrency behavior | `mcpace server list --json` and `mcpace server instances --client-id <client> --session-id <chat> --project-root <path>` |
| A discovered server is still plan-only | Review catalog trust level, then run `mcpace auto <query> --dry-run`. |
| A weak server needs more evidence | `mcpace lab probe --id <server> --refresh --json` |
| Load-test command cannot find a binary | Build first or pass `--binary`, `MCPACE_BINARY_PATH`, or `MCPACE_DEV_BINARY`. |

For release validation, run `npm run check` first, then Rust checks on a host with the pinned toolchain.
