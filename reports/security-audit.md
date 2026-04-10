# Security Audit

## Scope

This report covers only the launcher layer, source templates, and the release packaging baseline in this repository.

## Closed issues

1. Hardcoded bearer token removed from source config.
   - `mcp_settings.json -> bearerKeys[].token` now uses `${MCPACE_BEARER_TOKEN}`.

2. Insecure launcher fallback removed.
   - generated `mcpace.cmd` and `mcpace.sh` no longer embed a bearer token and resolve it from env override or ignored local auth state at runtime.

3. Secret disclosure path removed from `check.ps1`.
   - client config output is placeholder-only and no longer embeds a usable bearer value.

4. Committed OAuth transient state removed from source template.
   - `pendingAuthorization` is no longer allowed in `mcp_settings.json`.

5. Static admin password removed from source template.
   - the source template remains placeholder-only; the effective admin password hash now comes from `${MCPACE_ADMIN_PASSWORD_BCRYPT}` or from generated local auth bootstrap state.

## Current security posture

The repository is now aligned with a local trusted-workstation model:

- source secrets are externalized and runtime secrets are generated into ignored local state
- runtime state is generated and ignored
- optional integrations are opt-in
- source policy tests block obvious regressions

## Remaining risks

1. Workspace mounts remain broad by design.
   - If a valid bearer token is exposed, enabled filesystem-capable tools can act on the mounted workspace roots.

2. `browser` remains part of the required path.
   - This is intentional for current runtime behavior, but it keeps the stack coupled to host-side browser automation.

3. Runtime support is not fully CI-proven on every target platform.
   - Windows runtime smoke and macOS support are still not proven in CI.

## Operational guidance

- keep the hub bound to `127.0.0.1`
- use a unique local bearer token per machine
- if you set auth env vars in the shell or machine profile, remember that they override the local bootstrap state
- enable optional integrations only when they are needed
- treat generated runtime files as disposable local state
