const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { spawnSync } = require('node:child_process');
const { repoRoot, packageVersion } = require('./helpers');

const verifyScript = path.join(repoRoot, 'scripts', 'verify-npm-pack.mjs');

async function loadVerifyNpmPack() {
  return import(pathToFileURL(verifyScript).href);
}

function currentTargetKey() {
  if (process.platform === 'linux') {
    return `${process.platform}-${process.arch}-gnu`;
  }
  if (process.platform === 'win32') {
    return `${process.platform}-${process.arch}-msvc`;
  }
  return `${process.platform}-${process.arch}`;
}

function currentBinaryName() {
  return process.platform === 'win32' ? 'mcpace.exe' : 'mcpace';
}

test('verify-npm-pack passes on the default npm tarball mode', async () => {
  const { verifyNpmPack } = await loadVerifyNpmPack();
  const report = verifyNpmPack();

  assert.equal(report.status, 'pass');
  assert.equal(report.packageVersion, packageVersion());
  const expectedMode = report.repoVendoredBinaryFiles.length > 0 ? 'vendored-binary-bundle' : 'thin-launcher';
  assert.equal(report.packageMode, expectedMode);
  assert.equal(report.missingFiles.length, 0);
  assert.deepEqual(report.nonExecutableVendoredBinaryFiles, []);
  assert.ok(report.files.includes('LICENSE'));
});

test('verify-npm-pack rejects unsafe workspace shell metacharacters', async () => {
  const { verifyNpmPack } = await loadVerifyNpmPack();
  assert.throws(
    () => verifyNpmPack({ workspace: '@mcpace/cli&whoami' }),
    /invalid npm workspace value/
  );
});

test('verify-npm-pack requires staged vendored binaries to be included in the tarball', async () => {
  const targetKey = currentTargetKey();
  const vendorRoot = path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor');
  const targetDir = path.join(vendorRoot, targetKey);
  const binaryPath = path.join(targetDir, currentBinaryName());
  const alreadyHadBinary = fs.existsSync(binaryPath);

  if (!alreadyHadBinary) {
    fs.mkdirSync(targetDir, { recursive: true });
    fs.writeFileSync(binaryPath, process.platform === 'win32' ? '@echo off\r\necho staged\r\n' : '#!/usr/bin/env sh\necho staged\n', 'utf8');
    if (process.platform !== 'win32') {
      fs.chmodSync(binaryPath, 0o755);
    }
  }

  try {
    const { verifyNpmPack } = await loadVerifyNpmPack();
    const report = verifyNpmPack();

    assert.equal(report.status, 'pass');
    assert.equal(report.packageMode, 'vendored-binary-bundle');
    assert.ok(report.packedVendoredBinaryFiles.includes(`vendor/${targetKey}/${currentBinaryName()}`));
    assert.deepEqual(report.missingVendoredBinaryFiles, []);
  } finally {
    if (!alreadyHadBinary) {
      fs.rmSync(binaryPath, { force: true });
      for (const dir of [targetDir, vendorRoot]) {
        try {
          if (fs.existsSync(dir) && fs.readdirSync(dir).length === 0) fs.rmdirSync(dir);
        } catch {
          // Best-effort cleanup only; do not remove unrelated staged binaries.
        }
      }
    }
  }
});

test('verify-npm-pack rejects non-executable staged vendored binaries on POSIX', async () => {
  if (process.platform === 'win32') {
    return;
  }

  const targetKey = currentTargetKey();
  const vendorRoot = path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor');
  const targetDir = path.join(vendorRoot, targetKey);
  const binaryPath = path.join(targetDir, currentBinaryName());
  const alreadyHadBinary = fs.existsSync(binaryPath);
  const originalMode = alreadyHadBinary ? fs.statSync(binaryPath).mode & 0o777 : null;

  fs.mkdirSync(targetDir, { recursive: true });
  if (!alreadyHadBinary) {
    fs.writeFileSync(binaryPath, '#!/usr/bin/env sh\necho staged\n', 'utf8');
  }
  fs.chmodSync(binaryPath, 0o644);

  try {
    const { verifyNpmPack } = await loadVerifyNpmPack();
    const report = verifyNpmPack();

    assert.equal(report.status, 'fail');
    assert.match(report.reason, /non-executable vendored binaries/);
    assert.ok(report.nonExecutableVendoredBinaryFiles.some((entry) => entry.includes(`vendor/${targetKey}/${currentBinaryName()}`)));
  } finally {
    if (alreadyHadBinary) {
      fs.chmodSync(binaryPath, originalMode);
    } else {
      fs.rmSync(binaryPath, { force: true });
      for (const dir of [targetDir, vendorRoot]) {
        try {
          if (fs.existsSync(dir) && fs.readdirSync(dir).length === 0) fs.rmdirSync(dir);
        } catch {
          // Best-effort cleanup only; do not remove unrelated staged binaries.
        }
      }
    }
  }
});


test('verify-npm-pack CLI writes a fresh machine-readable report', () => {
  const tmpDir = fs.mkdtempSync(path.join(require('node:os').tmpdir(), 'mcpace-npm-pack-report-'));
  const reportPath = path.join(tmpDir, 'npm-pack.json');
  const result = spawnSync('node', [verifyScript, '--json', '--write', reportPath], {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: 120_000,
    maxBuffer: 4 * 1024 * 1024,
  });

  assert.equal(result.status, 0, `verify-npm-pack failed
STDOUT:
${result.stdout}
STDERR:
${result.stderr}`);
  const stdoutReport = JSON.parse(result.stdout);
  const writtenReport = JSON.parse(fs.readFileSync(reportPath, 'utf8'));

  assert.equal(writtenReport.schema, 'mcpace.npmPack.v1');
  assert.equal(writtenReport.status, 'pass');
  assert.equal(writtenReport.packageVersion, packageVersion());
  assert.deepEqual(writtenReport.nonExecutableVendoredBinaryFiles, []);
  assert.ok(writtenReport.fileDetails.some((entry) => entry.path === 'package.json'));
  const expectedReportPath = path.relative(repoRoot, reportPath).startsWith('..')
    ? reportPath
    : path.relative(repoRoot, reportPath).split(path.sep).join('/');
  assert.equal(stdoutReport.reportPath, expectedReportPath);
  assert.equal(writtenReport.reportPath, expectedReportPath);
});
