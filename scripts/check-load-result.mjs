#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';

const DEFAULT_THRESHOLDS = Object.freeze({
  'healthz readiness endpoint': { p95Ms: 250, p99Ms: 750 },
  'cached overview endpoint': { p95Ms: 1000, p99Ms: 2500 },
  'MCP initialize POST': { p95Ms: 1000, p99Ms: 2500 },
  'runtime resources endpoint': { p95Ms: 500, p99Ms: 1500 },
  'refresh overview endpoint': { p95Ms: 5000, p99Ms: 10000 },
});

function printHelp() {
  console.log(`Usage: node scripts/check-load-result.mjs <load-report.json> [--json]\n\nValidates the JSON produced by scripts/load-test-local.mjs --json.\nThe checker fails closed on request failures, failed edge probes, empty scenarios, and latency SLO violations.\n\nEnvironment overrides:\n  MCPACE_SLO_FAILED_REQUESTS_MAX       Default: 0\n  MCPACE_SLO_HEALTHZ_P95_MS            Default: 250\n  MCPACE_SLO_HEALTHZ_P99_MS            Default: 750\n  MCPACE_SLO_OVERVIEW_P95_MS           Default: 1000\n  MCPACE_SLO_OVERVIEW_P99_MS           Default: 2500\n  MCPACE_SLO_MCP_INIT_P95_MS           Default: 1000\n  MCPACE_SLO_MCP_INIT_P99_MS           Default: 2500\n  MCPACE_SLO_REQUIRE_EDGE_PROBES       Default: 1`);
}

function parseArgs(argv) {
  const args = [...argv];
  const json = args.includes('--json');
  const help = args.includes('-h') || args.includes('--help');
  const positional = args.filter((arg) => !arg.startsWith('--') && arg !== '-h');
  return { help, json, file: positional[0] || '' };
}

function envNumber(name, fallback) {
  const raw = process.env[name];
  if (raw === undefined || raw === '') return fallback;
  const parsed = Number(raw);
  if (!Number.isFinite(parsed) || parsed < 0) throw new Error(`${name} must be a non-negative number`);
  return parsed;
}

function envBool(name, fallback) {
  const raw = process.env[name];
  if (raw === undefined || raw === '') return fallback;
  return !['0', 'false', 'no', 'off', 'disabled'].includes(String(raw).trim().toLowerCase());
}

function thresholdsFromEnv() {
  return {
    failedRequestsMax: envNumber('MCPACE_SLO_FAILED_REQUESTS_MAX', 0),
    requireEdgeProbes: envBool('MCPACE_SLO_REQUIRE_EDGE_PROBES', true),
    scenarios: {
      'healthz readiness endpoint': {
        p95Ms: envNumber('MCPACE_SLO_HEALTHZ_P95_MS', DEFAULT_THRESHOLDS['healthz readiness endpoint'].p95Ms),
        p99Ms: envNumber('MCPACE_SLO_HEALTHZ_P99_MS', DEFAULT_THRESHOLDS['healthz readiness endpoint'].p99Ms),
      },
      'cached overview endpoint': {
        p95Ms: envNumber('MCPACE_SLO_OVERVIEW_P95_MS', DEFAULT_THRESHOLDS['cached overview endpoint'].p95Ms),
        p99Ms: envNumber('MCPACE_SLO_OVERVIEW_P99_MS', DEFAULT_THRESHOLDS['cached overview endpoint'].p99Ms),
      },
      'MCP initialize POST': {
        p95Ms: envNumber('MCPACE_SLO_MCP_INIT_P95_MS', DEFAULT_THRESHOLDS['MCP initialize POST'].p95Ms),
        p99Ms: envNumber('MCPACE_SLO_MCP_INIT_P99_MS', DEFAULT_THRESHOLDS['MCP initialize POST'].p99Ms),
      },
      'runtime resources endpoint': {
        p95Ms: envNumber('MCPACE_SLO_RESOURCES_P95_MS', DEFAULT_THRESHOLDS['runtime resources endpoint'].p95Ms),
        p99Ms: envNumber('MCPACE_SLO_RESOURCES_P99_MS', DEFAULT_THRESHOLDS['runtime resources endpoint'].p99Ms),
      },
      'refresh overview endpoint': {
        p95Ms: envNumber('MCPACE_SLO_REFRESH_OVERVIEW_P95_MS', DEFAULT_THRESHOLDS['refresh overview endpoint'].p95Ms),
        p99Ms: envNumber('MCPACE_SLO_REFRESH_OVERVIEW_P99_MS', DEFAULT_THRESHOLDS['refresh overview endpoint'].p99Ms),
      },
    },
  };
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
        // Keep the original parser error; it points at the full user-provided input.
      }
    }
    throw firstError;
  }
}

function readReport(file) {
  if (!file) throw new Error('missing load report path');
  const absolute = path.resolve(file);
  const report = parsePossiblyPrefixedJson(fs.readFileSync(absolute, 'utf8'));
  if (!report || typeof report !== 'object' || Array.isArray(report)) throw new Error('load report must be a JSON object');
  return { absolute, report };
}

function numberAt(value, fallback = Number.NaN) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function checkScenario(scenario, thresholds, failures, warnings) {
  const name = String(scenario?.name || 'unnamed scenario');
  const requests = numberAt(scenario?.requests, 0);
  const failed = numberAt(scenario?.failed, 0);
  if (requests <= 0) failures.push(`${name}: no requests were recorded`);
  if (failed > thresholds.failedRequestsMax) failures.push(`${name}: failed=${failed} > ${thresholds.failedRequestsMax}`);

  const limits = thresholds.scenarios[name];
  if (!limits) {
    warnings.push(`${name}: no latency threshold configured`);
    return;
  }

  const p95 = numberAt(scenario?.latencyMs?.p95);
  const p99 = numberAt(scenario?.latencyMs?.p99);
  if (!Number.isFinite(p95)) failures.push(`${name}: missing latencyMs.p95`);
  else if (p95 > limits.p95Ms) failures.push(`${name}: p95=${p95}ms > ${limits.p95Ms}ms`);
  if (!Number.isFinite(p99)) failures.push(`${name}: missing latencyMs.p99`);
  else if (p99 > limits.p99Ms) failures.push(`${name}: p99=${p99}ms > ${limits.p99Ms}ms`);
}

function checkReport(report, thresholds) {
  const failures = [];
  const warnings = [];
  const scenarios = Array.isArray(report.scenarios) ? report.scenarios : [];
  const edgeProbes = Array.isArray(report.edgeProbes) ? report.edgeProbes : [];

  if (report.passed === false) failures.push('report.passed=false');
  if (scenarios.length === 0) failures.push('report.scenarios is empty or missing');
  for (const scenario of scenarios) checkScenario(scenario, thresholds, failures, warnings);

  if (thresholds.requireEdgeProbes && edgeProbes.length === 0) failures.push('edge probes are required but report.edgeProbes is empty or missing');
  for (const probe of edgeProbes) {
    if (!probe?.pass) failures.push(`edge probe failed: ${probe?.name || 'unnamed probe'} status=${probe?.status ?? 'unknown'}`);
  }

  if (report.serverRuntime && report.serverRuntime.ok === false) {
    failures.push(`serverRuntime probe failed: status=${report.serverRuntime.status ?? 'unknown'} error=${report.serverRuntime.error || ''}`);
  }
  const latency = report.serverRuntime?.payload?.runtime?.http?.latency;
  if (latency && latency.schema !== 'mcpace.httpLatency.v1') {
    warnings.push(`serverRuntime latency schema is unexpected: ${latency.schema || 'missing'}`);
  } else if (!latency) {
    warnings.push('serverRuntime runtime.http.latency snapshot is missing');
  }
  const operations = report.serverRuntime?.payload?.runtime?.http?.operations;
  if (operations && operations.schema !== 'mcpace.operationTrace.v1') {
    warnings.push(`serverRuntime operations schema is unexpected: ${operations.schema || 'missing'}`);
  } else if (!operations) {
    warnings.push('serverRuntime runtime.http.operations snapshot is missing');
  }

  return {
    schema: 'mcpace.loadResultCheck.v1',
    generatedAt: new Date().toISOString(),
    status: failures.length === 0 ? 'pass' : 'failed',
    summary: { scenarios: scenarios.length, edgeProbes: edgeProbes.length, failures: failures.length, warnings: warnings.length },
    thresholds,
    failures,
    warnings,
  };
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
    return;
  }
  const thresholds = thresholdsFromEnv();
  const { absolute, report } = readReport(args.file);
  const result = { ...checkReport(report, thresholds), reportPath: absolute };
  if (args.json) {
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  } else if (result.status === 'pass') {
    process.stdout.write(`load result check: pass (${args.file})\n`);
    for (const warning of result.warnings) process.stdout.write(`warning: ${warning}\n`);
  } else {
    process.stderr.write(`load result check: failed (${args.file})\n`);
    for (const failure of result.failures) process.stderr.write(`- ${failure}\n`);
    for (const warning of result.warnings) process.stderr.write(`warning: ${warning}\n`);
  }
  if (result.status !== 'pass') process.exitCode = 1;
}

try {
  main();
} catch (error) {
  process.stderr.write(`${error?.message || String(error)}\n`);
  process.exitCode = 2;
}
