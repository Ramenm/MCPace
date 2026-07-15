import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { readRootPackageJson, repoRoot } from '../../scripts/lib/project-metadata.mjs';

function runBoundary(args = []) {
  return spawnSync(process.execPath, ['scripts/rust-boundary-contract.mjs', '--json', ...args], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
    maxBuffer: 8 * 1024 * 1024,
  });
}

test('Rust boundary contract pins typed seams and low-level HTTP ownership', () => {
  const result = runBoundary();
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.rustBoundaryContract.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.failures, 0);
  assert.equal(report.modernizationInventory.stringlyErrors, 16);
  assert.equal(report.modernizationInventory.rawHttpTcp, 4);
  for (const id of [
    'stringly-error-budget-tightened',
    'typed-boundary:src/init.rs',
    'typed-boundary:src/projects.rs',
    'typed-boundary:src/profile.rs',
    'typed-boundary:src/mcp_sources.rs',
    'typed-boundary:src/hub/runtime.rs',
    'typed-boundary:src/upstream/tool_cache.rs',
    'typed-boundary:src/upstream/stdio_runtime.rs',
    'typed-boundary:src/server/policy.rs',
    'typed-boundary:src/upstream/inventory.rs',
    'typed-boundary:src/upstream/session_pool.rs',
    'raw-http-tcp-allowlist',
    'stdio-jsonrpc-newline-boundary',
    'mcp-source-symlink-boundary',
  ]) {
    assert.ok(report.findings.some((item) => item.id === id && item.status === 'pass'), `${id} should pass`);
  }
});

test('Rust boundary contract is wired into npm scripts and CI/endgame gates', () => {
  const packageJson = readRootPackageJson();
  assert.equal(packageJson.scripts['check:rust-boundaries'], 'node scripts/rust-boundary-contract.mjs --json');

  const ciList = spawnSync(process.execPath, ['scripts/check-ci.mjs', '--json', '--list'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(ciList.status, 0, ciList.stderr || ciList.stdout);
  const listed = JSON.parse(ciList.stdout);
  assert.ok(listed.steps.some((step) => step.label === 'check:rust-boundaries'));

  const endgame = spawnSync(process.execPath, ['scripts/endgame-readiness.mjs', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
    maxBuffer: 8 * 1024 * 1024,
  });
  assert.equal(endgame.status, 0, endgame.stderr || endgame.stdout);
  const report = JSON.parse(endgame.stdout);
  assert.ok(report.findings.some((item) => item.id === 'rust-boundary-contract'));
});
