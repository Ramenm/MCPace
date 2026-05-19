# MCP fixture overhead

- Status: pass
- Generated: 2026-05-19T12:36:07.563Z
- Safety: starts third-party MCP servers = false, calls third-party tools = false

## Measurements

| Area | p50 | p95 | max |
|---|---:|---:|---:|
| Cold stdio total ms | 47.297 | 49.283 | 49.283 |
| Cold initialize ms | 47.139 | 49.085 | 49.085 |
| Warm tools/list ms | 0.092 | 0.184 | 0.204 |

## Checks

| Check | Status | Severity | Evidence |
|---|---:|---|---|
| cold-stdio-fixture-measured | pass | high | failures=0, p95=49.283ms |
| warm-tools-list-measured | pass | high | failures=0, p95=0.184ms |
| cold-stdio-not-paid-per-request-budget | pass | medium | cold p95=49.283ms |
| warm-tools-list-budget | pass | medium | warm p95=0.184ms |

## Notes

- This is actual MCP stdio lifecycle measurement against a local deterministic fixture, not a random package benchmark.
- Cold start is expected to be much higher than warm `tools/list`; production should avoid paying cold stdio startup per user request whenever policy allows reuse/cache.
