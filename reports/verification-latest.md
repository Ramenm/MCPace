# Verification Audit

Generated: `2026-04-12T07:48:52.5576933+03:00`
Profile: `standard`
Overall verdict: `pass`

## Summary

- Host OS: `Microsoft Windows 10.0.26200`
- PowerShell: `7.6.0`
- Node: `v24.14.1`
- Docker ready: `True`
- Results: pass=`47` fail=`0` warn=`0` not-proven=`0` skipped=`2` not-applicable=`0`

## Bootstrap

| Scenario | Verdict | Severity | Scope | Notes |
| --- | --- | --- | --- | --- |
| `bootstrap/missing-docker` | `pass` | `high` | `current-host` | exit=0; Required command is missing: docker |
| `bootstrap/missing-node` | `pass` | `high` | `current-host` | exit=0; Required command is missing: node |
| `bootstrap/non-pwsh-shell` | `pass` | `high` | `windows-current-host` | exit=1; �� 㤠���� �믮����� �業�਩ "boot.ps1", ⠪ ��� �� ᮤ�ন� ������ "#requires" ��� Windows PowerShell 7.0. ����室�  ��� ��... |
| `bootstrap/windows-bypass-check` | `pass` | `blocker` | `windows-current-host` | exit=0; ABP:     RUNNING PID 19588, up 4m, endpoint reachable  MCPace:  HEALTHY required path ready, servers 8/8, up ... |
| `bootstrap/windows-direct-check` | `pass` | `blocker` | `windows-current-host` | exit=0; ABP:     RUNNING PID 19588, up 4m, endpoint reachable  MCPace:  HEALTHY required path ready, servers 8/8, up ... |

## Clients

| Scenario | Verdict | Severity | Scope | Notes |
| --- | --- | --- | --- | --- |
| `clients/api-health` | `pass` | `high` | `current-host` | status=200; health=healthy |
| `clients/api-servers` | `pass` | `blocker` | `current-host` | server-count=16 |
| `clients/editor-profile` | `pass` | `high` | `current-host` | command=C:\Users\rmatv\AppData\Local\Temp\mcpace-verify-140b990f2e4d42f085c2e49c3235559a\manager\mcpace.cmd; args=0 |
| `clients/launcher-config` | `pass` | `high` | `current-host` | command=C:\mcpace\mcpace.cmd; args=0 |
| `clients/mcp-session` | `pass` | `blocker` | `current-host` | Smoke test passed. health=healthy, servers=16, tools=95, repeat-tools=95, reconnect-tools=95, resources=5, templates=0. |

## Destructive

| Scenario | Verdict | Severity | Scope | Notes |
| --- | --- | --- | --- | --- |
| `destructive/temp-auth-reset` | `pass` | `high` | `temp-state-root` | tokenChanged=True |
| `destructive/temp-reset-hub-data` | `pass` | `high` | `isolated-manager-root` | repair=exit=0; True  Repair completed. Both services are ready.; missing=none |

## Lifecycle

| Scenario | Verdict | Severity | Scope | Notes |
| --- | --- | --- | --- | --- |
| `lifecycle/boot-idempotent` | `pass` | `blocker` | `current-host` | exit=0; Boot start completed. Services are ready. |
| `lifecycle/check` | `pass` | `blocker` | `current-host` | exit=0; ABP:     RUNNING PID 19588, up 5m, endpoint reachable  MCPace:  HEALTHY required path ready, servers 8/8, up ... |
| `lifecycle/readiness` | `pass` | `blocker` | `current-host` | exit=0; ==> Checking current manager root completeness  ==> Running live regression suite on current manager root  ==... |
| `lifecycle/smoke` | `pass` | `blocker` | `current-host` | exit=0; Smoke test passed. health=healthy, servers=16, tools=95, repeat-tools=95, reconnect-tools=95, resources=5, te... |

## Misc

| Scenario | Verdict | Severity | Scope | Notes |
| --- | --- | --- | --- | --- |
| `destructive/live-auth-reset` | `skipped` | `medium` | `current-host` | Live destructive scenarios are disabled for standard and ci-runtime profiles. |
| `destructive/live-reset-hub-data` | `skipped` | `medium` | `current-host` | Live destructive scenarios are disabled for standard and ci-runtime profiles. |

## Persistence

| Scenario | Verdict | Severity | Scope | Notes |
| --- | --- | --- | --- | --- |
| `persistence/auth-bootstrap` | `pass` | `high` | `temp-state-root` | authState=C:\Users\rmatv\AppData\Local\Temp\mcpace-verify-persistence-9305041213764326b35e5f9977ca865e\data\server-st... |
| `persistence/backup` | `pass` | `medium` | `temp-state-root` | backup=C:\Users\rmatv\AppData\Local\Temp\mcpace-verify-persistence-9305041213764326b35e5f9977ca865e\backups\mcpace-da... |
| `persistence/effective-array-shape` | `pass` | `blocker` | `temp-state-root` | bearerKeys=1; users=1; browserArgs=6 |
| `persistence/env-override` | `pass` | `high` | `temp-state-root` | tokenSource=env; token=verification-bearer-token |
| `persistence/local-overrides` | `pass` | `high` | `temp-state-root` | configured=True; source=local-override; effective=False |
| `persistence/release-bundle` | `pass` | `high` | `current-host` | exit=0; Created release bundle: C:\Users\rmatv\AppData\Local\Temp\mcpace-verify-release-90af93294d75489aa7135222ea02a... |

## Platform

| Scenario | Verdict | Severity | Scope | Notes |
| --- | --- | --- | --- | --- |
| `platform/macos-manual-gate` | `pass` | `medium` | `manual-gate` | Manual gate references verify-manager.ps1. |
| `platform/ubuntu-workflow` | `pass` | `medium` | `github-actions` | Workflow references the repeatable verification entrypoint. |
| `platform/windows-current-host` | `pass` | `medium` | `windows-current-host` | Current audit was executed on this Windows host. |

## Readiness

| Scenario | Verdict | Severity | Scope | Notes |
| --- | --- | --- | --- | --- |
| `compatibility/effective-settings` | `pass` | `blocker` | `isolated-manager-root` | filesystem, serena, lean-ctx and git transforms are correct |
| `live-regression/boot` | `pass` | `blocker` | `current-host` | ABP and MCPace are ready |
| `live-regression/check` | `pass` | `blocker` | `current-host` | required path is ready |
| `live-regression/smoke` | `pass` | `blocker` | `current-host` | Smoke test passed. health=healthy, servers=16, tools=115, repeat-tools=115, reconnect-tools=115, resources=6, templat... |
| `policy/ro-mount` | `pass` | `high` | `isolated-manager-root` | docker blocked writes to read-only workspace |
| `policy/rw-mount` | `pass` | `high` | `isolated-manager-root` | host file write succeeded |
| `portability/boot` | `pass` | `blocker` | `isolated-manager-root` | temporary ABP and MCPace are ready |
| `portability/check` | `pass` | `blocker` | `isolated-manager-root` | temporary required path is ready |
| `portability/copy-manager-root` | `pass` | `high` | `isolated-manager-root` | C:\Users\rmatv\AppData\Local\Temp\mcpace-readiness-20260412073651-12ce01b2\manager |
| `portability/smoke` | `pass` | `blocker` | `isolated-manager-root` | Smoke test passed. health=degraded, servers=16, tools=95, repeat-tools=95, reconnect-tools=95, resources=5, templates=0. |
| `portability/workspace-registry` | `pass` | `high` | `isolated-manager-root` | external primary and ro extra resolved correctly |
| `portable-layout/current-root` | `pass` | `high` | `current-host` | required files are present |

## Servers

| Scenario | Verdict | Severity | Scope | Notes |
| --- | --- | --- | --- | --- |
| `servers/optional-default-disabled` | `pass` | `medium` | `current-host` | count=6; problematic=none |
| `servers/optional-source-enabled` | `pass` | `medium` | `current-host` | entries=lean-ctx:True/True |
| `servers/optional-user-enabled/firecrawl` | `pass` | `high` | `isolated-manager-root` | runtime-gated: required placeholder value is missing |
| `servers/optional-user-enabled/git` | `pass` | `high` | `isolated-manager-root` | effectiveEnabled=True; status=connected |
| `servers/optional-user-enabled/github` | `pass` | `high` | `isolated-manager-root` | effectiveEnabled=True; status=connected |
| `servers/optional-user-enabled/screenpipe` | `pass` | `high` | `isolated-manager-root` | effectiveEnabled=True; status=connected |
| `servers/optional-user-enabled/sentry` | `pass` | `high` | `isolated-manager-root` | runtime-gated: oauth approval or tokens are required before the server can be enabled |
| `servers/optional-user-enabled/windows-mcp` | `pass` | `high` | `isolated-manager-root` | runtime-gated: MCP endpoint probe failed: Подключение не установлено, т.к. конечный компьютер отверг запрос на подклю... |
| `servers/required-path` | `pass` | `blocker` | `current-host` | abp=running; connected=filesystem,memory,sequential-thinking,context7,fetch,serena,exa,wireshark-mcp; missing=none |

## Source

| Scenario | Verdict | Severity | Scope | Notes |
| --- | --- | --- | --- | --- |
| `source/pester-suite` | `pass` | `blocker` | `current-host` | exit=0; [95m [95mStarting discovery in 7 files.[0m  [95mDiscovery found 21 tests in 445ms.[0m  [95mRunning test... |

