import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('native npm package builder exists as the missing final publish lane', () => {
  const script = read('scripts/build-native-npm-package.mjs');
  assert.match(script, /mcpace\.nativeNpmPackageBuild\.v1/);
  assert.match(script, /validateBinaryForTarget/);
  assert.match(script, /copyRegularFileNoFollowSync/);
  assert.match(script, /npm', \['pack'/);
  assert.match(script, /mcpace:\s*\{/);
  assert.match(script, /rustTarget/);

  const result = spawnSync(process.execPath, ['scripts/build-native-npm-package.mjs', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.notEqual(result.status, 0, 'builder must fail closed without explicit target and binary');
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'failed');
});

test('publish workflow builds native packages before publishing the main launcher', () => {
  const workflow = read('.github/workflows/publish-npm.yml');
  assert.match(workflow, /native-packages:/);
  assert.match(workflow, /needs:\s*native-packages/);
  assert.match(workflow, /cargo fmt --check/);
  assert.match(workflow, /cargo clippy --all-targets --target/);
  assert.match(workflow, /cargo test --target/);
  assert.match(workflow, /node scripts\/build-native-npm-package\.mjs --target/);
  assert.match(workflow, /tarballs=\(dist\/npm\/\*\.tgz\)/);
  assert.match(workflow, /expected 6 native npm tarballs/);
  assert.match(workflow, /node scripts\/verify-npm-publish-contract\.mjs --enforce/);
  assert.ok(workflow.indexOf('Publish native npm packages') < workflow.indexOf('Publish main npm launcher'));
});


function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

test('publish workflow native matrix mirrors release target metadata', () => {
  const workflow = read('.github/workflows/publish-npm.yml');
  const releaseTargets = JSON.parse(read('release-targets.json'));
  const enabledTargets = releaseTargets.targets.filter((entry) => entry.publishEnabled !== false);
  for (const target of enabledTargets) {
    assert.match(workflow, new RegExp(`- key: ${escapeRegExp(target.key)}\\n`));
    assert.match(workflow, new RegExp(`rust_target: ${escapeRegExp(target.rustTarget)}`));
    assert.match(workflow, new RegExp(`binary_name: ${escapeRegExp(target.binaryName)}`));
  }
});

test('release completion documentation and scripts are part of the source release', () => {
  const manifest = JSON.parse(read('release-manifest.json'));
  assert.ok(manifest.includePaths.includes('docs/release-completion.md'));
  assert.ok(manifest.includePaths.includes('scripts/build-native-npm-package.mjs'));
  const docs = read('docs/release-completion.md');
  assert.match(docs, /A tarball that merely has the right filename is not enough/);
  assert.match(docs, /npm trusted publishers are configured/);
});
