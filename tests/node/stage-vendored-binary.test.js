const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { cleanChildEnv, repoRoot } = require('./helpers');
const stageScript = path.join(repoRoot, 'scripts', 'stage-vendored-binary.mjs');

function currentSupportedTargetKey() {
  if (process.platform === 'linux' && process.arch === 'x64') {
    return 'linux-x64-gnu';
  }
  if (process.platform === 'linux' && process.arch === 'arm64') {
    return 'linux-arm64-gnu';
  }
  if (process.platform === 'darwin' && process.arch === 'x64') {
    return 'darwin-x64';
  }
  if (process.platform === 'darwin' && process.arch === 'arm64') {
    return 'darwin-arm64';
  }
  if (process.platform === 'win32' && process.arch === 'x64') {
    return 'win32-x64-msvc';
  }
  return null;
}

function binaryNameForTargetKey(targetKey) {
  return targetKey.startsWith('win32-') ? 'mcpace.exe' : 'mcpace';
}

test('stage-vendored-binary copies a target binary into the vendor layout', () => {
  const targetKey = currentSupportedTargetKey();
  if (!targetKey) {
    return;
  }

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-stage-vendor-'));
  const sourceDir = path.join(tmp, 'source');
  const outputDir = path.join(tmp, 'vendor');
  const binaryName = binaryNameForTargetKey(targetKey);
  const sourcePath = path.join(sourceDir, binaryName);

  fs.mkdirSync(sourceDir, { recursive: true });
  fs.writeFileSync(sourcePath, '#!/usr/bin/env sh\necho staged\n', 'utf8');
  if (binaryName !== 'mcpace.exe') {
    fs.chmodSync(sourcePath, 0o755);
  }

  try {
    const result = spawnSync(
      process.execPath,
      [
        stageScript,
        '--json',
        '--binary-path',
        sourcePath,
        '--output-dir',
        outputDir,
        '--target-key',
        targetKey,
        '--clear-target-dir'
      ],
      {
        cwd: repoRoot,
        encoding: 'utf8',
        env: cleanChildEnv()
      }
    );

    assert.equal(result.status, 0, result.stderr);
    const report = JSON.parse(result.stdout);
    const destinationPath = path.join(outputDir, targetKey, binaryName);
    assert.equal(report.targetKey, targetKey);
    assert.equal(report.binaryName, binaryName);
    assert.equal(path.resolve(report.destinationPath), path.resolve(destinationPath));
    assert.equal(fs.existsSync(destinationPath), true);
    assert.equal(fs.readFileSync(destinationPath, 'utf8'), '#!/usr/bin/env sh\necho staged\n');
    if (binaryName !== 'mcpace.exe') {
      const mode = fs.statSync(destinationPath).mode & 0o777;
      assert.equal(mode, 0o755);
    }
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});
