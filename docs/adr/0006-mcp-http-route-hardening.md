# ADR 0006: Harden local MCP HTTP route semantics

## Context

MCPace exposes a local MCP endpoint at `/mcp` through the unified `serve` surface.
The repository targets the MCP 2025-11-25 baseline and the local transport is
documented as localhost-first.

## Problem / goal

The previous route behavior did not fully encode the security/protocol posture
that the docs already expected:

- invalid `Origin` handling for `/mcp` could degrade into a generic MCP parse
  error on POST;
- GET `/mcp` with `Accept: text/event-stream` returned `405` but did not
  advertise the supported method;
- POST `/mcp` accepted requests without validating that the Streamable HTTP
  `Accept` contract was present.

## Constraints and non-goals

- Keep the local source patch small and reversible.
- Do not introduce authentication or stateful HTTP sessions in this ADR.
- Do not claim full runtime proof without Rust/toolchain and real-host evidence.
- Preserve the current stateless initialize/tools/list behavior.

## Considered options

### A. Minimal route hardening

Validate `Origin` at route entry for GET and POST, return `403` for invalid
origins, add `Allow: POST` for unsupported SSE GET, and reject POST requests
that lack both `application/json` and `text/event-stream` in `Accept`.

### B. Full stateful Streamable HTTP session implementation

Mint and store `MCP-Session-Id`, require it on subsequent requests, add DELETE
session termination, and bind `MCP-Protocol-Version` to negotiated sessions.

### C. Leave behavior unchanged and only document it as preview

Avoid code changes and keep `/mcp` as a loose compatibility endpoint.

## Chosen solution

Choose option A.

It closes the highest-signal route correctness/security gaps without committing
to a larger session-store design. Option B remains the right next step once the
runtime ownership model is ready. Option C is too weak because the project
already claims a local endpoint and has tests around it.

## Consequences / risks

- Clients that omit the required Streamable HTTP `Accept` values now receive
  `400 Bad Request`.
- Invalid web origins now receive `403 Forbidden` before MCP JSON-RPC handling.
- Rust compilation still must be verified on a host with the pinned Rust
  toolchain.

## Plan of implementation

1. Reject forbidden origins in GET `/mcp` and POST `/mcp` route handling.
2. Add `Allow: POST` for unsupported SSE GET.
3. Add POST `Accept` validation for `application/json` and `text/event-stream`.
4. Add integration-style Rust tests for cross-origin and missing-Accept cases.
5. Run Node/source contract suite in this sandbox.
6. Run Rust fmt/clippy/tests on a Rust-enabled host.

## Open questions

- Whether MCPace should mint `MCP-Session-Id` for HTTP clients.
- Whether session ownership should be stateless, in-memory, or persisted.
- Which real tier-1 client trace should become the first release-blocking
  runtime proof.
