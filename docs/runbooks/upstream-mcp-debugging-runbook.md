# Upstream MCP debugging runbook

## Meaning of `closed stdout before responding to initialize`

The upstream stdio process exited, crashed, or closed stdout before sending the MCP `initialize` response. The next step is not to debug MCPace HTTP; it is to reproduce the upstream command in the same environment MCPace gives it.

## Triage order

1. Confirm the server is enabled intentionally.
2. Print sanitized command, cwd, args, timeout, and env var names.
3. Check that the command executable exists.
4. Run the command directly from the same shell.
5. Run it through `mcpace server test <name> --refresh --timeout-ms 30000 --json`.
6. Inspect stderr tail and child exit status.
7. For `npx`/`npx.CMD`, verify npm registry/cache/proxy/cert env passthrough.
8. For API servers, verify `env_vars` contains the API-key variable name.
9. For project-aware servers such as Serena, use a real project root and longer timeout.

## Common fixes

### npx/npx.CMD

Add env var names:

```json
"env_vars": [
  "NPM_CONFIG_REGISTRY",
  "NPM_CONFIG_USERCONFIG",
  "NPM_CONFIG_GLOBALCONFIG",
  "NPM_CONFIG_CACHE",
  "NODE_EXTRA_CA_CERTS",
  "SSL_CERT_FILE",
  "REQUESTS_CA_BUNDLE",
  "HTTP_PROXY",
  "HTTPS_PROXY",
  "NO_PROXY",
  "http_proxy",
  "https_proxy",
  "no_proxy",
  "CI"
]
```

### Serena

Use a real project:

```json
{
  "cwd": "C:\\Users\\rmatv\\Projects\\my-project",
  "args": ["...", "--project", "C:\\Users\\rmatv\\Projects\\my-project"],
  "initTimeout": 120000,
  "options": { "timeout": 120000 }
}
```

Do not use adapter temp paths for Serena smoke tests.
