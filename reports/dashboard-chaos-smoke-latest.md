# Dashboard chaos smoke report

Generated: 2026-05-17T15:29:10.510Z
Project: mcpace 0.6.5
Status: **pass**

## Scope

- Executes the embedded dashboard script in isolated VM tabs with a mocked DOM and fetch layer.
- Exercises random refreshes, force refresh, filter typing, enabled-only toggles, action calls, hidden/visible tab transitions, stale refresh cancellation, and partial log API failures.
- This is a source-level chaos/regression smoke. It does not replace browser-engine Playwright coverage or host-specific Rust runtime testing.

## Summary

- Tabs: 6
- Total operations: 720
- Elapsed: 9755.42 ms
- Max operation: 31.34 ms
- Max render: 3.83 ms
- Fetches: 953
- Aborted overlapping fetches: 154
- Partial failures contained: 14

## Checks

| Check | Status | Detail |
|---|---:|---|
| uses-visibilitychange | pass | hidden tabs are explicitly handled |
| uses-page-visibility-state | pass | dashboard can pause hidden-tab polling |
| uses-abort-controller | pass | overlapping refreshes can be cancelled |
| guards-stale-refreshes | pass | out-of-order responses are ignored |
| uses-settimeout-not-setinterval | pass | polling is re-armed after each refresh |
| partial-logs-failure-does-not-kill-overview | pass | logs failure is degraded, not fatal |
| dashboard-test-hook-present | pass | smoke can exercise runtime functions without a browser dependency |
| refresh-mode-visible | pass | operator can see refresh/backoff state |
| chaos-elapsed-budget | pass | elapsedMs=9755.42; budget=15000 |
| operation-latency-budget | pass | maxOperationMs=31.34; budget=120 |
| render-latency-budget | pass | maxRenderMs=3.83; budget=90 |
| all-tabs-rendered-server-list | pass | tabs=6 |
| all-tabs-kept-one-auto-timer | pass | tab1:1, tab2:1, tab3:1, tab4:1, tab5:1, tab6:1 |
| overlap-aborts-observed | pass | aborted=154 |
| partial-failures-contained | pass | partialFailures=14 |

## Caveats

- Hidden-tab behavior is simulated through `document.visibilityState`; real browsers can additionally throttle timers and background work.
- Add a Playwright lane when a browser engine is available to verify layout, focus, real network timing, and real tab lifecycle.
- No `cargo`/`rustc` host proof is implied by this report.
