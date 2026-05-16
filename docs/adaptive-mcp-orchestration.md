# Adaptive MCP orchestration

This document is the implementation-facing design for MCPace adaptive parallelism. It replaces the old idea of one static `concurrencyPolicy` as the only source of truth with an evidence-backed server profile and worker plan.

## Design goal

MCPace should maximize throughput through isolation, not blind trust. Unknown servers start conservative. Servers get more concurrency only after static metadata, safe probes, and runtime evidence support it. Any security, cost, credential, state-leak, or crash signal can only downgrade concurrency until an operator or later evidence raises confidence again.

## Stable baseline

MCPace treats the current stable transports as:

- `stdio`
- `streamable-http`

Legacy SSE is treated as `sse-legacy` and is not automatically parallelized. It can be recognized for compatibility, but it is not a default scheduler model and must not be silently merged with Streamable HTTP behavior.

## Server profile fields

Every server should resolve to a profile with:

- `parallelSafetyClass`
- `defaultPoolModel`
- `maxWorkers`
- `maxInFlightPerWorker`
- `transportStatus`
- `launcherKind`
- `lockDomains`
- `profileEvidence`

The first profile can be static. The final runtime decision must come from evidence plus policy.

## Safety classes

| Class | Meaning | Default behavior |
|---|---|---|
| `P0_unknown` | No positive evidence | singleton or isolated worker with `maxInFlightPerWorker=1` |
| `P0_unknown_stdio` | Local command/stdio without proof | process-pool is allowed, but each worker handles one in-flight request |
| `P1_readonly_candidate` | Looks read-heavy/read-only but not proven | bounded process pool, safe probes required |
| `P2_session_safe` | Session identity is meaningful | session-affine pool |
| `P3_project_safe` | Project/worktree/resource identity is meaningful | project pool with file/repo locks |
| `P4_stateless_remote_candidate` | Remote/stateless-looking API | HTTP/session pool with rate and credential limits |
| `PX_forbidden*` | High-risk, browser, destructive, or policy-blocked | explicit consent/policy gate |
| `PX_legacy_compat` | Legacy transport | disabled for auto-parallelism |

## Pool models

| Model | Purpose |
|---|---|
| `singleton` | One safe lane only |
| `process-pool` | Multiple isolated stdio worker processes |
| `session-pool` | One worker/context per chat/session boundary |
| `project-pool` | One or more workers per project/root/worktree |
| `credential-session-pool` | Auth subject and token refresh become scheduling boundaries |
| `remote-http-session-pool` | Streamable HTTP sessions and provider budgets are first-class |
| `legacy-disabled` | Legacy transport recognized but not auto-scheduled |

## Lock domains

Acquire locks in this order to reduce deadlocks:

1. budget/provider
2. credential
3. project/repo
4. file/db/object
5. browser/session
6. consent/single-flight

Writes and destructive tools require a lock even if the server profile looks parallel-safe. Unknown tools are never upgraded purely from names or descriptions.

## Probe rules

Safe probes may do:

- initialize
- ping
- tools/list
- resources/list
- prompts/list
- no-op/read-only probes against synthetic fixtures when trusted

Safe probes must not do:

- writes
- payments
- sends
- real browser workflows against production sites
- OAuth/payment side effects
- package installs or arbitrary external code execution

## Runtime degradation

Downgrade immediately on:

- cross-session data leakage
- auth or credential mix-up
- repeated crash/broken-pipe/invalid JSON
- destructive call without expected consent gate
- p95/p99 latency growth with no throughput gain
- lock contention that indicates a wrong pool model

Suggested degrade path:

`parallel-pool -> project/session-pool -> singleton -> disabled-until-reviewed`

## Legacy cleanup rule

Do not remove compatibility parsing when it protects users from old configs. Do remove legacy as a default behavior. In practice this means:

- parse `sse` as `sse-legacy`;
- never normalize `sse` to stable `streamable-http`;
- never auto-probe or auto-parallelize legacy transport;
- recommend migration to Streamable HTTP or stdio.
