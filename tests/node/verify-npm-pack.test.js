const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
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

test('verify-npm-pack passes on the default thin-launcher tarball', async () => {
  const { verifyNpmPack } = await loadVerifyNpmPack();
  const report = verifyNpmPack();

  assert.equal(report.status, 'pass');
  assert.equal(report.packageVersion, packageVersion());
  assert.equal(report.packageMode, 'thin-launcher');
  assert.equal(report.missingFiles.length, 0);
  assert.ok(report.files.includes('LICENSE'));
});

test('verify-npm-pack requires staged vendored binaries to be included in the tarball', async () => {
  const targetKey = currentTargetKey();
  const binaryPath = path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor', targetKey, currentBinaryName());
  fs.mkdirSync(path.dirname(binaryPath), { recursive: true });
  fs.writeFileSync(binaryPath, process.platform === 'win32' ? '@echo off\r\necho staged\r\n' : '#!/usr/bin/env sh\necho staged\n', 'utf8');
  if (process.platform !== 'win32') {
    fs.chmodSync(binaryPath, 0o755);
  }

  try {
    const { verifyNpmPack } = await loadVerifyNpmPack();
    const report = verifyNpmPack();

    assert.equal(report.status, 'pass');
    assert.equal(report.packageMode, 'vendored-binary-bundle');
    assert.ok(report.packedVendoredBinaryFiles.includes(`vendor/${targetKey}/${currentBinaryName()}`));
    assert.deepEqual(report.missingVendoredBinaryFiles, []);
  } finally {
    fs.rmSync(path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor'), { recursive: true, force: true });
  }
});
