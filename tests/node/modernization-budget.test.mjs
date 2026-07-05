import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

test('modernization budget keeps remaining legacy seams intentional and bounded', () => {
  const result = spawnSync(process.execPath, ['scripts/verify-modernization-budget.mjs', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.modernizationBudget.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.failures, 0);
  assert.ok(report.findings.some((item) => item.id === 'manual-cli-parsing' && item.actual <= item.max));
  assert.ok(report.findings.some((item) => item.id === 'cargo-lock-needs-refresh' && item.actual <= item.max));
});
