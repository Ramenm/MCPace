# Live Random MCP Probe

Schema: `mcpace.liveRandomMcpProbe.v5`  
Status: **pass**  
Mode: `fixture-replay`  
Generated: 2026-05-17T12:08:21.267Z

This report covers real package-manager downloads only when run with `--download`. It sends only `initialize`, `notifications/initialized`, and `tools/list`. It does not call tools.

## Summary

- Servers: 12
- OK: 10
- Failed/startup-blocked: 1
- Tools discovered: 91
- Policy mismatches: none
- Unexpected failures: none
- Server-side requests handled: none

## Results

| Server | Kind | Package | Status | Tools | Risk signals | Suggested policy |
|---|---|---|---:|---:|---|---|
| official-filesystem | npm | @modelcontextprotocol/server-filesystem@2026.1.14 | ok | 14 | filesystem | project-filesystem-single-writer |
| official-memory | npm | @modelcontextprotocol/server-memory@2026.1.26 | ok | 9 | memory-or-context | state-profile-single-session |
| official-sequential-thinking | npm | @modelcontextprotocol/server-sequential-thinking@2025.12.18 | ok | 1 | memory-or-context | state-profile-single-session |
| python-time | pypi | mcp-server-time@2026.1.26 | ok | 2 | local-utility | local-utility-multi-reader |
| python-git | pypi | mcp-server-git@2026.1.14 | ok | 12 | git-repository | project-repo-single-writer |
| python-fetch | pypi | mcp-server-fetch@2025.4.7 | ok | 1 | network-fetch | network-fetch-review |
| python-sqlite | pypi | mcp-server-sqlite@2025.4.25 | ok | 6 | database | database-path-single-writer |
| official-everything | npm | @modelcontextprotocol/server-everything@2026.1.26 | ok | 15 | protocol-fixture, database, credentials-or-auth | test-fixture-disabled |
| deprecated-brave-search | npm | @modelcontextprotocol/server-brave-search@0.6.2 | startup-error | 0 | network-or-external-api, credentials-or-auth | credential-scoped-review |
| context7 | npm | @upstash/context7-mcp@2.2.5 | ok | 2 | database, network-or-external-api, credentials-or-auth | network-docs-multi-reader-review |
| chrome-devtools | npm | chrome-devtools-mcp@0.26.0 | ok | 29 | browser-or-desktop | shared-exclusive-host-lock |
| ui5 | npm | @ui5/mcp-server@0.2.11 | skipped-by-policy | 0 | install-blocked | project-devtools-single-writer-review |

## Safety

- Package install scripts allowed: false
- User secrets passed to runtime: false
- Destructive tool calls allowed: false

## Notes

- Consolidated evidence from deterministic npm, npm canary, and PyPI/uv package-manager probes.
- Each live result used only initialize, notifications/initialized, and tools/list; no tools were called.
- Canaries that need credentials or have large/flaky dependency trees are represented as startup-error or skipped-by-policy rather than being force-enabled by default.
- This is smoke evidence, not a source-code security audit of third-party packages.
