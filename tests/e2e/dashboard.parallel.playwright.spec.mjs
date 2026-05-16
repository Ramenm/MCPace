import fs from 'node:fs';
import path from 'node:path';
import { test, expect } from '@playwright/test';

const repoRoot = process.env.MCPACE_REPO_ROOT
  ? path.resolve(process.env.MCPACE_REPO_ROOT)
  : path.resolve(new URL('../..', import.meta.url).pathname);
const dashboardHtml = fs.readFileSync(path.join(repoRoot, 'src/dashboard/index.html'), 'utf8');
const stateDir = process.env.MCPACE_PLAYWRIGHT_STATE_DIR || null;
const clientCount = Number.parseInt(process.env.MCPACE_PLAYWRIGHT_PARALLEL_CLIENTS || '4', 10);
const clients = Array.from({ length: Number.isSafeInteger(clientCount) && clientCount > 0 ? clientCount : 6 }, (_, index) => `client-${String(index + 1).padStart(2, '0')}`);

test.describe.configure({ mode: 'parallel' });

async function installPageMock(page, clientId, contextId) {
  await page.evaluate(({ clientId, contextId }) => {
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
      const servers = Array.from({ length: 36 }, (_, index) => ({
        name: `${clientId}-server-${String(index).padStart(3, '0')}`,
        kind: index % 5 === 0 ? 'streamable-http' : 'stdio',
        scopeClass: index % 3 === 0 ? 'project-local' : 'user',
        transportPreference: index % 5 === 0 ? 'streamable-http' : 'stdio-default',
        concurrencyPolicy: index % 2 === 0 ? 'isolated' : 'shared',
        effectiveEnabled: index % 4 !== 0,
        required: index % 9 === 0,
        stateBinding: index % 2 === 0 ? 'ephemeral' : 'persistent',
        credentialBinding: index % 7 === 0 ? 'user-secret' : 'none'
      }));
      const targets = Array.from({ length: 8 }, (_, index) => ({
        id: `${clientId}-surface-${index}`,
        displayName: `${clientId} Surface ${index}`,
        surfaceClass: index % 2 === 0 ? 'local' : 'remote',
        surfaceKind: index % 2 === 0 ? 'desktop' : 'web',
        supportedIngresses: index % 2 === 0 ? ['stdio', 'streamable-http'] : ['streamable-http'],
        nativeScopes: ['workspace', 'user']
      }));
      return {
        rootPath: `/tmp/mcpace-parallel-${clientId}-${sequence}`,
        generatedAtMs: Date.now() + sequence,
        cache: { bypassed: true },
        doctor: { project: { rustSourceReady: true, npmSurfaceReady: true, containerToolingReady: false } },
        hub: {
          status: 'running',
          health: 'healthy',
          warnings: sequence % 3 === 0 ? [`${clientId} synthetic warning ${sequence}`] : [],
          readyForReadOnlyOps: true,
          readyForRuntimeOps: true,
          effectiveEnabledServerCount: servers.filter((server) => server.effectiveEnabled).length,
          requiredServerCount: servers.filter((server) => server.required).length,
          activeProfile: clientId
        },
        readiness: {
          runtimePrerequisitesReady: true,
          sourceEnabledServerCount: servers.filter((server) => server.effectiveEnabled).length,
          profileEnabledServerCount: 12,
          requiredServerCount: servers.filter((server) => server.required).length,
          requiredSourceEnabledCount: servers.filter((server) => server.required && server.effectiveEnabled).length,
          serverCount: servers.length,
          missingRequiredSourceEnablement: [],
          missingRequiredCommands: [],
          activeProfile: clientId
        },
        runtime: {
          http: { activeConnections: sequence % 4, maxConnections: 16 },
          upstreamSessionPool: { size: sequence % 11, maxSize: 128, shardCount: 4 }
        },
        servers,
        clients: {
          targets,
          familyCounts: { claude: 2, cursor: 2, codex: 2, custom: 2 },
          surfaceClassCounts: { local: 4, remote: 4 },
          configuredClientKeyName: 'mcpServers'
        }
      };
    }

    window.__mcpaceFixture = { clientId, contextId, overviewCount: 0, logsCount: 0, actions: [] };
    window.__mcpaceClientSession = clientId;
    window.__mcpaceStartedSession = `started:${clientId}`;
    window.__mcpaceContextId = contextId;

    window.fetch = async (input, options = {}) => {
      const url = String(input);
      const signal = options.signal;
      if (url.startsWith('/api/overview')) {
        window.__mcpaceFixture.overviewCount += 1;
        const sequence = window.__mcpaceFixture.overviewCount;
        await delay(sequence % 3 === 0 ? 30 : 4, signal);
        return jsonResponse(200, makeOverview(sequence));
      }
      if (url.startsWith('/api/logs')) {
        window.__mcpaceFixture.logsCount += 1;
        const sequence = window.__mcpaceFixture.logsCount;
        await delay(sequence % 4 === 0 ? 20 : 3, signal);
        return jsonResponse(200, [{ event: `${clientId}-event-${sequence}`, level: 'info', tsMs: Date.now() }]);
      }
      if (url.startsWith('/api/actions/')) {
        window.__mcpaceFixture.actions.push(`${clientId}:${url}`);
        return jsonResponse(200, { ok: true, clientId, contextId, action: url });
      }
      return jsonResponse(404, { error: 'not found', clientId, contextId });
    };
  }, { clientId, contextId });
}

async function openStartedDashboardPage(context, clientId, contextId, suffix = 'primary') {
  const page = await context.newPage();
  const errors = [];
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  page.on('pageerror', (error) => errors.push(error.message));
  await installPageMock(page, clientId, contextId);
  await page.setContent(dashboardHtml, { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#runtime-ready')).not.toHaveText('—');
  await expect(page.locator('#root-path')).toContainText(`/tmp/mcpace-parallel-${clientId}-`);
  await page.evaluate((pageSuffix) => { window.__mcpacePageSuffix = pageSuffix; }, suffix);
  return { page, errors };
}

for (const clientId of clients) {
  test(`isolates already-started dashboard session for ${clientId}`, async ({ browser }, testInfo) => {
    const startedAt = Date.now();
    const contextId = `context:${clientId}:${testInfo.workerIndex}`;
    const context = await browser.newContext({ viewport: { width: 1180, height: 840 } });

    const primary = await openStartedDashboardPage(context, clientId, contextId, 'primary');
    await primary.page.locator('#server-search').fill(`${clientId}-server-00`);
    await primary.page.locator('#refresh-button').click();
    await primary.page.locator('#hub-up-button').click();

    const secondary = await openStartedDashboardPage(context, clientId, contextId, 'secondary');
    await secondary.page.locator('#toggle-enabled-filter').click();
    await secondary.page.locator('#refresh-button').click();

    const snapshot = await primary.page.evaluate(() => ({
      clientId: window.__mcpaceFixture.clientId,
      contextId: window.__mcpaceFixture.contextId,
      pageSuffix: window.__mcpacePageSuffix,
      refreshSeq: window.__mcpaceDashboard.state.refreshSeq,
      rootPath: window.__mcpaceDashboard.state.overview?.rootPath,
      clientSession: window.__mcpaceClientSession,
      startedSession: window.__mcpaceStartedSession,
      actions: window.__mcpaceFixture.actions.slice(),
      overviewCount: window.__mcpaceFixture.overviewCount,
      logsCount: window.__mcpaceFixture.logsCount
    }));
    const secondarySnapshot = await secondary.page.evaluate(() => ({
      clientId: window.__mcpaceFixture.clientId,
      contextId: window.__mcpaceFixture.contextId,
      pageSuffix: window.__mcpacePageSuffix,
      clientSession: window.__mcpaceClientSession,
      startedSession: window.__mcpaceStartedSession,
      rootPath: window.__mcpaceDashboard.state.overview?.rootPath
    }));

    expect(snapshot.clientId).toBe(clientId);
    expect(snapshot.contextId).toBe(contextId);
    expect(snapshot.clientSession).toBe(clientId);
    expect(snapshot.startedSession).toBe(`started:${clientId}`);
    expect(snapshot.rootPath).toContain(`/tmp/mcpace-parallel-${clientId}-`);
    expect(snapshot.actions).toContain(`${clientId}:/api/actions/hub-up`);
    expect(snapshot.overviewCount).toBeGreaterThanOrEqual(2);
    expect(snapshot.logsCount).toBeGreaterThanOrEqual(1);
    expect(secondarySnapshot.clientId).toBe(clientId);
    expect(secondarySnapshot.contextId).toBe(contextId);
    expect(secondarySnapshot.clientSession).toBe(clientId);
    expect(secondarySnapshot.startedSession).toBe(`started:${clientId}`);
    expect(secondarySnapshot.rootPath).toContain(`/tmp/mcpace-parallel-${clientId}-`);
    expect([...primary.errors, ...secondary.errors]).toEqual([]);

    if (stateDir) {
      fs.mkdirSync(stateDir, { recursive: true });
      fs.writeFileSync(path.join(stateDir, `${clientId}.json`), JSON.stringify({
        clientId,
        contextId,
        workerIndex: testInfo.workerIndex,
        parallelIndex: testInfo.parallelIndex,
        durationMs: Date.now() - startedAt,
        snapshot,
        secondarySnapshot
      }, null, 2));
    }

    await context.close();
  });
}
