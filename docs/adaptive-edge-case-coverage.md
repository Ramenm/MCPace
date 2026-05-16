# Adaptive edge-case coverage

This document describes the edge cases that must stay covered by `npm run verify:adaptive-parallelism`.

The adaptive scheduler must start conservative and only increase trust with evidence. It should optimize throughput through isolation, not through blind in-process concurrency.

## Covered edge classes

| Edge class | Expected default | Why |
|---|---|---|
| Unknown stdio package (`npx`, `uvx`, local command, OCI) | `P0_unknown_stdio`, `process-pool`, `maxInFlightPerWorker=1` | Local packages can execute code and may not be thread/process safe. Parallelism comes from isolated workers. |
| Legacy SSE | `PX_legacy_compat`, `legacy-disabled` | Compatibility parsing is allowed, but it must not become the stable Streamable HTTP path. |
| Streamable HTTP remote | `P4_stateless_remote_candidate`, `remote-http-session-pool` | HTTP session/provider/rate-limit boundaries matter more than local process locks. |
| Credential-scoped API | `P2_session_safe`, `credential-session-pool` | Credential subject and token refresh are scheduling boundaries. |
| Filesystem/project server | `P3_project_safe`, `project-pool`, project/file locks | Reads may fan out later; writes must serialize by project/path. |
| Git/repository server | `P3_project_safe`, `project-pool`, repo locks | Parallel across repositories is allowed; conflicting repo mutations serialize. |
| Browser automation | `PX_forbidden_browser_until_context_isolated`, `session-pool` | Browser state must be isolated by BrowserContext/session before parallelism. |
| Shared-exclusive desktop/host state | `PX_forbidden`, `singleton` | Host-global state is not safe to parallelize automatically. |
| Read-only stdio candidate | `P1_readonly_candidate`, `process-pool`, `maxInFlightPerWorker=1` | Tool names/metadata are advisory until safe probes and runtime evidence exist. |

## Non-goals for this source-level matrix

This matrix does not execute third-party MCP packages, make paid API calls, perform browser actions against production sites, or prove Rust runtime compilation. Those are live-host lanes.

## Required invariant

Every classified server must have at least one lock or scheduling domain. Unknown and high-risk servers must never get more than one in-flight request per worker by default.
