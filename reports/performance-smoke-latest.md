# Performance smoke report

Generated: 2026-05-19T12:35:45.124Z
Status: **pass**

## Scope

- Lightweight runtime HTTP benchmark against an in-process mock endpoint using `scripts/benchmark-runtime.mjs`.
- Synthetic tool-scale, mixed-upstream, and upstream-failsafe simulations using bounded memory budgets.
- This is a smoke/regression harness, not a replacement for host-specific Rust binary benchmarking.

## Summary

- Runtime HTTP failures: 0
- Runtime HTTP max p95: 11.87 ms
- Tool-scale: 41 ms, heap +1.2 MiB
- Mixed-upstreams: 35 ms, heap +2.5 MiB
- Upstream-failsafe: 1 ms, heap +0 MiB

## Checks

| Check | Status | Detail |
|---|---:|---|
| runtime-http-benchmark-ran | pass | exit=0 |
| runtime-http-no-failures | pass | failures=0 |
| runtime-http-latency-measured | pass | maxP95Ms=11.87 |
| toolScale-ran | pass | exit=0 |
| toolScale-status-pass | pass | status=pass |
| toolScale-heap-budget | pass | heapDeltaMiB=1.2; limit=256 |
| mixedUpstreams-ran | pass | exit=0 |
| mixedUpstreams-status-pass | pass | status=pass |
| mixedUpstreams-heap-budget | pass | heapDeltaMiB=2.5; limit=256 |
| upstreamFailsafe-ran | pass | exit=0 |
| upstreamFailsafe-status-pass | pass | status=pass |
| upstreamFailsafe-heap-budget | pass | heapDeltaMiB=0; limit=256 |

## Caveats

- No `cargo`/`rustc` host proof is implied by this report.
- Do not add hard latency gates until Ubuntu/macOS/Windows baselines exist. Use `--max-http-p95-ms` only after a baseline is accepted.
