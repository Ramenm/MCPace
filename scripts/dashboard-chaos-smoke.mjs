#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import vm from 'node:vm';
import { performance } from 'node:perf_hooks';
import { repoRoot, deriveProjectName, deriveProjectVersion } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const args = {
    json: false,
    write: 'reports/dashboard-chaos-smoke-latest.json',
    markdown: 'reports/dashboard-chaos-smoke-latest.md',
    tabs: 6,
    events: 120,
    servers: 150,
    clients: 24,
    maxElapsedMs: 15_000,
    maxOperationMs: 120,
    maxRenderMs: 90,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    const readValue = () => {
      const value = argv[index + 1];
      if (!value || value.startsWith('--')) throw new Error(`${token} requires a value`);
      index += 1;
      return value;
    };
    switch (token) {
      case '--json': args.json = true; break;
      case '--write': args.write = readValue(); break;
      case '--markdown': args.markdown = readValue(); break;
      case '--no-write': args.write = null; args.markdown = null; break;
      case '--tabs': args.tabs = parsePositiveInteger(readValue(), token); break;
      case '--events': args.events = parsePositiveInteger(readValue(), token); break;
      case '--servers': args.servers = parsePositiveInteger(readValue(), token); break;
      case '--clients': args.clients = parsePositiveInteger(readValue(), token); break;
      case '--max-elapsed-ms': args.maxElapsedMs = parsePositiveInteger(readValue(), token); break;
      case '--max-operation-ms': args.maxOperationMs = parsePositiveInteger(readValue(), token); break;
      case '--max-render-ms': args.maxRenderMs = parsePositiveInteger(readValue(), token); break;
      case '--help':
      case '-h':
        args.help = true;
        break;
      default:
        throw new Error(`unsupported dashboard-chaos-smoke argument: ${token}`);
    }
  }
  return args;
}

function parsePositiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}

function printHelp() {
  console.log(`Usage: node scripts/dashboard-chaos-smoke.mjs [options]

Runs a no-browser dashboard chaos smoke test by executing the embedded
src/dashboard/index.html script inside isolated VM tabs with a mocked DOM/fetch.
It exercises random refreshes, visibility changes, filter changes, action calls,
partial API failures, stale response cancellation, and render cost ceilings.

Options:
  --tabs 6                    Number of simulated tabs
  --events 120                Random operations per tab
  --servers 150               Mock server rows in overview payloads
  --clients 24                Mock client surfaces in overview payloads
  --max-elapsed-ms 15000      Total smoke budget
  --max-operation-ms 120      Per-event budget
  --max-render-ms 90          Render function budget
  --write <path>              JSON report path
  --markdown <path>           Markdown report path
  --no-write                  Do not write reports
  --json                      Print JSON report
`);
}

function extractDashboardScript() {
  const htmlPath = path.join(repoRoot, 'src/dashboard/index.html');
  const html = fs.readFileSync(htmlPath, 'utf8');
  const match = html.match(/<script>([\s\S]*?)<\/script>/);
  if (!match) throw new Error('dashboard script block not found');
  return { html, script: match[1] };
}

function makePrng(seed) {
  let value = seed >>> 0;
  return () => {
    value = (value * 1664525 + 1013904223) >>> 0;
    return value / 0x100000000;
  };
}

class MockElement {
  constructor(id) {
    this.id = id;
    this.textContent = '';
    this.innerHTML = '';
    this.value = '';
    this.disabled = false;
    this.listeners = new Map();
  }

  addEventListener(type, handler) {
    const handlers = this.listeners.get(type) || [];
    handlers.push(handler);
    this.listeners.set(type, handlers);
  }

  dispatch(type, payload = {}) {
    for (const handler of this.listeners.get(type) || []) {
      handler({ target: this, type, ...payload });
    }
  }
}

class MockDocument {
  constructor() {
    this.visibilityState = 'visible';
    this.elements = new Map();
    this.listeners = new Map();
  }

  getElementById(id) {
    if (!this.elements.has(id)) this.elements.set(id, new MockElement(id));
    return this.elements.get(id);
  }

  addEventListener(type, handler) {
    const handlers = this.listeners.get(type) || [];
    handlers.push(handler);
    this.listeners.set(type, handlers);
  }

  dispatch(type) {
    for (const handler of this.listeners.get(type) || []) {
      handler({ type });
    }
  }
}

function makeOverview(tabId, sequence, args) {
  const servers = Array.from({ length: args.servers }, (_, index) => ({
    name: `server-${String(index).padStart(3, '0')}`,
    kind: index % 5 === 0 ? 'streamable-http' : 'stdio',
    scopeClass: index % 7 === 0 ? 'project-local' : 'user',
    transportPreference: index % 5 === 0 ? 'streamable-http' : 'stdio-default',
    concurrencyPolicy: index % 3 === 0 ? 'shared' : 'isolated',
    effectiveEnabled: index % 4 !== 0,
    required: index % 11 === 0,
    stateBinding: index % 2 === 0 ? 'ephemeral' : 'persistent',
    credentialBinding: index % 6 === 0 ? 'user-secret' : 'none',
  }));
  const targets = Array.from({ length: args.clients }, (_, index) => ({
    id: `client-${index}`,
    displayName: `Client ${index}`,
    surfaceClass: index % 3 === 0 ? 'remote' : 'local',
    surfaceKind: index % 2 === 0 ? 'desktop' : 'web',
    supportedIngresses: index % 2 === 0 ? ['stdio', 'streamable-http'] : ['streamable-http'],
    nativeScopes: ['workspace', 'user', `tab-${tabId}`],
  }));
  return {
    rootPath: `/tmp/mcpace-tab-${tabId}`,
    generatedAtMs: Date.now() + sequence,
    cache: sequence % 3 === 0 ? { hit: true, ttlMs: 15000 } : { bypassed: sequence % 5 === 0 },
    doctor: { project: { rustSourceReady: true, npmSurfaceReady: true, containerToolingReady: false } },
    hub: {
      status: sequence % 9 === 0 ? 'stopped-ready' : 'running',
      health: 'healthy',
      warnings: sequence % 10 === 0 ? [`tab ${tabId} synthetic warning`] : [],
      readyForReadOnlyOps: true,
      readyForRuntimeOps: sequence % 8 !== 0,
      effectiveEnabledServerCount: servers.filter((server) => server.effectiveEnabled).length,
      requiredServerCount: servers.filter((server) => server.required).length,
      activeProfile: 'default',
    },
    readiness: {
      runtimePrerequisitesReady: sequence % 13 !== 0,
      sourceEnabledServerCount: servers.filter((server) => server.effectiveEnabled).length,
      profileEnabledServerCount: Math.floor(args.servers / 3),
      requiredServerCount: servers.filter((server) => server.required).length,
      requiredSourceEnabledCount: servers.filter((server) => server.required && server.effectiveEnabled).length,
      serverCount: servers.length,
      missingRequiredSourceEnablement: sequence % 17 === 0 ? ['paid-example'] : [],
      missingRequiredCommands: sequence % 19 === 0 ? ['uvx'] : [],
      activeProfile: 'default',
    },
    runtime: {
      http: { activeConnections: sequence % 8, maxConnections: 8 },
      upstreamSessionPool: { size: sequence % 23, maxSize: 128, shardCount: 4 },
    },
    servers,
    clients: {
      targets,
      familyCounts: { claude: 6, cursor: 5, codex: 4, custom: 9 },
      surfaceClassCounts: { local: 16, remote: 8 },
      configuredClientKeyName: 'mcpServers',
    },
  };
}

function makeLogs(tabId, sequence) {
  return Array.from({ length: 12 }, (_, index) => ({
    event: `event-${index}`,
    level: index % 7 === 0 ? 'warn' : 'info',
    tsMs: Date.now() - index * 1000,
    tabId,
    sequence,
  }));
}

function makeContext(tabId, script, args, prng) {
  const document = new MockDocument();
  const timers = new Map();
  let timerId = 0;
  let fetchCount = 0;
  let abortedFetches = 0;
  let partialFailures = 0;
  let sequence = 0;
  const consoleMessages = [];

  const window = {
    setTimeout(callback, delay) {
      timerId += 1;
      timers.set(timerId, { callback, delay });
      return timerId;
    },
    clearTimeout(id) {
      timers.delete(id);
    },
  };

  const fakeFetch = async (url, options = {}) => {
    fetchCount += 1;
    const signal = options.signal;
    const latency = 1 + Math.floor(prng() * 5);
    await new Promise((resolve) => setTimeout(resolve, latency));
    if (signal?.aborted) {
      abortedFetches += 1;
      const error = new Error('The operation was aborted.');
      error.name = 'AbortError';
      throw error;
    }
    const normalizedUrl = String(url);
    sequence += 1;
    if (normalizedUrl.startsWith('/api/logs') && sequence % 29 === 0) {
      partialFailures += 1;
      return { ok: false, status: 503, statusText: 'Service Unavailable', json: async () => ({ ok: false }) };
    }
    if (normalizedUrl.startsWith('/api/actions/')) {
      return { ok: true, status: 200, statusText: 'OK', json: async () => ({ ok: true, action: normalizedUrl }) };
    }
    if (normalizedUrl.startsWith('/api/logs')) {
      return { ok: true, status: 200, statusText: 'OK', json: async () => makeLogs(tabId, sequence) };
    }
    if (normalizedUrl.startsWith('/api/overview')) {
      return { ok: true, status: 200, statusText: 'OK', json: async () => makeOverview(tabId, sequence, args) };
    }
    return { ok: false, status: 404, statusText: 'Not Found', json: async () => ({ ok: false }) };
  };

  const context = vm.createContext({
    window,
    document,
    fetch: fakeFetch,
    console: {
      error(...parts) { consoleMessages.push(parts.map((part) => String(part?.message || part)).join(' ')); },
      log() {},
      warn(...parts) { consoleMessages.push(parts.map(String).join(' ')); },
    },
    AbortController,
    Promise,
    Date,
    JSON,
    String,
    Number,
    Array,
    Object,
    Math,
  });

  vm.runInContext(script, context, { filename: 'dashboard/index.html<script>' });
  return {
    tabId,
    document,
    window,
    context,
    get metrics() {
      return {
        fetchCount,
        abortedFetches,
        partialFailures,
        scheduledTimers: timers.size,
        consoleMessages: consoleMessages.length,
      };
    },
  };
}

async function flush() {
  await new Promise((resolve) => setTimeout(resolve, 8));
}

async function runOperation(tab, op, prng) {
  const dashboard = tab.window.__mcpaceDashboard;
  const started = performance.now();
  switch (op) {
    case 0:
      await dashboard.refreshDashboard({ force: true, reason: 'chaos-force' });
      break;
    case 1:
      await dashboard.refreshDashboard({ reason: 'chaos-auto' });
      break;
    case 2: {
      const input = tab.document.getElementById('server-search');
      input.value = prng() > 0.5 ? 'server-0' : 'streamable-http';
      input.dispatch('input');
      break;
    }
    case 3:
      tab.document.getElementById('toggle-enabled-filter').dispatch('click');
      break;
    case 4:
      tab.document.visibilityState = 'hidden';
      tab.document.dispatch('visibilitychange');
      await dashboard.refreshDashboard({ reason: 'hidden-auto' });
      break;
    case 5:
      tab.document.visibilityState = 'visible';
      tab.document.dispatch('visibilitychange');
      await flush();
      break;
    case 6:
      await dashboard.runAction('/api/actions/repair');
      break;
    case 7:
      dashboard.render();
      break;
    case 8: {
      const first = dashboard.refreshDashboard({ force: true, reason: 'chaos-overlap-a' });
      const second = dashboard.refreshDashboard({ force: true, reason: 'chaos-overlap-b' });
      await Promise.allSettled([first, second]);
      break;
    }
    default:
      await dashboard.refreshDashboard({ force: prng() > 0.65, reason: 'chaos-random' });
      break;
  }
  await flush();
  return Number((performance.now() - started).toFixed(2));
}

function sourceChecks(html, script) {
  return [
    { id: 'uses-visibilitychange', ok: /visibilitychange/.test(script), detail: 'hidden tabs are explicitly handled' },
    { id: 'uses-page-visibility-state', ok: /document\.visibilityState/.test(script), detail: 'dashboard can pause hidden-tab polling' },
    { id: 'uses-abort-controller', ok: /AbortController/.test(script), detail: 'overlapping refreshes can be cancelled' },
    { id: 'guards-stale-refreshes', ok: /refreshSeq/.test(script) && /refreshId !== state\.refreshSeq/.test(script), detail: 'out-of-order responses are ignored' },
    { id: 'uses-settimeout-not-setinterval', ok: /scheduleAutoRefresh/.test(script) && !/setInterval\s*\(/.test(script), detail: 'polling is re-armed after each refresh' },
    { id: 'partial-logs-failure-does-not-kill-overview', ok: /Promise\.allSettled/.test(script) && /Logs refresh failed/.test(script), detail: 'logs failure is degraded, not fatal' },
    { id: 'dashboard-test-hook-present', ok: /__mcpaceDashboard/.test(script), detail: 'smoke can exercise runtime functions without a browser dependency' },
    { id: 'refresh-mode-visible', ok: /id="refresh-mode"/.test(html), detail: 'operator can see refresh/backoff state' },
  ];
}

async function runChaos(args) {
  const { html, script } = extractDashboardScript();
  const staticChecks = sourceChecks(html, script);
  const prng = makePrng(0x6d637061);
  const tabs = Array.from({ length: args.tabs }, (_, index) => makeContext(index + 1, script, args, prng));
  await flush();

  const opDurations = [];
  const renderDurations = [];
  const started = performance.now();
  for (const tab of tabs) {
    const originalRender = tab.window.__mcpaceDashboard.render;
    tab.window.__mcpaceDashboard.render = () => {
      const renderStarted = performance.now();
      const result = originalRender();
      renderDurations.push(Number((performance.now() - renderStarted).toFixed(2)));
      return result;
    };
  }

  for (let eventIndex = 0; eventIndex < args.events; eventIndex += 1) {
    for (const tab of tabs) {
      const op = Math.floor(prng() * 9);
      opDurations.push(await runOperation(tab, op, prng));
    }
  }
  await flush();
  const elapsedMs = Number((performance.now() - started).toFixed(2));

  const maxOperationMs = opDurations.length ? Math.max(...opDurations) : 0;
  const avgOperationMs = opDurations.length
    ? opDurations.reduce((sum, value) => sum + value, 0) / opDurations.length
    : 0;
  const maxRenderMs = renderDurations.length ? Math.max(...renderDurations) : 0;
  const tabMetrics = tabs.map((tab) => ({
    tabId: tab.tabId,
    ...tab.metrics,
    refreshMode: tab.document.getElementById('refresh-mode').textContent,
    runtimeSummary: tab.document.getElementById('runtime-summary').textContent,
    serverListLength: tab.document.getElementById('server-list').innerHTML.length,
    warningListLength: tab.document.getElementById('warning-list').innerHTML.length,
  }));
  const dynamicChecks = [
    { id: 'chaos-elapsed-budget', ok: elapsedMs <= args.maxElapsedMs, detail: `elapsedMs=${elapsedMs}; budget=${args.maxElapsedMs}` },
    { id: 'operation-latency-budget', ok: maxOperationMs <= args.maxOperationMs, detail: `maxOperationMs=${maxOperationMs}; budget=${args.maxOperationMs}` },
    { id: 'render-latency-budget', ok: maxRenderMs <= args.maxRenderMs, detail: `maxRenderMs=${maxRenderMs}; budget=${args.maxRenderMs}` },
    { id: 'all-tabs-rendered-server-list', ok: tabMetrics.every((metric) => metric.serverListLength > 0), detail: `tabs=${tabMetrics.length}` },
    { id: 'all-tabs-kept-one-auto-timer', ok: tabMetrics.every((metric) => metric.scheduledTimers <= 1), detail: tabMetrics.map((metric) => `tab${metric.tabId}:${metric.scheduledTimers}`).join(', ') },
    { id: 'overlap-aborts-observed', ok: tabMetrics.some((metric) => metric.abortedFetches > 0), detail: `aborted=${tabMetrics.reduce((sum, metric) => sum + metric.abortedFetches, 0)}` },
    { id: 'partial-failures-contained', ok: tabMetrics.some((metric) => metric.partialFailures > 0) && tabMetrics.every((metric) => metric.serverListLength > 0), detail: `partialFailures=${tabMetrics.reduce((sum, metric) => sum + metric.partialFailures, 0)}` },
  ];

  return {
    schema: 'mcpace.dashboardChaosSmoke.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: [...staticChecks, ...dynamicChecks].every((check) => check.ok) ? 'pass' : 'fail',
    environment: {
      node: process.version,
      platform: process.platform,
      arch: process.arch,
      cpuCount: os.cpus().length,
      availableParallelism: os.availableParallelism?.() || os.cpus().length || null,
    },
    scenario: {
      tabs: args.tabs,
      eventsPerTab: args.events,
      servers: args.servers,
      clients: args.clients,
      maxElapsedMs: args.maxElapsedMs,
      maxOperationMs: args.maxOperationMs,
      maxRenderMs: args.maxRenderMs,
    },
    summary: {
      elapsedMs,
      totalOperations: opDurations.length,
      maxOperationMs,
      avgOperationMs: Number(avgOperationMs.toFixed(2)),
      maxRenderMs,
      renderCount: renderDurations.length,
      fetchCount: tabMetrics.reduce((sum, metric) => sum + metric.fetchCount, 0),
      abortedFetches: tabMetrics.reduce((sum, metric) => sum + metric.abortedFetches, 0),
      partialFailures: tabMetrics.reduce((sum, metric) => sum + metric.partialFailures, 0),
    },
    checks: [...staticChecks, ...dynamicChecks],
    tabMetrics,
  };
}

function markdownReport(report) {
  const lines = [];
  lines.push('# Dashboard chaos smoke report');
  lines.push('');
  lines.push(`Generated: ${report.generatedAt}`);
  lines.push(`Project: ${report.project.name} ${report.project.version}`);
  lines.push(`Status: **${report.status}**`);
  lines.push('');
  lines.push('## Scope');
  lines.push('');
  lines.push('- Executes the embedded dashboard script in isolated VM tabs with a mocked DOM and fetch layer.');
  lines.push('- Exercises random refreshes, force refresh, filter typing, enabled-only toggles, action calls, hidden/visible tab transitions, stale refresh cancellation, and partial log API failures.');
  lines.push('- This is a source-level chaos/regression smoke. It does not replace browser-engine Playwright coverage or host-specific Rust runtime testing.');
  lines.push('');
  lines.push('## Summary');
  lines.push('');
  lines.push(`- Tabs: ${report.scenario.tabs}`);
  lines.push(`- Total operations: ${report.summary.totalOperations}`);
  lines.push(`- Elapsed: ${report.summary.elapsedMs} ms`);
  lines.push(`- Max operation: ${report.summary.maxOperationMs} ms`);
  lines.push(`- Max render: ${report.summary.maxRenderMs} ms`);
  lines.push(`- Fetches: ${report.summary.fetchCount}`);
  lines.push(`- Aborted overlapping fetches: ${report.summary.abortedFetches}`);
  lines.push(`- Partial failures contained: ${report.summary.partialFailures}`);
  lines.push('');
  lines.push('## Checks');
  lines.push('');
  lines.push('| Check | Status | Detail |');
  lines.push('|---|---:|---|');
  for (const check of report.checks) {
    lines.push(`| ${check.id} | ${check.ok ? 'pass' : 'fail'} | ${String(check.detail).replace(/\|/g, '\\|')} |`);
  }
  lines.push('');
  lines.push('## Caveats');
  lines.push('');
  lines.push('- Hidden-tab behavior is simulated through `document.visibilityState`; real browsers can additionally throttle timers and background work.');
  lines.push('- Add a Playwright lane when a browser engine is available to verify layout, focus, real network timing, and real tab lifecycle.');
  lines.push('- No `cargo`/`rustc` host proof is implied by this report.');
  return `${lines.join('\n')}\n`;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
    return;
  }
  const report = await runChaos(args);
  if (args.write) {
    const outputPath = path.resolve(repoRoot, args.write);
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  }
  if (args.markdown) {
    const markdownPath = path.resolve(repoRoot, args.markdown);
    fs.mkdirSync(path.dirname(markdownPath), { recursive: true });
    fs.writeFileSync(markdownPath, markdownReport(report));
  }
  if (args.json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    console.log(`${report.status}: dashboard chaos smoke in ${report.summary.elapsedMs}ms; max op=${report.summary.maxOperationMs}ms; max render=${report.summary.maxRenderMs}ms`);
  }
  if (report.status !== 'pass') process.exitCode = 1;
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exitCode = 1;
});
