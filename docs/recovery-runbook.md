# Recovery Runbook

## 1. Windows Script Execution Is Blocked

Symptom:

- `PSSecurityException`
- `file ... is not digitally signed`

Why it happens:

- this repository ships unsigned local scripts;
- on Windows, `RemoteSigned` or stricter policy can block direct execution.

Recovery:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File .\start.ps1
pwsh -NoProfile -ExecutionPolicy Bypass -File .\check.ps1
pwsh -NoProfile -ExecutionPolicy Bypass -Command "Invoke-Pester -CI -Path ./tests"
```

On Linux/macOS, the wrapper surface is:

```bash
sh ./manager.sh check
sh ./manager.sh verify -Profile standard
```

Longer-term options:

- unblock the repo files after download;
- run from a trusted local clone without a blocking mark-of-the-web;
- adopt script signing later if this project needs a stricter enterprise bootstrap path.

## 2. `mcpace` Container Name Conflict Or Stale Container

Symptoms:

- `docker ... Conflict. The container name "/mcpace" is already in use`
- `check.ps1` tries to recreate the hub and fails
- `docker logs mcpace` shows `/app/mcp_settings.json` missing

Why it happens:

- the manager recreates the hub when the current container signature no longer matches the expected runtime signature;
- a stale or partially removed container can leave the name occupied;
- an older container can exist without the required settings bind mount.

Recovery:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File .\repair.ps1
```

Escalation if needed:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File .\repair.ps1 -ResetHubData
docker rm -f mcpace
pwsh -NoProfile -ExecutionPolicy Bypass -File .\boot.ps1
```

Use `-ResetHubData` only when normal repair does not recover the hub.

## 3. Required Path Is Incomplete

Symptom:

- `check.ps1` reports missing required servers;
- hub health is `degraded`;
- required-path summary is not ready.

What to inspect:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File .\check.ps1
Get-Content .\logs\launcher.log -Tail 120
Get-Content .\logs\abp.stderr.log -Tail 120
docker logs --tail 120 mcpace
```

Interpretation:

- `configuredEnabled=False` with `stateSource=local-override` means a local override turned the server off;
- `effectiveEnabled=False` with a disabled reason means runtime gating blocked the server even though source or overrides wanted it on;
- host bridge preflight failures are expected to disable platform-specific or missing-prerequisite bridges.

## 4. Local Overrides Cause Surprising Runtime Behavior

Symptom:

- source template says one thing, `check.ps1` shows another;
- optional servers keep coming back enabled or disabled after restart.

Why it happens:

- local enablement and OAuth state are harvested into `data/runtime/mcp_settings.local-overrides.json`.

What to inspect:

```powershell
Get-Content .\data\runtime\mcp_settings.local-overrides.json
pwsh -NoProfile -ExecutionPolicy Bypass -File .\check.ps1
```

Rule:

- fix source defaults in source files;
- fix local machine drift in local overrides or by resetting runtime state;
- do not "fix" effective settings directly, because they are regenerated.

## 5. Auth State Drift

Symptom:

- auth failures against `/api/servers` or `/mcp`;
- unclear whether env overrides or local bootstrap auth is active.

Recovery:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File .\auth.ps1 -Show
pwsh -NoProfile -ExecutionPolicy Bypass -File .\auth.ps1 -Reset
```

Rule:

- env vars win over local auth bootstrap state;
- local auth bootstrap is the normal path for a trusted local workstation.

## 6. Pre-Release Or Portability Validation

Use these gates before claiming the stack is healthy:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File .\verify-manager.ps1
pwsh -NoProfile -ExecutionPolicy Bypass -Command "Invoke-Pester -CI -Path ./tests"
pwsh -NoProfile -ExecutionPolicy Bypass -File .\check.ps1
pwsh -NoProfile -ExecutionPolicy Bypass -File .\smoke-test.ps1
pwsh -NoProfile -ExecutionPolicy Bypass -File .\validate-readiness.ps1
pwsh -NoProfile -ExecutionPolicy Bypass -File .\build-release.ps1
```

Interpretation:

- `verify-manager.ps1` is the canonical repeatable harness and writes the latest JSON + Markdown audit artifacts;
- Pester proves source and packaging contracts;
- `check.ps1` proves current live topology;
- `smoke-test.ps1` proves MCP handshake behavior;
- `validate-readiness.ps1` proves live stack, external manager-root portability, and Docker mount policy.
