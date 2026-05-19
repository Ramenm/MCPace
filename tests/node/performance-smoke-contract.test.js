const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { cleanChildEnv, repoRoot } = require('./helpers');

test('performance smoke harness emits bounded machine-readable proof', () => {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-perf-smoke-'));
  const jsonPath = path.join(tempDir, 'performance-smoke.json');
  const mdPath = path.join(tempDir, 'performance-smoke.md');
  const result = spawnSync(process.execPath, [
    'scripts/performance-smoke.mjs',
    '--json',
    '--requests', '8',
    '--concurrency', '2',
    '--servers', '20',
    '--tools', '2000',
    '--memory-limit-mib', '128',
    '--write', jsonPath,
    '--markdown', mdPath,
  ], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: cleanChildEnv(),
    timeout: 120000,
  });

  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
  assert.equal(report.schema, 'mcpace.performanceSmoke.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.reports.runtimeHttp.ok, true);
  assert.equal(report.reports.toolScale.status, 'pass');
  assert.equal(report.reports.mixedUpstreams.status, 'pass');
  assert.equal(report.reports.upstreamFailsafe.status, 'pass');
  assert.ok(report.summary.runtimeHttpMaxP95Ms >= 0);
  assert.ok(report.checks.some((check) => check.id === 'runtime-http-latency-measured' && check.ok));
  assert.ok(report.checks.some((check) => check.id === 'toolScale-heap-budget' && check.ok));
  assert.match(fs.readFileSync(mdPath, 'utf8'), /host-specific Rust binary benchmarking/);
});
