# External MCP tool and internet smoke

- Status: blocked
- Mode: live-internet
- Generated: 2026-05-17T11:06:23.903Z
- Project: mcpace 0.6.5

## Scenario matrix

| ID | Category | Internet | Risks | Default posture |
|---|---|---|---|---|
| local-filesystem | local-only | none required after the binary/package is present | local data access; path traversal policy; large file scanning | register disabled or least-scope path allowlist |
| npx-package-launch | package-manager | npm registry may be used on first run or version change | dependency chain abuse; postinstall scripts; version drift; registry outage | dry-run registration, pinned package, no tool call during install |
| uvx-python-launch | package-manager | PyPI may be used on first run or version change | version drift; native wheel availability; Python environment mismatch | treat launch as runtime install, not MCPace install |
| docker-image-launch | container-runtime | container registry may be used on first pull | image tag drift; privileged mounts; network and filesystem blast radius | pin digest and keep mounts read-only by default |
| github-api | external-api | requires GitHub API reachability and usually a token | token scope; rate limits; private repository data exposure | disabled until token + repo scope reviewed |
| brave-search-api | external-api | requires Brave Search API reachability and API key for real queries | paid quota; API key leakage; unexpected search costs | disabled until API key and budget reviewed |
| fetch-web | external-web | requires arbitrary web access | SSRF-like behavior; unexpected large downloads; untrusted content ingestion | host/domain allowlist and response-size limit |
| remote-streamable-http | remote-mcp | requires remote domain reachability and often auth | domain ownership confusion; auth token audience mismatch; remote downtime | explicit owned/not-owned labeling and auth review |

## Live results

| Endpoint | Required | Status | OK | Elapsed |
|---|---:|---:|---:|---:|
| mcp-docs | yes | fetch failed | no | 88.84ms |
| npm-registry-mcp-sdk | yes | fetch failed | no | 9.85ms |
| pypi-mcp | no | fetch failed | no | 6.18ms |
| github-api | yes | fetch failed | no | 5003.68ms |
| github-mcp-servers-repo | no | fetch failed | no | 5002.75ms |

## Checks

| Check | OK | Evidence |
|---|---:|---|
| covers-local-only-tools | yes | filesystem/git-style local tools |
| covers-package-manager-launchers | yes | npx, uvx, docker |
| covers-external-api-tools | yes | GitHub, Brave Search, Fetch/web |
| covers-remote-mcp-transport | yes | Streamable HTTP third-party domain |
| does-not-execute-third-party-packages | yes | matrix + optional HTTPS reachability only; no npx/uvx/docker MCP package is launched |
| live-internet-mode | no | required endpoints checked |
