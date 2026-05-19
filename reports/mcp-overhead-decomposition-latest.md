# MCP overhead decomposition

Generated: 2026-05-19T12:36:16.184Z
Status: **pass**

## Scenario

- Servers: 100
- Tools per server: 50
- Total synthetic tools: 5000
- Lookups: 20000
- Scheduler operations: 50000

## Summary

- Route index speedup over linear scan: 97.68x
- Visibility cache-hit speedup over uncached projection: 13989.4x
- Scheduler lock cycle: 0.0005 ms/op
- Small JSON-RPC roundtrip: 0.0014 ms/op
- 1k tools/list JSON-RPC roundtrip: 1.3603 ms/op
- Metadata classifier: 0.0007 ms/op
- Heap delta: 15.1731 MiB

## Optimizations locked by this benchmark

- Keep a prebuilt `qualifiedToolName -> route` index instead of scanning all tools per call.
- Cache client/session/project visibility projections and invalidate on server/tool/config change.
- Keep scheduler lock acquisition O(number-of-locks) and never proportional to total installed servers.
- Measure JSON-RPC payload overhead separately from process spawn and HTTP connection setup.
- Cache package metadata classification by normalized package fingerprint so registry/UI refreshes do not re-run signal inference.
- Do not run random MCP servers during ecosystem surveys; benchmark synthetic inventory or reviewed safe probes only.

## Checks

| Check | Status | Detail |
|---|---:|---|
| does-not-start-random-mcp-servers | pass | Synthetic benchmark only; no package bins, MCP processes, or tool invocations are executed. |
| route-index-is-faster-than-linear-scan | pass | speedup=97.68x |
| route-index-build-is-bounded | pass | index build 1.4035ms for 5000 tools |
| visibility-cache-hit-is-faster-than-uncached | pass | speedup=13989.4x |
| visibility-optimized-miss-is-not-slower-than-naive | pass | optimizedMiss=227.2941ms; naive=1137.338ms |
| scheduler-lock-overhead-is-bounded | pass | per operation 0.0005ms |
| scheduler-drains-all-synthetic-locks | pass | remaining locks 0 |
| small-json-rpc-overhead-is-bounded | pass | per roundtrip 0.0014ms |
| large-tools-list-json-rpc-is-measured | pass | 1000 descriptors; 1.3603ms/roundtrip |
| metadata-classifier-overhead-is-bounded | pass | per package 0.0007ms |
| heap-growth-under-budget | pass | heap delta 15.1731MiB; budget 256MiB |
