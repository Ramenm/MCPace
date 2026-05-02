# ADR 0011: Keep MCP HTTP route and source-registry logic modular

## Context

MCPace is evolving from source-proof infrastructure into a local MCP broker. The HTTP `/mcp` path now carries transport checks, session headers, standard MCP header validation, tool definitions, and upstream call routing. The MCP source registry also combines root files, fragment directories, configured include paths/dirs, and environment-provided sources.

Earlier implementations placed too much of this behavior in large root modules, which made future session and remote-upstream work harder to review.

## Decision

Keep root modules as orchestration layers and move focused behavior into child modules:

- Dashboard HTTP boundary:
  - `dashboard/mcp_http.rs`
  - `dashboard/http_boundary.rs`
  - `dashboard/http_headers.rs`
  - `dashboard/http_session.rs`
  - `dashboard/http_tools.rs`
  - `dashboard/tool_runtime.rs`
- MCP settings registry:
  - `mcp_sources/paths.rs`
  - `mcp_sources/write.rs`
  - `mcp_sources/write_helpers.rs`
- Upstream runtime:
  - `upstream/inventory.rs`
  - `upstream/lease_runtime.rs`
  - `upstream/stdio_runtime.rs`
  - `upstream/tool_cache.rs`
  - `upstream/policy_audit.rs`
  - `upstream/policy_suggestions.rs`
  - `upstream/diagnostics.rs`
  - `upstream/projection.rs`
  - `upstream/source_type.rs`
  - `upstream/process_config.rs`
  - `upstream/server_config.rs`
  - `upstream/session_pool.rs`.
- Adapter/client readability:
  - `adapter/discovery.rs` for tool/prompt/resource discovery helpers.
  - `client/actions/render_models.rs` for client export/install/restore render models.

## Non-goals

- Do not implement the durable HTTP session store in this ADR.
- Do not implement remote Streamable HTTP upstream forwarding in this ADR.
- Do not keep splitting purely for line count after source audit reports zero production large-module warnings. Future splits should be behavior-driven and gated by Cargo check/test/build when they touch runtime behavior.

## Consequences

- `src/dashboard.rs`, `src/upstream.rs`, `src/adapter.rs`, and `src/client/actions.rs` are below the current large-module audit warning threshold.
- MCP HTTP behavior is easier to harden without touching UI overview or socket worker code.
- Source-registry writes and path discovery are easier to review independently.
- Large modules remain tracked by source audit. After this pass, no production Rust file exceeds the current large-module warning threshold.

## Verification

- `cargo fmt --all -- --check`.
- `npm test`.
- Source-quality contract for dashboard modularization and zero production large-module warnings.
- `node scripts/audit-source.mjs --json --write reports/source-audit-latest.json`.
