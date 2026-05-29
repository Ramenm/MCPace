# Troubleshooting

Use the shortest check that proves the layer you are debugging.

| Symptom | Check |
|---|---|
| MCPace does not start | `mcpace doctor --json` |
| Client cannot connect | Confirm `http://127.0.0.1:39022/mcp` and run `mcpace serve status`. |
| Server was not imported | `mcpace server sources --json` |
| Wrong concurrency behavior | `mcpace server list --json` and `mcpace server instances --client-id <client> --session-id <chat> --project-root <path>` |
| A discovered server is still plan-only | Review catalog trust level, then run `mcpace auto <query> --dry-run`. |
| A weak server needs more evidence | `mcpace lab probe --id <server> --refresh --json` |
| Load-test command cannot find a binary | Build first or pass `--binary`, `MCPACE_BINARY_PATH`, or `MCPACE_DEV_BINARY`. |

For release validation, run `npm run check` first, then Rust checks on a host with the pinned toolchain.
