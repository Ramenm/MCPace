# ADR 0003 — Keep upstream tools behind explicit wrappers and optimize with pooled sessions

## Status

Accepted for the current local HTTP MCP runtime slice.

## Context

MCPace exposes one local MCP endpoint while discovering many configured upstream
MCP servers. Those upstreams can contain large tool schemas and very different
runtime properties: some are stateless, some are project-local, and some own
stateful interaction, memory, or other mutable state.

The fastest-looking design would be to advertise every upstream tool directly
from the top-level `tools/list`. That would make calls look more native, but it
would also expand every client startup with all upstream schemas, make disabled
or broken servers look callable, and bypass the scheduler/lease diagnostics that
protect stateful servers.

The current implementation instead exposes a small MCPace-native tool surface
and routes upstream stdio servers through explicit wrapper tools:

- `surface_manifest` for the honest native-vs-upstream surface contract;
- `upstream_catalog` for concise discovery;
- `upstream_policy_audit` for annotation/policy review before enabling or
  promoting upstream tools;
- `upstream_policy_suggest` for copyable policy candidates generated from the
  same audit signals without changing enforcement automatically;
- `upstream_tools` for one server's full schema on demand;
- `upstream_call` for one upstream tool call;
- `upstream_batch` for a state-preserving sequence in one initialized upstream
  session.

## Decision

Keep the wrapper-first upstream design as the default architecture.

Do **not** globally advertise every upstream tool name at the top level. Optimize
latency without increasing the default token surface by adding upstream-session
reuse behind the existing wrapper tools and then hardening that into durable
process-pool ownership. The endpoint must still make this explicit through
`surface_manifest` and runtime diagnostics so clients can distinguish native
MCPace management tools from proxied upstream MCP tools.

If direct-looking tools are needed later, add them only as an opt-in promoted
tool layer:

- per configured server/tool allowlist;
- generated from the same upstream schema source;
- still routed through the scheduler lease and pooled-session path;
- disabled by default so normal `tools/list` stays small.

## Rationale

- A small top-level tool list is cheaper for token usage and easier for clients
  to reason about.
- On-demand `upstream_tools` keeps full schemas available without forcing every
  schema into every startup.
- `upstream_batch` is the right current path for stateful sequences
  because it avoids repeated initialize/call/cleanup cycles inside one sequence.
- The missing speed layer is process/session reuse, not global direct passthrough.
- Keeping wrappers preserves MCPace diagnostics, leases, heartbeat loss handling,
  stale-response filtering, and future cancellation hooks.

## Consequences

### Good

- Default client startup remains small and stable.
- Broken, disabled, or non-stdio upstreams remain explicit diagnostics instead
  of appearing as callable top-level tools.
- Session pooling can improve repeated-call latency without changing the public
  wrapper contract.
- Optional promoted tools can be added later without forcing all clients to pay
  the token cost.

### Trade-offs

- `upstream_call` has a small wrapper-argument overhead compared with a direct
  top-level tool call.
- Cold first calls still pay upstream process startup and MCP initialize
  latency; the current pool only helps matching later calls in the same MCPace
  process.
- Clients that only understand direct tools need either wrapper guidance or a
  carefully scoped promoted-tool compatibility layer.

## Follow-up

1. Harden the current bounded in-process pool into durable upstream
   process/session ownership keyed by server, project root, client/session
   affinity, config fingerprint, and credential scope.
2. Attach lease renewal, stale-result guards, and cancellation propagation to
   long-lived pooled sessions, not just request-time wrapper calls.
3. Keep `upstream_batch` as the preferred API for stateful multi-call flows.
4. Add optional promoted direct tools only after pooling is stable and only for
   allowlisted hot tools.
5. Fix slow or timing-out upstreams, such as `serena`, as server-specific
   readiness work rather than expanding the whole tool surface.
