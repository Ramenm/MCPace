# Verification Scenario Matrix

This document is the normative scenario catalog for `verify-manager.ps1`.

Columns:

- `Scenario ID`: stable identifier used in JSON and Markdown reports
- `Preconditions`: required setup or environment assumptions
- `Command`: command or internal path the harness exercises
- `Expected`: acceptance target
- `Severity`: blocker level when the scenario fails

## Bootstrap

| Scenario ID | Preconditions | Command | Expected | Severity |
| --- | --- | --- | --- | --- |
| `bootstrap/windows-direct-check` | Windows host; PowerShell 7 installed | `pwsh -NoProfile -File .\check.ps1` | Direct script execution is deterministic; execution-policy failure is recorded as a blocker. | `blocker` |
| `bootstrap/windows-bypass-check` | Windows host; PowerShell 7 installed | `pwsh -NoProfile -ExecutionPolicy Bypass -File .\check.ps1` | Bypass bootstrap path succeeds and reaches a usable stack inspection path. | `blocker` |
| `bootstrap/missing-docker` | PowerShell 7 installed | `Assert-Prerequisites with docker hidden from PATH` | Missing Docker produces a deterministic prerequisite error. | `high` |
| `bootstrap/missing-node` | PowerShell 7 installed | `Assert-Prerequisites with node hidden from PATH` | Missing Node.js produces a deterministic prerequisite error. | `high` |
| `bootstrap/non-pwsh-shell` | Windows host; Windows PowerShell 5.1 available | `powershell.exe -NoProfile -File .\boot.ps1` | Unsupported shell fails deterministically instead of hanging. | `high` |

## Source

| Scenario ID | Preconditions | Command | Expected | Severity |
| --- | --- | --- | --- | --- |
| `source/pester-suite` | PowerShell 7 | `pwsh -NoProfile -ExecutionPolicy Bypass -Command "Invoke-Pester -CI -Path ./tests"` | Repository source suite passes without introducing new regressions. | `blocker` |

## Lifecycle

| Scenario ID | Preconditions | Command | Expected | Severity |
| --- | --- | --- | --- | --- |
| `lifecycle/boot-idempotent` | Docker ready; Node.js 18+; PowerShell 7 | `pwsh -NoProfile -ExecutionPolicy Bypass -File .\boot.ps1` | Boot path succeeds when rerun and does not leave the stack half-broken. | `blocker` |
| `lifecycle/check` | Stack booted | `pwsh -NoProfile -ExecutionPolicy Bypass -File .\check.ps1` | Check path exits 0 only when ABP and MCPace required path are ready. | `blocker` |
| `lifecycle/smoke` | Stack booted; Valid bearer token | `pwsh -NoProfile -ExecutionPolicy Bypass -File .\smoke-test.ps1` | Smoke path validates MCP initialize/repeat/reconnect against the public endpoint. | `blocker` |
| `lifecycle/readiness` | Stack booted; Docker ready; Node.js 18+; PowerShell 7 | `pwsh -NoProfile -ExecutionPolicy Bypass -File .\validate-readiness.ps1` | Readiness path validates live stack, portability, and Docker mount policy within the configured budget. | `blocker` |

## Clients

| Scenario ID | Preconditions | Command | Expected | Severity |
| --- | --- | --- | --- | --- |
| `clients/launcher-config` | PowerShell 7; Client launcher generated | `Get-ClientConfigJson / Write-ClientLauncher` | Generic launcher-first config references only `mcpace.cmd` or `mcpace.sh` with no direct upstream endpoints. | `high` |
| `clients/editor-profile` | PowerShell 7 | `pwsh -NoProfile -ExecutionPolicy Bypass -File .\setup-mcp-clients.ps1 -Overwrite` | Generated editor profile references only the launcher surface. | `high` |
| `clients/api-health` | Stack booted | `GET /health` | Health endpoint returns 200 and a supported health status. | `high` |
| `clients/api-servers` | Stack booted; Valid bearer token | `Authenticated GET /api/servers` | Authenticated server listing succeeds and returns an array-backed data envelope. | `blocker` |
| `clients/mcp-session` | Stack booted; Valid bearer token | `Authenticated MCP initialize + notifications/initialized + tools/list + resources/list + reconnect` | MCP session handshake, repeat request, and reconnect all succeed without session corruption. | `blocker` |

## Servers

| Scenario ID | Preconditions | Command | Expected | Severity |
| --- | --- | --- | --- | --- |
| `servers/required-path` | Stack booted | `Get-HubServerStatuses + Get-RequiredServerConnectivity + Get-ABPState` | Every required server is effectiveEnabled and connected; the browser path is reachable on the host. | `blocker` |
| `servers/optional-default-disabled` | Context generated | `Inspect optional server runtime entries with source defaults` | Default-disabled optional servers remain explained, stable, and correctly gated. | `medium` |
| `servers/optional-source-enabled` | Context generated | `Inspect optional source-enabled server runtime entries` | Source-enabled optional servers remain observable with correct configured/effective state and disabled reasons when locally overridden. | `medium` |
| `servers/optional-user-enabled/github` | Copied manager root; User-style enable override applied | `Enable optional server 'github' in isolated runtime context and verify live connection or explicit runtime gating.` | If prerequisites and credentials exist, the server reaches a live connected state; otherwise configuredEnabled remains true and runtime gating is explicit. | `high` |
| `servers/optional-user-enabled/git` | Copied manager root; User-style enable override applied | `Enable optional server 'git' in isolated runtime context and verify live connection or explicit runtime gating.` | If prerequisites and credentials exist, the server reaches a live connected state; otherwise configuredEnabled remains true and runtime gating is explicit. | `high` |
| `servers/optional-user-enabled/sentry` | Copied manager root; User-style enable override applied | `Enable optional server 'sentry' in isolated runtime context and verify live connection or explicit runtime gating.` | If prerequisites and credentials exist, the server reaches a live connected state; otherwise configuredEnabled remains true and runtime gating is explicit. | `high` |
| `servers/optional-user-enabled/windows-mcp` | Copied manager root; User-style enable override applied | `Enable optional server 'windows-mcp' in isolated runtime context and verify live connection or explicit runtime gating.` | If prerequisites and credentials exist, the server reaches a live connected state; otherwise configuredEnabled remains true and runtime gating is explicit. | `high` |
| `servers/optional-user-enabled/screenpipe` | Copied manager root; User-style enable override applied | `Enable optional server 'screenpipe' in isolated runtime context and verify live connection or explicit runtime gating.` | If prerequisites and credentials exist, the server reaches a live connected state; otherwise configuredEnabled remains true and runtime gating is explicit. | `high` |
| `servers/optional-user-enabled/firecrawl` | Copied manager root; User-style enable override applied | `Enable optional server 'firecrawl' in isolated runtime context and verify live connection or explicit runtime gating.` | If prerequisites and credentials exist, the server reaches a live connected state; otherwise configuredEnabled remains true and runtime gating is explicit. | `high` |

## Persistence

| Scenario ID | Preconditions | Command | Expected | Severity |
| --- | --- | --- | --- | --- |
| `persistence/auth-bootstrap` | Auth env vars cleared | `New-McpAceContext on temp state root` | Local auth state bootstraps automatically when env auth is absent. | `high` |
| `persistence/env-override` | Temp state root; Bearer env override set | `New-McpAceContext with MCPACE_BEARER_TOKEN override` | Environment override wins over local bootstrap auth without mutating repo-local runtime state. | `high` |
| `persistence/local-overrides` | Temp state root | `Write and reload local server overrides` | Configured enablement intent survives restarts through local overrides. | `high` |
| `persistence/effective-array-shape` | Temp state root | `Generate effective settings and inspect array-backed fields` | Generated effective settings preserve required arrays and workspace-aware transforms. | `blocker` |
| `persistence/backup` | Temp state root | `New-DataBackup` | Backup archive is created and retention policy remains bounded. | `medium` |
| `persistence/release-bundle` | PowerShell 7 | `pwsh -NoProfile -ExecutionPolicy Bypass -File .\build-release.ps1 -OutputDir <temp>` | Portable release bundle is built from manifest-tracked source without runtime state leakage. | `high` |

## Readiness

| Scenario ID | Preconditions | Command | Expected | Severity |
| --- | --- | --- | --- | --- |
| `portable-layout/current-root` | Current manager root | `validate-readiness.ps1 internal portable-layout check` | Current manager root contains the required portable layout. | `high` |
| `live-regression/boot` | Current manager root complete | `validate-readiness.ps1 internal live regression boot step` | Live stack boot reaches ready state. | `blocker` |
| `live-regression/check` | Live regression boot passed | `validate-readiness.ps1 internal live regression required-path check` | Current required path is ready. | `blocker` |
| `live-regression/smoke` | Live regression check passed | `validate-readiness.ps1 internal live regression smoke step` | Current live smoke step passes. | `blocker` |
| `portability/copy-manager-root` | Validation temp root created | `validate-readiness.ps1 internal manager root copy step` | Portable manager root copies successfully into an isolated environment. | `high` |
| `portability/workspace-registry` | Copied manager root | `validate-readiness.ps1 internal workspace registry step` | External primary and read-only extra workspaces resolve correctly. | `high` |
| `compatibility/effective-settings` | Copied manager root; Isolated effective settings generated | `validate-readiness.ps1 internal workspace-aware effective settings step` | Filesystem, serena, lean-ctx, and git transforms remain correct. | `blocker` |
| `portability/boot` | Copied manager root | `validate-readiness.ps1 internal isolated boot step` | Isolated ABP and MCPace are ready. | `blocker` |
| `portability/check` | Isolated boot passed | `validate-readiness.ps1 internal isolated required-path check` | Isolated required path is ready. | `blocker` |
| `portability/smoke` | Isolated required path ready | `validate-readiness.ps1 internal isolated smoke step` | Isolated smoke step passes. | `blocker` |
| `policy/rw-mount` | Copied manager root; Docker ready | `validate-readiness.ps1 internal read-write mount probe` | Docker read-write mount permits host file writes. | `high` |
| `policy/ro-mount` | Copied manager root; Docker ready | `validate-readiness.ps1 internal read-only mount probe` | Docker read-only mount blocks writes with a read-only filesystem error. | `high` |

## Destructive

| Scenario ID | Preconditions | Command | Expected | Severity |
| --- | --- | --- | --- | --- |
| `destructive/temp-auth-reset` | Temp state root; Non-destructive scenarios passed | `Reset-LocalAuthState in temp runtime context` | Auth reset changes auth material and the next context remains readable. | `high` |
| `destructive/temp-reset-hub-data` | Copied manager root; Isolated stack booted | `pwsh -NoProfile -ExecutionPolicy Bypass -File .\repair.ps1 -ResetHubData` | Destructive repair on isolated manager root returns the stack to a healthy readable state. | `high` |
| `destructive/live-auth-reset` | Backup created; Non-destructive current-host suite passed | `pwsh -NoProfile -ExecutionPolicy Bypass -File .\auth.ps1 -Reset` | Live auth reset completes and the stack remains recoverable. | `blocker` |
| `destructive/live-reset-hub-data` | Backup created; Non-destructive current-host suite passed | `pwsh -NoProfile -ExecutionPolicy Bypass -File .\repair.ps1 -ResetHubData` | Live destructive repair completes and the stack returns to a healthy readable state. | `blocker` |

## Platform

| Scenario ID | Preconditions | Command | Expected | Severity |
| --- | --- | --- | --- | --- |
| `platform/windows-current-host` | Windows host | `Current verification harness run` | Windows claims are backed by the current host report. | `medium` |
| `platform/ubuntu-workflow` | Runtime smoke Ubuntu workflow exists | `.github/workflows/runtime-smoke-ubuntu.yml` | Ubuntu runtime lane is represented as a workflow-driven support claim and report artifact producer. | `medium` |
| `platform/macos-manual-gate` | `docs/runtime-smoke-macos.md` exists | `docs/runtime-smoke-macos.md` | macOS remains explicitly documented as a manual gate and not auto-proven unless run. | `medium` |
