import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const repoRoot = path.resolve(import.meta.dirname, '..', '..');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function runAssuranceJson() {
  const result = spawnSync(process.execPath, ['scripts/project-assurance.mjs', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return JSON.parse(result.stdout);
}

test('project assurance model checks user-visible truth, safety, and unverified gates', () => {
  const report = runAssuranceJson();
  assert.equal(report.schema, 'mcpace.projectAssurance.v1');
  assert.equal(report.summary.fail, 0);
  assert.ok(report.summary.pass >= 8, 'expected most assurance claims to be statically proven');
  assert.equal(report.overall, 'needs-live-rust-proof');

  const claimIds = new Set(report.claims.map((claim) => claim.id));
  for (const id of [
    'safe-empty-default',
    'user-readiness',
    'operator-plan',
    'frontend-backend-contract',
    'server-launch-visible',
    'add-server-preflight',
    'http-boundary',
    'human-in-loop-tools',
    'release-reproducibility',
    'rust-runtime-unverified-here',
    'live-e2e-unverified-here',
  ]) {
    assert.ok(claimIds.has(id), `missing assurance claim ${id}`);
  }

  assert.ok(
    report.correctVerificationFlow.some((step) => step.includes('check:rust')),
    'assurance flow must not hide the Rust-host gate',
  );
  assert.ok(
    report.reviewModel.some((item) => /hidden by default/i.test(item.question)),
    'assurance model must state what the user should not see',
  );
});

test('assurance artifacts are part of the release bundle contract', () => {
  const manifest = JSON.parse(read('release-manifest.json'));
  for (const required of [
    'scripts/project-assurance.mjs',
    'reports/assurance.md',
    'reports/assurance.json',
  ]) {
    assert.ok(manifest.includePaths.includes(required), `release manifest missing ${required}`);
  }

  const packageJson = JSON.parse(read('package.json'));
  assert.match(packageJson.scripts.assurance, /project-assurance\.mjs --write/);
  assert.match(packageJson.scripts['check:assurance'], /project-assurance\.mjs --check/);
  assert.match(packageJson.scripts.check, /check:assurance/);
});
