const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { readJson, repoRoot } = require('./helpers');

async function loadMatrixModule() {
  return import(pathToFileURL(path.join(repoRoot, 'scripts', 'github-release-matrix.mjs')).href);
}

test('GitHub release matrix is generated from release-targets.json', async () => {
  const manifest = readJson('release-targets.json');
  const { nativeReleaseMatrix, releaseMatricesReport } = await loadMatrixModule();
  const enabled = manifest.targets.filter((target) => target.publishEnabled !== false);
  const matrix = nativeReleaseMatrix();
  const report = releaseMatricesReport();

  assert.equal(report.status, 'pass');
  assert.deepEqual(matrix, report.nativeMatrix);
  assert.equal(JSON.parse(report.nativeMatrixJson).include.length, enabled.length);
  assert.deepEqual(
    matrix.include.map((entry) => entry.target_key),
    enabled.map((target) => target.key)
  );

  for (const target of enabled) {
    const entry = matrix.include.find((candidate) => candidate.target_key === target.key);
    assert.ok(entry, `${target.key} must be present in the GitHub Actions matrix`);
    assert.equal(entry.os, target.runner);
    assert.equal(entry.rust_target, target.rustTarget);
    assert.equal(entry.package_name, target.packageName);
    assert.equal(entry.binary_name, target.binaryName);
  }
});
