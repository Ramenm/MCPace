const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');

function readText(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(readText(relativePath));
}

test('linux auto-check and setup scripts are wired into npm scripts', () => {
  const pkg = readJson('package.json');
  assert.match(pkg.scripts['verify:linux:auto'], /scripts\/linux-auto-check\.mjs/);
  assert.match(pkg.scripts['verify:linux:auto:host'], /--no-docker/);
  assert.match(pkg.scripts['verify:linux:auto:full'], /--profile full/);
  assert.match(pkg.scripts['setup:linux:auto'], /scripts\/linux-auto-setup\.sh/);
});

test('linux auto-check script covers release hygiene, executable bits, and Docker install proof', () => {
  const script = readText('scripts/linux-auto-check.mjs');
  assert.match(script, /checkReleaseManifestHygiene/);
  assert.match(script, /checkVendoredExecutableBits/);
  assert.match(script, /test:linux-npm-install:docker/);
  assert.match(script, /MCPACE_LINUX_CHECK_PROFILE/);
});

test('linux verification checklist documents pass criteria and upstream proof', () => {
  const doc = readText('docs/linux-verification-checklist.md');
  assert.match(doc, /Pass\/fail rule/);
  assert.match(doc, /Clean Linux npm install proof/);
  assert.match(doc, /Upstream MCP server proof/);
  assert.match(doc, /Do not claim Alpine support/);
});

test('linux npm install docker proof does not hardcode the x64 platform package in summary', () => {
  const script = readText('scripts/verify-linux-npm-install-docker.mjs');
  assert.match(script, /TARGET_PACKAGE_NAME/);
  assert.doesNotMatch(script, /node_modules\/@mcpace\/cli-linux-x64-gnu\/package\.json/);
});

test('release archive hygiene excludes local machine-state directories', () => {
  const manifest = readJson('release-manifest.json');
  assert.ok(!manifest.includePaths.includes('.codex'));
  const archiveScript = readText('scripts/archive-release.mjs');
  for (const marker of ['.claude', '.codex', '.omc', '%SystemDrive%']) {
    assert.match(archiveScript, new RegExp(marker.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  }
});


test('linux auto workflow runs the auto-check and uploads its report', () => {
  const workflow = readText('.github/workflows/linux-auto.yml');
  assert.match(workflow, /npm run verify:linux:auto/);
  assert.match(workflow, /npm run verify:linux:auto:full/);
  assert.match(workflow, /reports\/linux-auto-check-latest\.json/);
  assert.match(workflow, /reports\/linux-auto-check-full-latest\.json/);
  assert.match(workflow, /persist-credentials: false/);
});
