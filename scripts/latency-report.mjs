#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';

function printHelp() {
  console.log(`Usage: node scripts/latency-report.mjs <load-report.json> [--json]\n\nSummarizes client-side load-test latency plus the server-side runtime.http.latency and runtime.http.operations snapshots embedded by scripts/load-test-local.mjs.\n\nThe report also correlates matching client scenarios with server route measurements so high p95/p99 can be attributed to client/process overhead, server dispatch, parsing, or body reads.`);
}

function parseArgs(argv) {
  const json = argv.includes('--json');
  const help = argv.includes('-h') || argv.includes('--help');
  const file = argv.find((arg) => !arg.startsWith('-')) || '';
  return { json, help, file };
}

function parsePossiblyPrefixedJson(text) {
  const trimmed = text.trim();
  if (!trimmed) throw new Error('load report is empty');
  try {
    return JSON.parse(trimmed);
  } catch (firstError) {
    const start = trimmed.indexOf('{');
    const end = trimmed.lastIndexOf('}');
    if (start >= 0 && end > start) {
      try {
        return JSON.parse(trimmed.slice(start, end + 1));
      } catch {
        // Preserve the first parser error; it points at the original input.
      }
    }
    throw firstError;
  }
}

function parseReport(file) {
  if (!file) throw new Error('missing load report path');
  const absolute = path.resolve(file);
  const report = parsePossiblyPrefixedJson(fs.readFileSync(absolute, 'utf8'));
  if (!report || typeof report !== 'object' || Array.isArray(report)) throw new Error('load report must be a JSON object');
  return { absolute, report };
}

function latencyValue(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function routeForClientScenario(scenario) {
  const method = String(scenario.method || '').toUpperCase();
  const target = String(scenario.target || '');
  const [rawPath, rawQuery = ''] = target.split('?');
  const query = new URLSearchParams(rawQuery);
  const refresh = truthyQuery(query.get('refresh')) || truthyQuery(query.get('noCache'));
  if (method === 'GET' && rawPath === '/healthz') return refresh ? 'GET health.refresh' : 'GET health';
  if (method === 'GET' && rawPath === '/api/overview') return refresh ? 'GET api.overview.refresh' : 'GET api.overview.cached';
  if (method === 'GET' && rawPath === '/api/resources') return 'GET api.resources';
  if (method === 'POST' && rawPath === '/mcp') return 'POST mcp';
  if (method === 'GET' && rawPath === '/') return 'GET dashboard.index';
  return `${method} ${rawPath || 'unknown'}`;
}

function truthyQuery(value) {
  if (value === null || value === undefined) return false;
  const normalized = String(value).trim().toLowerCase();
  return normalized === '' || ['1', 'true', 'yes', 'on'].includes(normalized);
}

function clientRows(report) {
  return (Array.isArray(report.scenarios) ? report.scenarios : []).map((scenario) => ({
    layer: 'client',
    name: scenario.name || 'unnamed',
    route: routeForClientScenario(scenario),
    method: scenario.method || '',
    target: scenario.target || '',
    requests: Number(scenario.requests || 0),
    failed: Number(scenario.failed || 0),
    p50Ms: latencyValue(scenario.latencyMs?.p50),
    p95Ms: latencyValue(scenario.latencyMs?.p95),
    p99Ms: latencyValue(scenario.latencyMs?.p99),
    maxMs: latencyValue(scenario.latencyMs?.max),
    dispatchP95Ms: null,
    parseP95Ms: null,
    bodyReadP95Ms: null,
  }));
}

function runtimeSnapshots(report) {
  const snapshots = [];
  if (report.serverRuntime) snapshots.push(report.serverRuntime);
  for (const snapshot of Array.isArray(report.serverRuntimeSnapshots) ? report.serverRuntimeSnapshots : []) {
    if (snapshot?.runtime) snapshots.push(snapshot.runtime);
  }
  return snapshots;
}

function serverRows(report) {
  const rowsByRoute = new Map();
  for (const snapshot of runtimeSnapshots(report)) {
    const latency = snapshot?.payload?.runtime?.http?.latency;
    for (const route of Array.isArray(latency?.byRoute) ? latency.byRoute : []) {
      const row = {
        layer: 'server',
        name: route.route || 'unknown',
        route: route.route || 'unknown',
        method: '',
        target: '',
        requests: Number(route.count || 0),
        failed: Number(route.failed || 0),
        p50Ms: latencyValue(route.totalMs?.p50),
        p95Ms: latencyValue(route.totalMs?.p95),
        p99Ms: latencyValue(route.totalMs?.p99),
        maxMs: latencyValue(route.totalMs?.max),
        dispatchP95Ms: latencyValue(route.dispatchMs?.p95),
        parseP95Ms: latencyValue(route.parseMs?.p95),
        bodyReadP95Ms: latencyValue(route.bodyReadMs?.p95),
      };
      const existing = rowsByRoute.get(row.route);
      if (!existing || row.requests >= existing.requests) rowsByRoute.set(row.route, row);
    }
  }
  return [...rowsByRoute.values()];
}

function operationRows(report) {
  const operations = report.serverRuntime?.payload?.runtime?.http?.operations;
  return (Array.isArray(operations?.byName) ? operations.byName : []).map((operation) => ({
    layer: 'operation',
    name: operation.name || 'unknown',
    route: operation.name || 'unknown',
    method: '',
    target: '',
    requests: Number(operation.count || 0),
    failed: Number(operation.failed || 0),
    p50Ms: latencyValue(operation.durationMs?.p50),
    p95Ms: latencyValue(operation.durationMs?.p95),
    p99Ms: latencyValue(operation.durationMs?.p99),
    maxMs: latencyValue(operation.durationMs?.max),
    dispatchP95Ms: null,
    parseP95Ms: null,
    bodyReadP95Ms: null,
  }));
}

function worstRows(rows) {
  return [...rows]
    .sort((left, right) => right.p95Ms - left.p95Ms || right.failed - left.failed || right.requests - left.requests)
    .slice(0, 10);
}

function classifyCorrelation(client, server) {
  if (!server) return 'missing-server-route';
  if (client.failed > 0 || server.failed > 0) return 'failure-path-first';
  if (server.parseP95Ms > Math.max(server.dispatchP95Ms, server.bodyReadP95Ms) && server.parseP95Ms > 10) return 'server-parse-or-socket-bound';
  if (server.bodyReadP95Ms > Math.max(server.dispatchP95Ms, server.parseP95Ms) && server.bodyReadP95Ms > 10) return 'server-body-read-bound';
  if (server.dispatchP95Ms >= Math.max(10, server.p95Ms * 0.65)) return 'server-dispatch-bound';
  if (client.p95Ms > 0 && server.p95Ms <= client.p95Ms * 0.35 && client.p95Ms - server.p95Ms > 20) return 'client-process-or-network-bound';
  if (client.p95Ms > server.p95Ms * 1.5 && client.p95Ms - server.p95Ms > 10) return 'mixed-client-overhead';
  return 'server-and-client-aligned';
}

function correlationRows(client, server) {
  const serverByRoute = new Map(server.map((row) => [row.route, row]));
  return client.map((row) => {
    const match = serverByRoute.get(row.route) || null;
    const clientMinusServerP95Ms = match ? Math.round((row.p95Ms - match.p95Ms) * 100) / 100 : null;
    return {
      scenario: row.name,
      route: row.route,
      clientP95Ms: row.p95Ms,
      clientP99Ms: row.p99Ms,
      serverP95Ms: match?.p95Ms ?? null,
      serverP99Ms: match?.p99Ms ?? null,
      serverDispatchP95Ms: match?.dispatchP95Ms ?? null,
      serverParseP95Ms: match?.parseP95Ms ?? null,
      serverBodyReadP95Ms: match?.bodyReadP95Ms ?? null,
      clientMinusServerP95Ms,
      suspectedBottleneck: classifyCorrelation(row, match),
    };
  });
}

function summarize(report, absolute) {
  const client = clientRows(report);
  const server = serverRows(report);
  const operations = operationRows(report);
  const rows = [...client, ...server, ...operations];
  const correlations = correlationRows(client, server);
  return {
    schema: 'mcpace.latencyReport.v1',
    generatedAt: new Date().toISOString(),
    reportPath: absolute,
    source: {
      generatedAt: report.generatedAt || null,
      baseUrl: report.baseUrl || null,
      binary: report.binary || null,
      options: report.options || {},
      passed: Boolean(report.passed),
      serverRuntimeOk: Boolean(report.serverRuntime?.ok),
      serverRuntimeSnapshots: Array.isArray(report.serverRuntimeSnapshots) ? report.serverRuntimeSnapshots.length : 0,
    },
    totals: {
      rows: rows.length,
      clientRows: client.length,
      serverRows: server.length,
      operationRows: operations.length,
      correlations: correlations.length,
      missingServerRoutes: correlations.filter((row) => row.suspectedBottleneck === 'missing-server-route').length,
      failedRequests: rows.reduce((sum, row) => sum + row.failed, 0),
    },
    worstByP95: worstRows(rows),
    correlations,
    rows,
  };
}

function printText(summary) {
  console.log(`MCPace latency report: ${summary.source.passed ? 'source passed' : 'source failed'}`);
  console.log(`source: ${summary.reportPath}`);
  console.log(`baseUrl: ${summary.source.baseUrl || 'unknown'}`);
  console.log('\nWorst p95 rows:');
  for (const row of summary.worstByP95) {
    const dispatch = row.dispatchP95Ms === null ? '' : ` dispatchP95=${row.dispatchP95Ms}ms`;
    const parse = row.parseP95Ms === null ? '' : ` parseP95=${row.parseP95Ms}ms`;
    const body = row.bodyReadP95Ms === null ? '' : ` bodyReadP95=${row.bodyReadP95Ms}ms`;
    console.log(`- [${row.layer}] ${row.name}: p95=${row.p95Ms}ms p99=${row.p99Ms}ms max=${row.maxMs}ms failed=${row.failed}/${row.requests}${dispatch}${parse}${body}`);
  }
  console.log('\nClient/server correlation:');
  for (const row of summary.correlations) {
    const server = row.serverP95Ms === null ? 'serverP95=missing' : `serverP95=${row.serverP95Ms}ms dispatchP95=${row.serverDispatchP95Ms}ms`;
    console.log(`- ${row.scenario}: clientP95=${row.clientP95Ms}ms ${server} suspect=${row.suspectedBottleneck}`);
  }
}

try {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
  } else {
    const { absolute, report } = parseReport(args.file);
    const summary = summarize(report, absolute);
    if (args.json) process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
    else printText(summary);
    if (!summary.source.passed) process.exitCode = 1;
  }
} catch (error) {
  process.stderr.write(`${error?.message || String(error)}\n`);
  process.exitCode = 2;
}
