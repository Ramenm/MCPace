# Architecture

MCPace is a local MCP process scheduler for concurrent AI agents, not just another gateway.

```text
AI client -> http://127.0.0.1:39022/mcp -> MCPace scheduler -> upstream MCP server instance
```

The endpoint stays stable. The scheduler decides whether an upstream server should be shared, serialized, cloned per session, cloned per project, served from a pool, or disabled.

## Why the scheduler exists

Many local MCP servers are stdio processes with implicit state: current project, last file, credentials, browser profile, repository root, cache, or transaction context. Two chats using one process can accidentally mix that state.

MCPace treats concurrency as runtime policy:

```text
chat=A -> filesystem#9a21bc30
chat=B -> filesystem#7df48211
```

The trace must answer which chat, client, project, lease, queue, or pool worker handled a request.

## Discovery boundary

MCPace has three discovery loops:

| Loop | Purpose | Safety rule |
|---|---|---|
| Source discovery | Load servers already present in local MCP settings. | Infer a conservative policy. |
| Catalog discovery | Search approved catalogs or cached registry metadata. | Produce an install/review plan. |
| One-command auto setup | Add approved or trusted candidates and probe them. | Never silently execute unknown public packages. |

Unknown servers become candidates, plans, probes, and policy suggestions first. They become runnable only after local trust policy, an approved catalog entry, or explicit operator action allows it.

## Three planes

### Control plane

Owns configuration and policy:

- `mcpace.config.json` for defaults, UI surface, catalog settings, and server policy;
- `mcp_settings.json` plus `mcp_settings.d/*.json` for upstream definitions;
- local approved-server catalog and permission manifests;
- CLI commands such as `server set-policy`, `server instances`, and `server leases`.

### Runtime plane

Owns execution:

- source loading and profile inference;
- leases for exclusive or affinity-bound access;
- scheduler lanes for shared, serialized, session-isolated, project-isolated, and pool modes;
- stdio process launch and Streamable HTTP forwarding;
- audit logging around upstream tool calls.

The default is conservative: unknown stdio runs serialized or isolated until evidence proves it can be shared.

### UI and observability plane

Owns the local operator view:

- health and configured servers;
- planned instances and active leases;
- concurrency map;
- warnings for unknown, disabled, missing-policy, or single-writer servers;
- tool-call audit trail.

The HTTP dashboard is the primary surface. A desktop tray should remain a thin launcher/status wrapper around the same APIs.

## Concurrency modes

| Mode | Use when | Behavior |
|---|---|---|
| `shared` | Server is stateless or externally synchronized. | Reuse one upstream route with parallel calls. |
| `serialized` | Server may have shared process state. | One in-flight call at a time. |
| `session-isolated` | State belongs to chat/client/session. | Reuse one worker per affinity key. |
| `project-isolated` | State belongs to repo/worktree/project. | Reuse one worker per project key. |
| `pool` | Server is stateless but expensive to start. | Keep multiple workers and route to a safe worker. |
| `disabled` | Server is unsafe, broken, or pending review. | Do not route calls. |

## Runtime state classes

Execution mode is the scheduler decision. Runtime classification is the evidence behind it.

| Field | Examples | Purpose |
|---|---|---|
| `runtimeType` | `stateless`, `stateful`, `external`, `interactive`, `side-effecting`, `legacy`, `unknown` | Broad dashboard/search label. |
| `stateClass` | `stateless`, `session-stateful`, `project-stateful`, `credential-stateful`, `remote-session-stateful`, `host-stateful`, `unknown-conservative` | Chooses the affinity or conflict key. |
| `effectClass` | `read-only`, `external-read`, `ephemeral-state`, `project-mutating`, `external-mutating`, `host-mutating`, `process-exec`, `unknown` | Drives audit and sharing risk. |

The classifier uses layered evidence: configured policy, transport/auth shape, package metadata, safe MCP probe output, tool schemas, and runtime observations. Package or server names are weak identity only; they must not widen concurrency by themselves.

## Demonstration scenario

Use a tiny stdio MCP server with mutable fields:

```text
current_project
last_file
auth_context
```

Run two chats at once. Without MCPace, state can interleave. With MCPace, `serialized` prevents corruption, `session-isolated` prevents corruption while keeping chats parallel, and `pool` scales only proven stateless servers.

## Runtime classification guardrails

Unknown stdio servers become lease/session/project-bound until live evidence proves they are safe to share. Classification uses token and boundary matching instead of naive substring matching, so GitHub-style APIs are not treated as local `git` workers and short destructive tokens such as `rm` only count as standalone command/tool tokens.

## Protocol and diagnostics boundaries

Protocol transports keep their output channels strict. The MCP stdio surface writes only JSON-RPC messages to stdout, one message per line, while operator diagnostics go through a shared stderr helper. This prevents human-readable warnings from corrupting stdio clients that parse stdout as protocol frames.


## Client configuration editing boundary

Client install, export, and restore flows centralize MCP server config mutation in `src/config_edit.rs`. That module is the typed edit boundary for JSON, TOML, and YAML client settings: it detects newlines, builds MCPace-managed TOML blocks, updates JSON server entries, updates YAML `mcpServers` entries, and reports `ConfigEditError` variants back to CLI actions.

`src/client/actions/config_update.rs` is intentionally limited to diff rendering and sensitive-value redaction. New client config mutation must go through `src/config_edit.rs` rather than reintroducing format-specific patchers in action modules. Once the Rust lockfile can be refreshed with Cargo, the TOML internals should move behind the same boundary to `toml_edit` for format-preserving edits.

Local setup and serve health checks share `src/http_probe.rs` for bounded HTTP reads, chunked-body decoding, SSE JSON extraction, connect-timeout handling, and wildcard-host normalization. New probe call sites should use this module instead of adding ad-hoc `TcpStream` readers in feature modules. Long-term outbound HTTP should move to a maintained HTTP client, but the current shared probe gives the existing raw-socket path one audited boundary.

HTTP response serialization for the dashboard is also isolated behind `src/dashboard/response.rs`. That module now exposes `ResponseWriteError`/`ResponseWriteResult`, so write failures and header-validation failures stay typed until the outer dashboard boundary renders them for callers. The same boundary pattern now covers runtime paths, client metadata/restore backups, server loading, MCP source write validation, restart guards, tool-result options, lab loading, upstream server config, and update checks. These local enums keep subsystem causes structured until a CLI/API edge renders them, which makes the later `thiserror` migration mechanical instead of another project-wide string rewrite.

The modernization inventory also parses `Result<Ok, Err>` signatures with nested generic depth instead of shallow `Result<[^>]+, String>` matching. That keeps the budget honest: nested return types such as `Result<BTreeMap<String, SourceServerRecord>, String>` now count when they should, while successful values like `Result<BTreeMap<String, String>, E>` do not count as stringly errors.
