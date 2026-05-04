const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { read } = require('./helpers');

test('release dry-run workflow proves source and platform package lanes without publishing', () => {
  const workflow = read(path.join('.github', 'workflows', 'release-dry-run.yml'));
  assert.match(workflow, /name: release-dry-run/);
  assert.match(workflow, /workflow_dispatch:/);
  assert.doesNotMatch(workflow, /pull_request:/);
  assert.match(workflow, /source-and-contracts:/);
  assert.match(workflow, /native_matrix:/);
  assert.match(workflow, /node scripts\/github-release-matrix\.mjs --github-output/);
  assert.match(workflow, /matrix: \${\{ fromJson\(needs\.source-and-contracts\.outputs\.native_matrix\) \}\}/);
  assert.match(workflow, /native-platform-proof:/);
  assert.match(workflow, /runtime-lifecycle-proof:/);
  assert.match(workflow, /node scripts\/run-rust-tests\.mjs --json --suite/);
  assert.match(workflow, /npm run test:rust:ci/);
  assert.match(workflow, /npm run build:release-artifacts/);
  assert.match(workflow, /node scripts\/stage-platform-package-binary\.mjs --json/);
  assert.match(workflow, /node scripts\/verify-platform-packages\.mjs --json/);
  assert.match(workflow, /actions\/cache@v5/);
  assert.match(workflow, /hashFiles\('Cargo\.lock', 'rust-toolchain\.toml'\)/);
  assert.match(workflow, /persist-credentials: false/);
  assert.doesNotMatch(workflow, /target_key: linux-x64-gnu/);
  assert.doesNotMatch(workflow, /target_key: darwin-x64/);
  assert.doesNotMatch(workflow, /npm publish/);
});

test('release workflow creates attestable assets and only drafts a GitHub Release on tags or explicit dispatch', () => {
  const workflow = read(path.join('.github', 'workflows', 'release.yml'));
  assert.match(workflow, /name: release-artifacts/);
  assert.match(workflow, /workflow_dispatch:/);
  assert.match(workflow, /tags:\r?\n\s+- 'v\*\.\*\.\*'/);
  assert.match(workflow, /id-token: write/);
  assert.match(workflow, /attestations: write/);
  assert.match(workflow, /source-release:/);
  assert.match(workflow, /native_matrix:/);
  assert.match(workflow, /node scripts\/github-release-matrix\.mjs --github-output/);
  assert.match(workflow, /matrix: \${\{ fromJson\(needs\.source-release\.outputs\.native_matrix\) \}\}/);
  assert.match(workflow, /native-platform-release:/);
  assert.match(workflow, /runtime-lifecycle-release-proof:/);
  assert.match(workflow, /node scripts\/run-rust-tests\.mjs --json --suite/);
  assert.match(workflow, /npm run test:rust:ci/);
  assert.match(workflow, /github-draft-release:/);
  assert.match(workflow, /runtime-lifecycle-release-proof/);
  assert.match(workflow, /release_tag:/);
  assert.match(workflow, /dist\/release-upload/);
  assert.match(workflow, /mcpace-\$\{target_key\}/);
  assert.match(workflow, /actions\/attest@v4/);
  assert.match(workflow, /node scripts\/stage-vendored-binary\.mjs --json/);
  assert.match(workflow, /node scripts\/verify-vendored-binary\.mjs --json/);
  assert.match(workflow, /node scripts\/stage-platform-package-binary\.mjs --json/);
  assert.match(workflow, /node scripts\/verify-platform-packages\.mjs --json/);
  assert.match(workflow, /actions\/cache@v5/);
  assert.match(workflow, /hashFiles\('Cargo\.lock', 'rust-toolchain\.toml'\)/);
  assert.match(workflow, /persist-credentials: false/);
  assert.doesNotMatch(workflow, /target_key: linux-x64-gnu/);
  assert.doesNotMatch(workflow, /target_key: darwin-x64/);
  assert.match(workflow, /gh release create/);
  assert.match(workflow, /--draft/);
});

test('GitHub workflows do not persist checkout credentials in read-only worktrees', () => {
  for (const workflowPath of [
    path.join('.github', 'workflows', 'ci.yml'),
    path.join('.github', 'workflows', 'release-dry-run.yml'),
    path.join('.github', 'workflows', 'release.yml'),
    path.join('.github', 'workflows', 'publish-npm.yml'),
  ]) {
    const workflow = read(workflowPath);
    const checkoutCount = (workflow.match(/uses: actions\/checkout@v6/g) || []).length;
    const disabledCredentialCount = (workflow.match(/persist-credentials: false/g) || []).length;
    assert.ok(checkoutCount > 0, `${workflowPath} must use checkout`);
    assert.equal(disabledCredentialCount, checkoutCount, `${workflowPath} must disable persisted checkout credentials`);
  }
});

test('npm publish workflow is manually gated for trusted publishing from prebuilt release tarballs', () => {
  const workflow = read(path.join('.github', 'workflows', 'publish-npm.yml'));
  assert.match(workflow, /name: publish-npm/);
  assert.match(workflow, /workflow_dispatch:/);
  assert.match(workflow, /id-token: write/);
  assert.match(workflow, /environment: npm-publish/);
  assert.doesNotMatch(workflow, /npm install -g/);
  assert.match(workflow, /package-manager-cache: false/);
  assert.match(workflow, /npm exec --yes --package=npm@11\.13\.0 -- npm --version/);
  assert.match(workflow, /MCPACE_NPM_EXEC_PACKAGE: npm@11\.13\.0/);
  assert.match(workflow, /gh release download/);
  assert.match(workflow, /node scripts\/verify-release-checksums\.mjs --json --artifact-dir dist\/npm/);
  assert.match(workflow, /node scripts\/sync-platform-packages\.mjs --json --repository-url/);
  assert.match(workflow, /node scripts\/verify-publish-readiness\.mjs --json/);
  assert.match(workflow, /node scripts\/publish-npm-artifacts\.mjs --json --artifact-dir dist\/npm/);
});

test('full docker proof script derives the expected binary version dynamically', () => {
  const script = read(path.join('scripts', 'verify-ubuntu-docker-full.mjs'));
  assert.match(script, /deriveProjectVersion/);
  assert.doesNotMatch(script, /0\\\.3\\\.0/);
  assert.ok(script.includes("expectedVersion.replace(/\\./g, '\\\\.')"));
});

test('docker proof scripts restore bind-mount permissions before host cleanup', () => {
  for (const scriptPath of [
    path.join('scripts', 'verify-ubuntu-docker-fast.mjs'),
    path.join('scripts', 'verify-ubuntu-docker-e2e.mjs'),
    path.join('scripts', 'verify-ubuntu-docker-full.mjs')
  ]) {
    const script = read(scriptPath);
    assert.match(script, /chmod -R a\+rwX \/work/);
  }
});

test('windows release archives avoid zip.exe backslash entries', () => {
  const script = read(path.join('scripts', 'archive-release.mjs'));
  assert.match(script, /process\.platform === 'win32'/);
  assert.match(script, /createArchiveWithPowerShell/);
  assert.match(script, /DirectorySeparatorChar/);
  assert.match(script, /AltDirectorySeparatorChar/);
});

test('full docker proof keeps the default no-upstream-server plan honest', () => {
  const script = read(path.join('scripts', 'verify-ubuntu-docker-full.mjs'));
  assert.match(script, /requiresHubOwnedStdio!==false/);
  assert.match(script, /const servers=Array\.isArray\(data\)\?data:Array\.isArray\(data\.servers\)\?data\.servers:null/);
});

test('linux npm install docker proof validates local tarballs without publishing', () => {
  const script = read(path.join('scripts', 'verify-linux-npm-install-docker.mjs'));
  assert.match(script, /sync-platform-packages\.mjs --json/);
  assert.match(script, /stage-platform-package-binary\.mjs --json --target-key/);
  assert.match(script, /verify-platform-packages\.mjs --json --target-key/);
  assert.match(script, /npm pack "packages\/npm\/cli-\$TARGET_KEY" --json/);
  assert.match(script, /npm install --ignore-scripts --no-audit --no-fund/);
  assert.match(script, /\.\/node_modules\/\.bin\/mcpace version/);
  assert.doesNotMatch(script, /npm publish/);
});

test('macOS proof-lane verifier keeps no-local-mac coverage explicit', () => {
  const script = read(path.join('scripts', 'verify-macos-proof-lanes.mjs'));
  assert.match(script, /darwin-x64/);
  assert.match(script, /darwin-arm64/);
  assert.match(script, /macos-15-intel/);
  assert.match(script, /macos-15/);
  assert.match(script, /MacOSLaunchMode::LaunchAgent/);
  assert.match(script, /--cargo-check/);
  assert.doesNotMatch(script, /npm publish/);
});
