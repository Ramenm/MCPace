# Security notes

MCPace's built-in HTTP listener is loopback-only. Keep it on `127.0.0.1` (or `::1`) and use the documented trusted HTTPS reverse-proxy/tunnel pattern for remote access; bearer authentication and exact same-authority Origin checks remain required at that boundary.

Security rules to keep visible in docs and defaults:

- configured upstream MCP servers are local extensions and should be trusted before use;
- unknown public packages must not be executed silently by discovery;
- `mcpace auto` may install only approved/trusted candidates by default;
- safe probes may run `initialize` and `tools/list`, but must not call upstream tools;
- logs and diagnostics should redact tokens, API keys, passwords, private keys, bearer values, and authorization headers;
- user-specific settings belong outside the repository, for example through `MCPACE_MCP_SETTINGS`.

Use [`../SECURITY.md`](../SECURITY.md) for vulnerability reporting and supported-version policy.

## Transport contract gate

`npm run check:mcp-transport` statically guards MCP stdout/stderr framing, Streamable HTTP header requirements, session lifecycle, dashboard Host/Origin checks, and dashboard security response headers. This is a source-review gate; live protocol conformance still belongs on the Rust-enabled release host.

`npm run check:rust-boundaries` locks the typed-error migration seams and the raw HTTP/TCP allowlist so local protocol/security boundary ownership cannot silently drift.

`npm run check:endgame` aggregates MCP transport, Rust boundary contracts, release-readiness, Rust live proof, supply-chain evidence, modernization budget, and source archive hygiene into one release-facing status report. Use `npm run check:endgame:enforce` only on a release host with the pinned Rust toolchain installed.
