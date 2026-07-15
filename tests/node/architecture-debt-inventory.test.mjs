import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

test('architecture debt inventory reports large modules, public surface, and legacy hotspots', () => {
  const result = spawnSync(process.execPath, ['scripts/architecture-debt-inventory.mjs', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.architectureDebtInventory.v1');
  assert.ok(report.summary.rustFiles > 0);
  assert.ok(report.summary.largeRustProductionFiles > 0);
  assert.ok(report.rust.topProductionRustFiles.some((item) => item.file === 'src/server/loader.rs'));
  assert.ok(report.libSurface.publicModuleCount > 0);
  assert.ok(report.legacy.hotspotLiterals.some((item) => item.literal === 'sse-legacy'));
  assert.ok(report.recommendedSplits.length > 0);
});

test('legacy boundary guard keeps legacy and compatibility markers quarantined', () => {
  const result = spawnSync(process.execPath, ['scripts/legacy-boundary-guard.mjs', '--json', '--enforce'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.legacyBoundaryGuard.v1');
  assert.equal(report.status, 'pass');
  assert.ok(report.allowedFiles > 0);
});
