const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { readJson, repoRoot, packageVersion } = require('./helpers');

const platformPackagesModule = path.join(repoRoot, 'scripts', 'lib', 'npm-platform-packages.mjs');
const releaseTargetsModule = path.join(repoRoot, 'scripts', 'lib', 'release-targets.mjs');
const platformRuntimeModule = path.join(repoRoot, 'packages', 'npm', 'cli', 'lib', 'platform.js');

async function loadPlatformPackages() {
  return import(pathToFileURL(platformPackagesModule).href);
}

async function loadReleaseTargets() {
  return import(pathToFileURL(releaseTargetsModule).href);
}

async function loadPlatformRuntime() {
  return import(pathToFileURL(platformRuntimeModule).href);
}

test('release-targets.json is the source of truth for publishable native targets', async () => {
  const { releaseTargetsManifest, enabledReleaseTargets, githubMatrixInclude } = await loadReleaseTargets();
  const manifest = releaseTargetsManifest();
  const targets = enabledReleaseTargets();
  const matrix = githubMatrixInclude();

  assert.equal(manifest.schemaVersion, 1);
  assert.equal(manifest.mainPackageName, '@mcpace/cli');
  assert.equal(targets.length, 6);
  assert.deepEqual(new Set(targets.map((target) => target.key)).size, targets.length);
  assert.deepEqual(new Set(targets.map((target) => target.packageName)).size, targets.length);
  assert.deepEqual(matrix.map((entry) => entry.target_key).sort(), targets.map((target) => target.key).sort());
  assert.ok(targets.some((target) => target.key === 'win32-arm64-msvc'));
  assert.ok(Array.isArray(manifest.plannedTargets));
  assert.ok(manifest.plannedTargets.some((target) => target.key === 'linux-x64-musl'));
});

test('main npm launcher declares every native platform package as optional', async () => {
  const { PLATFORM_PACKAGE_TARGETS, expectedOptionalDependencies } = await loadPlatformPackages();
  const rootPackage = readJson('package.json');
  const cliPackage = readJson(path.join('packages', 'npm', 'cli', 'package.json'));
  const version = packageVersion();

  assert.deepEqual(rootPackage.workspaces, ['packages/npm/cli']);
  assert.deepEqual(cliPackage.optionalDependencies, expectedOptionalDependencies(version));
  assert.equal(PLATFORM_PACKAGE_TARGETS.length, 6);
  assert.ok(PLATFORM_PACKAGE_TARGETS.some((target) => target.key === 'win32-arm64-msvc'));
});

test('platform package manifests stay aligned with resolver targets and npm host filters', async () => {
  const { PLATFORM_PACKAGE_TARGETS, platformPackageJson } = await loadPlatformPackages();
  const { SUPPORTED_TARGETS, packageNamesForTarget } = await loadPlatformRuntime();
  const supportedKeys = new Set(SUPPORTED_TARGETS.map((target) => target.key));
  const version = packageVersion();

  for (const target of PLATFORM_PACKAGE_TARGETS) {
    assert.ok(supportedKeys.has(target.key), `${target.key} must be resolvable by the npm launcher`);
    const packagePath = path.join('packages', 'npm', `cli-${target.key}`, 'package.json');
    const manifest = readJson(packagePath);
    const expected = platformPackageJson(target, version);

    for (const key of ['name', 'version', 'license', 'os', 'cpu', 'libc', 'files', 'engines', 'publishConfig']) {
      assert.deepEqual(manifest[key] ?? null, expected[key] ?? null, `${packagePath}.${key}`);
    }

    assert.deepEqual(packageNamesForTarget(target), [target.packageName]);
    assert.equal(fs.existsSync(path.join(repoRoot, 'packages', 'npm', `cli-${target.key}`, 'README.md')), true);
    assert.equal(fs.existsSync(path.join(repoRoot, 'packages', 'npm', `cli-${target.key}`, 'LICENSE')), true);
  }
});
