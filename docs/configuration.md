# Configuration

Common files:

| Path | Purpose |
| --- | --- |
| `mcpace.config.json` | Runtime defaults, scheduling policy, UI surface, and include paths. |
| `mcp_settings.json` | Root MCP server settings. |
| `mcp_settings.d/*.json` | Per-server fragments written by install/import/up flows. |
| `catalog/approved-servers.json` | Local review catalog for trusted, approved, blocked, or review-only servers. |
| `manifests/*.permissions.json` | Optional permission hints for risky servers. |

Personal upstream MCP server definitions such as `browser`, `playwright`,
`context7`, or `time` belong in user-scoped MCP settings (for example
`MCPACE_MCP_SETTINGS`, `MCPACE_MCP_SETTINGS_DIRS`, or user-owned
`mcp_settings.d/*.json` fragments). The repository `mcpace.config.json`
should not ship those personal upstreams; its `servers` object is only for
optional scheduling/policy overrides when a server is already present in the
merged user/project MCP settings registry.

Clients should point at MCPace itself, not at each upstream server. For example,
Codex and Cursor only need `http://127.0.0.1:39022/mcp`; MCPace then loads
upstreams from the merged settings sources.

`mcpace up` installs or repairs user-level persistence by default. On Windows,
Linux, and macOS it immediately hands the first runtime to that user supervisor
and waits for a healthy endpoint; the next `serve start` is only a status check,
not a competing detached owner. A same-configuration `mcpace serve restart`
keeps that supervisor ownership. Use `mcpace up --no-autostart` only when a
session-only runtime is intentional.

- **Windows:** the current-user Run entry is visible as `MCPace Agent` and
  points at `mcpace-agent-launcher.exe`, a GUI-subsystem sidecar next to
  `mcpace.exe`. The launcher reads the validated per-user plan, starts without a
  terminal, and restarts non-zero agent exits with bounded backoff. It also
  hydrates persistent MCPace path settings such as `MCPACE_MCP_SETTINGS` from
  the user/machine registry.
- **Ubuntu/Linux:** MCPace enables `~/.config/systemd/user/mcpace-agent.service`.
  The unit uses `Restart=on-failure`, does not require a desktop session, and
  starts with the user's systemd manager. Boot-before-login additionally
  requires user lingering; ordinary desktop/server login does not.
- **macOS:** MCPace bootstraps a user LaunchAgent immediately, with
  keep-alive-on-failure behavior. Stop, restart, repair, and disable operations
  use `launchctl` to preserve one launchd-owned runtime and unload it before
  removing the plist.

CLI commands fall back to the installed Windows autostart plan when no
`--root`, `MCPACE_ROOT`, or current-directory root is available, so commands
such as `mcpace serve restart` can work from a normal home-directory shell after
installation. WSL is a special case: a Linux user service cannot start the WSL
virtual machine after Windows reboot; Windows must start the distribution first.

## Config-first import

```bash
mcpace server import ./mcp.json --dry-run
mcpace server import ./mcp.json --force
mcpace server sources --json
mcpace up
```

Supported input shapes:

```json
{ "mcpServers": { "local": { "command": "npx", "args": ["-y", "pkg"] } } }
```

```json
{ "servers": { "localGateway": { "serverUrl": "http://127.0.0.1:8010/mcp" } } }
```

Normalization rules:

| Input | Normalized result |
| --- | --- |
| `command` | `type: "stdio"` |
| `url`, `serverUrl`, `httpUrl`, `endpoint` | `type: "streamable-http"` plus `url` |
| `transport: "command"` or `"stdio"` | stdio server |
| `transport: "http"` or `"remote"` | Streamable HTTP server |
| `disabled: true` | `enabled: false` |
| MCPace self-entry | skipped to avoid loops |

Direct callable upstreams are stdio and Streamable HTTP. Remote HTTPS endpoints use TLS with the operating system certificate verifier and support configured headers such as `Authorization`; redirects are not followed so credentials cannot cross origins. Header values may use existing `${ENV_NAME}` expansion (quote them in the shell when adding a server). MCPace does not run upstream OAuth/PKCE authorization flows yet, so OAuth-only endpoints require an OAuth-capable stdio adapter. Plain HTTP remains restricted to exact loopback IP addresses or `localhost`.

## Detecting newly added servers

MCPace re-enumerates `mcp_settings.json`, sorted `mcp_settings.d/*.json` fragments, configured include paths/directories, and explicit environment sources whenever it builds the server registry. Adding, changing, disabling, or removing a fragment therefore does not require an MCPace restart; the content fingerprint also invalidates stale upstream tool caches. Broker tools such as `upstream_search`, `upstream_tools`, and `upstream_call` see the new registry on the next request.

MCPace deliberately advertises `tools.listChanged: false` because its HTTP transport does not maintain an unsolicited notification stream. The default broker tool surface stays stable. Clients using optional native/hybrid projected tools must request `tools/list` again or reconnect before those projected names appear.

## Install examples

```bash
mcpace install npm:@modelcontextprotocol/server-memory --as memory --dry-run
mcpace install pypi:mcp-server-demo --as demo --dry-run
mcpace install oci:ghcr.io/example/mcp-server --as container-demo --dry-run
mcpace install http://127.0.0.1:8010/mcp --as local-gateway --dry-run
mcpace install . --as filesystem --dry-run
mcpace install -- npx -y @modelcontextprotocol/server-memory
```

Use `--dry-run` before writing and `--as <name>` to choose a stable server name.

## Automatic policy inference

MCPace applies conservative policy before manual overrides:

- imported source-only servers are classified from transport, command, URL, args, config flags, and profile hints;
- declared servers without a full policy inherit generic source inference;
- explicit policy fields win one by one;
- unproven stdio remains conservative until metadata, tool policy, or safe MCP probe evidence improves confidence.

Typical defaults:

| Server signal | Default policy | Why |
| --- | --- | --- |
| `filesystem` | project/session isolated | File scope is project/worktree-bound. |
| `memory`, `context`, `sequential-thinking` | session isolated | Mutable chat context should not bleed. |
| `git`, worktree, repo tools | project single-writer | Repository mutation needs a conflict domain. |
| `sqlite`, SQL/database tools | database/project single-writer | Writes need serialization. |
| `fetch`, web/API read tools | budgeted multi-reader | Usually read-mostly, still resource-limited. |
| `time`, calculator, read-only utilities | multi-reader | Stateless local tools can share. |
| browser/desktop/shell/process tools | serialized or isolated | Host state and side effects are fragile. |
| unknown | unknown-conservative | Safe default until reviewed or probed. |

Classification fields exposed in JSON and dashboard views:

| Field | Common values |
| --- | --- |
| `runtimeType` | `stateless`, `stateful`, `external`, `interactive`, `side-effecting`, `legacy`, `unknown` |
| `stateClass` | `stateless`, `session-stateful`, `project-stateful`, `credential-stateful`, `remote-session-stateful`, `host-stateful`, `unknown-conservative` |
| `effectClass` | `read-only`, `external-read`, `ephemeral-state`, `project-mutating`, `external-mutating`, `host-mutating`, `process-exec`, `unknown` |

## Dynamic server discovery

User path:

```bash
mcpace auto --dry-run
mcpace auto
mcpace auto filesystem --json
```

Config block:

```json
{
  "dynamicDiscovery": {
    "enabled": true,
    "mode": "auto",
    "registryEndpoints": ["https://registry.modelcontextprotocol.io"],
    "catalogPaths": ["./catalog/approved-servers.json"],
    "registryCachePath": "./catalog/registry-cache.json",
    "autoRefreshRegistry": true,
    "registryCacheTtlHours": 24,
    "autoInstall": "trusted-only",
    "installUnknown": "plan-only",
    "maxAutoInstallsPerRun": 4,
    "probeAfterInstall": true,
    "defaultCommand": "auto"
  }
}
```

`mcpace auto --dry-run` does not launch external packages. Package download/execution happens through the configured launcher only after the trust gate passes. The embedded starter entries use exact package versions; automatic discovery skips candidates whose launcher is unavailable, and a completed install returns failure when its mandatory live probe fails instead of reporting a broken server as ready.

Advanced/debug commands:

```bash
mcpace server discover filesystem --json
mcpace server discover --auto
mcpace server discover --refresh --json
mcpace server test <name> --refresh --json
```

## Execution policy

```bash
mcpace server set-policy filesystem --mode session-isolated --affinity client,project,chat
mcpace server set-policy git --mode project-isolated --affinity client,project
mcpace server set-policy fetch --mode pool --max-workers 4 --queue-timeout-ms 5000
mcpace server set-policy shell --mode serialized --reuse-policy sticky
```

This writes a human-readable `execution` block and the canonical scheduler `policy` block inside `mcpace.config.json`.

## Inspect runtime routing

```bash
mcpace server list --json
mcpace server instances --client-id cursor --session-id chat-a --project-root .
mcpace server leases --json
mcpace dashboard
```

`server instances` is a planning view: it shows how MCPace would route the current client/session/project before a live conflict happens.

## UI surface

```json
{
  "uiSurface": {
    "enabled": true,
    "surface": "dashboard-http",
    "localOnly": true,
    "refreshIntervalMs": 15000,
    "showConcurrencyMap": true,
    "showAuditTrail": true,
    "auditTail": 60
  }
}
```

Keep the dashboard as the primary surface while MCPace is local-first. A desktop tray should only launch/status-wrap the same `/api/overview` data later.

## Approved catalog

```json
{
  "servers": {
    "filesystem": {
      "trustLevel": "review",
      "recommendedMode": "session-isolated",
      "permissionManifest": "manifests/filesystem.permissions.json"
    }
  }
}
```

Use the catalog for review decisions, recommended execution modes, permission hints, and notes. The binary embeds the small curated starter catalog so a fresh npm/native installation can resolve common names without repository files. Any configured local catalog has higher precedence and can replace or block an embedded/registry candidate. Unknown official-registry entries remain review-only, regardless of publisher-supplied trust fields or a custom cache filename.

Official Registry package versions, fixed runtime/package arguments, and required environment/argument/header metadata are preserved. Unknown package managers and custom package registry bases are not silently reinterpreted as public npm packages. Live `tools/list` follows bounded pagination in one MCP session, rejects repeated cursors and malformed/duplicate tools, and caches only a complete validated catalog.

## Options

| Option | Purpose |
| --- | --- |
| `--as <name>` | Set server name. |
| `--path <path>` | Add path scopes for servers that need them. |
| `--env KEY=VALUE` | Add environment variables. |
| `--header KEY=VALUE` | Add HTTP headers for remote servers. |
| `--settings <path>` | Write to a specific MCP settings file. |
| `--dry-run` | Preview without writing. |
| `--force` | Replace an existing fragment. |
| `--disabled` | Write the server as disabled. |

## Runtime classification guardrails

`runtimeType=unknown` and `stateClass=unknown-conservative` are safe failure modes. New servers can still be discovered and probed, but they do not become shared concurrent workers until MCPace has metadata, tool annotations, live MCP surface evidence, or explicit policy. Single-writer and project-isolated servers advertise `maxWorkers=1`; scaling comes from separate project/session/process partitions, not parallel calls into one fragile worker.
