# MCP overhead stress report

Generated: 2026-05-19T12:36:06.785Z
Status: **pass**

## Scenario

- Servers: 100
- Synthetic tools: 100000
- Scheduler operations: 25000
- Metadata profiles: 1000

## Results

- Tool catalog/index: 250.4 ms, heap +7.548 MiB, retained top-k 25.
- Known-tool lookup: 1.83 ms for 16384 lookups; unknown forwarded 0.
- Scheduler: 118.32 ms for 25000 ops; started 14393; deferrals 17649.
- Profile classification: 6.52 ms for 1000 metadata profiles.
- Total: 377.95 ms; heap +31.55 MiB.

## Safety invariants

- Starts MCP servers: false
- Calls MCP tools: false
- Executes third-party package code: false
- Auto-enables random packages: false

## Checks

| Check | Status | Detail |
|---|---:|---|
| no-random-mcp-server-start | pass | Stress harness never starts third-party MCP packages and never sends tools/call. |
| catalog-volume | pass | 100000/100000 synthetic tools indexed. |
| tool-search-top-k-bounded | pass | 25 retained candidates. |
| projection-bounded | pass | projected=64; firstPage=72. |
| known-tool-lookup-fails-closed | pass | unknownHits=8192; unknownForwarded=0. |
| scheduler-drains | pass | ticks=3759; violations=0. |
| scheduler-blocks-disabled-unknown-review | pass | disabled=5187; unknown=1956; review=3464. |
| scheduler-starts-safe-work | pass | started=14393; finished=14393. |
| metadata-classification-disabled-by-default | pass | 1000 metadata profiles classified; executeDefault=0. |
| heap-budget | pass | heapDeltaMiB=31.55; budget=256. |

## Notes

- This harness measures MCP hub overhead with synthetic server/tool profiles only. It is intentionally not a random MCP server execution harness.
- Latency is recorded as evidence, but host-specific latency budgets should be baselined per OS/architecture before becoming hard release gates.
