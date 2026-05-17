# Live Random MCP Probe

Schema: `mcpace.liveRandomMcpProbe.v4`  
Status: **pass**  
Mode: `live-download-probe`  
Generated: 2026-05-17T13:05:32.995Z

This report covers real package-manager downloads only when run with `--download`. It sends only `initialize`, `notifications/initialized`, and `tools/list`. It does not call tools.

## Summary

- Servers: 1
- OK: 1
- Failed/startup-blocked: 0
- Tools discovered: 23
- Policy mismatches: none
- Unexpected failures: none
- Server-side requests handled: none

## Results

| Server | Kind | Package | Status | Tools | Risk signals | Suggested policy |
|---|---|---|---:|---:|---|---|
| playwright-official | npm | @playwright/mcp@0.0.75 | ok | 23 | browser-or-desktop, shell-or-process, mutable-or-destructive-tools, open-world-annotation | shared-exclusive-host-lock |

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
