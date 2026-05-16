# MCP lifecycle and blast-radius hardening

This document defines the safety envelope for MCP server lifecycle operations. It is intentionally stricter than a happy-path install guide because MCP servers can expose local files, browser automation, databases, cloud APIs, paid providers, and arbitrary tool execution.

## Lifecycle model

MCPace must keep these states distinct:

| State | Meaning | Safe default |
|---|---|---:|
| registered | A JSON fragment exists in `mcp_settings.d/` or another configured source. | Allowed for review. |
| disabled | The entry exists but has `enabled=false`. | Preferred for paid/risky servers. |
| enabled | The entry is eligible for discovery/runtime/client use. | Requires explicit enable. |
| tested | A live smoke probe has been run by `server test`. | Required before trusted use. |
| removed | The target entry was deleted from its source. | Must delete only the target entry. |
| replaced | `--force` overwrote an existing normalized-match entry. | Must be explicit and auditable. |

## Paid and risky server posture

Paid, browser, cloud, database, filesystem, shell, or remote-provider servers should be registered but disabled first. The user/operator should inspect the package or remote domain, credential variables, expected tools, cost model, and side-effect behavior before explicit enable. Tool execution should require explicit consent when it can mutate state, touch paid APIs, read sensitive data, or execute code.

Recommended sequence:

```bash
mcpace server add paid-billing \
  --command npx \
  --arg -y \
  --arg @vendor/paid-billing-mcp \
  --env PAID_BILLING_API_KEY='${PAID_BILLING_API_KEY}' \
  --disabled \
  --json

mcpace server sources --json
mcpace server test paid-billing --refresh --json
mcpace server enable paid-billing --json
```

Do not interpret registration as permission to spend money. Registration is config-only; cost happens later when a launcher downloads/executes a package, a remote MCP endpoint receives requests, or a tool calls a billed API.

## Owned by MCPace vs not owned by MCPace

Owned by MCPace:

- generated config fragments under the MCPace project root;
- the local dashboard and local Streamable HTTP endpoint;
- source inventory, warnings, and verification reports;
- explicit enable/disable/remove state transitions managed by MCPace.

Not owned by MCPace:

- npm/PyPI/Docker package contents fetched by `npx`, `uvx`, Docker, or other launchers;
- upstream MCP server domain behavior;
- paid provider billing semantics;
- OAuth issuer policy and token lifetime;
- downstream API side effects;
- client-specific behavior after a client imports or launches a config entry.

## Supply-chain and package-manager risks

`npx`, `uvx`, Docker, and remote HTTP MCP servers are supply-chain boundaries. A cache miss, unpinned package, typo package, package takeover, registry outage, private-registry override, or `@latest` resolution can change what code runs at runtime. MCPace must not claim package verification unless it has a pinned package, lockfile/provenance proof, checksum/signature proof, or provider-specific verification.

Use pinned versions for production whenever possible. Keep `--disabled` until the package/domain and credentials are reviewed. Keep live tests isolated from production credentials and paid quotas.

## Corrupt source and normalized duplicate rules

A corrupt or unreadable unrelated fragment must not break inventory of every other source. The loader should warn and skip the bad source, then continue with valid sources. A forced replace must remove an existing normalized-match key before inserting the replacement key, otherwise case-only or punctuation-only renames can create duplicate effective server names in one file.

## Required regression checks

`npm run verify:lifecycle-blast-radius` must check:

- paid server registers disabled and does not echo secret values in command output;
- explicit enable and disable transitions preserve state clarity;
- remove dry-run does not mutate files;
- remove deletes only the target entry;
- removed server can be re-added;
- duplicate normalized names are blocked without `--force`;
- source hardening exists for corrupt-fragment isolation;
- source hardening exists for normalized duplicate removal on forced replace;
- docs distinguish local ownership from upstream/package/provider ownership;
- package-manager risk is documented for unpinned launchers.

## Remaining release proof

This source archive can prove source and executable smoke behavior only for the vendored binary it contains. A release gate still needs a Rust host to rebuild the binary, run Rust tests, rerun the lifecycle smoke against the rebuilt artifact, and then regenerate release evidence.
