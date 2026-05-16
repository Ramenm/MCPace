# Browser E2E and external MCP tool verification

This pass separates three different concerns that are easy to mix up:

1. **Dashboard browser behavior**: real Chromium tabs, reloads, slow API responses,
   partial failures, and console/page errors.
2. **Browser automation tool choice**: Playwright is the default live lane;
   Puppeteer and Cypress stay documented alternatives until there is a reason to
   support multiple browser automation stacks in CI.
3. **External MCP tool behavior**: local-only tools, package-manager-launched
   tools, container-launched tools, remote HTTP tools, and paid/API-key tools
   have different blast radii.

## Default browser lane

Run the browser lane explicitly:

```bash
npm run verify:playwright-e2e
# or
npm run verify:browser-experience
```

The wrapper uses `npx --package @playwright/test@1.60.0` so the source archive
need not vendor Playwright or `node_modules`. It prefers a system Chromium
binary (`MCPACE_PLAYWRIGHT_CHROMIUM`, `/usr/bin/chromium`, Google Chrome paths)
and sets `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1`.

The E2E spec starts a local HTTP fixture, serves `src/dashboard/index.html`, and
mocks dashboard API endpoints. It opens multiple real Chromium tabs and covers:

- initial render;
- multiple tabs in one browser context;
- manual refresh;
- search/filter changes;
- reload;
- hub action POST;
- hidden/visible tab transitions;
- slow `/api/overview` responses;
- intermittent `/api/logs` failures;
- browser console/page errors as test failures.

This is still a local fixture test. It does not replace a Rust-host test against
an actual `mcpace dashboard` process.

## Tooling alternatives

| Tool | Good fit | Why not default here |
|---|---|---|
| Playwright | Cross-browser E2E, multiple tabs/contexts, network routing, fixture control | Default live browser lane |
| Puppeteer | Lower-level Chrome/Firefox automation via DevTools/WebDriver BiDi | Useful fallback, but narrower test-runner ergonomics for multi-browser CI |
| Cypress | Interactive application E2E and component-style workflows | More app-runner oriented; less aligned with local CLI fixture + many-tab chaos |

## External MCP tools and internet checks

Run the source-only matrix:

```bash
npm run verify:external-tool-internet
```

Run live DNS/HTTPS/API reachability checks:

```bash
npm run verify:external-tool-internet:live
```

The live mode checks public documentation/registry/API endpoints only. It does
not execute third-party MCP packages, does not send credentials, and does not
invoke paid tools.

## What remains live-host only

- Full Playwright against the compiled Rust dashboard binary.
- macOS/Windows browser runs.
- Browser trace/video artifact upload in CI.
- Live MCP server execution for specific packages.
- Paid API-key tools with real budget/quota enforcement.
- `npx`, `uvx`, and Docker package/image pull behavior under offline/cache-miss
  conditions.

## Parallel client/session isolation lane

The Playwright lane now has two layers. It is kept as an explicit browser lane instead of being folded into every fast source verification, because it installs test-only browser tooling into a temporary prefix and is intentionally heavier than source-only checks:

1. `dashboard.playwright.spec.mjs` opens multiple tabs in one browser context.
   This catches same-session tab behavior: refresh races, hidden/visible tabs,
   reloads, and partial API failures.
2. `dashboard.parallel.playwright.spec.mjs` opens independent browser contexts
   for separate clients and runs those tests in parallel workers. This catches
   cross-client contamination: shared storage, stale root paths, action routing,
   and started-session state leaking into another client.

`tests/e2e/playwright.config.mjs` enables `fullyParallel: true` and uses
`MCPACE_PLAYWRIGHT_WORKERS` so CI can control worker count. The wrapper records
`MCPACE_PLAYWRIGHT_STATE_DIR` evidence; `reports/playwright-dashboard-e2e-latest.json`
contains the observed client count, worker count, and any session conflicts.

Expected source-level guarantees:

- separate client sessions use separate `browser.newContext()` calls;
- already-started session state stays inside that context;
- a new page opened inside the same context sees the same started session;
- another client context does not inherit that state;
- the report fails if fewer than two Playwright workers execute the parallel
  client lane.

## Overhead audit

Run:

```bash
npm run verify:overhead-audit
```

The overhead audit checks that browser tooling stays test-only, that the npm CLI
launcher has no runtime dependencies, that Playwright is installed only into a
temporary prefix for the E2E lane, and that the Node launcher overhead over the
native binary is measured on the current host.

This audit is intentionally conservative. The launcher timing is a host-local
smoke signal, not a universal benchmark. Treat regressions as prompts for a real
Rust-host benchmark before claiming a release performance number.

## Multi-client runtime audit

Run the source-level audit with:

```bash
npm run verify:multi-client-runtime
```

This audit complements Playwright. Playwright proves browser session isolation
with `browser.newContext()` and parallel workers; the runtime audit checks the
Rust/Node source contracts that decide whether HTTP sessions, stdio sessions,
hub leases, and upstream session pools stay separated. It also records the main
limit: strict stdio multi-client isolation is not fully automatic when a client
sends no session/conversation/client-instance/transport-session signal.
