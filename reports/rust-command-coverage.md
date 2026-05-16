# Rust command coverage

Version: `0.6.5`
Legacy shell entrypoints removed: `true`
Native Rust command count: `59`
Planned command groups: `none`
Bridge-only commands: `0`

## Native Rust commands

- `version`
- `doctor`
- `setup`
- `service`
- `service install`
- `service status`
- `service uninstall`
- `service print`
- `dashboard`
- `serve`
- `serve start`
- `serve restart`
- `serve status`
- `serve stop`
- `init`
- `hub up`
- `hub down`
- `hub status`
- `hub repair`
- `hub logs`
- `hub lease list`
- `hub lease acquire`
- `hub lease renew`
- `hub lease release`
- `stdio-shim`
- `mcp-server`
- `profile show`
- `projects list`
- `candidates`
- `connect`
- `client list`
- `client plan`
- `client install`
- `client restore`
- `client export`
- `lab list`
- `lab matrix`
- `lab coverage`
- `lab gaps`
- `lab report`
- `lab show`
- `server list`
- `server capabilities`
- `server add`
- `server import`
- `server enable`
- `server disable`
- `server remove`
- `server sources`
- `server test`
- `server starter`
- `server install`
- `server presets`
- `server candidates`
- `verify doctor`
- `verify readiness`
- `repair`
- `release build`
- `update check`

## Implemented read-only / preview notes

| Area | Note |
|---|---|
| `client` | list + plan + config-writing install + install restore are implemented; export stays preview-only for blocked/public lanes until config-writing support expands |
| `connect` | client-first read-only wiring guide that resolves the MCPace endpoint, upstream sources, readiness blockers, selected client target, and exact next commands without mutating configs |
| `dashboard` | local web control surface over native JSON read paths and safe lifecycle actions |
| `hub` | local lifecycle/state/log/repair plus file-backed runtime lease acquire/renew/release/list surface; explicit upstream wrapper calls now acquire/heartbeat-renew/release scheduler leases, while persistent process-pool/session ownership is still pending |
| `init` | bootstrap only; seeds local state layout and reports readiness |
| `lab` | fixture inventory, readiness matrix, gap report, and scenario drill-down |
| `mcp-server` | internal MCP compatibility surface for stdio bootstrap/fallback flows with lease-gated and heartbeat-renewed upstream wrapper calls |
| `profile` | mutation not implemented yet |
| `projects` | scan/mutation not implemented yet |
| `release` | local release artifact/proof bundle wrapper only; builds artifacts and checksums without publishing to npm or GitHub |
| `serve` | public one-port localhost HTTP surface with start/stop/status lifecycle commands |
| `server` | list, capabilities, source inventory, data-driven preset listing/install/starter, candidate delegation, BYO MCP fragment creation/import with dry-run/force support, enable/disable toggles, removal, and live stdio smoke tests |
| `service` | user-level autostart install/status/uninstall/print via the auto-launch crate; uses the current executable path instead of requiring mcpace in PATH |
| `setup` | one-command local bootstrap that starts serve, installs supported local client config entries, verifies readiness, and smokes /healthz plus /mcp |
| `stdio-shim` | bootstrap-only proof surface; normalizes context and ensures hub up but does not forward live MCP stdio traffic |
| `update` | safe update-check guidance only; reports package-manager commands and never self-updates the running binary |
| `verify` | readiness/doctor only |
