# Security notes

MCPace is local-first. Keep it bound to `127.0.0.1` unless authentication, origin policy, and operator warnings are explicitly configured.

Security rules to keep visible in docs and defaults:

- configured upstream MCP servers are local extensions and should be trusted before use;
- unknown public packages must not be executed silently by discovery;
- `mcpace auto` may install only approved/trusted candidates by default;
- safe probes may run `initialize` and `tools/list`, but must not call upstream tools;
- logs and diagnostics should redact tokens, API keys, passwords, private keys, bearer values, and authorization headers;
- user-specific settings belong outside the repository, for example through `MCPACE_MCP_SETTINGS`.

Use [`../SECURITY.md`](../SECURITY.md) for vulnerability reporting and supported-version policy.
