# Architecture

MCPace is not primarily “another MCP gateway”. It is a local MCP process scheduler for concurrent AI agents.

```text
AI client -> http://127.0.0.1:39022/mcp -> MCPace scheduler -> upstream MCP server instance
```

The gateway part gives every client one stable local URL. The scheduler part decides whether an upstream MCP server should be shared, serialized, cloned per session, cloned per project, or served from a worker pool.

## Why this niche matters

Many useful local MCP servers are stdio processes. They are easy to start, but they often keep implicit process state such as `current_project`, `last_file`, `auth_context`, caches, browser profile, repository root, or database transaction context. Two AI chats calling the same process at the same time can accidentally mix state.

MCPace treats concurrency as a first-class runtime policy:

```text
chat=A -> filesystem#9a21bc30
chat=B -> filesystem#7df48211
```

The trace must answer: which chat/client/project got which server instance, lease, queue, or pool worker?

## Dynamic discovery boundary

MCPace has two discovery loops:

- **source discovery**: load whatever servers already exist in local MCP settings files and infer a safe policy;
- **catalog discovery**: search approved catalogs or a cached registry response and convert metadata into a reviewable install plan;
- **one-command auto setup**: `mcpace auto` refreshes the registry cache when needed, adds only approved/trusted catalog entries up to `maxAutoInstallsPerRun`, then probes them.

This boundary is deliberate. Discovery, install planning, and trusted setup can be automatic, but execution of a random public package cannot be silent. New unknown servers should become candidates, plans, probes, and policy suggestions first; they become runnable servers only after trust policy, a local approved catalog, or explicit operator approval allows it.

## Three planes

### 1. Control plane

The control plane owns configuration and policy:

- `mcpace.config.json` for defaults, server policy, UI surface, and catalog settings;
- `mcp_settings.json` and `mcp_settings.d/*.json` for upstream server definitions;
- a local approved-server catalog for trust/review metadata;
- permission manifests and tool policies for higher-risk stdio servers;
- CLI commands such as `server set-policy`, `server instances`, and `server leases`.

### 2. Runtime plane

The runtime plane owns execution:

- server discovery and profile inference;
- leases for exclusive or affinity-bound access;
- scheduler lanes for shared, serialized, session-isolated, project-isolated, and pool modes;
- stdio process launch and remote Streamable HTTP forwarding;
- session pools for reusing safe workers;
- audit logging around actual upstream tool calls.

The default posture is conservative: unknown local stdio should run as serialized until explicit evidence or operator policy proves it can be shared or pooled.

### 3. UI and observability plane

The UI surface must be local-first and lightweight:

- show health and configured servers;
- show planned instances and runtime leases;
- show the current concurrency map;
- show audit trail entries for tool calls and batches;
- show warnings when a server is unknown, disabled, single-writer, or missing policy.

The current HTTP dashboard is the right first surface. A desktop tray should stay a thin launcher/status wrapper later, not a second control system.

## Concurrency modes

| Mode | Use when | Behavior |
| --- | --- | --- |
| `shared` | The server is stateless or externally synchronized. | Reuse one upstream route with parallel calls. |
| `serialized` | The server may have shared process state. | One in-flight call at a time; safe but slower. |
| `session-isolated` | State belongs to a chat/client/session. | Clone or reuse one worker per affinity key. |
| `project-isolated` | State belongs to a repo/worktree/project. | Clone or reuse one worker per project key. |
| `pool` | The server is stateless but expensive to start. | Keep multiple workers and route to least-busy/sticky workers. |
| `disabled` | The server is unsafe, broken, or pending review. | Do not route calls. |

## Runtime state classes

Execution mode is the scheduler decision; runtime type is the evidence label behind that decision. MCPace records three fields for every server:

| Field | Examples | Purpose |
| --- | --- | --- |
| `runtimeType` | `stateless`, `stateful`, `external`, `interactive`, `side-effecting`, `legacy`, `unknown` | Broad dashboard/search label. |
| `stateClass` | `stateless`, `session-stateful`, `project-stateful`, `credential-stateful`, `remote-session-stateful`, `host-stateful`, `unknown-conservative` | Tells the scheduler which affinity key matters. |
| `effectClass` | `read-only`, `external-read`, `ephemeral-state`, `project-mutating`, `external-mutating`, `host-mutating`, `process-exec`, `unknown` | Tells policy/audit whether calls are read-mostly, mutating, or host-control. |

The classifier uses conservative layers: source/registry hints first, configured policy overrides second, then live `initialize`/`tools/list` and tool annotations after probe. Unknown stdio servers do not become shared just because their package installed; they stay `unknown-conservative` or stateful until evidence proves otherwise.

## Killer demo

Use a tiny stdio MCP server with mutable in-memory fields:

```text
current_project
last_file
auth_context
```

Run two chats at once. Without MCPace, the state interleaves. With MCPace:

- `serialized` prevents corruption but queues work;
- `session-isolated` prevents corruption and runs chats in parallel;
- `pool` scales safe stateless servers.

This demo explains the product better than “one endpoint”, because it shows the pain MCPace actually removes: process/session conflicts.

### Runtime classification guardrails

The runtime plane is conservative by default: unknown stdio servers become lease/session/project-bound until live evidence proves they are safe to share. Auto-classification is token-based rather than naive substring-based, so code-hosting APIs such as GitHub are not confused with local `git` workers, and short destructive tokens such as `rm` only count as standalone tool/command tokens.
