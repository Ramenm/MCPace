import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const repoRoot = path.resolve(import.meta.dirname, '..', '..');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

test('local proof script exposes a safe current-host command plan', () => {
  const result = spawnSync(process.execPath, ['scripts/local-proof.mjs', '--plan-only', '--json', '--full'], {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.localProof.v1');
  assert.equal(report.mode.planOnly, true);
  assert.equal(report.mode.full, true);
  assert.ok(report.results.some((item) => item.id === 'node-contracts' && /npm(\.cmd)? run check/.test(item.command)));
  assert.ok(report.results.some((item) => item.id === 'release-dry-run'));
  assert.ok(report.results.some((item) => item.id === 'source-zip-build'));
  assert.ok(report.results.some((item) => item.id === 'rust-contracts'));
});

test('platform testing instructions and local proof ship in the source bundle', () => {
  const packageJson = readJson('package.json');
  assert.match(packageJson.scripts['proof:local'], /local-proof\.mjs --write/);
  assert.match(packageJson.scripts['proof:local:plan'], /local-proof\.mjs --plan-only --json/);

  const docs = read('docs/platform-testing.md');
  assert.match(docs, /npm run proof:local -- --full/);
  assert.match(docs, /Windows PowerShell/);
  assert.match(docs, /platform-proof/);

  const manifest = readJson('release-manifest.json');
  for (const required of [
    'docs/platform-testing.md',
    'scripts/local-proof.mjs',
  ]) {
    assert.ok(manifest.includePaths.includes(required), `release manifest missing ${required}`);
  }
});
