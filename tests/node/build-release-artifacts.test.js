const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { packageVersion, repoRoot } = require('./helpers');

async function loadBuildReleaseArtifacts() {
  return import(pathToFileURL(path.join(repoRoot, 'scripts', 'build-release-artifacts.mjs')).href);
}

function createExistingReleaseFixtures(baseDir) {
  const archiveName = `mcpace-v${packageVersion()}-190426-235959.zip`;
  const archivePath = path.join(baseDir, archiveName);
  const reportPath = path.join(baseDir, 'existing-verification.json');

  fs.writeFileSync(archivePath, 'fake release zip bytes', 'utf8');
  fs.writeFileSync(
    reportPath,
    JSON.stringify(
      {
        version: packageVersion(),
        checkedAt: '2026-04-19T23:59:59.000Z',
        distribution: {
          currentTarget: 'linux-x64-gnu',
          currentTargetPackagingMode: 'blocked-without-vendored-binary-or-rust-toolchain',
          vendoredBinaryTargets: []
        },
        sourceProof: {
          status: 'pass'
        },
        buildProof: {
          status: 'blocked'
        },
        runtimeProof: {
          status: 'blocked'
        },
        releaseProof: {
          status: 'partial',
          archive: {
            name: archiveName,
            path: `dist/${archiveName}`
          }
        }
      },
      null,
      2
    ),
    'utf8'
  );

  return {
    archiveName,
    archivePath,
    reportPath
  };
}

test('build-release-artifacts bundles an archive, verification report, checksums, and manifest', async () => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-existing-release-'));
  const outputDir = fs.mkdtempSync(path.join(repoRoot, '.tmp-release-bundle-'));
  const { archiveName, archivePath, reportPath } = createExistingReleaseFixtures(fixtureDir);

  try {
    const { buildReleaseArtifacts } = await loadBuildReleaseArtifacts();
    const report = buildReleaseArtifacts({
      outputDir,
      existingReportPath: reportPath,
      existingArchivePath: archivePath
    });
    assert.equal(report.source, 'existing-artifacts');
    assert.equal(report.releaseProofStatus, 'partial');
    assert.equal(report.archive.name, archiveName);
    assert.equal(report.verificationReport.name, 'verification-latest.json');

    const bundledArchivePath = path.join(outputDir, archiveName);
    const bundledReportPath = path.join(outputDir, 'verification-latest.json');
    const checksumsPath = path.join(outputDir, 'SHA256SUMS.txt');
    const manifestPath = path.join(outputDir, 'release-artifacts.json');

    assert.equal(fs.existsSync(bundledArchivePath), true);
    assert.equal(fs.existsSync(bundledReportPath), true);
    assert.equal(fs.existsSync(checksumsPath), true);
    assert.equal(fs.existsSync(manifestPath), true);

    const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
    assert.equal(manifest.projectName, 'mcpace');
    assert.equal(manifest.version, packageVersion());
    assert.equal(manifest.archive.name, archiveName);
    assert.equal(manifest.archive.path, path.join(path.basename(outputDir), archiveName).replace(/\\/g, '/'));
    assert.equal(manifest.verificationReport.sourceProofStatus, 'pass');
    assert.equal(manifest.verificationReport.releaseProofStatus, 'partial');
    assert.equal(manifest.distribution.currentTarget, 'linux-x64-gnu');
    assert.equal(manifest.checksums.fileCount, 2);
    assert.deepEqual(
      manifest.checksums.entries.map((entry) => entry.name),
      [archiveName, 'verification-latest.json']
    );

    const checksumLines = fs
      .readFileSync(checksumsPath, 'utf8')
      .trim()
      .split(/\r?\n/)
      .filter(Boolean);
    assert.equal(checksumLines.length, 2);
    assert.ok(checksumLines.every((line) => line.includes('  ')));
  } finally {
    fs.rmSync(fixtureDir, { recursive: true, force: true });
    fs.rmSync(outputDir, { recursive: true, force: true });
  }
});


test('build-release-artifacts rejects existing inputs when report/archive names drift', async () => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-existing-release-drift-'));
  const outputDir = fs.mkdtempSync(path.join(repoRoot, '.tmp-release-bundle-drift-'));
  const { archivePath, reportPath } = createExistingReleaseFixtures(fixtureDir);
  const mismatchedArchivePath = path.join(fixtureDir, 'different-name.zip');
  fs.copyFileSync(archivePath, mismatchedArchivePath);

  try {
    const { buildReleaseArtifacts } = await loadBuildReleaseArtifacts();
    assert.throws(
      () => buildReleaseArtifacts({
        outputDir,
        existingReportPath: reportPath,
        existingArchivePath: mismatchedArchivePath
      }),
      /existing release artifact mismatch/
    );
  } finally {
    fs.rmSync(fixtureDir, { recursive: true, force: true });
    fs.rmSync(outputDir, { recursive: true, force: true });
  }
});
