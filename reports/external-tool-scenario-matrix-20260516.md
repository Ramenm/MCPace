# External MCP tool scenario matrix

This matrix is the product-facing map for local, package-launched, remote, and
paid MCP server scenarios. It intentionally separates registration from runtime
execution.

| Scenario | Examples | Internet dependency | Main risk | Expected MCPace posture |
|---|---|---|---|---|
| Local-only stdio | filesystem, local git | none after installed | local data exposure | disabled or least-scope path allowlist |
| npx-launched stdio | npm MCP packages | npm registry on first run/cache miss | dependency chain abuse, version drift | pin package, no execution during install |
| uvx-launched stdio | Python MCP packages | PyPI on first run/cache miss | environment mismatch, version drift | treat as runtime install surface |
| Docker-launched | containerized MCP server | image registry on first pull | image drift, mounts, network blast radius | pin digest, read-only mounts by default |
| External API | GitHub, Brave Search | provider API + credentials | token scope, rate limits, paid quota | disabled until token/domain/budget reviewed |
| Fetch/web | webpage fetch server | arbitrary web access | SSRF-like behavior, large downloads | allowlist + response size limits |
| Remote Streamable HTTP | third-party MCP URL | remote domain + auth | ownership confusion, token audience mismatch | explicit owned/not-owned labeling |

## Do not confuse these layers

- `mcpace server add/install` registers configuration.
- Package managers (`npx`, `uvx`) may fetch or update packages later at runtime.
- Docker may pull images later at runtime.
- Remote HTTP servers belong to their upstream domain owner unless the user owns
  that domain.
- API-key and OAuth tools can cost money or expose private data only when enabled
  and invoked; they must remain disabled until reviewed.
