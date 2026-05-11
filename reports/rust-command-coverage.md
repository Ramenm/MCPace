# Rust Command Coverage

- legacy shell entrypoints removed from repo: **yes**
- source-level native command surfaces today: **40**
- grouped commands implemented now: **`client`, `hub`, `init`, `lab`, `release`, `repair`, `server`, `verify`**
- planned grouped commands still remaining: **0**
- partially unimplemented grouped sub-surfaces: **1** (`client export` remains preview-only for blocked/public lanes)
- bridge-only commands remaining: **0**
- explicit not-yet-implemented commands/capabilities: **persistent upstream process-pool/session ownership, config-writing client export for blocked/public lanes**

| Surface | Status | Notes |
|---|---|---|
| `version` | native-rust | reads version from `mcpace.config.json` |
| `doctor` | native-rust | host/source diagnostics without PowerShell |
| `dashboard` | native-rust | local web dashboard over native JSON read paths and safe hub actions |
| `serve` | native-rust | foreground one-port localhost HTTP surface for UI, health, and MCP ingress |
| `serve start` | native-rust | starts the one-port HTTP surface in the background |
| `serve status` | native-rust | reports background serve lifecycle and URL state |
| `serve stop` | native-rust | stops the background serve lifecycle cleanly |
| `init` | native-rust | seeds runtime layout and reports readiness |
| `hub up` | native-rust | starts the local file-backed hub loop |
| `hub down` | native-rust | requests a clean hub stop and reports final health |
| `hub status` | native-rust | reports lifecycle state, readiness, warnings, and repair recommendations |
| `hub repair` | native-rust | archives corrupt runtime files and rewrites a clean stopped baseline |
| `hub logs` | native-rust | reads structured hub event logs with bounded single-archive rotation |
| `hub lease list` | native-rust | lists/prunes active runtime leases from the file-backed lease store |
| `hub lease acquire` | native-rust | grants or blocks scheduler leases using planner-derived mutex/capacity/project/state-profile/host keys |
| `hub lease renew` | native-rust | extends an active lease TTL and records a renewal timestamp |
| `hub lease release` | native-rust | releases an active scheduler lease by id |
| `stdio-shim --json` | native-rust-bootstrap | normalizes client/session/project context, ensures the hub is up, and reports a sticky lease plus adapter preview without forwarding live MCP traffic |
| `mcp-server` | native-rust-bootstrap | internal compatibility lane for stdio bootstrap/fallback flows with lease-gated and heartbeat-renewed upstream wrapper calls |
| `profile show --json` | native-rust-read-only | legacy settings file may still influence active profile resolution |
| `projects list --json` | native-rust-read-only | registry inspection only |
| `candidates --json` | native-rust | candidate catalog read path |
| `client list` | native-rust-read-only | verified/generic client target catalog inspection |
| `client plan` | native-rust-read-only | resolves client/session/project context, derived leases, and server arbitration plan |
| `client install` | native-rust | config-writing MCPace-owned block patcher for the catalog-declared local install surfaces (`mcpace client list --json` / `installSupport`) with dry-run/diff previews and restoreable backups |
| `client restore` | native-rust | restores the latest or named backup created by config-writing `client install` |
| `client export` | native-rust-bootstrap | preview-only adapter contract with blockers and next actions; does not yet patch blocked/public client configs |
| `lab list` | native-rust-read-only | runtime fixture inventory |
| `lab matrix` | native-rust-read-only | readiness counts by suite/category/proof layer |
| `lab coverage` | native-rust-read-only | coverage slices for signals, client archetypes, policies, and checks |
| `lab gaps` | native-rust-read-only | prioritized capability gap report from runtime fixtures |
| `lab report` | native-rust-read-only | combined readiness summary, blocked scenarios, and recommended next steps |
| `lab show` | native-rust-read-only | scenario drill-down with outstanding capability details |
| `server list` | native-rust-read-only | combines `mcpace.config.json` and `mcp_settings.json` plus profile overrides |
| `server capabilities` | native-rust-read-only | capability/installer/platform inspection with effective enablement |
| `server candidates` | native-rust-read-only | grouped wrapper over candidate catalog |
| `verify doctor` | native-rust-read-only | grouped wrapper over doctor report |
| `verify readiness` | native-rust-read-only | profile-aware readiness summary for required/runtime-enabled servers and host prerequisites |
| `repair` | native-rust | grouped shorthand for `hub repair` on the public CLI surface |
| `release build` | native-rust | wraps the local source release artifact builder and proof bundle without publishing to npm/GitHub |
