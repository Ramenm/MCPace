# Configuration

Common files:

- `mcpace.config.json` — runtime defaults, scheduling policy, UI surface, and include paths.
- `mcp_settings.json` — root MCP server settings.
- `mcp_settings.d/*.json` — per-server fragments written by `mcpace install` or `mcpace up` import.
- `catalog/approved-servers.json` — optional local review catalog for known-good or blocked servers.

## Config-first import

```bash
mcpace server import ./mcp.json --dry-run
mcpace server import ./mcp.json --force
mcpace server sources
mcpace up
```

Supported input shapes:

```json
{ "mcpServers": { "local": { "command": "npx", "args": ["-y", "pkg"] } } }
```

```json
{ "servers": { "remote": { "serverUrl": "https://example.com/mcp" } } }
```

## Install examples

```bash
mcpace install npm:@modelcontextprotocol/server-memory --as memory --dry-run
mcpace install pypi:mcp-server-demo --as demo --dry-run
mcpace install https://example.com/mcp --as remote-example --dry-run
mcpace install . --as filesystem --dry-run
mcpace install -- npx -y @modelcontextprotocol/server-memory
```

## Automatic policy inference

MCPace now applies a conservative automatic policy before any manual override is needed:

- source-only servers imported from `mcp_settings.json` or `mcp_settings.d/*.json` are classified from name, command, URL, args, and transport;
- declared servers in `mcpace.config.json` that do not specify a full `policy` inherit the same generic source inference instead of falling back to blank/unknown scheduler fields;
- explicit `policy` fields still win one by one, so users can override only the field they care about.

Typical defaults:

| Server signal | Default scheduling policy | Why |
| --- | --- | --- |
| `filesystem` | `isolated-per-project` | file state is project/worktree-bound |
| `memory`, `context`, `sequential-thinking` | `single-session`/state-profile | mutable chat/session context should not bleed |
| `git`, `worktree`, repository tools | `single-writer` per project | repository mutation should serialize |
| `sqlite`, SQL/database tools | `single-writer` per project/db | database writes need a conflict domain |
| `fetch`, web/API read tools | `multi-reader` with a small limit | usually safe to parallelize but still budgeted |
| `time`, `calculator`, read-only utilities | `multi-reader` | stateless local tools can share safely |
| browser/desktop/shell/process tools | exclusive/serialized | host state and side effects are fragile |
| unknown | conservative `single-writer` | safe default until reviewed |

Each loaded server also gets an explicit classification surface:

| Class field | Common values | How it is inferred |
| --- | --- | --- |
| `runtimeType` | `stateless`, `stateful`, `external`, `interactive`, `side-effecting`, `legacy`, `unknown` | Transport plus source/package/command names and later live probe evidence. |
| `stateClass` | `stateless`, `session-stateful`, `project-stateful`, `credential-stateful`, `remote-session-stateful`, `host-stateful`, `unknown-conservative` | Determines the partition key: none, chat/session, project/worktree, credential, remote session, host profile, or conservative lease. |
| `effectClass` | `read-only`, `external-read`, `ephemeral-state`, `project-mutating`, `external-mutating`, `host-mutating`, `process-exec`, `unknown` | Determines whether calls can be shared, must be serialized, or need host/security review. |

Operators may override `runtimeType`, `stateClass`, and `effectClass` inside a server `policy`, but the normal path is automatic. MCPace intentionally treats unproven stdio servers as conservative until source hints, tool policy, or live MCP probe evidence makes a safer class obvious.

The local approved catalog is still advisory in this bundle; it records review decisions and recommended modes, but it is not yet a hard runtime gate.

## Dynamic server discovery

The user-facing workflow is one command:

```bash
mcpace auto --dry-run       # preview what would be discovered, installed, and probed
mcpace auto                 # refresh stale cache, add approved/trusted servers, probe live tools
mcpace auto filesystem --json
```

MCPace handles new MCP servers without asking the user to choose `stdio` vs `streamable-http` or a concurrency type:

1. **Already configured servers** are loaded dynamically from `mcp_settings.json`, `mcp_settings.d/*.json`, configured include paths, and environment-provided source files.
2. **Not-yet-configured servers** are discovered from the local approved catalog plus the optional MCP Registry cache.
3. **Auto mode** refreshes the registry cache when it is missing or older than `registryCacheTtlHours`, chooses approved/trusted candidates, writes server fragments, and then probes live `initialize`/`tools/list` evidence.
4. **Runtime policy** stays evidence-first: name/command/URL/package metadata give the initial conservative policy, then probe output and tool annotations can tighten/read-only classify the surface.

Unknown public registry packages are not silently executed. They remain plan-only until a local catalog or explicit install policy marks them as review/approved. This keeps the one-command path safe while still making trusted servers automatic.

Advanced/debug equivalents remain available:

```bash
mcpace server discover filesystem --json
mcpace server discover --auto              # same user-facing auto semantics
mcpace server discover --refresh --json    # force registry refresh only
mcpace server discover --auto-install      # legacy alias for the curated sweep
```

The related config block is:

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

Package download/execution is performed by the configured launcher (`npx`, `uvx`, Docker, or remote URL) only after a candidate passes the trust gate. `mcpace auto --dry-run` never launches external packages.

## Execution policy

Use `server set-policy` to describe how an upstream server may run:

```bash
mcpace server set-policy filesystem --mode session-isolated --affinity client,project,chat
mcpace server set-policy git --mode project-isolated --affinity client,project
mcpace server set-policy fetch --mode pool --max-workers 4 --queue-timeout-ms 5000
mcpace server set-policy shell --mode serialized --reuse-policy sticky
```

This writes both a human-readable `execution` block and the canonical scheduler `policy` block inside `mcpace.config.json`:

```json
{
  "servers": {
    "filesystem": {
      "execution": {
        "protocol": "mcpace.execution.v1",
        "mode": "session-isolated",
        "affinity": ["chat", "client", "project"],
        "queueTimeoutMs": 10000,
        "reusePolicy": "sticky-session",
        "maxWorkers": 1,
        "maxInFlightPerWorker": 1
      }
    }
  }
}
```

## Inspect runtime routing

```bash
mcpace server instances --client-id cursor --session-id chat-a --project-root .
mcpace server leases --json
```

`server instances` is a planning view. It tells you how MCPace would route the current client/session/project context before you need to debug a live conflict.

## UI surface

`mcpace.config.json` now includes a lightweight local-first UI section:

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

Keep the dashboard as the primary surface while the product is still local-first. Add a desktop tray later only as a launcher/status shell around the same `/api/overview` data.

## Approved catalog

The local catalog is intentionally small:

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

Use it to record review decisions, recommended execution mode, permission hints, and notes. Do not make it mandatory for personal use; teams can enable strict review by setting `approvedCatalog.requireApprovalForUnknown`.

## Options

- `--as <name>` sets the server name.
- `--path <path>` appends path arguments for servers that need explicit scopes.
- `--env KEY=VALUE` adds environment variables.
- `--header KEY=VALUE` adds HTTP headers for remote servers.
- `--settings <path>` writes to a specific MCP settings file.
- `--dry-run` previews without writing.
- `--force` replaces an existing fragment.
- `--disabled` writes the server as disabled.

### Runtime classification guardrails

The scheduler treats `runtimeType=unknown` and `stateClass=unknown-conservative` as a safe failure mode: new servers can still be discovered and probed, but they do not become shared concurrent workers until MCPace has metadata, tool annotations, or explicit policy evidence.

The classifier intentionally uses token/boundary matching for server names, package names, commands, and live tool metadata. This avoids bad auto-mode decisions such as treating a GitHub API server as a local `git` repository server, or treating an unrelated word containing `rm` as a destructive remove command. Single-writer and project-isolated servers also advertise `maxWorkers=1`; scaling comes from separate project/session/process partitions, not from parallel calls into one fragile worker.
