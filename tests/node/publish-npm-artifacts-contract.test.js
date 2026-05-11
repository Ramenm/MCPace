const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { repoRoot } = require('./helpers');

async function loadPublishModule() {
  return import(pathToFileURL(path.join(repoRoot, 'scripts', 'publish-npm-artifacts.mjs')).href);
}

test('publish npm artifact runner uses exact npm exec package without global install when configured', async () => {
  // Arrange
  const { buildNpmInvocation } = await loadPublishModule();

  // Act
  const invocation = buildNpmInvocation(['publish', 'dist/npm/mcpace.tgz', '--dry-run'], {
    platform: 'linux',
    env: { MCPACE_NPM_EXEC_PACKAGE: 'npm@11.13.0' },
  });

  // Assert
  assert.equal(invocation.command, 'npm');
  assert.deepEqual(invocation.args, [
    'exec',
    '--yes',
    '--package=npm@11.13.0',
    '--',
    'npm',
    'publish',
    'dist/npm/mcpace.tgz',
    '--dry-run',
  ]);
  assert.match(invocation.displayCommand, /npm exec --yes --package=npm@11\.13\.0 -- npm publish/);
});

test('publish npm artifact runner falls back to normal npm command when exact package is not configured', async () => {
  // Arrange
  const { buildNpmInvocation } = await loadPublishModule();

  // Act
  const invocation = buildNpmInvocation(['view', '@mcpace/cli@0.4.1', 'version', '--json'], {
    platform: 'linux',
    env: {},
  });

  // Assert
  assert.equal(invocation.command, 'npm');
  assert.deepEqual(invocation.args, ['view', '@mcpace/cli@0.4.1', 'version', '--json']);
  assert.equal(invocation.displayCommand, 'npm view @mcpace/cli@0.4.1 version --json');
});

test('publish npm artifact runner keeps Windows shell invocation deterministic', async () => {
  // Arrange
  const { buildNpmInvocation } = await loadPublishModule();

  // Act
  const invocation = buildNpmInvocation(['publish', 'artifact.tgz'], {
    platform: 'win32',
    env: { MCPACE_NPM_EXEC_PACKAGE: 'npm@11.13.0' },
  });

  // Assert
  assert.equal(invocation.command, 'cmd.exe');
  assert.deepEqual(invocation.args, [
    '/d',
    '/s',
    '/c',
    'npm',
    'exec',
    '--yes',
    '--package=npm@11.13.0',
    '--',
    'npm',
    'publish',
    'artifact.tgz',
  ]);
});
