import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { deriveProjectVersion, repoRoot } from '../../scripts/lib/project-metadata.mjs';

test('release artifact builder creates a verified single-root source ZIP from the manifest', () => {
  const outDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-release-test-'));
  try {
    const result = spawnSync(process.execPath, [
      'scripts/build-release-artifacts.mjs',
      '--json',
      '--out-dir', outDir,
      '--timestamp', '210526-120001'
    ], {
      cwd: repoRoot,
      encoding: 'utf8',
      windowsHide: true,
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const payload = JSON.parse(result.stdout);
    assert.equal(payload.status, 'pass');
    assert.equal(payload.rootName, `mcpace-v${deriveProjectVersion()}-210526-120001`);
    assert.equal(payload.verificationReport.sourceProofStatus, 'pass');
    assert.equal(fs.existsSync(payload.archive.path), true, 'ZIP archive was not created');
    assert.equal(fs.existsSync(payload.manifestPath), true, 'artifact manifest was not created');

    const listing = spawnSync('unzip', ['-Z1', payload.archive.path], {
      cwd: repoRoot,
      encoding: 'utf8',
      windowsHide: true,
    });
    assert.equal(listing.status, 0, listing.stderr || listing.stdout);
    const files = listing.stdout.trim().split(/\r?\n/);
    assert.ok(files.every((entry) => entry.startsWith(`${payload.rootName}/`)), 'archive must contain exactly one root directory');
    for (const required of ['README.md', 'docs/README.md', 'reports/summary.md', 'scripts/build-release-artifacts.mjs']) {
      assert.ok(files.includes(`${payload.rootName}/${required}`), `archive missing ${required}`);
    }
    for (const forbidden of ['node_modules/', '.git/', 'target/', 'dist/', '.cache/']) {
      assert.equal(files.some((entry) => entry.includes(`/${forbidden}`)), false, `archive includes forbidden ${forbidden}`);
    }
  } finally {
    fs.rmSync(outDir, { recursive: true, force: true });
  }
});

test('release artifact builder dry-run validates manifest without creating a ZIP', () => {
  const outDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-release-dry-run-'));
  try {
    const result = spawnSync(process.execPath, [
      'scripts/build-release-artifacts.mjs',
      '--json',
      '--dry-run',
      '--out-dir', outDir,
      '--timestamp', '210526-120002'
    ], {
      cwd: repoRoot,
      encoding: 'utf8',
      windowsHide: true,
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const payload = JSON.parse(result.stdout);
    assert.equal(payload.dryRun, true);
    assert.equal(payload.releaseProofStatus, 'dry-run');
    assert.equal(payload.verificationReport.sourceProofStatus, 'pass');
    assert.equal(fs.existsSync(payload.archive.path), false, 'dry-run should not create a ZIP archive');
    assert.equal(fs.existsSync(payload.manifestPath), true, 'dry-run should still write a manifest for inspection');
  } finally {
    fs.rmSync(outDir, { recursive: true, force: true });
  }
});
