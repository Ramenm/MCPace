# MCP overhead profile

- Status: pass
- Generated: 2026-05-19T12:36:16.797Z
- Project: mcpace 0.6.5
- Elapsed: 246.009ms
- Safety: starts MCP servers = false, calls tools = false

## Hot-path metrics

| Area | Metric | Value |
|---|---|---:|
| JSON-RPC route | p95 us/op | 2.124 |
| Session shard key | p95 us/op | 1.197 |
| Lock admission | p95 us/op | 0.881 |
| Tool index build | ms | 84.657 |
| Tool search projection | p95 ms/query | 9.808 |
| Package policy classification | p95 us/op | 0.751 |

## Checks

| Check | OK | Severity | Evidence |
|---|---:|---|---|
| json-rpc-routing-overhead-budget | yes | high | p95 2.124us |
| session-shard-key-overhead-budget | yes | high | p95 1.197us |
| lock-admission-overhead-budget | yes | high | p95 0.881us |
| lock-admission-leaves-no-active-locks | yes | critical | 0 active locks remain |
| disabled-and-review-gates-are-still-hit | yes | critical | {"admitted":45845,"blockedDisabled":1725,"blockedReview":2430,"blockedConflict":0,"conflictsPrevented":0} |
| tool-index-build-heap-budget | yes | high | 44.859MiB for 100000 tools |
| tool-exact-lookup-overhead-budget | yes | medium | p95 0.45us |
| tool-search-projection-overhead-budget | yes | medium | p95 9.808ms for 100000 tools |
| package-policy-overhead-budget | yes | medium | p95 0.751us across 100 package profiles |
| classification-budget | yes | medium | p95 0.751us across 100 package profiles |
| tool-index-budget | yes | medium | 0.847us/tool to build 100000 tools |
| scheduler-budget | yes | medium | p95 0.881us |

## Recommendations

- Prefer one long-lived hub process; npm launcher cold-start overhead is acceptable for install/CLI but should not be paid per MCP request.
- Keep unknown and credentialed servers disabled/review-gated; optimizing should never convert registry discovery into execution.
- Expose large tool catalogs through exact qualified-name routing plus top-K read-only projection, not by dumping every discovered tool into every model context.
- Use session/client/project/credential/transport identity as the sharding key for stateful servers; stateless candidates can be widened only after live evidence.
