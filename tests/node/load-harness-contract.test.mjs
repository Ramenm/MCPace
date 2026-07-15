import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { readRootPackageJson, repoRoot } from '../../scripts/lib/project-metadata.mjs';

const loadHarness = path.join(repoRoot, 'scripts', 'load-test-local.mjs');

test('local load harness bounds server worker fanout before launching the binary', () => {
  const result = spawnSync(process.execPath, [loadHarness, '--max-connections', '257'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 1, result.stdout || result.stderr);
  assert.match(result.stderr, /--max-connections must be <= 256/);
});

test('local load harness bounds the global active-request overload probe', () => {
  const result = spawnSync(process.execPath, [loadHarness, '--global-active-request-limit', '1025'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 1, result.stdout || result.stderr);
  assert.match(result.stderr, /--global-active-request-limit must be <= 1024/);
});

test('local load harness bounds server request bodies before launching the binary', () => {
  const result = spawnSync(process.execPath, [loadHarness, '--max-body-bytes', '16777217'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 1, result.stdout || result.stderr);
  assert.match(result.stderr, /--max-body-bytes must be <= 16777216/);
});

test('local load harness documents its adaptive server connection default', () => {
  const result = spawnSync(process.execPath, [loadHarness, '--help'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(result.stdout, /Default: min\(256, max\(16, concurrency \* 2\)\)/);
  assert.match(result.stdout, /--global-active-request-limit <n>/);
  assert.match(result.stdout, /--max-requests-per-scenario <n>/);
  assert.match(result.stdout, /Default: 100; use 0 for duration-only stress/);
});


test('local load harness captures server-side runtime latency evidence', () => {
  const source = fs.readFileSync(loadHarness, 'utf8');
  assert.match(source, /DEFAULT_MAX_REQUESTS_PER_SCENARIO = 100/);
  assert.match(source, /takeRequestSlot/);
  assert.match(source, /waitForReadyHttp/);
  assert.match(source, /server did not pass loopback readiness check/);
  assert.match(source, /cached overview warmup/);
  assert.match(source, /warmups\.every/);
  assert.match(source, /requestJson\(\{ port: server\.port, target: '\/api\/resources' \}\)/);
  assert.match(source, /serverRuntime/);
  assert.match(source, /serverRuntimeSnapshots/);
  assert.match(source, /runtime resources endpoint/);
  assert.match(source, /refresh overview endpoint/);
  assert.match(source, /target: '\/api\/overview\?refresh=1',[\s\S]*expectedStatuses: \[200, 429\]/);
  assert.match(source, /Server-side latency snapshot/);
  assert.match(source, /Server-side operation snapshot/);
});

test('load result checker and latency report understand expanded latency evidence', () => {
  const checker = path.join(repoRoot, 'scripts', 'check-load-result.mjs');
  const reporter = path.join(repoRoot, 'scripts', 'latency-report.mjs');
  const sample = path.join(repoRoot, '.tmp-load-report-contract.json');
  fs.writeFileSync(sample, JSON.stringify({
    generatedAt: '2026-06-23T00:00:00.000Z',
    binary: '/tmp/mcpace',
    root: '/tmp/root',
    baseUrl: 'http://127.0.0.1:0',
    options: {},
    scenarios: [
      { name: 'healthz readiness endpoint', method: 'GET', target: '/healthz', requests: 1, failed: 0, latencyMs: { p95: 1, p99: 1 } },
      { name: 'cached overview endpoint', method: 'GET', target: '/api/overview', requests: 1, failed: 0, latencyMs: { p95: 1, p99: 1 } },
      { name: 'runtime resources endpoint', method: 'GET', target: '/api/resources', requests: 1, failed: 0, latencyMs: { p95: 1, p99: 1 } },
      { name: 'refresh overview endpoint', method: 'GET', target: '/api/overview?refresh=1', requests: 1, failed: 0, latencyMs: { p95: 1, p99: 1 } },
      { name: 'MCP initialize POST', method: 'POST', target: '/mcp', requests: 1, failed: 0, latencyMs: { p95: 1, p99: 1 } },
    ],
    edgeProbes: [{ name: 'edge', pass: true, status: 403, expected: [403] }],
    serverRuntime: {
      ok: true,
      status: 200,
      payload: { runtime: { http: { latency: { schema: 'mcpace.httpLatency.v1', byRoute: [{ route: 'GET health', count: 1, failed: 0, totalMs: { p50: 1, p95: 1, p99: 1, max: 1 }, dispatchMs: { p95: 0.5 } }] }, operations: { schema: 'mcpace.operationTrace.v1', byName: [{ name: 'cache.health', count: 1, failed: 0, durationMs: { p50: 1, p95: 1, p99: 1, max: 1 } }] } } } },
    },
    passed: true,
  }), 'utf8');
  try {
    const check = spawnSync(process.execPath, [checker, sample, '--json'], { cwd: repoRoot, encoding: 'utf8', windowsHide: true });
    assert.equal(check.status, 0, check.stderr || check.stdout);
    const report = spawnSync(process.execPath, [reporter, sample, '--json'], { cwd: repoRoot, encoding: 'utf8', windowsHide: true });
    assert.equal(report.status, 0, report.stderr || report.stdout);
    const parsed = JSON.parse(report.stdout);
    assert.equal(parsed.schema, 'mcpace.latencyReport.v1');
    assert.equal(parsed.totals.serverRows, 1);
    assert.equal(parsed.totals.operationRows, 1);
  } finally {
    fs.rmSync(sample, { force: true });
  }
});

test('latency report correlates client scenarios with separated server route labels', () => {
  const reporter = path.join(repoRoot, 'scripts', 'latency-report.mjs');
  const sample = path.join(repoRoot, '.tmp-latency-correlation.json');
  fs.writeFileSync(sample, JSON.stringify({
    generatedAt: '2026-06-23T00:00:00.000Z',
    binary: '/tmp/mcpace',
    root: '/tmp/root',
    baseUrl: 'http://127.0.0.1:0',
    options: {},
    scenarios: [
      { name: 'cached overview endpoint', method: 'GET', target: '/api/overview', requests: 10, failed: 0, latencyMs: { p95: 100, p99: 120 } },
      { name: 'refresh overview endpoint', method: 'GET', target: '/api/overview?refresh=1', requests: 10, failed: 0, latencyMs: { p95: 900, p99: 1000 } },
    ],
    edgeProbes: [{ name: 'edge', pass: true, status: 403, expected: [403] }],
    serverRuntime: {
      ok: true,
      status: 200,
      payload: { runtime: { http: { latency: { schema: 'mcpace.httpLatency.v1', byRoute: [
        { route: 'GET api.overview.cached', count: 10, failed: 0, totalMs: { p50: 20, p95: 30, p99: 40, max: 50 }, parseMs: { p95: 1 }, bodyReadMs: { p95: 1 }, dispatchMs: { p95: 25 } },
        { route: 'GET api.overview.refresh', count: 10, failed: 0, totalMs: { p50: 700, p95: 850, p99: 950, max: 1100 }, parseMs: { p95: 1 }, bodyReadMs: { p95: 1 }, dispatchMs: { p95: 840 } },
      ] } } } },
    },
    passed: true,
  }), 'utf8');
  try {
    const result = spawnSync(process.execPath, [reporter, sample, '--json'], { cwd: repoRoot, encoding: 'utf8', windowsHide: true });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const parsed = JSON.parse(result.stdout);
    assert.equal(parsed.totals.missingServerRoutes, 0);
    assert.ok(parsed.correlations.some((row) => row.route === 'GET api.overview.cached'));
    assert.ok(parsed.correlations.some((row) => row.route === 'GET api.overview.refresh' && row.suspectedBottleneck === 'server-dispatch-bound'));
  } finally {
    fs.rmSync(sample, { force: true });
  }
});

test('latency comparison reports release-vs-perf p95 deltas', () => {
  const comparer = path.join(repoRoot, 'scripts', 'latency-compare.mjs');
  const left = path.join(repoRoot, '.tmp-latency-left.json');
  const right = path.join(repoRoot, '.tmp-latency-right.json');
  const report = (p95) => ({
    binary: '/tmp/mcpace',
    passed: true,
    scenarios: [{ name: 'MCP initialize POST', method: 'POST', target: '/mcp', requests: 10, failed: 0, rps: 100, latencyMs: { p95, p99: p95 + 10, max: p95 + 20 } }],
    serverRuntime: { ok: true, payload: { runtime: { http: { latency: { byRoute: [{ route: 'POST mcp', count: 10, failed: 0, totalMs: { p95, p99: p95 + 5, max: p95 + 10 }, dispatchMs: { p95: p95 - 1 } }] }, operations: { byName: [{ name: 'mcp.initialize', count: 10, failed: 0, durationMs: { p95, p99: p95 + 4, max: p95 + 8 } }] } } } } },
  });
  fs.writeFileSync(left, JSON.stringify(report(100)), 'utf8');
  fs.writeFileSync(right, JSON.stringify(report(80)), 'utf8');
  try {
    const result = spawnSync(process.execPath, [comparer, left, right, '--json'], { cwd: repoRoot, encoding: 'utf8', windowsHide: true });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const parsed = JSON.parse(result.stdout);
    assert.equal(parsed.schema, 'mcpace.latencyComparison.v1');
    assert.equal(parsed.rows[0].delta.p95Pct, -20);
    assert.equal(parsed.summary.improvements >= 1, true);
    assert.equal(parsed.summary.operationRows, 1);
    assert.ok(parsed.operations.some((row) => row.name === 'mcp.initialize'));
  } finally {
    fs.rmSync(left, { force: true });
    fs.rmSync(right, { force: true });
  }
});

test('runtime proof and slow-client probe scripts are wired as explicit npm scripts', () => {
  const packageJson = readRootPackageJson();
  const manifest = JSON.parse(fs.readFileSync(path.join(repoRoot, 'release-manifest.json'), 'utf8'));
  assert.equal(packageJson.scripts['proof:runtime'], 'node scripts/runtime-proof.mjs');
  assert.equal(packageJson.scripts['fault:slow-client'], 'node scripts/slow-client-probe.mjs');
  assert.equal(packageJson.scripts['latency:compare'], 'node scripts/latency-compare.mjs');
  for (const script of ['scripts/runtime-proof.mjs', 'scripts/slow-client-probe.mjs', 'scripts/latency-compare.mjs']) {
    assert.ok(manifest.includePaths.includes(script), `${script} must ship in the source bundle`);
    const syntax = spawnSync(process.execPath, ['--check', path.join(repoRoot, script)], { encoding: 'utf8', windowsHide: true });
    assert.equal(syntax.status, 0, syntax.stderr || syntax.stdout);
  }
});

test('runtime evidence suite adds session churn and started-server slowloris probes', () => {
  const packageJson = readRootPackageJson();
  const manifest = JSON.parse(fs.readFileSync(path.join(repoRoot, 'release-manifest.json'), 'utf8'));
  const expectedScripts = {
    'evidence:runtime': 'scripts/runtime-evidence.mjs',
    'probe:session-churn': 'scripts/session-churn-probe.mjs',
    'probe:slowloris': 'scripts/slowloris-probe.mjs',
  };
  for (const [npmScript, script] of Object.entries(expectedScripts)) {
    assert.equal(packageJson.scripts[npmScript], `node ${script}`);
    assert.ok(manifest.includePaths.includes(script), `${script} must ship in the source bundle`);
    const syntax = spawnSync(process.execPath, ['--check', path.join(repoRoot, script)], { encoding: 'utf8', windowsHide: true });
    assert.equal(syntax.status, 0, syntax.stderr || syntax.stdout);
  }
  assert.ok(manifest.includePaths.includes('scripts/lib/runtime-probe.mjs'), 'shared runtime probe helper must ship');

  const plan = spawnSync(process.execPath, [path.join(repoRoot, 'scripts', 'runtime-evidence.mjs'), '--plan', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(plan.status, 0, plan.stderr || plan.stdout);
  const parsed = JSON.parse(plan.stdout);
  assert.equal(parsed.schema, 'mcpace.runtimeEvidence.v1');
  assert.ok(parsed.plan.some((step) => step.name === 'probe:session-churn'));
  assert.ok(parsed.plan.some((step) => step.name === 'probe:slowloris'));
});
