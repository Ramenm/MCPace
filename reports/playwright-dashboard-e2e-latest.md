# Playwright dashboard E2E smoke

- Status: pass
- Generated: 2026-05-16T14:00:05.602Z
- Project: mcpace 0.6.4
- Tool: @playwright/test@1.60.0
- Chromium: /usr/bin/chromium
- Elapsed: 23636.28ms
- Install elapsed: 4086.09ms
- Parallel clients: 4
- Parallel workers observed: 2
- Parallel conflicts: 0

## Checks

| Check | OK | Evidence |
|---|---:|---|
| chromium-executable-found | yes | /usr/bin/chromium |
| playwright-package-available-in-temp-prefix | yes | @playwright/test@1.60.0 |
| real-playwright-invoked | yes | Playwright CLI output observed |
| multiple-tabs-and-network-degradation-covered | yes | tests/e2e/dashboard.playwright.spec.mjs |
| multi-worker-parallel-configured | yes | tests/e2e/playwright.config.mjs uses configurable workers and fullyParallel |
| parallel-client-session-spec-covered | yes | tests/e2e/dashboard.parallel.playwright.spec.mjs |
| parallel-client-sessions-isolated-at-runtime | yes | 4 clients across 2 workers; conflicts=0 |
| console-errors-fail-test | yes | browser console errors are captured |
| playwright-execution-pass | yes | elapsed 23636ms |

## Output tail

```text

Running 5 tests using 2 workers

  ✓  1 tests/e2e/dashboard.parallel.playwright.spec.mjs:142:3 › isolates already-started dashboard session for client-01 (4.8s)
  ✓  2 tests/e2e/dashboard.parallel.playwright.spec.mjs:142:3 › isolates already-started dashboard session for client-02 (5.6s)
  ✓  3 tests/e2e/dashboard.parallel.playwright.spec.mjs:142:3 › isolates already-started dashboard session for client-03 (3.2s)
  ✓  4 tests/e2e/dashboard.parallel.playwright.spec.mjs:142:3 › isolates already-started dashboard session for client-04 (5.0s)
  ✓  5 tests/e2e/dashboard.playwright.spec.mjs:139:1 › dashboard stays usable across real Chromium tabs, content reloads, slow APIs, and partial failures (7.2s)

  5 passed (18.2s)


```
