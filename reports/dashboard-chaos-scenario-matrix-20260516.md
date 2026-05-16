# Dashboard chaos scenario matrix — 2026-05-16

## Facts verified by source/tests

- The dashboard is a single embedded HTML/JS surface at `src/dashboard/index.html`.
- The source-level chaos smoke is implemented in `scripts/dashboard-chaos-smoke.mjs`.
- `npm run verify:dashboard-chaos` writes JSON and Markdown evidence.

## Scenario coverage

| Scenario | Why it matters | Current coverage | Remaining gap |
|---|---|---|---|
| One tab, normal refresh | Baseline dashboard health | VM smoke + fetch mock | Real browser layout |
| Many tabs open | Avoid per-tab load amplification and stale UI | 6-tab default chaos smoke | Cross-window browser scheduling |
| Hidden tab | Avoid background polling and wasted work | `document.visibilityState` + hidden transition smoke | Actual browser timer throttling |
| Visible tab resume | User switches back and expects fresh state | `visibilitychange` visible event smoke | Real focus/paint timing |
| Repeated manual refresh | Avoid overlapping requests and stale renders | `AbortController` + `refreshSeq` smoke | Real network abort propagation |
| Slow/out-of-order overview | Old response must not overwrite new one | Overlap abort/stale guard source checks | Browser DevTools trace |
| Logs endpoint fails | Overview should remain useful | partial `/api/logs` 503 smoke | Real server fault injection |
| Large server list | Catch obvious render stalls | 150 servers default; 250 in benchmark | Browser layout/paint cost |
| Search and enabled-only filter | Common operator interaction | random input/toggle operations | Real accessibility/focus checks |
| Action button + refresh | Repair/hub actions should refresh cleanly | random `/api/actions/repair` calls | Real CLI/runtime side effects |

## Recommended next live/browser pass

When browser engines are available, run a Playwright-based lane that opens at
least two pages in one context, mocks slow/failing network routes, switches the
front tab repeatedly, reloads pages, and records browser-level timings. Keep the
source-level VM smoke as the fast regression gate.
