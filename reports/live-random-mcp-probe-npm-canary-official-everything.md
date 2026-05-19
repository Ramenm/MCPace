# Live Random MCP Probe

Schema: `mcpace.liveRandomMcpProbe.v2`  
Status: **pass**  
Mode: `live-download-probe`  
Generated: 2026-05-17T12:06:20.617Z

This report covers real package-manager downloads only when run with `--download`. It sends only `initialize`, `notifications/initialized`, and `tools/list`. It does not call tools.

## Summary

- Servers: 1
- OK: 1
- Failed/startup-blocked: 0
- Tools discovered: 15
- Policy mismatches: none

## Results

| Server | Kind | Package | Status | Tools | Risk signals | Suggested policy |
|---|---|---|---:|---:|---|---|
| official-everything | npm | @modelcontextprotocol/server-everything@2026.1.26 | ok | 15 | protocol-fixture, database, credentials-or-auth | test-fixture-disabled |

## Safety

- Package install scripts allowed: false
- User secrets passed to runtime: false
- Destructive tool calls allowed: false

## Notes

- Only initialize, notifications/initialized, and tools/list were sent.
- No user API keys or user home directory were passed to runtime processes.
- npm install uses --ignore-scripts, --no-audit, --no-fund, and --omit=dev.
- PyPI installs happen in a disposable venv; runtime processes receive a stripped environment.
- Runtime network namespace isolation uses unshare -Urn when this host allows it; otherwise the probe falls back to stripped env + timeout only.
- This is a smoke probe. It is not a source security audit and it does not prove destructive tool behavior is safe.
