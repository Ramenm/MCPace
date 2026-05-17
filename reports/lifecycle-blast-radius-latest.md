# Lifecycle and blast-radius smoke

Generated: 2026-05-17T15:28:57.865Z
Status: pass
Project: mcpace 0.6.5

## Checks

| Check | Status | Detail |
|---|---:|---|
| paid-server-registers-disabled-without-output-secret-leak | pass | exit=0 36ms |
| server-enable-is-explicit-state-transition | pass | exit=0 40ms |
| server-disable-is-explicit-state-transition | pass | exit=0 38ms |
| server-remove-dry-run-does-not-delete | pass | exit=0 34ms |
| server-remove-deletes-only-target-entry | pass | exit=0 51ms |
| server-can-be-readded-after-remove | pass | exit=0 42ms |
| normalized-duplicate-without-force-blocked | pass | exit=1 server 'Paid Billing' already exists in /tmp/mcpace-lifecycle-blast-Q9Ul1J/mcp_settings.d/paid-billing.json; rerun with --force to replace it |
| source-force-replace-removes-normalized-duplicate-key | pass | Force replace removes an existing normalized-match key before inserting the replacement key. |
| source-corrupt-settings-fragment-isolated | pass | Registry/source-report loaders skip unreadable JSON sources with warnings instead of failing the entire registry. |
| docs-distinguish-owned-and-upstream-domains | pass | Install docs distinguish local MCPace ownership from upstream package/domain/provider ownership. |
| docs-blast-radius-require-disabled-review-consent | pass | Lifecycle docs require paid/risky servers to be registered disabled, then explicitly enabled/consented. |
| docs-tool-safety-covers-arbitrary-code-execution | pass | Tool safety docs treat MCP tools as arbitrary-code/data-access surfaces with explicit consent. |
| supply-chain-unpinned-launchers-are-documented-risk | pass | 4 launcher presets are unpinned; docs must explicitly call out package-manager risk. |

## Observations

- Executable lifecycle checks exercise the vendored binary available in this source archive; source-only hardening checks cover Rust changes that still need cargo/rustc proof.
- Paid/risky servers should stay disabled through registration and only become active through an explicit enable/consent transition.
- Package-manager launchers such as npx/uvx/docker remain upstream/supply-chain surfaces; this smoke does not execute those packages.
- Corrupt-fragment isolation and normalized duplicate replacement were patched at source level; they are not release-proven until a Rust host rebuilds the binary and runs the Rust lanes.

## Warnings

- This smoke suite does not execute remote paid tools, npx packages, uvx packages, Docker images, or browser automation packages.
- Rust source changes remain source-checked only in this sandbox because cargo/rustc are unavailable here.
- A real release gate must rebuild the vendored binary and rerun this smoke against the rebuilt artifact.

