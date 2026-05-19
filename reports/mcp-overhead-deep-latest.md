# MCP overhead deep audit

Generated: 2026-05-19T12:36:18.129Z
Status: **pass**
Project: mcpace 0.6.5
Node: v24.15.0 on win32/x64

## Scenario

- Servers: 100
- Tools: 50000
- Operations: 10000
- Runs: 7
- Profile refreshes per run: 8

## Summary

- Config merge p95: 13.958 ms
- Cold shared profile inference p95/server: 3.402 µs
- Cached shared profile refresh p95/server: 0.126 µs
- Tool index/search p95/tool: 1.413 µs
- Route lookup: 0.858 µs/lookup
- Scheduler lock routing p95/op: 2.015 µs
- Max p95 heap delta: 53.849 MiB
- Mass package survey packages: 100

## Checks

| Check | Status | Detail |
|---|---:|---|
| config-fragment-merge-p95-budget | pass | p95 13.958ms <= 500ms |
| cold-profile-inference-per-server-budget | pass | p95 3.402us/server <= 250us |
| cached-profile-inference-per-server-budget | pass | p95 0.126us/server <= 120us |
| cached-refresh-actually-hits | pass | hits=800, misses=100 |
| worker-plan-build-materializes-all-servers | pass | 100/100 plans |
| tool-index-build-per-tool-budget | pass | p95 1.413us/tool <= 75us |
| route-lookups-hit-and-stay-cheap | pass | hits=10000/10000, 0.858us/lookup <= 40us |
| tool-search-retains-bounded-candidates | pass | 32 retained |
| scheduler-per-operation-budget | pass | p95 2.015us/op <= 75us |
| scheduler-keeps-review-gates-active | pass | review=769, disabled=1647 |
| heap-delta-budget | pass | max p95 heap delta 53.849MiB <= 256MiB |
| mass-survey-safety-proof-present | pass | mass survey pass, packages=100, safe=true |
