# MCPace

`MCPace` is a source-first PowerShell launcher for a single local MCP entrypoint backed by:

- `agent-browser-protocol` (ABP) on the host
- `MCPace` in Docker

This repository is the source of truth. Runtime state, logs, generated launchers, effective settings, and release archives are generated locally and must not be committed.

## Repository model

- `mcpace.config.json` is the runtime topology contract.
- `mcp_settings.json` is a source template only.
- `data/runtime/mcp_settings.local-overrides.json` stores local per-server overrides that survive restarts.
- `data/runtime/mcp_settings.effective.json` is generated at runtime after env expansion and workspace transforms.
- `logs/`, `data/`, `backups/`, `dist/`, `mcpace.cmd`, and `mcpace.sh` are generated artifacts.

Security baseline:

- first run bootstraps local auth state automatically under ignored runtime paths.
- `MCPACE_BEARER_TOKEN` and `MCPACE_ADMIN_PASSWORD_BCRYPT` are supported as explicit overrides, not required for the normal local first run.
- `check.ps1` prints launcher-based client config output and never prints a usable bearer token.
- Only safe optional integrations with managed zero-touch prerequisites may be source-enabled by default. Secret-backed, OAuth-gated, or host-specific integrations remain opt-in in `mcp_settings.json`.

## Requirements

Required:

- Docker Engine or Docker Desktop
- Node.js 18+
- PowerShell 7 (`pwsh`)

Windows PowerShell 5.1 is not a supported runtime shell for this project.

Optional:

- `MCPACE_BEARER_TOKEN` to override the locally bootstrapped bearer token
- `MCPACE_ADMIN_PASSWORD_BCRYPT` to override the locally bootstrapped admin password hash
- `uvx` for host-side bridges such as `windows-mcp`
- `GITHUB_PERSONAL_ACCESS_TOKEN` if `github` is explicitly enabled
- `FIRECRAWL_API_KEY` if `firecrawl` is explicitly enabled
- an interactive Windows desktop session if `windows-mcp` is explicitly enabled

## Install

Default local flow does not require pre-setting auth env vars. On first run the launcher creates ignored local auth state automatically.

If you want deterministic overrides for CI or a specific machine, copy `.env.example` into your own local env management flow and set the auth variables explicitly.

Examples:

```bash
export MCPACE_BEARER_TOKEN='replace-with-random-token'
export MCPACE_ADMIN_PASSWORD_BCRYPT='$2b$10$replace-with-your-bcrypt-hash'
```

```powershell
$env:MCPACE_BEARER_TOKEN = 'replace-with-random-token'
$env:MCPACE_ADMIN_PASSWORD_BCRYPT = '$2b$10$replace-with-your-bcrypt-hash'
```

## Run

Recommended lifecycle:

```bash
pwsh ./start.ps1
```

Cross-platform wrapper:

```bash
sh ./manager.sh verify -Profile standard
```

Windows note:

- if the host blocks unsigned scripts, run the same commands via `pwsh -NoProfile -ExecutionPolicy Bypass -File .\start.ps1` or unblock the repo files first;
- the same rule applies to `check.ps1`, `smoke-test.ps1`, `validate-readiness.ps1`, and local Pester runs.
- the repeatable audit entrypoint is `pwsh -NoProfile -ExecutionPolicy Bypass -File .\verify-manager.ps1`.

```bash
pwsh ./check.ps1
```

Show current local web credentials and bearer token:

```bash
pwsh ./auth.ps1 -Show
```

Regenerate local auth state and apply it to the stack:

```bash
pwsh ./auth.ps1 -Reset
```

```bash
pwsh ./smoke-test.ps1
```

Bounded readiness validation:

```bash
pwsh ./validate-readiness.ps1
```

Generate a VS Code / Cursor client profile:

```bash
pwsh ./setup-mcp-clients.ps1 -Overwrite
```

Build a portable release bundle:

```bash
pwsh ./build-release.ps1
```

## Configuration

Public client access stays single-entrypoint:

- `client -> launcher wrapper -> mcp-remote -> MCPace -> per-server transport`

Host-only bridges remain separate:

- `launcher -> host bridge startup/preflight -> MCPace connects to host bridge`

Operational rules:

- all clients connect only to `MCPace`
- generic/manual clients should use `mcpace.cmd` or `mcpace.sh`
- `browser` stays part of the required path
- `windows-mcp` stays Windows-only and opt-in
- safe managed optional integrations such as `lean-ctx` may be enabled in source config by default
- secret-backed, OAuth-gated, and host-specific integrations remain opt-in
- local per-server `enabled` and `oauth` state survives restarts via runtime-only overrides under `data/runtime/`
- runtime enablement and local auth resolution happen in generated effective settings, not by mutating the source template

## Workspaces

The launcher distinguishes:

- `manager root`: the repository or release bundle root
- `primary workspace`: the main project root for cwd-sensitive tools
- `extra workspaces`: explicit additional mounts

Container paths:

- `/workspace` for the primary workspace compatibility alias
- `/workspaces/<name>` for canonical workspace mounts
- `/app/data` for manager-owned runtime data

## Supported checks

Source validation:

- `pwsh -NoProfile -Command "Invoke-Pester -CI -Path ./tests"`
- GitHub Actions `ci.yml` on `ubuntu-latest` and `windows-latest`

Runtime validation:

- local `boot.ps1`, `check.ps1`, `smoke-test.ps1`
- bounded `validate-readiness.ps1`
- full JSON + Markdown audit via `verify-manager.ps1`
- manual GitHub Actions workflow `runtime-smoke-ubuntu.yml`
- manual GitHub Actions workflow `runtime-smoke-windows.yml`
- manual macOS gate documented in `docs/runtime-smoke-macos.md`

Support matrix:

- Windows source validation: proven in CI
- Ubuntu source validation: proven in CI
- Ubuntu runtime smoke: manual workflow available
- Windows runtime smoke: manual workflow available
- macOS runtime support: manual gate documented in `docs/runtime-smoke-macos.md`

Only claims backed by the checks above should be treated as supported.

## Source policy

The following must never be committed:

- real bearer tokens
- real admin password hashes used outside local testing
- OAuth transient state such as `pendingAuthorization`
- generated effective settings
- logs, backups, runtime PIDs, server-install state

If a change affects release contents, update `release-manifest.json`.

## GitHub Repo Kit

This repository also carries a repo-local GitHub preparation kit for future reuse:

- `skills/github-project-prepare/` for baseline repo setup and git hygiene
- `skills/github-project-repair/` for CI/governance/runtime drift repair
- `skills/github-project-cleanup/` for explicit opt-in cleanup or deslop work
- `.github/pull_request_template.md` plus issue forms for `repair` and `cleanup`

Rule:

- cleanup work is not implicit; it should run only on an explicit user request or a dedicated cleanup issue
- local OMX runtime state under `/.omx/` is disposable and must stay ignored

## Project files

- `lib/` runtime and helper modules
- `tests/` Pester test suite
- `skills/` repo-local reusable agent skills for GitHub/git workflows
- `docs/` design notes and contract docs
- `reports/` point-in-time audit documents
- `.github/workflows/` CI and manual runtime workflows

Documentation starting points:

- `docs/operating-contract.md` fixes what the project does, where each layer runs, and how the lifecycle is supposed to behave
- `docs/recovery-runbook.md` documents operator recovery paths and known Windows/runtime traps
- `docs/verification-matrix.md` is the normative scenario catalog for the repeatable verification harness
- `docs/github-repo-kit.md` documents the local GitHub/git skill pack and governance surface
- `reports/stability-audit.md` records the currently verified baseline and reproduced failure modes
- `reports/verification-latest.md` and `reports/verification-latest.json` are the latest harness-produced local audit artifacts
- `manager.sh` and `manager.cmd` provide command-style entrypoints over the PowerShell scripts

## License

Apache-2.0. See [LICENSE](LICENSE).
