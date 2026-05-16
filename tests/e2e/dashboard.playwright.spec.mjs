import fs from 'node:fs';
import path from 'node:path';
import { test, expect } from '@playwright/test';

const repoRoot = process.env.MCPACE_REPO_ROOT
  ? path.resolve(process.env.MCPACE_REPO_ROOT)
  : path.resolve(new URL('../..', import.meta.url).pathname);
const dashboardHtml = fs.readFileSync(path.join(repoRoot, 'src/dashboard/index.html'), 'utf8');

async function installDashboardApiMock(page, serverCount = 80, clientCount = 14) {
  await page.evaluate(({ serverCount, clientCount }) => {
    const delay = (ms, signal) => new Promise((resolve, reject) => {
      if (signal?.aborted) {
        reject(new DOMException('Aborted', 'AbortError'));
        return;
      }
      const timer = setTimeout(resolve, ms);
      signal?.addEventListener('abort', () => {
        clearTimeout(timer);
        reject(new DOMException('Aborted', 'AbortError'));
      }, { once: true });
    });

    function jsonResponse(status, payload) {
      return new Response(JSON.stringify(payload), {
        status,
        statusText: status >= 400 ? 'Synthetic error' : 'OK',
        headers: { 'content-type': 'application/json', 'cache-control': 'no-store' }
      });
    }

    function makeOverview(sequence) {
      const servers = Array.from({ length: serverCount }, (_, index) => ({
        name: `server-${String(index).padStart(3, '0')}`,
        kind: index % 6 === 0 ? 'streamable-http' : 'stdio',
        scopeClass: index % 7 === 0 ? 'project-local' : 'user',
        transportPreference: index % 6 === 0 ? 'streamable-http' : 'stdio-default',
        concurrencyPolicy: index % 3 === 0 ? 'shared' : 'isolated',
        effectiveEnabled: index % 4 !== 0,
        required: index % 11 === 0,
        stateBinding: index % 2 === 0 ? 'ephemeral' : 'persistent',
        credentialBinding: index % 5 === 0 ? 'user-secret' : 'none'
      }));
      const targets = Array.from({ length: clientCount }, (_, index) => ({
        id: `client-${index}`,
        displayName: `Client ${index}`,
        surfaceClass: index % 3 === 0 ? 'remote' : 'local',
        surfaceKind: index % 2 === 0 ? 'desktop' : 'web',
        supportedIngresses: index % 2 === 0 ? ['stdio', 'streamable-http'] : ['streamable-http'],
        nativeScopes: ['workspace', 'user']
      }));
      return {
        rootPath: `/tmp/mcpace-playwright-seq-${sequence}`,
        generatedAtMs: Date.now() + sequence,
        cache: sequence % 2 === 0 ? { hit: true, ttlMs: 15000 } : { bypassed: true },
        doctor: { project: { rustSourceReady: true, npmSurfaceReady: true, containerToolingReady: false } },
        hub: {
          status: sequence % 9 === 0 ? 'stopped-ready' : 'running',
          health: 'healthy',
          warnings: sequence % 8 === 0 ? [`synthetic warning ${sequence}`] : [],
          readyForReadOnlyOps: true,
          readyForRuntimeOps: sequence % 7 !== 0,
          effectiveEnabledServerCount: servers.filter((server) => server.effectiveEnabled).length,
          requiredServerCount: servers.filter((server) => server.required).length,
          activeProfile: 'default'
        },
        readiness: {
          runtimePrerequisitesReady: sequence % 13 !== 0,
          sourceEnabledServerCount: servers.filter((server) => server.effectiveEnabled).length,
          profileEnabledServerCount: Math.floor(serverCount / 3),
          requiredServerCount: servers.filter((server) => server.required).length,
          requiredSourceEnabledCount: servers.filter((server) => server.required && server.effectiveEnabled).length,
          serverCount,
          missingRequiredSourceEnablement: sequence % 17 === 0 ? ['paid-example'] : [],
          missingRequiredCommands: sequence % 19 === 0 ? ['uvx'] : [],
          activeProfile: 'default'
        },
        runtime: {
          http: { activeConnections: sequence % 8, maxConnections: 8 },
          upstreamSessionPool: { size: sequence % 23, maxSize: 128, shardCount: 4 }
        },
        servers,
        clients: {
          targets,
          familyCounts: { claude: 4, cursor: 3, codex: 2, custom: 5 },
          surfaceClassCounts: { local: 9, remote: 5 },
          configuredClientKeyName: 'mcpServers'
        }
      };
    }

    window.__mcpaceFixture = { overviewCount: 0, logsCount: 0, actions: [] };
    window.fetch = async (input, options = {}) => {
      const url = String(input);
      const signal = options.signal;
      if (url.startsWith('/api/overview')) {
        window.__mcpaceFixture.overviewCount += 1;
        const sequence = window.__mcpaceFixture.overviewCount;
        await delay(sequence % 4 === 0 ? 75 : 8, signal);
        return jsonResponse(200, makeOverview(sequence));
      }
      if (url.startsWith('/api/logs')) {
        window.__mcpaceFixture.logsCount += 1;
        const sequence = window.__mcpaceFixture.logsCount;
        await delay(sequence % 3 === 0 ? 40 : 5, signal);
        if (sequence % 5 === 0) return jsonResponse(503, { error: 'synthetic logs outage' });
        return jsonResponse(200, Array.from({ length: 6 }, (_, index) => ({
          event: `event-${index}`,
          level: index % 5 === 0 ? 'warn' : 'info',
          tsMs: Date.now() - index * 1000,
          sequence
        })));
      }
      if (url.startsWith('/api/actions/')) {
        window.__mcpaceFixture.actions.push(url);
        return jsonResponse(200, { ok: true, action: url, count: window.__mcpaceFixture.actions.length });
      }
      return jsonResponse(404, { error: 'not found' });
    };
  }, { serverCount, clientCount });
}

async function loadDashboard(page) {
  await page.setContent(dashboardHtml, { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#runtime-ready')).not.toHaveText('—');
  await expect(page.locator('#root-path')).toContainText('/tmp/mcpace-playwright-seq-');
}

async function simulateVisibility(page, state) {
  await page.evaluate((nextState) => {
    Object.defineProperty(Document.prototype, 'visibilityState', {
      configurable: true,
      get: () => nextState
    });
    document.dispatchEvent(new Event('visibilitychange'));
  }, state);
}

test('dashboard stays usable across real Chromium tabs, content reloads, slow APIs, and partial failures', async ({ browser }) => {
  const errors = [];
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const pages = await Promise.all(Array.from({ length: 5 }, () => context.newPage()));

  for (const page of pages) {
    page.on('console', (message) => {
      if (message.type() === 'error') errors.push(message.text());
    });
    page.on('pageerror', (error) => errors.push(error.message));
    await installDashboardApiMock(page);
    await loadDashboard(page);
    await expect(page.locator('#server-list')).not.toContainText('No servers match');
  }

  await Promise.all(pages.map((page, index) => page.locator('#server-search').fill(`server-00${index}`)));
  await Promise.all(pages.map((page) => page.locator('#toggle-enabled-filter').click()));
  await Promise.all(pages.map((page) => page.locator('#refresh-button').click()));

  await simulateVisibility(pages[1], 'hidden');
  await simulateVisibility(pages[2], 'hidden');
  const reloadedPage = await context.newPage();
  reloadedPage.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  reloadedPage.on('pageerror', (error) => errors.push(error.message));
  await installDashboardApiMock(reloadedPage);
  await loadDashboard(reloadedPage);
  await pages[3].close();
  pages[3] = reloadedPage;
  await pages[4].locator('#hub-up-button').click();
  await simulateVisibility(pages[1], 'visible');
  await simulateVisibility(pages[2], 'visible');

  for (const page of pages) {
    await expect(page.locator('#refresh-mode')).toContainText(/auto|refreshing|degraded|paused/);
    await expect(page.locator('#root-path')).toContainText('/tmp/mcpace-playwright-seq-');
    const dashboardState = await page.evaluate(() => ({
      refreshSeq: window.__mcpaceDashboard.state.refreshSeq,
      hasOverview: Boolean(window.__mcpaceDashboard.state.overview),
      hasTimer: window.__mcpaceDashboard.state.autoRefreshTimer !== null,
      inFlight: window.__mcpaceDashboard.state.refreshInFlight,
      fixture: window.__mcpaceFixture
    }));
    expect(dashboardState.hasOverview).toBeTruthy();
    expect(dashboardState.hasTimer).toBeTruthy();
    expect(dashboardState.refreshSeq).toBeGreaterThanOrEqual(1);
    expect(dashboardState.fixture.overviewCount).toBeGreaterThanOrEqual(1);
    expect(dashboardState.fixture.logsCount).toBeGreaterThanOrEqual(1);
  }

  const actionPage = await pages[4].evaluate(() => window.__mcpaceFixture.actions.slice());
  expect(actionPage).toContain('/api/actions/hub-up');
  expect(errors).toEqual([]);
});
