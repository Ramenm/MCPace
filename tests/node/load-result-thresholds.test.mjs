import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { readRootPackageJson, repoRoot } from '../../scripts/lib/project-metadata.mjs';

const checker = path.join(repoRoot, 'scripts', 'check-load-result.mjs');

function writeReport(overrides = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-load-check-'));
  const report = {
    generatedAt: '2026-06-22T00:00:00.000Z',
    binary: '/tmp/mcpace',
    root: '/tmp/root',
    baseUrl: 'http://127.0.0.1:39022',
    options: { durationMs: 1000, concurrency: 2 },
    passed: true,
    scenarios: [
      { name: 'healthz readiness endpoint', method: 'GET', target: '/healthz', requests: 10, failed: 0, latencyMs: { p95: 10, p99: 12 } },
      { name: 'cached overview endpoint', method: 'GET', target: '/api/overview', requests: 10, failed: 0, latencyMs: { p95: 50, p99: 80 } },
      { name: 'MCP initialize POST', method: 'POST', target: '/mcp', requests: 10, failed: 0, latencyMs: { p95: 60, p99: 90 } },
    ],
    edgeProbes: [
      { name: 'rejects spoofed Host header', expected: [403], status: 403, pass: true },
      { name: 'rejects cross-origin MCP POST', expected: [403], status: 403, pass: true },
    ],
    ...overrides,
  };
  const file = path.join(dir, 'load.json');
  fs.writeFileSync(file, `${JSON.stringify(report, null, 2)}\n`);
  return { dir, file };
}

test('load result checker passes a clean load-test report', () => {
  const { dir, file } = writeReport();
  try {
    const result = spawnSync(process.execPath, [checker, file, '--json'], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const parsed = JSON.parse(result.stdout);
    assert.equal(parsed.schema, 'mcpace.loadResultCheck.v1');
    assert.equal(parsed.status, 'pass');
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('load result checker fails closed on latency and edge-probe regressions', () => {
  const { dir, file } = writeReport({
    passed: false,
    scenarios: [
      { name: 'healthz readiness endpoint', method: 'GET', target: '/healthz', requests: 10, failed: 1, latencyMs: { p95: 9999, p99: 9999 } },
    ],
    edgeProbes: [{ name: 'rejects spoofed Host header', expected: [403], status: 200, pass: false }],
  });
  try {
    const result = spawnSync(process.execPath, [checker, file, '--json'], { encoding: 'utf8' });
    assert.equal(result.status, 1, result.stdout || result.stderr);
    const parsed = JSON.parse(result.stdout);
    assert.equal(parsed.status, 'failed');
    assert.ok(parsed.failures.some((item) => item.includes('report.passed=false')));
    assert.ok(parsed.failures.some((item) => item.includes('p95=')));
    assert.ok(parsed.failures.some((item) => item.includes('edge probe failed')));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('load checker is shipped and wired as an npm script', () => {
  const packageJson = readRootPackageJson();
  const manifest = JSON.parse(fs.readFileSync(path.join(repoRoot, 'release-manifest.json'), 'utf8'));
  assert.equal(packageJson.scripts['check:load-result'], 'node scripts/check-load-result.mjs');
  assert.ok(manifest.includePaths.includes('scripts/check-load-result.mjs'));
});
