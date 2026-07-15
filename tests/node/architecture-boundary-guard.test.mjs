import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

test('architecture boundary guard enforces phase-two module and legacy boundaries', () => {
  const result = spawnSync(process.execPath, ['scripts/architecture-boundary-guard.mjs', '--json', '--enforce'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.architectureBoundaryGuard.v1');
  assert.equal(report.status, 'pass');
  const ids = new Map(report.checks.map((check) => [check.id, check]));
  assert.equal(ids.get('inline-test-modules')?.actual, 0);
  assert.equal(ids.get('service-legacy-cleanup-quarantined')?.ok, true);
  assert.ok(ids.get('service-rs-production-lines')?.actual <= ids.get('service-rs-production-lines')?.max);
});
