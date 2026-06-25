import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';
import { trustedNpmCliPath } from '../../scripts/lib/process.mjs';

function runPublishContract(args = ['--json']) {
  const result = spawnSync(process.execPath, ['scripts/verify-npm-publish-contract.mjs', ...args], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  return result;
}

test('npm publish contract detects missing native package artifacts before release publish', () => {
  const result = runPublishContract();
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.npmPublishContract.v1');
  assert.equal(report.mainPackageName, '@mcpace/cli');
  assert.equal(report.enabledTargetCount, 6);
  assert.equal(report.publishable, false, 'source-only bundle must not be considered directly publishable to npm');
  assert.equal(report.binaryPackageGaps.length, 6, 'all enabled native target packages must be accounted for before publish');
  assert.ok(report.binaryPackageGaps.every((gap) => gap.packageName.startsWith('@mcpace/cli-')));
  assert.ok(report.binaryPackageProof.every((entry) => Object.hasOwn(entry, 'sourceBinaryPath')));
  assert.ok(report.binaryPackageProof.every((entry) => Object.hasOwn(entry, 'tarballStatus')));
  assert.ok(report.binaryPackageGaps.every((gap) => /native binary|prebuilt npm tarball/.test(gap.reason)));
  const binaryCheck = report.checks.find((entry) => entry.id === 'binary-packages-or-tarballs-exist');
  assert.equal(binaryCheck?.status, 'failed');
});

test('npm publish workflow uses pinned npm for publish and enforces native package contract', () => {
  const workflow = fs.readFileSync(path.join(repoRoot, '.github', 'workflows', 'publish-npm.yml'), 'utf8');
  assert.match(workflow, /node scripts\/verify-npm-publish-contract\.mjs --enforce/);
  assert.match(workflow, /build-native-npm-package\.mjs/);
  assert.match(workflow, /Download native package artifacts/);
  assert.match(workflow, /Publish native npm packages/);
  assert.match(workflow, /npm exec --yes --package=npm@11\.13\.0 -- npm publish --dry-run --access public/);
  assert.match(workflow, /npm exec --yes --package=npm@11\.13\.0 -- npm publish --access public/);
  assert.doesNotMatch(workflow, /\n\s+npm publish(?:\s|$)/, 'workflow must not publish with an ambient npm binary');
});

test('npm publish enforce mode fails closed when native packages are not staged', () => {
  const result = runPublishContract(['--enforce']);
  assert.notEqual(result.status, 0, 'enforce mode must fail closed until native package artifacts exist');
  const report = JSON.parse(result.stdout);
  assert.equal(report.publishable, false);
  assert.ok(report.failedChecks.some((entry) => entry.id === 'binary-packages-or-tarballs-exist'));
});



test('npm publish contract does not trust empty native package source directories', () => {
  const script = fs.readFileSync(path.join(repoRoot, 'scripts', 'verify-npm-publish-contract.mjs'), 'utf8');
  assert.match(script, /sourcePackageBinaryPath/);
  assert.match(script, /path\.join\(packageDir, 'bin', binaryName\)/);
  assert.match(script, /&& sourceBinaryPath/);
  assert.match(script, /expected native binary/);
  assert.match(script, /packageTargetMetadata/);
  assert.match(script, /binary-package-target-metadata-matches-release-targets/);
  assert.match(script, /verifyNativePackageTarball/);
  assert.match(script, /package\/package\.json/);
  assert.match(script, /package\/bin\/\$\{binaryName\}/);
  assert.match(script, /bin\.mcpace/);
});

test('release source ZIP includes the npm publish contract guard script', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(repoRoot, 'release-manifest.json'), 'utf8'));
  assert.ok(manifest.includePaths.includes('scripts/verify-npm-publish-contract.mjs'));
  assert.ok(manifest.includePaths.includes('scripts/build-native-npm-package.mjs'));
  assert.ok(manifest.includePaths.includes('docs/release-completion.md'));
});


test('workspace check:publish-contract script also fails closed when native packages are not staged', () => {
  const npmCli = trustedNpmCliPath('npm');
  assert.ok(npmCli, 'could not resolve trusted npm CLI path');
  const result = spawnSync(process.execPath, [npmCli, 'run', 'check:publish-contract'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.notEqual(result.status, 0, 'package.json script must not turn a blocked publish contract into a green check');
  const jsonStart = result.stdout.indexOf('{');
  assert.notEqual(jsonStart, -1, result.stdout);
  const report = JSON.parse(result.stdout.slice(jsonStart));
  assert.equal(report.enforce, true);
  assert.equal(report.publishable, false);
  assert.ok(report.failedChecks.some((entry) => entry.id === 'binary-packages-or-tarballs-exist'));
});
