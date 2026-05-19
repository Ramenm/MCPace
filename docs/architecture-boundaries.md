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

6. **HTTP MCP boundary (`src/dashboard/*`)**
   - `dashboard.rs` owns listener orchestration and route selection.
   - `dashboard/mcp_http.rs` owns MCP HTTP route semantics and JSON-RPC dispatch.
   - `dashboard/http_boundary.rs`, `http_headers.rs`, and `http_session.rs` own Origin/Accept, standard MCP header checks, and session-id shaping.
   - `dashboard/http_tools.rs` and `tool_runtime.rs` own HTTP tool definitions and execution bridge.

7. **MCP source registry (`src/mcp_sources.rs`, `src/mcp_sources/*`)**
   - Owns root `mcp_settings.json`, `mcp_settings.d/*.json`, configured include paths/dirs, and environment-provided settings sources.
   - Root config filenames are defaults; runtime wording should refer to the merged MCP settings registry when more than one source can be active.

8. **Upstream runtime (`src/upstream.rs`, `src/upstream/*`)**
   - Owns stdio upstream probing/calling, tool-list cache, inventory, policy audit, lease attachment, source-type inference, process env shaping, and diagnostics redaction.
   - Remaining upstream root logic is tracked as P1 refactor debt until Rust compile/test/build is green.


9. **Client catalog boundary (`src/client_catalog/*`)**
   - `src/client_catalog/builtin.rs` owns static built-in client defaults.
   - `src/client_catalog.rs` owns catalog types, external registry parsing, merge behavior, and selector resolution.

10. **stdio MCP argument parsing (`src/mcp_server/args.rs`)**
   - Owns process argv parsing and help output for the stdio MCP server surface.
   - Keeps `src/mcp_server.rs` focused on JSON-RPC/MCP lifecycle and command bridging.

## Extension rules

- Add new MCP protocol versions and JSON-RPC codes in `mcp_protocol.rs`, not inside adapters.
- Keep HTTP MCP route semantics in `src/dashboard/mcp_http.rs`; do not grow `src/dashboard.rs` back into a monolithic router.
- Keep merged MCP source discovery in `src/mcp_sources/*`; avoid reintroducing root-only `mcp_settings.json` loaders.
- Add new tools by extending `TOOL_SPECS`, `tool_definition`, and `execute_tool` in `mcp_server.rs`.
- Reuse the command bridge helpers in `mcp_server.rs` instead of hand-rolling `app::run` buffers.
- Preserve request/notification semantics: requests receive exactly one response; notifications receive none.
- Keep command modules usable directly from the CLI before exposing them through MCP.
- Keep static built-in client target defaults in `src/client_catalog/builtin.rs`; do not mix generated/default catalog data with registry merge behavior.
- Keep stdio MCP argv parsing in `src/mcp_server/args.rs`; do not mix process CLI parsing with JSON-RPC request handling.
- Keep useful-MCP auto-install planning in `src/mcp_autoinstall.rs` and the command wrapper in `src/server/install.rs`; do not mix package discovery or profiling into the generic configured-server renderer.

## Machine-checked boundaries

`npm run audit:source` currently guards the most important small boundaries:

- **Protocol primitives stay transport and command agnostic.** `src/mcp_protocol.rs` may define JSON-RPC/MCP envelopes, errors, and protocol helpers, but it must not spawn commands, open sockets, call the CLI router, or depend on runtime state modules.
- **Resource defaults stay side-effect free.** `src/resources.rs` may calculate limits and limiter state, but it must not shell out, own network sockets, or read/write project state.
- **HTTP adapter errors remain structured.** `src/dashboard.rs` routes go through a handler/error-boundary split so internal route failures return JSON `500` responses instead of silent connection closes.
- **Dashboard root stays modular.** A source-quality contract checks that `src/dashboard.rs` keeps MCP HTTP route, session, header, tool definition, and tool runtime logic in child modules.

These checks are intentionally narrow. If a new boundary cannot be described as a deterministic source invariant, document it as a warning/backlog item rather than a critical CI blocker.

## v0.5.5 adapter boundary detail

The adapter layer is now split into smaller behavior boundaries:

- `src/adapter.rs` owns public adapter types, tool exposure options, management-surface shaping, projection orchestration, and environment-derived defaults.
- `src/adapter/discovery.rs` owns upstream search, route planning, prompt/resource discovery, compact result shaping, and projection helper utilities that are explicitly shared with the adapter root using `pub(super)`.
- `src/adapter/profile.rs` owns `adapter_profile` and client capability summaries derived from `initialize` input instead of hardcoded client maps.
- `src/adapter/proxy_uri.rs` owns encoded MCPace proxy URIs for upstream resources and templates.

Do not move client-specific branching into this layer unless it is derived from MCP `initialize` capabilities or explicit config. MCPace should stay a broker over a merged MCP settings registry, not a hardcoded map of clients or upstream server catalogs.

## Runtime state and cache lifecycle

The lifecycle contract for durable config, recoverable state, disposable cache, process-local sessions, restart behavior, and reinstall behavior is maintained in [`runtime-state-cache-lifecycle.md`](runtime-state-cache-lifecycle.md). Changes to critical runtime modules must preserve that contract or update it with matching tests.


## Lifecycle hardening

See `runtime-state-cache-lifecycle.md` for storage classes and `system-lifecycle-hardening.md` for the full install-to-uninstall hardening contract.
