# Operating Contract

## Purpose

`MCPace` is a source-first launcher for one local client-facing MCP endpoint.

It exists to:

- start and supervise `agent-browser-protocol` (ABP) on the host;
- generate effective MCPace settings from source templates plus local runtime state;
- run one Dockerized `MCPace` instance that fans out to the required and enabled optional servers;
- expose a single launcher-based client entrypoint via `mcpace.cmd` / `mcpace.sh`.

It does not exist to:

- act as a multi-tenant service;
- persist runtime state in git;
- expose every upstream server directly to the client;
- guarantee every optional integration on every platform.

## Where Each Layer Runs

### Host layer

Runs on the local workstation:

- `start.ps1`, `boot.ps1`, `check.ps1`, `repair.ps1`, `smoke-test.ps1`, `validate-readiness.ps1`;
- `manager.sh` and `manager.cmd` as cross-platform command wrappers over the PowerShell entrypoints;
- `auth.ps1`, client launcher generation, logs, backups, runtime state;
- `agent-browser-protocol` on the host;
- optional host bridges such as `windows-mcp` when preflight passes.

### Docker layer

Runs inside one container named by `mcpace.config.json -> hub.containerName`:

- `MCPace`;
- required container-backed MCP servers;
- managed optional installs that target the hub container, such as `lean-ctx`.

### Workspace layer

The launcher mounts workspace roots into the hub container:

- primary workspace -> `/workspace` and `/workspaces/<primary-name>`;
- extra workspaces -> `/workspaces/<name>`;
- read-only extra workspaces must stay read-only in Docker policy checks.

### Client layer

Clients should talk only to the generated launcher:

- `mcpace.cmd` on Windows;
- `mcpace.sh` on POSIX shells.

Public client path:

- `client -> launcher -> MCPace -> per-server transport`

Internal path:

- host ABP and optional host bridges are launcher-managed runtime dependencies, not the primary client surface.

## Source Of Truth And Generated State

### Source-controlled inputs

- `mcpace.config.json` is the topology contract.
- `mcp_settings.json` is a source template only.
- `release-manifest.json` defines release bundle contents.
- `lib/` and top-level `*.ps1` scripts define the launcher behavior.

### Generated local state

- `data/runtime/mcp_settings.effective.json`
- `data/runtime/mcp_settings.local-overrides.json`
- `data/server-state/auth-state.json`
- `logs/`
- `backups/`
- generated `mcpace.cmd` / `mcpace.sh`

Generated state is disposable local runtime state and must not be committed.

## Effective Settings Precedence

The manager resolves runtime behavior in this order:

1. source defaults from `mcpace.config.json` and `mcp_settings.json`;
2. local persisted overrides from `data/runtime/mcp_settings.local-overrides.json`;
3. environment expansion for auth and other placeholders;
4. runtime gating for platform, placeholder availability, missing commands, and preflight;
5. generated effective settings written to `data/runtime/mcp_settings.effective.json`.

Interpretation rule:

- `configuredEnabled` answers "what the user or source requested";
- `effectiveEnabled` answers "what the launcher will actually run now";
- `disabled reason` explains why a configured server is still off.

## Lifecycle Contract

### `start.ps1`

Interactive manager UI. Use when you want the dashboard and hotkeys.

Expected result:

- ABP is started or reused;
- host bridges and managed optional installs are reconciled;
- MCPace is started or restarted as needed;
- the terminal remains attached to the dashboard.

### `boot.ps1`

Non-interactive one-shot stack boot.

Expected result:

- exits `0` when ABP and MCPace are ready;
- exits non-zero when boot completes with warnings or fails.

### `check.ps1`

Operational inspection command.

Expected result:

- prints manager root, workspace mounts, host bridge state, runtime server placement, required-path health, launcher path, and auth source;
- exits `0` only when ABP is reachable and MCPace is ready.

### `smoke-test.ps1`

Transport-level verification.

Expected result:

- verifies the MCP handshake and repeat/reconnect behavior against the public MCP endpoint.

### `validate-readiness.ps1`

Full readiness and portability gate.

Expected result:

- validates the live stack in the current manager root;
- validates a copied external manager root with multi-root workspaces;
- validates Docker read-write and read-only mount policy;
- finishes within the configured overall duration budget.

### `build-release.ps1`

Portable bundle builder.

Expected result:

- emits a zip bundle containing only the release manifest contents;
- excludes generated runtime state.

## Required Path

The project is considered operational only when all of the following are true:

- host prerequisites are available;
- ABP is reachable on the configured host port;
- MCPace is healthy on the configured hub port;
- every required MCP server in the current effective settings is connected;
- `check.ps1` exits `0`;
- `smoke-test.ps1` succeeds.

The browser path remains a required runtime dependency for the current launcher design, even though it is not the public client-facing endpoint.

## Stability Invariants

- The repository is the source of truth; runtime state must be regenerated, not hand-edited in source files.
- The public client contract stays single-entrypoint through the launcher.
- Source templates must not contain live secrets or committed OAuth transient state.
- Optional integrations that need secrets, OAuth, or host-specific software stay opt-in unless they have managed zero-touch install and preflight.
- Support claims are valid only when backed by the checks listed in `README.md`, the scenario definitions in [verification-matrix.md](verification-matrix.md), and the current audit reports in `reports/`.

## Recovery Expectations

Normal recovery surface:

- `pwsh ./repair.ps1`
- `sh ./manager.sh verify -Profile standard`
- `pwsh ./auth.ps1 -Show`
- `pwsh ./auth.ps1 -Reset`
- `pwsh ./check.ps1`
- `pwsh ./verify-manager.ps1`

For a fuller operator guide, use [recovery-runbook.md](recovery-runbook.md).
