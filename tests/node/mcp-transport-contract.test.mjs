import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { repoRoot, readRootPackageJson } from '../../scripts/lib/project-metadata.mjs';

function runScript(args = []) {
  return spawnSync(process.execPath, ['scripts/mcp-transport-contract.mjs', '--json', ...args], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
    maxBuffer: 4 * 1024 * 1024,
  });
}

test('MCP transport contract checker keeps stdio, Streamable HTTP, and local dashboard boundaries explicit', () => {
  const result = runScript();
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.mcpTransportContract.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.failures, 0);
  for (const id of [
    'stdio-newline-jsonrpc-framing',
    'stdio-diagnostics-stay-on-stderr',
    'streamable-http-post-accept-contract',
    'streamable-http-session-lifecycle',
    'http-host-origin-boundary-centralized',
    'dashboard-security-response-headers',
  ]) {
    assert.ok(report.findings.some((item) => item.id === id && item.status === 'pass'), `${id} should pass`);
  }
});

test('MCP transport contract checker is wired into package and CI scripts', () => {
  const packageJson = readRootPackageJson();
  assert.equal(packageJson.scripts['check:mcp-transport'], 'node scripts/mcp-transport-contract.mjs --json');
  assert.match(packageJson.scripts['check:ci'], /check-ci\.mjs/);
  const ciList = spawnSync(process.execPath, ['scripts/check-ci.mjs', '--list', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(ciList.status, 0, ciList.stderr || ciList.stdout);
  const listed = JSON.parse(ciList.stdout);
  assert.ok(listed.steps.some((step) => step.label === 'check:mcp-transport'));
});
