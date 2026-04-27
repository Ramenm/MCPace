const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { spawnSync } = require('node:child_process');
const { cleanChildEnv, readJson, repoRoot } = require('./helpers');

const proofScript = path.join(repoRoot, 'scripts', 'proof-report.mjs');
const clientCatalogModule = path.join(repoRoot, 'scripts', 'lib', 'client-catalog.mjs');

async function loadCatalogHelpers() {
  return import(pathToFileUrl(clientCatalogModule));
}

test('proof report cli emits a deterministic no-run report', async () => {
  const result = spawnSync(
    process.execPath,
    [proofScript, '--json', '--no-run', '--checked-at', '2026-04-19T21:00:00Z'],
    {
      cwd: repoRoot,
      encoding: 'utf8',
      env: cleanChildEnv()
    }
  );

  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(result.stdout);
  const truth = readJson('docs/product-truth.json');
  const { resolveInstallSupportTargets, resolveProofFocusTargets } = await loadCatalogHelpers();
  const proofFocusSurfaces = resolveProofFocusTargets(truth).map((target) => target.id);
  const installSupportedSurfaces = resolveInstallSupportTargets(truth).map((target) => target.id);

  assert.equal(report.version, readJson('package.json').version);
  assert.equal(report.checkedAt, '2026-04-19T21:00:00Z');
  assert.match(report.productTruth.currentPromise, /One local MCPace endpoint/i);
  assert.deepEqual(report.productTruth.proofFocusSurfaces, proofFocusSurfaces);
  assert.deepEqual(report.productTruth.installSupportedSurfaces, installSupportedSurfaces);
  assert.ok(report.productTruth.installSupportedSurfaces.length >= proofFocusSurfaces.length);
  assert.equal(report.capabilityInventory.totalCapabilities, 24);
  assert.equal(report.capabilityInventory.claimStatusCounts['connectable-preview'], 1);
  assert.equal(report.environment.node, process.version);
  assert.ok(Array.isArray(report.distribution.vendoredBinaryTargets));
  assert.equal(typeof report.distribution.currentTarget, 'string');
  assert.equal(typeof report.environment.toolchainPolicy.supportedContributorToolchain, 'boolean');
  assert.ok([
    'self-contained-vendored-binary',
    'vendored-binary-staged-but-unverified',
    'source-build-required',
    'blocked-without-vendored-binary-or-rust-toolchain'
  ].includes(report.distribution.currentTargetPackagingMode));
  assert.equal(report.sourceProof.status, 'not-run');
  assert.equal(report.releaseProof.status, 'not-run');
  assert.ok(['blocked', 'not-run'].includes(report.buildProof.status));
  assert.ok(['blocked', 'not-run'].includes(report.runtimeProof.status));
});

test('proof report module can write a report file', async () => {
  const { collectReport, resolveCommandInvocation, writeReport } = await import(pathToFileUrl(proofScript));
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-proof-report-'));
  const outputPath = path.join(tmp, 'verification-latest.json');

  try {
    assert.deepEqual(resolveCommandInvocation('npm', ['test'], 'win32'), {
      bin: 'cmd.exe',
      args: ['/d', '/s', '/c', 'npm', 'test'],
      displayCommand: 'npm test'
    });
    assert.deepEqual(resolveCommandInvocation('npm', ['test'], 'linux'), {
      bin: 'npm',
      args: ['test'],
      displayCommand: 'npm test'
    });

    const report = collectReport({ noRun: true, checkedAt: '2026-04-19T21:05:00Z' });
    const writtenPath = writeReport(report, outputPath);
    assert.equal(writtenPath, outputPath);
    const written = JSON.parse(fs.readFileSync(outputPath, 'utf8'));
    assert.equal(written.checkedAt, '2026-04-19T21:05:00Z');
    assert.equal(written.sourceProof.reason, 'proof commands were skipped via --no-run');
    assert.equal(written.productTruth.entrypointContract.product, 'serve');
    assert.ok(Array.isArray(written.productTruth.proofFocusSurfaces));
    assert.equal(written.capabilityInventory.claimStatusCounts.supported, 12);
    assert.equal(typeof written.distribution.currentTarget, 'string');
    assert.equal(typeof written.environment.toolchainPolicy.requiredNodeMajor, 'number');
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

function pathToFileUrl(filePath) {
  return pathToFileURL(filePath).href;
}
