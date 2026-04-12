# Summary

## Current state

The repository has been normalized into a source-first layout:

- source templates stay placeholder-only, while the normal local flow bootstraps ignored auth state automatically
- only safe managed optional integrations are source-enabled by default; secret-backed and host-specific ones stay opt-in
- generated launchers resolve the bearer token from env override or local auth state
- runtime state stays under generated directories only
- governance files, tests, CI, and release manifest are present

## What is now true

- `mcp_settings.json` is a source template, not a local state dump
- `check.ps1` prints placeholder-only client config output
- `auth.ps1` provides explicit local credential recovery via `-Show` and `-Reset`
- `validate-readiness.ps1` has a bounded overall duration
- `build-release.ps1` creates a portable release bundle from tracked source
- Pester tests cover source policy, runtime security, packaging, and governance baseline

## What is still not proven

- Windows runtime smoke in CI
- macOS runtime support
- clean-host end-to-end provisioning on every supported platform

Current verification source of truth:

- scenario catalog: `docs/verification-matrix.md`
- latest local harness audit: `reports/verification-latest.md`
- latest local harness JSON: `reports/verification-latest.json`

## Recommended verification path

```bash
pwsh ./verify-manager.ps1
pwsh ./check.ps1
pwsh ./smoke-test.ps1
pwsh ./validate-readiness.ps1
pwsh -NoProfile -Command "Invoke-Pester -CI -Path ./tests"
pwsh ./build-release.ps1
```
