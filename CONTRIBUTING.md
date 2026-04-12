# Contributing

## Development baseline

- Use `pwsh` for all project scripts.
- Keep source control clean: never commit `data/`, `logs/`, `backups/`, generated launchers, or release archives.
- Treat `mcp_settings.json` as a source template only. Do not commit runtime OAuth state, effective settings, or local secrets.

## Required local environment

- `MCPACE_BEARER_TOKEN`
- `MCPACE_ADMIN_PASSWORD_BCRYPT`
- Docker
- Node.js 18+
- PowerShell 7

## Common commands

```powershell
pwsh ./check.ps1
pwsh ./smoke-test.ps1
pwsh ./validate-readiness.ps1
pwsh ./build-release.ps1
pwsh -NoProfile -Command "Invoke-Pester -CI -Path ./tests"
```

## Pull request expectations

- Add or update tests for behavior changes.
- Keep public docs aligned with the actual runtime behavior.
- Do not introduce new insecure defaults or hardcoded credentials.
- If a change affects release contents, update `release-manifest.json`.
