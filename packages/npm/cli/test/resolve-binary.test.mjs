import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createExecutableFixture, resolveBinary } from '../lib/resolve-binary.js';
import { binaryNameForTarget, detectTarget } from '../lib/platform.js';

function writeSourceWorkspaceMarker(root) {
  fs.writeFileSync(path.join(root, 'Cargo.toml'), '[package]\nname = "mcpace"\nversion = "0.6.9"\n', 'utf8');
  fs.writeFileSync(path.join(root, 'package.json'), '{"name":"mcpace-workspace","version":"0.6.9"}\n', 'utf8');
}

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
  writeSourceWorkspaceMarker(tmp);
  const bin = createExecutableFixture(path.join(tmp, process.platform === 'win32' ? 'mcpace.exe' : 'mcpace'));
  process.env.MCPACE_DEV_BINARY = bin;
  try {
    assert.equal(resolveBinary(), path.resolve(bin));
  } finally {
    delete process.env.MCPACE_DEV_BINARY;
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});



test('resolveBinary accepts quoted explicit env paths from shell snippets', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-quoted-env-'));
  const bin = createExecutableFixture(path.join(tmp, process.platform === 'win32' ? 'mcpace.exe' : 'mcpace'));
  process.env.MCPACE_BINARY_PATH = `"${bin}"`;
  try {
    assert.equal(resolveBinary(), path.resolve(bin));
  } finally {
    delete process.env.MCPACE_BINARY_PATH;
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('resolveBinary rejects directories passed as explicit binary paths', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-dir-env-'));
  process.env.MCPACE_BINARY_PATH = tmp;
  try {
    assert.throws(() => resolveBinary(), /not a file/);
  } finally {
    delete process.env.MCPACE_BINARY_PATH;
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

test('resolveBinary ignores accidental consumer-project target binaries outside MCPace source workspaces', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'consumer-project-'));
  createExecutableFixture(path.join(tmp, 'target', 'release', process.platform === 'win32' ? 'mcpace.exe' : 'mcpace'));
  try {
    assert.throws(
      () => resolveBinary({ repoRoot: tmp, ignoreVendoredBinary: true, ignoreInstalledBinaryPackage: true }),
      /Unable to resolve the mcpace binary/
    );
  } finally {
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
    assert.throws(
      () => resolveBinary({
        repoRoot: tmp,
        ignoreDevBinary: true,
        ignoreVendoredBinary: true,
        ignoreInstalledBinaryPackage: true,
      }),
      /Supported targets:/
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

function optionalPackageRootFor(packageRoot, pkgName) {
  return path.join(path.dirname(path.dirname(packageRoot)), ...pkgName.split('/'));
}

function writeMainCliPackage(packageRoot, version = '0.7.3') {
  fs.mkdirSync(packageRoot, { recursive: true });
  fs.writeFileSync(path.join(packageRoot, 'package.json'), JSON.stringify({ name: '@mcpace/cli', version }, null, 2));
}

function writeOptionalBinaryPackage(packageRoot, target, overrides = {}) {
  const pkgName = target.packageName ?? target.npmPackage ?? `@mcpace/cli-${target.key}`;
  const pkgRoot = optionalPackageRootFor(packageRoot, pkgName);
  const bin = path.join(pkgRoot, 'bin', binaryNameForTarget(target));
  fs.mkdirSync(path.dirname(bin), { recursive: true });
  const packageJson = {
    name: pkgName,
    version: '0.7.3',
    os: target.os,
    cpu: target.cpu,
    mcpace: { target: target.key },
    ...overrides.packageJson,
  };
  fs.writeFileSync(path.join(pkgRoot, 'package.json'), JSON.stringify(packageJson, null, 2));
  if (overrides.symlinkTo) {
    fs.symlinkSync(overrides.symlinkTo, bin);
  } else {
    createExecutableFixture(bin);
  }
  return { pkgName, pkgRoot, bin };
}

test('resolveBinary accepts a matching installed optional native package', () => {
  const target = detectTarget();
  if (!target) return;

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-optional-ok-'));
  const packageRoot = path.join(tmp, 'node_modules', '@mcpace', 'cli');
  try {
    writeMainCliPackage(packageRoot);
    const { bin } = writeOptionalBinaryPackage(packageRoot, target);
    assert.equal(
      resolveBinary({ repoRoot: path.join(tmp, 'repo-root'), packageRoot, target, ignoreDevBinary: true, ignoreVendoredBinary: true }),
      path.resolve(bin)
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('resolveBinary rejects installed optional native packages with version drift', () => {
  const target = detectTarget();
  if (!target) return;

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-optional-version-'));
  const packageRoot = path.join(tmp, 'node_modules', '@mcpace', 'cli');
  try {
    writeMainCliPackage(packageRoot);
    writeOptionalBinaryPackage(packageRoot, target, { packageJson: { version: '0.0.0' } });
    assert.throws(
      () => resolveBinary({ repoRoot: path.join(tmp, 'repo-root'), packageRoot, target, ignoreDevBinary: true, ignoreVendoredBinary: true }),
      /version mismatch/
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('resolveBinary rejects installed optional native packages with missing or wrong target metadata', () => {
  const target = detectTarget();
  if (!target) return;

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-optional-target-'));
  const packageRoot = path.join(tmp, 'node_modules', '@mcpace', 'cli');
  try {
    writeMainCliPackage(packageRoot);
    writeOptionalBinaryPackage(packageRoot, target, { packageJson: { mcpace: { target: 'wrong-target' } } });
    assert.throws(
      () => resolveBinary({ repoRoot: path.join(tmp, 'repo-root'), packageRoot, target, ignoreDevBinary: true, ignoreVendoredBinary: true }),
      /target mismatch/
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('resolveBinary rejects symlinked binaries inside installed optional native packages', (t) => {
  const target = detectTarget();
  if (!target) return;

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-optional-symlink-'));
  const packageRoot = path.join(tmp, 'node_modules', '@mcpace', 'cli');
  try {
    writeMainCliPackage(packageRoot);
    const outside = createExecutableFixture(path.join(tmp, 'outside-mcpace'));
    try {
      writeOptionalBinaryPackage(packageRoot, target, { symlinkTo: outside });
    } catch (error) {
      t.skip(`symlink unavailable in this environment: ${error?.message || error}`);
      return;
    }
    assert.throws(
      () => resolveBinary({ repoRoot: path.join(tmp, 'repo-root'), packageRoot, target, ignoreDevBinary: true, ignoreVendoredBinary: true }),
      /symbolic link/
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('resolveBinary rejects cwd-relative explicit env binary overrides', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-relative-env-'));
  const binName = process.platform === 'win32' ? 'mcpace.exe' : 'mcpace';
  createExecutableFixture(path.join(tmp, binName));
  process.env.MCPACE_BINARY_PATH = binName;
  const originalCwd = process.cwd();
  try {
    process.chdir(tmp);
    assert.throws(() => resolveBinary(), /must be an absolute path/);
  } finally {
    process.chdir(originalCwd);
    delete process.env.MCPACE_BINARY_PATH;
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('resolveBinary rejects symlinked installed optional package.json metadata', (t) => {
  const target = detectTarget();
  if (!target) return;

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-optional-pkgjson-symlink-'));
  const packageRoot = path.join(tmp, 'node_modules', '@mcpace', 'cli');
  try {
    writeMainCliPackage(packageRoot);
    const { pkgRoot } = writeOptionalBinaryPackage(packageRoot, target);
    const realPackageJson = path.join(pkgRoot, 'package.real.json');
    fs.renameSync(path.join(pkgRoot, 'package.json'), realPackageJson);
    try {
      fs.symlinkSync(realPackageJson, path.join(pkgRoot, 'package.json'));
    } catch (error) {
      t.skip(`symlink unavailable in this environment: ${error?.message || error}`);
      return;
    }
    assert.throws(
      () => resolveBinary({ repoRoot: path.join(tmp, 'repo-root'), packageRoot, target, ignoreDevBinary: true, ignoreVendoredBinary: true }),
      /symbolic link/,
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});
