# Architecture Boundaries

MCPace keeps protocol handling, command orchestration, and runtime state changes in separate layers. This makes the native Rust surface easier to extend without coupling MCP transports to CLI subcommands.

## Layers

1. **CLI router (`src/app.rs`)**
   - Normalizes command aliases.
   - Delegates to command modules.
   - Does not own MCP protocol rules.

2. **Protocol primitives (`src/mcp_protocol.rs`)**
   - Owns MCP protocol version negotiation.
   - Owns JSON-RPC 2.0 response and error envelopes.
   - Distinguishes requests with ids from notifications without ids.

3. **MCP stdio adapter (`src/mcp_server.rs`)**
   - Reads newline-delimited JSON-RPC messages.
   - Performs MCP lifecycle handling and tool dispatch.
   - Converts MCP tool calls into explicit MCPace CLI command vectors.
   - Must return MCP tool results inside a JSON-RPC `result` envelope.

4. **Command modules (`src/{client,hub,server,verify,...}`)**
   - Own command-specific argument parsing, loading, rendering, and side effects.
   - Return JSON when invoked with `--json` so protocol adapters can treat them as stable command contracts.

5. **Runtime state (`src/runtimepaths.rs`, `src/hub/*`)**
   - Own path derivation, hub state files, leases, logs, and repair behavior.
   - Remains transport-agnostic.

## Extension rules

- Add new MCP protocol versions and JSON-RPC codes in `mcp_protocol.rs`, not inside adapters.
- Add new tools by extending `TOOL_SPECS`, `tool_definition`, and `execute_tool` in `mcp_server.rs`.
- Reuse the command bridge helpers in `mcp_server.rs` instead of hand-rolling `app::run` buffers.
- Preserve request/notification semantics: requests receive exactly one response; notifications receive none.
- Keep command modules usable directly from the CLI before exposing them through MCP.
