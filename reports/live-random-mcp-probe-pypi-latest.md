# Live Random MCP Probe

Schema: `mcpace.liveRandomMcpProbe.v4`  
Status: **pass**  
Mode: `live-download-probe`  
Generated: 2026-05-17T13:10:18.900Z

This report covers real package-manager downloads only when run with `--download`. It sends only `initialize`, `notifications/initialized`, and `tools/list`. It does not call tools.

## Summary

- Servers: 4
- OK: 4
- Failed/startup-blocked: 0
- Tools discovered: 21
- Policy mismatches: none
- Unexpected failures: none
- Server-side requests handled: none

## Results

| Server | Kind | Package | Status | Tools | Risk signals | Suggested policy |
|---|---|---|---:|---:|---|---|
| python-time | pypi | mcp-server-time@2026.1.26 | ok | 2 | local-utility | local-utility-multi-reader |
| python-git | pypi | mcp-server-git@2026.1.14 | ok | 12 | git-repository, mutable-or-destructive-tools | project-repo-single-writer |
| python-fetch | pypi | mcp-server-fetch@2025.4.7 | ok | 1 | network-fetch | network-fetch-review |
| python-sqlite | pypi | mcp-server-sqlite@2025.4.25 | ok | 6 | database, mutable-or-destructive-tools | database-path-single-writer |

## Safety

- Package install scripts allowed: false
- User secrets passed to runtime: false
- Destructive tool calls allowed: false

## Notes

- Only initialize, notifications/initialized, and tools/list were sent.
- No user API keys or user home directory were passed to runtime processes.
- npm install uses --ignore-scripts, --no-audit, --no-fund, --omit=dev, isolated HOME/cache, and a whitelisted package-manager environment.
- PyPI installs happen in a disposable venv with isolated cache/HOME and a whitelisted package-manager environment; runtime processes receive a stripped environment.
- Runtime network namespace isolation uses unshare -Urn when this host allows it; otherwise the probe falls back to stripped env + timeout only.
- This is a smoke probe. It is not a source security audit and it does not prove destructive tool behavior is safe.
- Some canaries are hard-skipped unless --allow-heavy-installs is passed because package-manager installs can hang in restricted mirrors.
