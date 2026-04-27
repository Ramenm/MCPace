import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createExecutableFixture, resolveBinary } from '../lib/resolve-binary.js';
import { binaryNameForTarget, detectTarget } from '../lib/platform.js';

test('resolveBinary prefers MCPACE_BINARY_PATH', async (t) => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-bin-'));
  const bin = createExecutableFixture(path.join(tmp, process.platform === 'win32' ? 'mcpace.exe' : 'mcpace'));
  t.mock.method(process, 'cwd', () => tmp);
  process.env.MCPACE_BINARY_PATH = bin;
  try {
    assert.equal(resolveBinary(), path.resolve(bin));
  } finally {
    delete process.env.MCPACE_BINARY_PATH;
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('resolveBinary prefers MCPACE_DEV_BINARY when explicit path is given', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-dev-'));
  const bin = createExecutableFixture(path.join(tmp, process.platform === 'win32' ? 'mcpace.exe' : 'mcpace'));
  process.env.MCPACE_DEV_BINARY = bin;
  try {
    assert.equal(resolveBinary(), path.resolve(bin));
  } finally {
    delete process.env.MCPACE_DEV_BINARY;
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});


test('resolveBinary rejects a non-executable explicit binary path on unix', () => {
  if (process.platform === 'win32') {
    return;
  }
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-noexec-'));
  const bin = path.join(tmp, 'mcpace');
  fs.writeFileSync(bin, '#!/usr/bin/env sh\necho nope\n', 'utf8');
  fs.chmodSync(bin, 0o644);
  process.env.MCPACE_BINARY_PATH = bin;
  try {
    assert.throws(() => resolveBinary(), /not executable/);
  } finally {
    delete process.env.MCPACE_BINARY_PATH;
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('resolveBinary prefers a vendored binary from the workspace repo', () => {
  const target = detectTarget();
  if (!target) {
    return;
  }

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-vendor-repo-'));
  const bin = createExecutableFixture(
    path.join(tmp, 'packages', 'npm', 'cli', 'vendor', target.key, binaryNameForTarget(target))
  );

  try {
    assert.equal(
      resolveBinary({ repoRoot: tmp, packageRoot: path.join(tmp, 'unused-package-root'), ignoreDevBinary: true }),
      path.resolve(bin)
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('resolveBinary prefers a vendored binary next to the installed package', () => {
  const target = detectTarget();
  if (!target) {
    return;
  }

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-vendor-package-'));
  const packageRoot = path.join(tmp, 'node_modules', '@mcpace', 'cli');
  const bin = createExecutableFixture(
    path.join(packageRoot, 'vendor', target.key, binaryNameForTarget(target))
  );

  try {
    assert.equal(
      resolveBinary({ repoRoot: path.join(tmp, 'repo-root'), packageRoot, ignoreDevBinary: true }),
      path.resolve(bin)
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('resolveBinary throws a helpful error when no binary is available', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-none-'));
  try {
    assert.throws(() => resolveBinary({ repoRoot: tmp, ignoreDevBinary: true, ignoreVendoredBinary: true }), /Supported targets:/);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});
