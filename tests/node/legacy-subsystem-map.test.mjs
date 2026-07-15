import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';
import { listWorkingTreeFiles } from '../../scripts/lib/repo-files.mjs';

test('legacy subsystem map reports modernization seams by subsystem', () => {
  const result = spawnSync(process.execPath, ['scripts/legacy-subsystem-map.mjs', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.legacySubsystemMap.v1');
  assert.ok(report.rustFiles > 100);
  const ids = new Map(report.findings.map((item) => [item.id, item]));
  assert.equal(ids.get('dependencies.compat-crates')?.status, 'done');
  assert.equal(ids.get('source.generated-partials')?.status, 'done');
  assert.ok(['blocked', 'done'].includes(ids.get('dependencies.cargo-lock-refresh')?.status));
  assert.equal(ids.get('cli.manual-argv')?.replacement.includes('clap'), true);
  assert.equal(ids.get('config.lossless-editing')?.replacement.includes('toml_edit'), true);
  assert.equal(ids.get('mcp.stdio-preview')?.replacement.includes('mcpace stdio'), true);
});

test('checked-in eval sweep partial streams are removed from the source tree', () => {
  const offenders = listWorkingTreeFiles(repoRoot)
    .map((file) => file.replace(`${repoRoot}${path.sep}`, '').split(path.sep).join('/'))
    .filter((relative) => relative.endsWith('.partial.jsonl'));
  assert.deepEqual(offenders, []);
});
