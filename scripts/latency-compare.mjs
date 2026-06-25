#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';

function printHelp() {
  console.log(`Usage: node scripts/latency-compare.mjs <baseline-load-report.json> <candidate-load-report.json> [options]\n\nCompares two MCPace load-test reports, for example release vs perf profile.\n\nOptions:\n  --json                       Emit JSON only\n  --fail-on-regression         Exit non-zero if any matching client scenario regresses beyond threshold\n  --p95-regression-pct <n>     Allowed p95 regression percent. Default: 15\n  --p99-regression-pct <n>     Allowed p99 regression percent. Default: 25`);
}

function parseArgs(argv) {
  const options = { json: false, failOnRegression: false, p95RegressionPct: 15, p99RegressionPct: 25, help: false };
  const files = [];
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const readValue = () => {
      const value = argv[index + 1];
      if (!value) throw new Error(`${arg} requires a value`);
      index += 1;
      return value;
    };
    switch (arg) {
      case '--json': options.json = true; break;
      case '--fail-on-regression': options.failOnRegression = true; break;
      case '--p95-regression-pct': options.p95RegressionPct = numeric(readValue(), arg); break;
      case '--p99-regression-pct': options.p99RegressionPct = numeric(readValue(), arg); break;
      case '-h':
      case '--help': options.help = true; break;
      default:
        if (arg.startsWith('-')) throw new Error(`unknown argument: ${arg}`);
        files.push(arg);
    }
  }
  return { ...options, baseline: files[0] || '', candidate: files[1] || '' };
}

function numeric(value, name) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 0) throw new Error(`${name} must be a non-negative number`);
  return parsed;
}

function parsePossiblyPrefixedJson(text) {
  const trimmed = text.trim();
  try {
    return JSON.parse(trimmed);
  } catch (firstError) {
    const start = trimmed.indexOf('{');
    const end = trimmed.lastIndexOf('}');
    if (start >= 0 && end > start) return JSON.parse(trimmed.slice(start, end + 1));
    throw firstError;
  }
}

function readReport(file) {
  if (!file) throw new Error('missing baseline or candidate report path');
  const absolute = path.resolve(file);
  const report = parsePossiblyPrefixedJson(fs.readFileSync(absolute, 'utf8'));
  return { absolute, report };
}

function numberAt(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function scenarioRows(report) {
  const map = new Map();
  for (const scenario of Array.isArray(report.scenarios) ? report.scenarios : []) {
    const key = scenario.name || `${scenario.method || ''} ${scenario.target || ''}`.trim() || 'unnamed';
    map.set(key, {
      name: key,
      method: scenario.method || '',
      target: scenario.target || '',
      requests: numberAt(scenario.requests),
      failed: numberAt(scenario.failed),
      rps: numberAt(scenario.rps),
      p50: numberAt(scenario.latencyMs?.p50),
      p95: numberAt(scenario.latencyMs?.p95),
      p99: numberAt(scenario.latencyMs?.p99),
      max: numberAt(scenario.latencyMs?.max),
    });
  }
  return map;
}

function pctDelta(baseline, candidate) {
  if (!baseline && !candidate) return 0;
  if (!baseline) return candidate > 0 ? 100 : 0;
  return ((candidate - baseline) / baseline) * 100;
}

function serverRows(report) {
  const map = new Map();
  const latency = report.serverRuntime?.payload?.runtime?.http?.latency;
  for (const route of Array.isArray(latency?.byRoute) ? latency.byRoute : []) {
    const key = route.route || 'unknown';
    map.set(key, {
      name: key,
      requests: numberAt(route.count),
      failed: numberAt(route.failed),
      p50: numberAt(route.totalMs?.p50),
      p95: numberAt(route.totalMs?.p95),
      p99: numberAt(route.totalMs?.p99),
      max: numberAt(route.totalMs?.max),
      dispatchP95: numberAt(route.dispatchMs?.p95),
    });
  }
  return map;
}

function operationRows(report) {
  const map = new Map();
  const operations = report.serverRuntime?.payload?.runtime?.http?.operations;
  for (const operation of Array.isArray(operations?.byName) ? operations.byName : []) {
    const key = operation.name || 'unknown';
    map.set(key, {
      name: key,
      requests: numberAt(operation.count),
      failed: numberAt(operation.failed),
      rps: 0,
      p50: numberAt(operation.durationMs?.p50),
      p95: numberAt(operation.durationMs?.p95),
      p99: numberAt(operation.durationMs?.p99),
      max: numberAt(operation.durationMs?.max),
    });
  }
  return map;
}

function compareMaps(left, right, options, layer) {
  const names = [...new Set([...left.keys(), ...right.keys()])].sort();
  const rows = [];
  const regressions = [];
  for (const name of names) {
    const base = left.get(name) || null;
    const cand = right.get(name) || null;
    const row = {
      layer,
      name,
      status: base && cand ? 'matched' : base ? 'missing-candidate' : 'new-candidate',
      baseline: base,
      candidate: cand,
      deltaP50Pct: null,
      deltaP95Pct: null,
      deltaP99Pct: null,
      deltaMaxPct: null,
      deltaRpsPct: null,
      deltaFailed: null,
    };
    if (base && cand) {
      row.deltaP50Pct = roundPct(pctDelta(base.p50, cand.p50));
      row.deltaP95Pct = roundPct(pctDelta(base.p95, cand.p95));
      row.deltaP99Pct = roundPct(pctDelta(base.p99, cand.p99));
      row.deltaMaxPct = roundPct(pctDelta(base.max, cand.max));
      row.deltaRpsPct = typeof base.rps === 'number' || typeof cand.rps === 'number' ? roundPct(pctDelta(base.rps || 0, cand.rps || 0)) : null;
      row.deltaFailed = cand.failed - base.failed;
      row.delta = {
        p50Pct: row.deltaP50Pct,
        p95Pct: row.deltaP95Pct,
        p99Pct: row.deltaP99Pct,
        maxPct: row.deltaMaxPct,
        rpsPct: row.deltaRpsPct,
        failed: row.deltaFailed,
      };
      if (layer === 'client') {
        if (row.deltaP95Pct > options.p95RegressionPct) regressions.push(`${name}: p95 regressed ${row.deltaP95Pct.toFixed(1)}% > ${options.p95RegressionPct}%`);
        if (row.deltaP99Pct > options.p99RegressionPct) regressions.push(`${name}: p99 regressed ${row.deltaP99Pct.toFixed(1)}% > ${options.p99RegressionPct}%`);
        if (row.deltaFailed > 0) regressions.push(`${name}: failed requests increased by ${row.deltaFailed}`);
      }
    }
    rows.push(row);
  }
  rows.sort((a, b) => (b.deltaP95Pct ?? -Infinity) - (a.deltaP95Pct ?? -Infinity));
  return { rows, regressions };
}

function roundPct(value) {
  return Math.round(value * 10) / 10;
}

function compareReports(baseline, candidate, options) {
  const clientComparison = compareMaps(scenarioRows(baseline.report), scenarioRows(candidate.report), options, 'client');
  const serverComparison = compareMaps(serverRows(baseline.report), serverRows(candidate.report), options, 'server');
  const operationComparison = compareMaps(operationRows(baseline.report), operationRows(candidate.report), options, 'operation');
  const regressions = [...clientComparison.regressions];
  const client = clientComparison.rows;
  const server = serverComparison.rows;
  const operations = operationComparison.rows;
  return {
    schema: 'mcpace.latencyComparison.v1',
    generatedAt: new Date().toISOString(),
    baseline: { path: baseline.absolute, generatedAt: baseline.report.generatedAt || null, binary: baseline.report.binary || null, passed: Boolean(baseline.report.passed) },
    candidate: { path: candidate.absolute, generatedAt: candidate.report.generatedAt || null, binary: candidate.report.binary || null, passed: Boolean(candidate.report.passed) },
    thresholds: { p95RegressionPct: options.p95RegressionPct, p99RegressionPct: options.p99RegressionPct },
    status: regressions.length === 0 ? 'pass' : 'regressed',
    summary: {
      clientRows: client.length,
      serverRows: server.length,
      operationRows: operations.length,
      improvements: client.filter((row) => (row.deltaP95Pct ?? 0) < 0).length,
      regressions: regressions.length,
    },
    regressions,
    client,
    server,
    operations,
    rows: [...client, ...server, ...operations],
  };
}

function printText(summary) {
  console.log(`MCPace latency compare: ${summary.status}`);
  console.log(`baseline: ${summary.baseline.path}`);
  console.log(`candidate: ${summary.candidate.path}`);
  if (summary.regressions.length) {
    console.log('\nRegressions:');
    for (const item of summary.regressions) console.log(`- ${item}`);
  }
  console.log('\nScenario deltas:');
  for (const row of summary.client.filter((item) => item.status === 'matched').slice(0, 10)) {
    console.log(`- ${row.name}: p95 ${row.baseline.p95} -> ${row.candidate.p95}ms (${row.deltaP95Pct.toFixed(1)}%), p99 ${row.baseline.p99} -> ${row.candidate.p99}ms (${row.deltaP99Pct.toFixed(1)}%), rps ${row.baseline.rps} -> ${row.candidate.rps} (${(row.deltaRpsPct ?? 0).toFixed(1)}%)`);
  }
}

try {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    printHelp();
  } else {
    const summary = compareReports(readReport(options.baseline), readReport(options.candidate), options);
    if (options.json) process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
    else printText(summary);
    if (options.failOnRegression && summary.status !== 'pass') process.exitCode = 1;
  }
} catch (error) {
  process.stderr.write(`${error?.message || String(error)}\n`);
  process.exitCode = 2;
}
