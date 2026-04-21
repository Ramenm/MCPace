# Rust Command Coverage

- legacy shell entrypoints removed from repo: **yes**
- source-level native command surfaces today: **26**
- grouped commands implemented now: **`client`, `hub`, `init`, `lab`, `repair`, `server`, `verify`**
- planned grouped commands still remaining: **1** (`release`)
- partially unimplemented grouped sub-surfaces: **1** (`client install/export`)
- bridge-only commands remaining: **0**
- explicit not-yet-implemented commands: **client install, live stdio/http forwarding, config-writing client export, release**

| Surface | Status | Notes |
|---|---|---|
| `version` | native-rust | reads version from `mcpace.config.json` |
| `doctor` | native-rust | host/source diagnostics without PowerShell |
| `init` | native-rust | seeds runtime layout and reports readiness |
| `hub up` | native-rust | starts the local file-backed hub loop |
| `hub down` | native-rust | requests a clean hub stop and reports final health |
| `hub status` | native-rust | reports lifecycle state, readiness, warnings, and repair recommendations |
| `hub repair` | native-rust | archives corrupt runtime files and rewrites a clean stopped baseline |
| `hub logs` | native-rust | reads structured hub event logs with bounded single-archive rotation |
| `stdio-shim --json` | native-rust-bootstrap | normalizes client/session/project context, ensures the hub is up, and reports a sticky lease plus adapter preview without forwarding live MCP traffic |
| `profile show --json` | native-rust-read-only | legacy settings file may still influence active profile resolution |
| `projects list --json` | native-rust-read-only | registry inspection only |
| `candidates --json` | native-rust | candidate catalog read path |
| `client list` | native-rust-read-only | verified/generic client target catalog inspection |
| `client plan` | native-rust-read-only | resolves client/session/project context, derived leases, and server arbitration plan |
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
| `client export` | native-rust-bootstrap | preview-only adapter contract with blockers and next actions; does not write client config yet |
| `client install` | planned | onboarding target after ingress/config core |
| `release` | planned | build/release target |
