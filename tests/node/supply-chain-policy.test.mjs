import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';

const repoRoot = path.resolve(import.meta.dirname, '../..');

function nodeScript(script, args = [], options = {}) {
  return spawnSync(process.execPath, [path.join(repoRoot, script), ...args], {
    cwd: repoRoot,
    encoding: 'utf8',
    ...options,
  });
}

test('dependency policy passes for the repository lockfile and npm defaults', () => {
  const result = nodeScript('scripts/verify-dependency-policy.mjs', ['--json']);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'pass');
  assert.equal(report.failures, 0);
});

test('package lock omits hoisted native optional packages from source installs', () => {
  const cliPackage = JSON.parse(fs.readFileSync(path.join(repoRoot, 'packages/npm/cli/package.json'), 'utf8'));
  const lock = JSON.parse(fs.readFileSync(path.join(repoRoot, 'package-lock.json'), 'utf8'));
  for (const name of Object.keys(cliPackage.optionalDependencies ?? {})) {
    assert.equal(lock.packages?.[`node_modules/${name}`], undefined, `${name} should not be locked as a hoisted registry package in source installs`);
    assert.deepEqual(
      lock.packages?.[`packages/npm/cli/node_modules/${name}`],
      { version: cliPackage.version, optional: true },
      `${name} should be represented as an omitted optional workspace stub with an explicit version`,
    );
  }
});

test('workflow policy passes without SHA enforcement and keeps tag-pinning as warnings', () => {
  const result = nodeScript('scripts/verify-workflow-policy.mjs', ['--json']);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.ok(['pass', 'warn'].includes(report.status));
  assert.equal(report.failures, 0);
  assert.ok(report.findings.some((item) => item.id === 'release-attestation-step' && item.status === 'pass'));
});

test('workflow policy rejects direct untrusted expression interpolation in inline shell', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-workflow-policy-'));
  try {
    const workflowDir = path.join(dir, '.github/workflows');
    fs.mkdirSync(workflowDir, { recursive: true });
    fs.writeFileSync(path.join(workflowDir, 'bad.yml'), `name: bad\non: workflow_dispatch\npermissions:\n  contents: read\njobs:\n  bad:\n    runs-on: ubuntu-latest\n    steps:\n      - run: |\n          echo "${'${{ inputs.name }}'}"\n`);
    fs.writeFileSync(path.join(workflowDir, 'publish-npm.yml'), `name: publish-npm\non: workflow_dispatch\npermissions:\n  contents: read\n  id-token: write\njobs:\n  publish:\n    if: startsWith(github.ref, 'refs/tags/')\n    environment: npm-publish\n    runs-on: ubuntu-latest\n    steps:\n      - run: node scripts/verify-npm-publish-contract.mjs --enforce\n      - run: npm exec --yes --package=npm@11.13.0 -- npm publish\n`);
    fs.writeFileSync(path.join(workflowDir, 'release.yml'), `name: release\non: workflow_dispatch\npermissions:\n  contents: read\n  id-token: write\n  attestations: write\njobs:\n  release:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/attest@v4\n        with:\n          subject-path: dist/*\n`);
    const result = nodeScript('scripts/verify-workflow-policy.mjs', ['--json', '--repo', dir]);
    assert.notEqual(result.status, 0, result.stdout);
    const report = JSON.parse(result.stdout);
    assert.equal(report.status, 'fail');
    assert.ok(report.findings.some((item) => item.id === 'workflow-inline-expression' && item.status === 'fail'));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('dependency policy rejects lockfiles without integrity on external packages', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-dep-policy-'));
  try {
    fs.mkdirSync(path.join(dir, 'packages/npm/cli'), { recursive: true });
    fs.writeFileSync(path.join(dir, '.npmrc'), 'ignore-scripts=true\n');
    fs.writeFileSync(path.join(dir, 'package.json'), JSON.stringify({ name: 'root', version: '1.0.0' }));
    fs.writeFileSync(path.join(dir, 'packages/npm/cli/package.json'), JSON.stringify({ name: '@mcpace/cli', version: '1.0.0', optionalDependencies: {} }));
    fs.writeFileSync(path.join(dir, 'package-lock.json'), JSON.stringify({ lockfileVersion: 3, packages: { '': {}, 'node_modules/bad': { name: 'bad', version: '1.0.0', resolved: 'https://registry.npmjs.org/bad/-/bad-1.0.0.tgz' }, 'packages/npm/cli': { name: '@mcpace/cli', version: '1.0.0' } } }));
    const result = nodeScript('scripts/verify-dependency-policy.mjs', ['--json', '--repo', dir]);
    assert.notEqual(result.status, 0, result.stdout);
    const report = JSON.parse(result.stdout);
    assert.equal(report.status, 'fail');
    assert.ok(report.findings.some((item) => item.id === 'external-packages-have-integrity' && item.status === 'fail'));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
