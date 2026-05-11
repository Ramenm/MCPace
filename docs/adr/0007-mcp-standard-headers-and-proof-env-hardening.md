# ADR 0007: Compatibility validation for MCP standard headers and proof child environments

## Context

MCPace exposes a local Streamable HTTP endpoint at `/mcp` and uses Node-based
source/release proof scripts to launch archive, package, Docker, npm, and Rust
verification helpers. The repository keeps source proof, build proof, runtime
proof, and release proof separate.

The active stable MCP baseline is still documented as 2025-11-25 in this repo,
while the MCP draft and SEP-2243 describe future Streamable HTTP request headers
(`Mcp-Method`, `Mcp-Name`) mirrored from the JSON-RPC body. Older clients may not
send those headers yet.

## Problem / goal

Two boundary risks were visible:

1. A client or intermediary could send `Mcp-Method` / `Mcp-Name` headers that do
   not match the JSON-RPC body. If different components trusted different
   sources of truth, that would create routing or request-smuggling ambiguity.
2. Several proof scripts spawned child processes with the full parent
   environment. In sandboxed or CI environments that environment can include
   registry credentials, package-index credentials, or agent/runtime context that
   source proof jobs do not need.

## Constraints and non-goals

- Preserve compatibility with clients that do not yet send `Mcp-Method` and
  `Mcp-Name`.
- Do not claim full compliance with the draft standard-header requirement until
  MCPace intentionally moves to a strict draft/later protocol mode.
- Do not remove explicit npm publish credentials from the publish script; limit
  them to publish-specific child commands.
- Do not attempt a broad runtime architecture rewrite without Rust build proof
  and real-host MCP client traces.

## Considered options

### A. Mismatch-only header validation and shared safe child-env helper

Reject `Mcp-Method` / `Mcp-Name` only when the client sends them and they differ
from the JSON-RPC body. Centralize source-proof child environment allowlisting in
`scripts/lib/safe-child-env.mjs`; keep the CommonJS test helper aligned.

### B. Strict draft header mode immediately

Require `Mcp-Method` for every POST and `Mcp-Name` for `tools/call`,
`resources/read`, and `prompts/get` immediately.

### C. Leave headers and child environments unchanged

Treat both issues as documentation-only concerns until runtime proof is complete.

## Chosen solution

Choose option A.

It removes the header/body split-brain risk without breaking older clients, and
it reduces credential exposure in proof subprocesses without preventing explicit
publish credentials where they are actually needed.

## Consequences / risks

- Old clients that omit the standard headers continue to work.
- Clients that send conflicting `Mcp-Method` or `Mcp-Name` now receive
  `400 Bad Request`.
- Full strict header compliance remains a future migration.
- Child processes may no longer inherit incidental environment variables. Any
  future verification script that truly needs an extra variable must add it
  deliberately to the allowlist or pass it as an explicit override.

## Plan of implementation

1. Add `validate_mcp_standard_headers` and `mcp_standard_header_name` in
   `src/dashboard.rs`.
2. Add route tests for mismatched `Mcp-Method` and `Mcp-Name`.
3. Add `scripts/lib/safe-child-env.mjs` and use it from proof/package/Rust helper
   scripts.
4. Keep npm publish credentials scoped to `scripts/publish-npm-artifacts.mjs`.
5. Add Node security-contract checks for both protections.
6. Update API/spec, test-strategy, verification, security, and memory-bank docs.

## Open questions

- When MCPace should move from mismatch-only compatibility to strict standard
  header enforcement.
- Whether strict mode should be protocol-version gated, config gated, or both.
- Whether future `Mcp-Param-*` / `x-mcp-header` support belongs inside MCPace or
  should remain delegated to upstream MCP servers.
