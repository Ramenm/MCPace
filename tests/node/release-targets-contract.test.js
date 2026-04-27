const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { read, readJson, packageVersion, repoRoot } = require('./helpers');

const expectedEnabledTargetKeys = [
  'linux-x64-gnu',
  'linux-arm64-gnu',
  'darwin-x64',
  'darwin-arm64',
  'win32-x64-msvc',
  'win32-arm64-msvc'
];

async function loadRuntimeTargets() {
  return import(pathToFileURL(path.join(repoRoot, 'packages', 'npm', 'cli', 'lib', 'targets.js')).href);
}

test('release target manifest is the package topology source of truth', async () => {
  const manifest = readJson('release-targets.json');
  const launcher = readJson('packages/npm/cli/package.json');
  const targetsJs = read('packages/npm/cli/lib/targets.js');
  const { SUPPORTED_TARGETS, PLANNED_TARGETS } = await loadRuntimeTargets();

  assert.equal(manifest.schemaVersion, 1);
  assert.equal(manifest.mainPackageName, '@mcpace/cli');
  const enabled = manifest.targets.filter((target) => target.publishEnabled !== false);
  assert.deepEqual(enabled.map((target) => target.key), expectedEnabledTargetKeys);
  assert.deepEqual(SUPPORTED_TARGETS.map((target) => target.key), expectedEnabledTargetKeys);
  assert.deepEqual(Object.keys(launcher.optionalDependencies).sort(), enabled.map((target) => target.npmPackage).sort());

  for (const target of enabled) {
    assert.match(targetsJs, new RegExp(`"key": "${target.key}"`));
    assert.match(targetsJs, new RegExp(`"packageName": "${target.npmPackage.replace('/', '\\/')}"`));
    assert.match(targetsJs, new RegExp(`"rustTarget": "${target.rustTarget}"`));
    const packageJson = readJson(path.join('packages', 'npm', `cli-${target.key}`, 'package.json'));
    assert.equal(packageJson.name, target.npmPackage);
    assert.equal(packageJson.version, packageVersion());
    assert.deepEqual(packageJson.os, target.os);
    assert.deepEqual(packageJson.cpu, target.cpu);
    if (target.libc) assert.deepEqual(packageJson.libc, target.libc);
    assert.equal(packageJson.publishConfig.access, 'public');
    assert.equal(fs.existsSync(path.join(repoRoot, 'packages', 'npm', `cli-${target.key}`, 'README.md')), true);
  }

  assert.deepEqual(PLANNED_TARGETS.map((target) => target.key), manifest.plannedTargets.map((target) => target.key));
});

test('planned targets are explicit and not advertised as supported by the npm launcher', async () => {
  const manifest = readJson('release-targets.json');
  const { SUPPORTED_TARGETS, PLANNED_TARGETS } = await loadRuntimeTargets();
  const supportedKeys = new Set(SUPPORTED_TARGETS.map((target) => target.key));

  assert.ok(manifest.plannedTargets.some((target) => target.key === 'linux-x64-musl'));
  assert.ok(manifest.plannedTargets.some((target) => target.key === 'linux-arm64-musl'));
  assert.deepEqual(PLANNED_TARGETS.map((target) => target.key), manifest.plannedTargets.map((target) => target.key));
  for (const target of manifest.plannedTargets) {
    assert.equal(typeof target.reason, 'string');
    assert.equal(supportedKeys.has(target.key), false);
  }
});
