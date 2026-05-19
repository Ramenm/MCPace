# Upstream fail-safe hardening

This document defines how MCPace should behave when only part of a multi-server upstream topology is healthy. It covers mixed stdio/plain HTTP upstreams, large tool catalogs, tool-call failures, stale caches, retries, circuit breakers, and stateful batches.

## Source contracts

MCPace must preserve three boundaries:

1. **Client-facing MCPace must stay available** when at least one configured upstream is unhealthy.
2. **Discovery may degrade** by marking failed servers and using bounded stale metadata, but it must not make unsafe calls just because a stale catalog entry exists.
3. **Tool calls are authoritative live operations**. A stale `tools/list` cache can help the user find a tool name, but `upstream_call` still needs a live callable upstream and must return a contained failure when that upstream is down.

## Failure classes

| failure | expected behavior | retry/circuit rule | user-facing status |
|---|---|---|---|
| disabled server | skip, do not probe | no retry | `disabled` |
| unsupported transport | skip with adapter/migration hint | no retry | `blocked-unsupported-transport` |
| HTTPS direct upstream unsupported | skip with bridge hint | no retry | `blocked-https-upstream` |
| legacy HTTP+SSE | skip with migration hint | no retry | `blocked-legacy-sse-upstream` |
| command missing / bad cwd | skip with exact diagnostic | no retry until config changes | `blocked-command-not-found` |
| startup timeout | do not block other servers | circuit opens after repeated failures | `startup-timeout` |
| `tools/list` timeout | keep other catalogs; optionally show stale catalog | circuit opens after repeated failures | `catalog-timeout` |
| invalid JSON-RPC / malformed response | mark protocol error; isolate server | circuit opens after repeated failures | `protocol-error` |
| upstream tool returns `isError` | bridge stays OK, upstream result is error | no automatic retry unless transient class is known | `upstream-is-error` |
| tool timeout / process exit | return contained failure, release lease, invalidate pooled session | one bounded retry if configured | `timeout` / `process-exit` |
| flapping server | retry only within budget; keep circuit state visible | half-open after cooldown | `retry-success` or `circuit-open` |

## Discovery semantics

All-server discovery commands such as `upstream_probe`, `upstream_catalog`, `surface_manifest`, and `adapter_profile includeLiveCatalog=true` should be **partial-results first**:

- healthy servers contribute fresh tools;
- stale-cache servers may contribute a flagged stale sample for discovery only;
- failing servers contribute diagnostics but no live tools;
- blocked servers contribute status and repair hints;
- one failed server must not fail discovery for every other server;
- responses should expose `okCount`, `failedCount`, `skippedCount`, `cacheHitCount`, and degraded/partial status.

## Tool-call semantics

`upstream_call` and projected upstream calls should behave as live operations:

- live call succeeds: `bridgeOk=true`, `upstreamOk=true`, `isError=false`;
- upstream tool returns MCP `isError=true`: `bridgeOk=true`, `upstreamOk=false`, top-level tool result is an MCP tool error, not a JSON-RPC protocol error;
- transport/process/protocol failure: return a contained `upstreamOk=false` result when possible; JSON-RPC protocol errors are reserved for malformed MCPace requests or unsupported MCPace methods;
- no stale cache should be treated as proof the tool call is possible;
- pooled stdio sessions must be discarded after process exit, protocol violation, lease loss, or timeout.

## Batch semantics

`upstream_batch` is a same-server/stateful primitive. It should not be used for fan-out across many servers.

- Default same-server batch behavior is **stateful fail-fast**: if a call fails in a way that invalidates the session, stop the remaining sequence.
- Independent cross-server work should be planned as multiple `upstream_call` lanes by `adapter_route` so one failed server does not cancel unrelated work.
- If a future continue-on-error mode is added, it must be explicit and must still stop on session-invalidating transport/protocol failures.

## Circuit breaker model

A circuit breaker is a runtime protection, not durable user config.

- Closed: normal probing/calls.
- Open: repeated failures exceeded threshold; skip expensive live probe/call and return diagnostic quickly.
- Half-open: after cooldown, allow one bounded retry.
- Success closes the circuit; failure reopens it.

Circuit state should be per server and per operation family when useful (`tools/list` vs `tools/call`) so a broken mutating tool does not hide read-only catalog data unnecessarily.

## Cache rules

| cache | may survive restart | may survive reinstall | usable when upstream is down | safe for calls |
|---|---:|---:|---:|---:|
| memory tools/list cache | no | no | no | no |
| disk tools/list cache | yes, TTL/key bounded | yes, version/protocol keyed | discovery only, flagged stale | no |
| projection cache | derived, disposable | no unless version keyed | only as stale discovery metadata | no |
| session pool | no | no | no | no |
| circuit state | process-local by default | no | yes, as skip hint | no |

## Verification

Run:

```bash
npm run verify:mixed-upstreams
npm run verify:upstream-failsafe
npm run benchmark:mixed-upstreams
npm run benchmark:upstream-failsafe
```

`verify:upstream-failsafe` simulates:

- healthy stdio and HTTP servers;
- stale-cache discovery fallback;
- startup/list timeouts;
- invalid JSON/protocol errors;
- tool timeout and process exit;
- upstream `isError`;
- flapping server recovery;
- circuit-open states;
- stateful batch fail-fast versus independent-call continue behavior;
- 50 servers and 200k configured tool slots within memory budget.

## Release gate

Public runtime claims must not be strengthened until this gate and the mixed-topology gate pass alongside Rust-host proof and a real-client trace.
