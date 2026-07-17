# Troubleshooting

Use the shortest check that proves the layer you are debugging.

| Symptom | Check |
| --- | --- |
| MCPace does not start | `mcpace doctor --json` |
| MCPace is missing after login/reboot | Run `mcpace autostart verify --json`, then `mcpace up` to repair the current user's entry and plan. |
| Ubuntu does not restore MCPace | Run `systemctl --user status mcpace-agent.service` and `journalctl --user -u mcpace-agent.service -n 100`. A headless boot before login additionally needs user lingering. |
| macOS does not restore MCPace | Run `mcpace autostart verify --json`, then `launchctl print "gui/$(id -u)/MCPace Agent"`. Run `mcpace up` to rewrite, bootstrap, and health-check the current user's LaunchAgent. |
| WSL does not restore MCPace after Windows reboot | WSL itself must first be started by Windows. Inside Ubuntu, verify that `mcpace` resolves through the Linux installation and Linux optional native package, not a Windows executable exposed through `/mnt/c`. |
| Client cannot connect | Confirm `http://127.0.0.1:39022/mcp` and run `mcpace serve status`. |
| Stop reports a legacy state without a cooperative token | Verify that the recorded PID and executable are the old MCPace runtime, stop it through the OS supervisor or process manager, run `mcpace cleanup runtime`, then rerun `mcpace up`. MCPace deliberately refuses an unsafe PID-only kill. |
| Server was not imported | `mcpace server sources --json` |
| Upstream catalog is empty but servers exist elsewhere | Run `mcpace server sources --json`. If only root `mcp_settings.json` appears, run `mcpace serve restart` from an environment that can see `MCPACE_MCP_SETTINGS`/`MCPACE_MCP_SETTINGS_DIRS`; Windows autostart hydrates persistent MCPace env from the registry. |
| A terminal window appears when Windows logs in | Install a build/package that includes `mcpace-agent-launcher.exe` next to `mcpace.exe`, then run `mcpace up` and `mcpace autostart verify --json`. The Run entry should point at `mcpace-agent-launcher.exe`, not directly at `mcpace.exe`. |
| Wrong concurrency behavior | `mcpace server list --json` and `mcpace server instances --client-id <client> --session-id <chat> --project-root <path>` |
| A discovered server is still plan-only | Review catalog trust level, then run `mcpace auto <query> --dry-run`. |
| A weak server needs more evidence | `mcpace lab probe --id <server> --refresh --json` |
| Load-test command cannot find a binary | Build first or pass `--binary`, `MCPACE_BINARY_PATH`, or `MCPACE_DEV_BINARY`. |

For release validation, run `npm run check` first, then Rust checks on a host with the pinned toolchain.
