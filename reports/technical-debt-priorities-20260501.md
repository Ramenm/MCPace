# Technical debt priorities — v0.5.5

## Scope

This report focuses on the current source tree after the native upstream smoke, hardcode sweep, and module-boundary split. It intentionally separates verified debt from runtime work that is still only a planned capability.

## Confirmed debt

| Priority | Category | Item | Evidence | Risk if ignored | Effort | Recommendation |
|---|---|---|---|---|---|---|
| High | Verification | Full Rust check/test/build not confirmed | `cargo fmt --all -- --check` passes, but `cargo check --all-targets --locked` is blocked by crates.io DNS/dependency access while resolving `auto-launch`; offline check is blocked because `getrandom` is not cached | Rust compile/runtime regressions can survive Node source contracts | Medium | Run Cargo lanes on a host with dependency access or a populated Cargo cache before any runtime/beta claim |
| High | Runtime proof | Real client -> MCPace -> upstream stdio trace missing | Reports still mark runtime proof blocked | Product readiness may be overstated despite source-level improvements | Medium | Record a trace for initialize/initialized/tools/list/tools/call with one real stdio MCP server |
| Medium | Product runtime | Durable HTTP session store not implemented | `/mcp` currently has compatible session-id mint/echo and affinity headers, but no server-side session map/expiry/DELETE lifecycle | Different clients/chats/sessions remain only partially isolated at the HTTP transport layer | High | Implement session map with protocol/client/project/expiry/DELETE semantics behind a compatibility flag first |
| Medium | Product runtime | Remote Streamable HTTP upstream connector not implemented | Remote URL entries are registry/inventory entries, not a callable forwarding path | “Any MCP from any source” remains stdio-first | High | Add connector trait for stdio/http, with SSRF/auth/token/SSE/session controls |
| Medium | Architecture | Behavior-driven split work remains after Cargo is green | Source audit now reports zero production large-module warnings, but `adapter`, `client actions`, leases, and catalog files are still high-complexity areas | Future feature work may re-couple discovery/projection/install/session concerns | Medium | Do not split further by line count; split only when a behavior boundary is being changed and Cargo check/test is available |
| Low/Medium | Documentation drift | Historical docs/reports still include default `127.0.0.1:39022` examples | grep finds default endpoint examples in tests, historical smoke reports, and guides | Users may miss that endpoint is configurable if they read old reports first | Low | Keep default examples, but prefer resolved/configurable wording in current README/docs |

## What was improved in v0.5.5

- Dashboard, MCP stdio tool surface, upstream runtime, adapter discovery, and client action helper code were split into focused child modules.
- Source audit now reports zero production large-module warnings and zero production unwrap findings.
- Extracted Rust test modules are classified as tests by source audit.
- `mcpace server test` adds a native upstream `initialize`/`tools/list` smoke before clients are wired.
- Client-facing endpoint guidance uses the runtime path resolver rather than a compiled-in URL.

## Recommended order

1. Get a full Cargo check/test/build pass on a host with dependency access.
2. Record one real runtime trace through `/mcp` and one stdio upstream tool.
3. Implement durable HTTP session store and strict-session mode behind config.
4. Implement remote Streamable HTTP upstream forwarding behind explicit SSRF/auth guardrails.
5. Continue behavior-driven module splits only when touching that behavior and after Rust compile is green.
