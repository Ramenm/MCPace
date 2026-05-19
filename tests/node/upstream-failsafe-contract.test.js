import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const read = (relative) => fs.readFileSync(path.join(repoRoot, relative), 'utf8');
const exists = (relative) => fs.existsSync(path.join(repoRoot, relative));

test('upstream fail-safe documentation is part of the adapter/runtime docs', () => {
  assert.equal(exists('docs/upstream-failsafe-hardening.md'), true);
  const doc = read('docs/upstream-failsafe-hardening.md');
  for (const required of [
    /stale cache/i,
    /circuit breaker/i,
    /partial-results/i,
    /stateful fail-fast/i,
    /flapping server/i,
    /stale `tools\/list` cache can help the user find a tool name, but `upstream_call` still needs a live callable upstream/i,
  ]) {
    assert.match(doc, required);
  }
  assert.match(read('docs/README.md'), /upstream-failsafe-hardening\.md/);
  assert.match(read('docs/dynamic-adapter.md'), /upstream-failsafe-hardening\.md/);
});

test('upstream fail-safe simulator covers degraded server and tool-call cases', () => {
  const result = spawnSync(process.execPath, [
    'scripts/simulate-upstream-failsafe.mjs',
    '--servers', '50',
    '--tools', '200000',
    '--json',
    '--memory-limit-mib', '512',
  ], {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: 30_000,
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.upstreamFailsafeSimulation.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.results.configuredToolSlots, 200000);
  assert.ok(report.results.healthyServers > 0);
  assert.ok(report.results.failedDiscoveryServers > 0);
  assert.ok(report.results.blockedServers > 0);
  assert.ok(report.results.staleFallbackServers > 0);
  assert.ok(report.results.circuitOpenServers > 0);
  assert.ok(report.results.upstreamOkCount > 0);
  assert.ok(report.results.upstreamFailedCount > 0);
  assert.ok(report.budgets.perServerFailureIsolation);
  assert.ok(report.budgets.staleCacheSemantics);
  assert.ok(report.budgets.circuitBreakerCovered);
  assert.ok(report.budgets.flappingRecoveryCovered);
  assert.ok(report.budgets.batchModesDiverge);
});

test('package scripts and readiness gates include upstream fail-safe proof', () => {
  const pkg = JSON.parse(read('package.json'));
  assert.match(pkg.scripts['verify:upstream-failsafe'], /simulate-upstream-failsafe\.mjs/);
  assert.match(pkg.scripts['benchmark:upstream-failsafe'], /--tools 500000/);
  assert.match(read('scripts/install-readiness-harness.mjs'), /upstream-failsafe/);
  assert.match(read('scripts/local-quality-suite.mjs'), /upstream-failsafe/);
});
