# Configuration

Common files:

| Path | Purpose |
|---|---|
| `mcpace.config.json` | Runtime defaults, scheduling policy, UI surface, and include paths. |
| `mcp_settings.json` | Root MCP server settings. |
| `mcp_settings.d/*.json` | Per-server fragments written by install/import/up flows. |
| `catalog/approved-servers.json` | Local review catalog for trusted, approved, blocked, or review-only servers. |
| `manifests/*.permissions.json` | Optional permission hints for risky servers. |

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
{ "servers": { "remote": { "serverUrl": "https://example.com/mcp" } } }
```

Normalization rules:

| Input | Normalized result |
|---|---|
| `command` | `type: "stdio"` |
| `url`, `serverUrl`, `httpUrl`, `endpoint` | `type: "streamable-http"` plus `url` |
| `transport: "command"` or `"stdio"` | stdio server |
| `transport: "http"` or `"remote"` | Streamable HTTP server |
| `disabled: true` | `enabled: false` |
| MCPace self-entry | skipped to avoid loops |

## Install examples

```bash
mcpace install npm:@modelcontextprotocol/server-memory --as memory --dry-run
mcpace install pypi:mcp-server-demo --as demo --dry-run
mcpace install oci:ghcr.io/example/mcp-server --as container-demo --dry-run
mcpace install https://example.com/mcp --as remote-example --dry-run
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
|---|---|---|
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
|---|---|
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

`mcpace auto --dry-run` does not launch external packages. Package download/execution happens through the configured launcher only after the trust gate passes.

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

Use the catalog for review decisions, recommended execution modes, permission hints, and notes. Personal use can stay permissive; teams can require approval for unknown servers.

## Options

| Option | Purpose |
|---|---|
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
