# Multi-client runtime isolation

This note records what MCPace handles automatically and what still requires a
client or operator signal when several MCP clients, browser tabs, or sessions run
at the same time.

## What is automatic

- Streamable HTTP sessions are server-issued by MCPace. After initialization the
  client must echo the `Mcp-Session-Id`/`MCP-Session-Id` header, and MCPace keeps
  that value in the upstream lease context.
- HTTP upstream routing includes client id, session id, project root, transport,
  and metadata where those signals exist.
- The hub lease scheduler enforces request mutex keys, capacity keys,
  `parallelismLimit`, host locks, project roots, and same-session takeover rules.
- Upstream session pools are sharded by server/client/session/project/transport
  so different client sessions do not all have to wait on one pool mutex by
  default.
- Playwright E2E covers both same-session tabs and independent client sessions.
  Independent clients use separate `browser.newContext()` calls and run in
  parallel workers.

## What is not fully automatic

MCPace cannot invent a strictly unique identity for a stdio client that provides
no session, conversation, client-instance, or transport-session signal. In that
case it derives a stable planned lease from the client id, project/cwd, and
transport so routing remains deterministic, but two live windows of the same
client in the same project can share that derived lease.

For strict stdio multi-client isolation, pass one of these signals:

```bash
mcpace mcp serve --session-id <unique-session>
MCPACE_SESSION_ID=<unique-session>
MCPACE_CLIENT_INSTANCE_ID=<unique-client-instance>
MCPACE_TRANSPORT_SESSION_ID=<unique-transport-session>
```

A client that sends `_meta.com.mcpace/context` metadata with `sessionId`,
`conversationId`, `clientInstanceId`, `transportSessionId`, and `projectRoot`
gets stronger routing than a generic stdio process with no metadata.

## Upstream pool sharding

The source default is intentionally bounded, not unlimited. On multi-core hosts
MCPace now allows several upstream session pool shards by default while keeping a
small cap so a misconfigured host does not spawn unbounded upstream processes.
Operators can tune these values:

```bash
MCPACE_UPSTREAM_SESSION_POOL_LIMIT=16
MCPACE_UPSTREAM_SESSION_POOL_SHARDS=8
```

Use host-specific values only after measuring with real upstream servers. The
source-level audit checks that the default is not a single global shard on every
host.

## Verification commands

```bash
npm run verify:multi-client-runtime
npm run verify:browser-experience
npm run verify:playwright-e2e
```

The source audit is cheap and deterministic. The Playwright lane is heavier: it
uses a temporary test-tool install, real Chromium when available, separate
browser contexts for independent clients, and multiple workers.

## Remaining live-host proof

This source pass does not replace:

- `cargo check/test/clippy/build` on a Rust host;
- full Playwright against a live compiled Rust dashboard HTTP server;
- real MCP client traces from Claude Desktop, Cursor, Windsurf, Codex, or other
  clients that may or may not send strong session metadata;
- live third-party MCP server execution.
