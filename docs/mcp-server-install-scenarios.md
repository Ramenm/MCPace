# MCP server install scenarios and ownership boundaries

This document defines what MCPace means by "installing" an MCP server and which scenario checks must remain covered before a release.

## Facts from the current implementation

- `mcpace server install <preset>` and `mcpace server add ...` register an MCP server by writing an MCP settings fragment under `mcp_settings.d/` by default.
- Registration does not download an npm/PyPI package, start a process, call a remote endpoint, or invoke a tool.
- Runtime execution is deferred until `mcpace server test`, a connected MCP client, or the MCPace runtime actually launches the configured command or connects to the configured URL.
- Re-running an install with the same normalized server name is blocked unless `--force` is provided.
- `--force` replaces the existing entry; it is not a package reinstall command.
- `--disabled` is available on `server add` so paid or risky servers can be registered for review without enabling them.

## Domain ownership model

There are two different domains to keep separate:

1. **The MCPace endpoint domain** — this is the endpoint clients use to reach MCPace. By default it is local, such as `127.0.0.1:<port>/mcp`. If `serve.publicUrl` or `MCPACE_PUBLIC_MCP_URL` is set, that URL must point to infrastructure controlled by the user or their organization.
2. **The upstream MCP server domain** — this is the URL or package configured for a specific upstream server. A remote URL like `https://vendor.example/mcp` belongs to that upstream provider unless the user hosts it. An npm package namespace also belongs to that package publisher, not MCPace.

MCPace should not imply ownership or trust over an upstream package, npm namespace, remote HTTP domain, OAuth issuer, or billing account just because the user added it to settings.


### Ownership summary

Owned by MCPace:

- generated `mcp_settings.d/*.json` fragments inside the project root;
- the local dashboard and local MCPace endpoint;
- install/readiness reports produced by MCPace;
- explicit enable/disable/remove state transitions that MCPace writes.

Not owned by MCPace:

- upstream MCP server domain behavior;
- npm/PyPI/Docker package contents;
- provider billing and API quotas;
- OAuth issuer policy;
- downstream side effects once a connected client or runtime invokes a tool.

## What happens on common installs

| Scenario | What MCPace should do | What MCPace must not claim |
|---|---|---|
| Install a stdio preset | Write command/args into a fragment. | It has downloaded or verified the package. |
| Reinstall the same preset | Fail without `--force`; replace with `--force`. | It has updated global packages. |
| Add a remote HTTP server | Store URL and optional headers. | The remote domain is owned or trusted by MCPace. |
| Add paid/expensive server | Prefer disabled registration until reviewed. | Tool calls are free or bounded. |
| Configure 100 servers | Keep config inventory deterministic and bounded. | All 100 can safely run concurrently. |
| Mix stdio and HTTP | Preserve transport type and route using server metadata. | Every client supports every transport directly. |

## Cost and billing boundaries

Installing/registering a server is not the expensive part. Cost may appear later when:

- `npx`, `uvx`, Docker, or another launcher downloads dependencies;
- a paid remote MCP server receives a request;
- a tool call reaches an external API with billing;
- a browser/database/cloud/server tool performs side effects;
- a client auto-discovers and exposes many tools, creating accidental usage.

For paid servers, prefer this sequence:

```bash
mcpace server add paid-analytics \
  --command npx \
  --arg -y \
  --arg @vendor/paid-analytics-mcp \
  --env PAID_ANALYTICS_API_KEY='${PAID_ANALYTICS_API_KEY}' \
  --disabled \
  --json

mcpace server sources --json
mcpace server test paid-analytics --refresh --json
mcpace server enable paid-analytics --json
```

Only enable after the owner has reviewed package/domain, credentials, expected tool surface, cost model, and side-effect behavior.

## Required regression scenarios

The executable smoke suite is `npm run verify:mcp-install-scenarios`.

It must cover:

- dry-run does not write files;
- install writes one fragment;
- install materializes command/args and defers package execution;
- repeated install without `--force` fails;
- `--force` replaces;
- custom stdio add works;
- remote Streamable HTTP add works;
- non-http(s) remote URL is rejected;
- paid server can be registered disabled;
- 100 distinct server fragments can be registered and inventoried.

## What remains unproven by this smoke suite

- Live package install/cache behavior for `npx`, `uvx`, Docker, or other launchers.
- Provider billing behavior.
- Real-client behavior across Claude Desktop, VS Code, Codex, Cursor, Windsurf, or other clients.
- Concurrent runtime launch of 100 real servers.
- Long-running process isolation, memory ceilings, credential scoping, and per-tool cost budgets.
- Whether a third-party package or remote domain is safe.

Those require separate live-host and provider-specific tests.
