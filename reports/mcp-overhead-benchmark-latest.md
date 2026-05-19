# MCP overhead benchmark

Generated: 2026-05-19T12:36:06.067Z
Status: **pass**

## Inputs

- Packages classified: 100
- Servers: 100
- Tools: 5000
- Scheduling decisions: 20000

## Measurements

- Classification: 1.937 ms (19.371 µs/package)
- Registry build: 3.081 ms (0.616 µs/tool)
- Scheduler decisions: 28.767 ms (1.438 µs/decision)
- Scheduler started/blocked: 11076 started, 1984 disabled, 2200 unknown-tool, 4320 review-gated

## Checks

| Check | OK | Detail |
|---|---:|---|
| classifier-under-budget | yes | 1.937ms <= 120ms |
| registry-build-under-budget | yes | 3.081ms <= 180ms for 5000 tools |
| scheduler-under-budget | yes | 28.767ms <= 450ms for 20000 decisions |
| scheduler-per-decision-under-budget | yes | 1.438µs <= 75µs |
| scheduler-drains-locks | yes | 0 active locks after drain |
| disabled-and-unknown-never-start-random-servers | yes | randomServerStarts=0, disabled=1984, unknownTool=2200 |
| review-gate-observed | yes | 4320 review-gated operations blocked |
| heap-under-budget | yes | max stage heap delta 3.984MiB <= 128MiB |

## Safety notes

- No MCP package binary is executed in this benchmark.
- No initialize, tools/list, or tools/call is sent to random packages.
- The benchmark measures hub-side classification, registry, and scheduling overhead only.
