# Dashboard chaos and tab-lifecycle verification

This pass covers the dashboard behaviors that are easy to miss when only one
happy-path tab is open: random refresh clicks, background tabs, visible-tab
resume, stale responses, partial API failures, and large rendered lists.

## Operator command

```bash
npm run verify:dashboard-chaos
```

For a heavier local run:

```bash
npm run benchmark:dashboard-chaos
```

The smoke writes:

- `reports/dashboard-chaos-smoke-latest.json`
- `reports/dashboard-chaos-smoke-latest.md`

## What the smoke exercises

The harness executes the embedded `src/dashboard/index.html` script inside
multiple isolated VM-backed tabs with a mocked DOM and `fetch` layer. It does
not need a browser binary, so it can run in the source-only sandbox.

It checks these scenarios:

- several dashboard tabs open at once;
- random manual refreshes and forced refreshes;
- random search input changes and enabled-only filter toggles;
- simulated hidden/visible tab transitions via `document.visibilityState`;
- overlapping refreshes that should abort stale requests;
- partial `/api/logs` failures that must not blank the overview;
- large server/client payloads to catch obvious render stalls;
- exactly one scheduled auto-refresh timer per tab.

## Dashboard behavior hardened in this pass

- Replaced fixed `setInterval` polling with `scheduleAutoRefresh()` so the next
  poll is armed only after the current refresh settles.
- Added `AbortController` and a monotonic `refreshSeq` guard so old/out-of-order
  refreshes cannot overwrite newer state.
- Added Page Visibility handling so hidden tabs pause normal polling and refresh
  once when they become visible again.
- Made log API failure degrade the logs area instead of failing the whole
  overview refresh.
- Added a visible refresh-mode indicator for operators.

## Caveats

This is still not a full browser E2E lane. It does not verify CSS layout,
actual browser timer throttling, real focus behavior, real network priority, or
visual rendering. When a browser is available, add a Playwright lane with
Chromium/WebKit/Firefox coverage for:

- two or more real pages in one browser context;
- repeated `page.reload()`, `page.bringToFront()`, and visibility/focus changes;
- slow `/api/overview` and failing `/api/logs` network routes;
- action buttons while refresh is in flight;
- layout stability under 100+ server rows.
