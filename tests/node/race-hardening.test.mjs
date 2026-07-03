import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';
import { cleanChildEnv } from '../../scripts/lib/safe-child-env.mjs';
import { readRegularFileStableSync, withFileLockSync } from '../../scripts/lib/atomic-fs.mjs';
import { trustedNpmCliPath } from '../../scripts/lib/process.mjs';

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('atomic filesystem helper uses exclusive temp files, stable file descriptors and rename commit', () => {
  const source = read('scripts/lib/atomic-fs.mjs');
  assert.match(source, /O_CREAT \| constants\.O_EXCL/);
  assert.match(source, /O_NOFOLLOW/);
  assert.match(source, /fs\.fstatSync\(fd, \{ bigint: true \}\)/);
  assert.match(source, /fs\.renameSync\(tempPath, target\)/);
  assert.doesNotMatch(source, /fs\.accessSync/);
});

test('release builder avoids access-before-use, non-atomic copy and visible archive unlink patterns', () => {
  const source = read('scripts/build-release-artifacts.mjs');
  assert.match(source, /copyRegularFileNoFollowSync/);
  assert.match(source, /lstatStableDirectorySync/);
  assert.match(source, /withFileLockSync/);
  assert.match(source, /writeFileAtomicSync\(manifestPath/);
  assert.doesNotMatch(source, /fs\.existsSync\(source\)/);
  assert.doesNotMatch(source, /fs\.copyFileSync/);
  assert.doesNotMatch(source, /fs\.rmSync\(archivePath/);
  assert.doesNotMatch(source, /fs\.writeFileSync\(manifestPath/);
});

test('zip writer reads archived files from stable file descriptors and writes atomically', () => {
  const source = read('scripts/lib/zip-writer.mjs');
  assert.match(source, /readRegularFileStableSync\(absolutePath, \{ maxBytes: maxFileBytes \}\)/);
  assert.match(source, /writeFileAtomicSync\(archivePath/);
  assert.match(source, /validateSafeZipPath/);
  assert.doesNotMatch(source, /fs\.statSync\(absolutePath\)/);
  assert.doesNotMatch(source, /fs\.readFileSync\(absolutePath\)/);
});

test('stable regular-file reader rejects symlink inputs instead of following them', (t) => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-stable-read-'));
  try {
    const target = path.join(tmp, 'target.txt');
    const link = path.join(tmp, 'link.txt');
    fs.writeFileSync(target, 'secret\n');
    try {
      fs.symlinkSync(target, link);
    } catch (error) {
      t.skip(`symlink unavailable in this environment: ${error?.message || error}`);
      return;
    }
    assert.throws(() => readRegularFileStableSync(link), /ELOOP|symbolic|non-regular|not a regular/);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('file lock fails closed and does not auto-remove stale-looking locks', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-lock-'));
  const lock = path.join(tmp, 'release.lock');
  try {
    fs.writeFileSync(lock, `${JSON.stringify({ pid: 999999, createdAtMs: 1 })}\n`);
    assert.throws(() => withFileLockSync(lock, () => 'unreachable'), /lock is already held|appears stale/);
    assert.equal(fs.existsSync(lock), true, 'stale-looking lock must not be removed automatically');
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('clean child environment drops common compiler, loader, Node and npm poisoning knobs', () => {
  const cleaned = cleanChildEnv({}, {
    PATH: '/usr/bin',
    NODE_OPTIONS: '--require ./evil.js',
    RUSTFLAGS: '-C link-arg=-Wl,evil',
    RUSTC_WRAPPER: '/tmp/fake-rustc-wrapper',
    LD_PRELOAD: '/tmp/fake.so',
    DYLD_INSERT_LIBRARIES: '/tmp/fake.dylib',
    npm_execpath: '/tmp/fake-npm-cli.js',
    CARGO_HOME: '/tmp/cargo-home',
  });
  assert.equal(cleaned.PATH, '/usr/bin');
  assert.equal(cleaned.CARGO_HOME, '/tmp/cargo-home');
  for (const key of ['NODE_OPTIONS', 'RUSTFLAGS', 'RUSTC_WRAPPER', 'LD_PRELOAD', 'DYLD_INSERT_LIBRARIES', 'npm_execpath']) {
    assert.equal(Object.hasOwn(cleaned, key), false, `${key} must not be forwarded by default`);
  }
});

test('npm CLI resolver ignores an injected npm_execpath outside the Node installation', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-fake-npm-'));
  try {
    const fake = path.join(tmp, 'npm-cli.js');
    fs.writeFileSync(fake, 'console.log("fake")\n');
    const resolved = trustedNpmCliPath('npm', { npm_execpath: fake });
    assert.notEqual(resolved, path.resolve(fake));
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('publish workflow moves dispatch input through an intermediate env variable before shell use', () => {
  const workflow = read('.github/workflows/publish-npm.yml');
  assert.match(workflow, /MCPACE_PUBLISH_DRY_RUN: \$\{\{[^\n]*inputs\.dry_run[^\n]*\}\}/);
  assert.match(workflow, /MCPACE_VERSION_OVERRIDE: \$\{\{[^\n]*inputs\.version_override[^\n]*\}\}/);
  assert.match(workflow, /\[ "\$MCPACE_PUBLISH_DRY_RUN" = "true" \]/);
  const runBlocks = workflow.match(/run: \|[\s\S]*?(?=\n\s{6}- name:|\n\s{2}[A-Za-z_-]+:|\n?$)/g) || [];
  for (const block of runBlocks) {
    assert.doesNotMatch(block, /\$\{\{[^\n]*(inputs\.dry_run|inputs\.version_override)[^\n]*\}\}/, 'workflow input must not be interpolated directly inside a shell script');
  }
});

test('tooling and local proof command discovery avoid shell command-v probes', () => {
  for (const relativePath of ['scripts/tooling-preflight.mjs', 'scripts/local-proof.mjs']) {
    const source = read(relativePath);
    assert.doesNotMatch(source, /command -v/);
    assert.doesNotMatch(source, /shell: true/);
  }
});
