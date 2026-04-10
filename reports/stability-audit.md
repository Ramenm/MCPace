# Stability Audit

## Scope

This report records the current verified stability baseline for the repository as checked on `2026-04-12` on a Windows host.

It complements, but does not replace:

- `README.md`
- `docs/operating-contract.md`
- `docs/recovery-runbook.md`
- `docs/verification-matrix.md`
- `reports/summary.md`

For the repeatable harness output, use:

- `reports/verification-latest.md`
- `reports/verification-latest.json`

## Verified Commands

The following commands were run successfully on this host:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -Command "Invoke-Pester -CI -Path ./tests"
pwsh -NoProfile -ExecutionPolicy Bypass -File .\check.ps1
pwsh -NoProfile -ExecutionPolicy Bypass -File .\validate-readiness.ps1
```

Observed results:

- Pester: `18/18` tests passed.
- `check.ps1`: passed after recovery, with ABP running and MCPace healthy.
- `validate-readiness.ps1`: passed in about `118.9s` out of a `240s` budget.

## Reproduced Failure Modes

### 1. Windows execution policy blocks normal script entry

Without `-ExecutionPolicy Bypass`, both direct script execution and the default Pester invocation failed with `PSSecurityException` because the scripts are unsigned.

Impact:

- the README happy path is not sufficient on a Windows host with restrictive execution policy;
- source and runtime checks can look "unstable" before the actual launcher logic even starts.

Current mitigation:

- run the commands through `pwsh -ExecutionPolicy Bypass ...` or unblock the repo files before normal use.

### 2. Hub recreation can hit a transient Docker name-conflict path

`logs/launcher.log` shows cases where the manager decided that the existing `mcpace` container no longer matched the expected runtime signature, attempted recreation, and then hit:

- `Conflict. The container name "/mcpace" is already in use`

Impact:

- `check.ps1` can fail during a recreate path even though the stack is otherwise recoverable;
- this presents as operational instability rather than a deterministic source regression.

Current mitigation:

- run `repair.ps1`;
- if necessary, remove the stale `mcpace` container and boot again.

### 3. A stale `mcpace` container can start without the required settings bind mount

`docker logs mcpace` showed fatal `ENOENT` errors for `/app/mcp_settings.json`.

Impact:

- the hub exits before it can satisfy health or smoke checks;
- the problem is runtime-state drift, not source-template corruption.

Current mitigation:

- recreate the stack with `repair.ps1`;
- use `repair.ps1 -ResetHubData` only if normal repair is insufficient.

## Current Healthy Baseline

After recovery, the stack reached this baseline:

- ABP reachable on the configured host port;
- MCPace healthy on the configured hub port;
- required path connected for the required runtime servers;
- smoke test passing for initialize, repeat requests, reconnect, tools, and resources;
- external copied manager root validated with a primary read-write workspace and one extra read-only workspace;
- Docker mount policy validated for both read-write and read-only cases.

## What Is Proven vs Not Yet Proven

Proven on this host:

- source policy contract;
- runtime security contract;
- local override persistence model;
- packaging/release bundle contract;
- current Windows live stack and portability validation.

Not yet proven automatically:

- brand-new clean-host provisioning without prior local state;
- Linux end-to-end runtime smoke in the current session;
- macOS end-to-end runtime smoke in the current session;
- enterprise-safe bootstrap without relying on execution-policy bypass or file unblocking.

## Interpretation

The project is not currently "randomly unstable" at the source-contract level.

The verified picture is narrower and more useful:

- source and packaging contracts are stable once executed under the expected `pwsh` path;
- the biggest instability is in bootstrap and runtime operations on Windows;
- the two most important operator risks are execution-policy friction and stale hub-container lifecycle drift.
