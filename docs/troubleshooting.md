# Troubleshooting

Start with:

```bash
mcpace doctor --json
mcpace serve status
mcpace server sources --json
mcpace server list --json
mcpace connect
```

Common fixes:

- Empty tool list: MCPace may have no upstream servers yet. Add one explicitly or import an existing config.
- npm server fails: check `node --version`, `npm --version`, then run the generated `npx -y ...` command manually.
- PyPI server fails: install/update `uvx`, then run the generated `uvx ...` command manually.
- Docker server fails: verify Docker is running and the image can be pulled.
- Remote URL fails: verify URL, auth headers, and server trust before connecting clients.
- Client already configured: run `mcpace client install <client> --dry-run --diff` first.
