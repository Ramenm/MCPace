# macOS Runtime Smoke Gate

This document defines the manual runtime gate for macOS until a suitable Docker-capable automation path exists.

## Baseline

- Docker Desktop is running
- Node.js 18+ is available
- PowerShell 7 (`pwsh`) is installed and used for every project script

Windows PowerShell 5.1 is not a supported runtime shell for this project.

## Manual gate

From the repository root:

```bash
sh ./manager.sh verify -Profile ci-runtime
```

## Acceptance criteria

- `verify-manager.ps1 -Profile ci-runtime` produces `reports/verification-latest.md` and `reports/verification-latest.json`
- the harness reports passing `lifecycle/boot-idempotent`, `lifecycle/check`, `lifecycle/smoke`, and `clients/mcp-session`
- `/health` resolves to `healthy` or `degraded` only when the required path is still ready
- authenticated `GET /api/servers` succeeds
- authenticated MCP initialize and follow-up `tools/list` requests succeed

## Failure handling

- treat any `401` from `/api/servers` or `/mcp` as a release blocker
- treat any generated effective settings file that collapses arrays into objects or `null` as a release blocker
- if runtime succeeds only under a shell other than `pwsh`, the result does not count as supported
