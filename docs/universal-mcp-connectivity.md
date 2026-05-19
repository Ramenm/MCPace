# Universal MCP connectivity plan

## What is real now

MCPace keeps packaged upstream defaults empty and expects users to bring MCP servers through settings. The current source now accepts upstream MCP server definitions from:

1. `mcp_settings.json` at the project root;
2. per-server fragments in `mcp_settings.d/*.json`;
3. `mcpace.config.json` → `mcpSettings.includePaths` and `mcpSettings.includeDirs`;
4. `MCPACE_MCP_SETTINGS` and `MCPACE_MCP_SETTINGS_DIRS`, using the platform path-list separator.

Client-facing MCPace URL selection is now config-driven:

1. `MCPACE_PUBLIC_MCP_URL`, when a public/relay endpoint must be advertised;
2. `mcpace.config.json` → `serve.publicUrl`;
3. `MCPACE_SERVE_HOST` / `MCPACE_SERVE_PORT` / `MCPACE_SERVE_PATH`;
4. `mcpace.config.json` → `serve.host` / `serve.port` / `serve.mcpPath`;
5. default `http://127.0.0.1:39022/mcp`.

## Session and chat separation

HTTP upstream calls derive affinity from explicit tool arguments, metadata, and request headers. The accepted request headers now include:

```text
Mcp-Session-Id
x-mcp-session-id
x-mcpace-session-id
x-mcpace-conversation-id
x-mcpace-chat-id
x-codex-session-id
x-codex-conversation-id
x-mcp-client-id
x-mcpace-client-id
x-codex-client-id
x-mcpace-project-root
x-mcpace-workspace-root
x-codex-project-root
```

`initialize` responses now return `Mcp-Session-Id` and `MCP-Protocol-Version` headers. MCPace stores the session in a bounded in-process store, requires the session header on later stateful requests, touches active sessions, rejects missing/invalid/unknown/expired/protocol-mismatched sessions, and closes known sessions on `DELETE /mcp`.

## What is still not real

- Full remote HTTP upstream forwarding is not proven. Stdio upstreams remain the callable lane.
- Hosted relay/auth/OAuth is not implemented.
- Cross-process or relay-grade HTTP session persistence is not complete; the implemented store is in-process and local-runtime oriented.
- Rust build/test/runtime proof has not been executed in the current sandbox.

## Target architecture

```text
client surface catalog
        │
        ▼
advertised MCPace URL resolver
        │
        ▼
/mcp Streamable HTTP endpoint
        │
        ├── HTTP session/client/project affinity
        │
        ├── tool/resource/prompt adapter
        │
        └── upstream registry
              ├── root mcp_settings.json
              ├── mcp_settings.d/*.json fragments
              ├── included settings files/directories
              └── env-provided settings files/directories
                    │
                    ├── callable stdio upstreams
                    └── planned remote HTTP upstream lane
```

## Next architectural steps

1. Extend the in-process HTTP session store into cross-process/relay-grade persistence only after the local runtime proof is green.
2. Implement remote HTTP upstream forwarding separately from stdio, including auth/header policy and DNS/SSRF controls.
3. Add a real-host trace suite: client → `/mcp` → `initialize` → `tools/list` → `upstream_call` → upstream stdio server.
4. Move the built-in client catalog to data files once the catalog extension path is stable.


## Route/path compatibility

The local router accepts the configured `serve.mcpPath` as well as the default `/mcp` path. This keeps existing clients working while allowing future relay or reverse-proxy deployments to advertise a non-default path without lying to client install/export.

## v0.5.5 inventory follow-up

Doctor/readiness now uses the same multi-source MCP settings registry as runtime routing. A server added through `mcpSettings.includePaths` or `MCPACE_MCP_SETTINGS` is therefore visible to:

- `upstream.rs` live routing and stdio calls;
- `server/loader.rs` server inventory;
- `doctor.rs` runtime prerequisite discovery.

HTTP session ids are now stateful inside the running MCPace process: MCPace creates, touches, expires, rejects, and closes `Mcp-Session-Id` records. Remaining hardening is cross-process persistence, auth-bound session identity for non-local modes, and broader real-client compatibility traces.


## v0.5.5 convenience pass

MCPace now has two native BYO-MCP convenience commands:

```bash
mcpace server sources --json
mcpace server add filesystem --command npx --arg @modelcontextprotocol/server-filesystem --arg .
mcpace server disable filesystem --dry-run
mcpace server enable filesystem --dry-run
mcpace server remove filesystem --dry-run
```

`server sources` exposes the exact registry that runtime/upstream loading uses. `server add` writes a single-server fragment under `mcp_settings.d/` by default, with `--dry-run`, `--force`, `--settings`, `--env`, `--header`, `--command`, and `--url` support. `server enable` / `server disable` flip the entry without deleting JSON, and `server remove` deletes a matching server from the source where it was discovered, with `--settings` available for explicit source targeting. This reduces root JSON editing and makes future server onboarding/removal more native while keeping the packaged upstream catalog empty.


## v0.5.5 readiness follow-up

A whole-code static pass found a Rust compile-risk drift: `upstream.rs` called `infer_source_type` but did not define the helper locally. The helper is now present and normalizes HTTP aliases such as `streamable-http`, `remote-http`, `sse`, and `url` to the runtime diagnostic class `http`, so remote entries are reported as blocked HTTP upstreams instead of generic missing-command entries while stdio remains the callable lane.

A current project config schema now lives in `schemas/mcpace-config.schema.json` to keep `serve.*` and `mcpSettings.*` from drifting without contract coverage.


## v0.5.5 module split and upstream smoke follow-up

The largest HTTP/MCP runtime roots were split along existing boundaries instead of rewriting behavior: dashboard response/header/session/tool/runtime helpers now live under `src/dashboard/`, MCP stdio tool-surface construction lives under `src/mcp_server/tool_surface.rs`, and extracted test modules live in child `tests.rs` files. Source audit was updated so those extracted tests remain test debt, not production debt.

The native BYO lifecycle is now:

```bash
mcpace server add filesystem --command npx --arg @modelcontextprotocol/server-filesystem --arg .
mcpace server sources --json
mcpace server test filesystem --refresh --json
mcpace server disable filesystem --dry-run
mcpace server enable filesystem --dry-run
mcpace server remove filesystem --dry-run
```

`server test` uses the same upstream probe path as runtime diagnostics, so users can verify a stdio upstream reaches `tools/list` before installing or exporting a client configuration.

## v0.5.5 client-first import follow-up

From a user's point of view, the common case is not always “write a new MCP JSON block”; often they already have an MCP config from another client, repo, or teammate. `mcpace server import --from <path>` now provides a native migration path: it reads an existing `mcpServers` object, preserves each server JSON entry, and writes MCPace-managed fragments under `mcp_settings.d/` by default. Use `--dry-run` first, `--force` only when replacing existing normalized names, and `--settings <target.json>` when importing many entries into one explicit source file.

This does not change the current runtime boundary: stdio upstreams are callable today; remote Streamable HTTP upstreams are still inventory-only until the remote connector, auth isolation, SSRF controls, session mapping, and stream handling are implemented.

## v0.5.5 client-first connect guide

From the user's point of view, separate commands are still too much unless MCPace can tell them what to run next. `mcpace connect` now provides a read-only top-down wiring report:

```bash
mcpace connect
mcpace connect codex
mcpace connect cursor-local --server filesystem --json
```

The report resolves the configured MCPace endpoint, selected client target, merged upstream source inventory, readiness blockers, and exact next commands. It intentionally composes existing read paths (`runtimepaths`, `mcp_sources`, `server` inventory, `client_catalog`, and `verify`) and does not mutate MCP settings or client configs.

This is not a replacement for runtime proof. The next runtime gate is still a real client trace through `/mcp` and a callable stdio upstream.

## v0.6.x automatic useful MCP install pass

Useful MCP onboarding is now package/URL/command-derived instead of static-catalog-first. MCPace does not ship a packaged upstream-server catalog by default. `mcpace server install` derives a reviewable settings fragment from what the user provides:

```bash
mcpace server install npm:@modelcontextprotocol/server-filesystem --as filesystem --path . --dry-run
mcpace server install pypi:mcp-server-git --as git --path . --dry-run
mcpace server install --url https://example.com/mcp --as remote-docs --dry-run
mcpace server install custom-local --command node --arg ./server.js --dry-run
```

After registration, `mcpace server test <name> --refresh --json` collects live stdio initialize/tools-list evidence. The adaptive profile remains conservative until source evidence, safe probes, and optional operator policy hints justify a faster lane. Browser/desktop, shell/process, memory/context, filesystem, git, database, credential/API, and remote HTTP servers are separated by detected evidence and lock domains rather than by a static packaged grouping file.
