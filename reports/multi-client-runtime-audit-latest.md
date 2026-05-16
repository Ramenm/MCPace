# Multi-client runtime audit

Generated: 2026-05-16T15:05:08.440Z

Status: **pass**

Project: mcpace v0.6.5

Default upstream pool max: 8
Default upstream shard max: 4

| Check | Severity | Result | Evidence |
|---|---:|---:|---|
| http-streamable-session-id-is-generated-and-required | critical | pass | src/dashboard/http_session.rs generates OS-random MCP HTTP session ids and rejects missing stateful session headers. |
| http-upstream-context-keeps-client-session-project-identity | critical | pass | src/dashboard/tool_runtime.rs builds upstream lease context from MCP/forwarded headers, metadata, and project roots. |
| upstream-pool-shards-by-client-session-project-transport | high | pass | src/dashboard.rs hashes server, client id, session id, project root, and transport when selecting an upstream pool shard. |
| default-upstream-pool-allows-bounded-multiclient-distribution | high | pass | src/resources.rs AUTO_UPSTREAM_SESSION_POOL_MAX=8, AUTO_UPSTREAM_SESSION_SHARD_MAX=4. |
| stdio-fallback-limit-is-visible-not-silent | high | pass | src/client/context.rs derives stable planned leases but warns when no external session/conversation/client-instance/transport-session id exists. |
| hub-leases-block-conflicting-client-work | critical | pass | src/hub/leases.rs enforces request mutex/capacity lanes and same-session takeover rules. |
| playwright-covers-parallel-independent-client-contexts | medium | pass | Playwright lane uses separate BrowserContexts, parallel worker config, and recorded conflict evidence. |
| package-scripts-wire-multiclient-audit-into-experience | medium | pass | package.json exposes verify:multi-client-runtime and includes it in browser/experience verification. |
| docs-explain-automatic-versus-required-client-identity | medium | pass | Docs distinguish HTTP automatic sessioning from stdio/client metadata requirements and Playwright context isolation. |

## Accepted limits / not proven here

- **stdio-clients-without-any-session-signal** (medium, accepted-limit): MCPace can derive a stable planned lease, but it cannot prove two same-client/same-project stdio processes are separate unless the client supplies a session/conversation/client-instance/transport-session signal.
- **source-audit-not-live-rust-concurrency-proof** (medium, not-proven-here): This audit confirms source contracts and browser E2E wiring. Rust runtime parallel throughput still needs cargo build/test and live-host concurrency measurement.

## Files reviewed

- src/resources.rs
- src/dashboard.rs
- src/dashboard/http_session.rs
- src/dashboard/mcp_http.rs
- src/dashboard/tool_runtime.rs
- src/client/context.rs
- src/hub/leases.rs
- tests/e2e/dashboard.parallel.playwright.spec.mjs
- tests/e2e/playwright.config.mjs
- scripts/playwright-dashboard-e2e.mjs
- docs/universal-runtime-policy.md
- docs/browser-e2e-and-external-tooling.md

