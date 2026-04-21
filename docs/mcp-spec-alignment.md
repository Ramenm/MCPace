# MCP Spec Alignment

## Baseline

MCPace currently targets the MCP specification revision **2025-11-25**.

## What matters most for the current hub design

1. `stdio` is a subprocess transport. The client launches the server, writes MCP
   messages to `stdin`, and reads MCP messages from `stdout`.
2. `Streamable HTTP` can handle multiple client connections, optional SSE
   streaming, and stateful session management through `Mcp-Session-Id`.
3. MCP is stateful: initialization, operation, and shutdown are distinct phases.
4. roots are delivered as file URIs and should be treated as routing/context
   hints rather than the sole security boundary.
5. long-running requests should respect timeout, cancellation, and progress
   semantics.
6. `tasks` exist in the current spec, but they are experimental and should not
   be part of the first correctness claim for MCPace.

## First-wave support obligations

1. `stdio` transport as the local subprocess path.
2. `Streamable HTTP` as the local and later remote HTTP transport path.
3. HTTP security hygiene:
   - validate `Origin`;
   - bind localhost for local-only transports;
   - apply auth where the HTTP lane supports it.
4. For `stdio`, credentials should come from the environment rather than the HTTP
   authorization flow.
5. Keep session identity sticky once a client/server pair has initialized.
6. Use cancellation/progress awareness for long-running requests so the hub can
   recover from hung or abandoned work.

## MCPace policy inferences from the spec

These are **MCPace design decisions**, not claims that every MCP server already
implements them.

- Because `stdio` is a single subprocess stream, MCPace should own the child
  process and arbitrate access rather than letting unrelated clients write to the
  same `stdin` directly.
- Because Streamable HTTP sessions are stateful, MCPace should not merge traffic
  from unrelated logical sessions under one anonymous route key. The runtime
  should mint an internal lease id when the client gives no explicit session id.
- Because cancellation/progress exist, long-running hub work should be designed
  around request leases, deadlines, and cancellation-aware scheduling rather than
  blind fire-and-forget concurrency.
- Because tasks are experimental, first-wave correctness should not depend on a
  task queue API existing everywhere.

## Current implementation boundary

The current Rust CLI proves grouped inspection/readiness surfaces and a client
arbitration planning command.
It does **not** yet prove the full transport/runtime lifecycle.

## Honesty rule

Naming the checked revision is required. Saying only "latest spec" is not good
enough.
