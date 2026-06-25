import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';
import { createZipFromDirectory } from '../../scripts/lib/zip-writer.mjs';
import { createExecutableFixture, resolveBinary } from '../../packages/npm/cli/lib/resolve-binary.js';
import { binaryNameForTarget, detectTarget } from '../../packages/npm/cli/lib/platform.js';

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('ZIP writer rejects absolute root names and documents classic ZIP size limits', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-zip-root-'));
  try {
    fs.writeFileSync(path.join(tmp, 'file.txt'), 'ok\n', 'utf8');
    assert.throws(
      () => createZipFromDirectory(tmp, path.join(tmp, 'out.zip'), { rootName: '/absolute-root' }),
      /ZIP root name must be relative/,
    );
    assert.match(read('scripts/lib/zip-writer.mjs'), /ZIP64 is not implemented/);
    assert.match(read('scripts/lib/zip-writer.mjs'), /duplicate ZIP entry path after normalization/);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('release builder keeps portable archive path and copied-path accounting guards', () => {
  const builder = read('scripts/build-release-artifacts.mjs');
  assert.doesNotMatch(builder, /copied\.push\(\.\.\.result\.copied\);\s*copied\.push\(\.\.\.result\.copied\);/);
  assert.match(builder, /WINDOWS_RESERVED_SEGMENT/);
  assert.match(builder, /portablePathCollisions/);
  assert.match(builder, /path segment has a trailing space or dot/);
});

test('libc detection does not execute ldd through attacker-controlled PATH', () => {
  const platform = read('packages/npm/cli/lib/platform.js');
  assert.doesNotMatch(platform, /execFileSync\(['"]ldd['"]/);
  assert.match(platform, /TRUSTED_LDD_PATHS/);
  assert.match(platform, /env: PLATFORM_PROBE_ENV/);
});

test('resolveBinary rejects vendored binaries that escape through symlinked parent directories', (t) => {
  if (process.platform === 'win32') {
    return;
  }
  const target = detectTarget();
  if (!target) {
    return;
  }

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-vendor-parent-symlink-'));
  const packageRoot = path.join(tmp, 'node_modules', '@mcpace', 'cli');
  const outsideRoot = path.join(tmp, 'outside-vendor');
  const outsideBin = path.join(outsideRoot, target.key, binaryNameForTarget(target));
  try {
    fs.mkdirSync(packageRoot, { recursive: true });
    createExecutableFixture(outsideBin);
    try {
      fs.symlinkSync(outsideRoot, path.join(packageRoot, 'vendor'), 'dir');
    } catch (error) {
      t.skip(`symlink unavailable in this environment: ${error?.message || error}`);
      return;
    }
    assert.throws(
      () => resolveBinary({
        repoRoot: path.join(tmp, 'repo-root'),
        packageRoot,
        target,
        ignoreDevBinary: true,
      }),
      /escapes expected package or workspace root/,
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('Rust runtime writes use create_new temp files, fsync, private runtime dirs, and serve start locking', () => {
  const runtimepaths = read('src/runtimepaths.rs');
  const serve = read('src/serve.rs');
  assert.match(runtimepaths, /create_new\(true\)/);
  assert.match(runtimepaths, /sync_all\(\)/);
  assert.match(runtimepaths, /fsync_parent_dir_best_effort/);
  assert.match(runtimepaths, /ensure_private_dir/);
  assert.match(runtimepaths, /from_mode\(0o700\)/);
  assert.doesNotMatch(runtimepaths, /fs::write\(&temp_path, contents\)/);
  assert.match(runtimepaths, /serve_start_lock_path/);
  assert.match(serve, /acquire_serve_start_lock/);
  assert.match(serve, /create_new\(true\)\.write\(true\)\.open\(&lock_path\)/);
});
