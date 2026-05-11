# ADR 0008 — Configurable MCP ingress and source registry

## Context

MCPace's product goal is one native MCPace endpoint that can front user-supplied MCP servers and expose a predictable config surface to many client families. The previous implementation already kept packaged upstream defaults empty, but several boundaries were still too fixed for future use:

- client install/export previews advertised a fixed localhost URL;
- `serve start/status` defaulted to a compiled-in port;
- upstream discovery read only the root `mcp_settings.json`;
- HTTP requests without a session header could collapse unrelated clients/chats into the same anonymous upstream affinity key.

The MCP Streamable HTTP transport requires a single endpoint path for POST/GET, recommends local binding for local servers, and defines session behavior around `Mcp-Session-Id` when a server uses stateful sessions.

## Problem / goal

Make the project less hardcoded without pretending to support every MCP transport/runtime path yet. The immediate goal is:

- keep the proven default `http://127.0.0.1:39022/mcp`;
- allow the advertised client URL to come from project config or environment;
- allow additional MCP settings files without recompilation;
- make HTTP session/project affinity easier for different clients, chats, and workspaces;
- keep all changes reversible and test-covered in the source proof lane.

## Constraints and non-goals

- Do not add packaged upstream MCP server recommendations.
- Do not claim remote HTTP upstream forwarding is production-ready; callable forwarding is still stdio-first.
- Do not implement hosted OAuth/relay in this pass.
- Do not do a large Rust module split without a Rust toolchain and runtime trace.

## Considered options

### Option A — Keep current hardcoded local endpoint

Low risk, but it keeps client install/export tied to one localhost URL and makes cloud/public client surfaces awkward.

### Option B — Make endpoint and MCP source registry configurable

Preserves defaults while adding explicit override points through `mcpace.config.json` and env vars.

### Option C — Replace local serve with full hosted relay/session service

More complete for cloud clients, but too large and risky without real-host traces, auth design, and deployment model.

## Decision

Choose Option B.

New configuration surface:

```json
{
  "ports": {
    "serve": 39022
  },
  "serve": {
    "host": "127.0.0.1",
    "port": 39022,
    "mcpPath": "/mcp",
    "publicUrl": ""
  },
  "mcpSettings": {
    "includePaths": []
  }
}
```

Environment overrides:

```text
MCPACE_SERVE_HOST
MCPACE_SERVE_PORT
MCPACE_SERVE_PATH
MCPACE_PUBLIC_MCP_URL
MCPACE_MCP_SETTINGS
```

Merge order for upstream settings:

```text
root mcp_settings.json
→ mcpace.config.json mcpSettings.includePaths
→ MCPACE_MCP_SETTINGS path list
```

Later duplicate server names override earlier entries and emit warnings.

## Consequences / risks

- Client install/export no longer has to be compiled to one URL.
- Source registry can ingest user- or workspace-specific MCP server files.
- Configuring `serve.publicUrl` means the user is responsible for pointing clients at a real reachable MCPace endpoint.
- HTTP upstream forwarding remains stdio-callable only; HTTP upstream config is discoverable/diagnostic but not yet a complete call lane.
- Rust compile/test is still required on a machine with `cargo`.

## Implementation plan

1. Add `runtimepaths::resolve_serve_endpoint` and use it for `serve`, client install, and client export.
2. Add `mcp_sources` registry module and route upstream/server loading through it.
3. Mint `Mcp-Session-Id` on HTTP `initialize` and accept additional client/chat/project affinity headers.
4. Add Node contract tests for configurable ingress, source registry, and session-affinity markers.
5. Keep build/runtime/release proof blocked until Rust and real-host traces are available.

## Open questions

- Should strict `Mcp-Session-Id` enforcement be enabled only after a durable HTTP session store exists?
- Which remote HTTP upstream authentication model should ship first: OAuth resource metadata, static headers, or relay-owned credentials?
- Should client catalogs be split into external JSON bundles by default instead of keeping built-ins compiled into Rust?

### Follow-up: router/probe alignment

Client install/export must never advertise an endpoint path that the local HTTP router does not accept. The unified serve router now accepts the configured `serve.mcpPath` in addition to `/mcp`, and `mcpace setup` probes the resolved MCP path with the required Streamable HTTP `Accept` header.
