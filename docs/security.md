# Security notes

Treat every upstream MCP server as code or a remote service you choose to trust. Local stdio servers can run commands on the machine; remote servers can receive requests and credentials you configure.

Recommended defaults:

- MCPace should not install a default upstream server.
- Run `--dry-run` before mutating settings.
- Grant filesystem servers only explicit paths.
- Review npm/PyPI/Docker package provenance before adding it.
- Keep secrets out of committed JSON; prefer environment variables or client-supported secret inputs.
- Use least-privilege tokens for remote MCP servers.
- Run `mcpace server test <name> --refresh` before wiring clients.

MCPace skips its own endpoint during import to avoid loops and keeps client/project/server sessions isolated so one client route does not reuse another client route's upstream session.
