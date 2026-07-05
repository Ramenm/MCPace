import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

test('modernization inventory detects remaining self-owned infrastructure seams without reintroducing compat crates', () => {
  const result = spawnSync(process.execPath, ['scripts/modernization-inventory.mjs', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.modernizationInventory.v1');
  const ids = new Set(report.findings.map((item) => item.id));
  assert.equal(ids.has('cargo-path-compat-dependencies'), false, 'upstream standard crates should not be redirected to crates/compat');
  if (ids.has('cargo-lock-needs-refresh')) {
    const lockFinding = report.findings.find((item) => item.id === 'cargo-lock-needs-refresh');
    assert.equal(lockFinding.severity, 'high');
    assert.ok(lockFinding.recommendation.includes('Cargo.lock'));
  }
  assert.ok(ids.has('manual-cli-parsing'), 'inventory should still find handwritten argv parsing until clap migration lands');
  assert.ok(ids.has('manual-config-patching'), 'inventory should find hand-written TOML/YAML patching until toml_edit migration lands');
});
