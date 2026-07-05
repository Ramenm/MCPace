import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { createZipFromDirectory, listZipEntries } from '../../scripts/lib/zip-writer.mjs';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function writeFixture(root) {
  fs.mkdirSync(path.join(root, 'src'), { recursive: true });
  fs.writeFileSync(path.join(root, 'README.md'), '# clean fixture\n');
  fs.writeFileSync(path.join(root, 'src', 'main.rs'), 'fn main() {}\n');
  fs.mkdirSync(path.join(root, 'data', 'runtime', 'service'), { recursive: true });
  fs.mkdirSync(path.join(root, 'data', 'runtime', 'tool-list-cache'), { recursive: true });
  fs.mkdirSync(path.join(root, 'data', 'runtime', 'bin'), { recursive: true });
  fs.writeFileSync(path.join(root, 'data', 'runtime', 'mcpace.sqlite'), 'sqlite-state');
  fs.writeFileSync(path.join(root, 'data', 'runtime', 'service', 'mcpace-autostart.vbs'), 'WScript.Echo "bad"\n');
  fs.writeFileSync(path.join(root, 'data', 'runtime', 'tool-list-cache', 'server.json'), '{}\n');
  fs.writeFileSync(path.join(root, 'data', 'runtime', 'bin', 'mcpace.exe'), 'binary');
}

function runNodeScript(script, args, options = {}) {
  return spawnSync(process.execPath, [path.join(repoRoot, script), ...args], {
    cwd: repoRoot,
    encoding: 'utf8',
    windowsHide: true,
    ...options,
  });
}

function pythonRunner() {
  const candidates = process.platform === 'win32'
    ? [['python'], ['py', '-3'], ['python3']]
    : [['python3'], ['python']];
  for (const [command, ...prefixArgs] of candidates) {
    const result = spawnSync(command, [...prefixArgs, '--version'], {
      cwd: repoRoot,
      encoding: 'utf8',
      windowsHide: true,
    });
    if (!result.error && result.status === 0) return { command, prefixArgs };
  }
  throw new Error('no usable Python interpreter found for cleanzip_fast.py');
}

test('cleanzip drops generated runtime/state artifacts by path prefix, not just directory basename', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-cleanzip-fixture-'));
  const out = path.join(os.tmpdir(), `mcpace-cleanzip-${process.pid}-${Date.now()}.zip`);
  try {
    writeFixture(tmp);
    const python = pythonRunner();
    const result = spawnSync(python.command, [...python.prefixArgs, path.join(repoRoot, 'cleanzip_fast.py'), tmp, out], {
      cwd: repoRoot,
      encoding: 'utf8',
      windowsHide: true,
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const entries = listZipEntries(out);
    assert.ok(entries.includes('README.md'));
    assert.ok(entries.includes('src/main.rs'));
    assert.equal(entries.some((entry) => entry.startsWith('data/runtime/')), false, `runtime entries leaked: ${entries.join(', ')}`);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
    fs.rmSync(out, { force: true });
  }
});

test('clean archive verifier rejects runtime artifacts in directories and ZIPs', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-clean-verify-'));
  const zip = path.join(os.tmpdir(), `mcpace-clean-verify-${process.pid}-${Date.now()}.zip`);
  try {
    writeFixture(tmp);
    let result = runNodeScript('scripts/verify-clean-archive.mjs', ['--json', '--source-tree', '--repo', tmp]);
    assert.notEqual(result.status, 0, result.stdout);
    let report = JSON.parse(result.stdout);
    assert.equal(report.status, 'fail');
    assert.ok(report.checks[0].violations.some((issue) => issue.path === 'data/runtime/mcpace.sqlite'));
    assert.ok(report.checks[0].violations.some((issue) => issue.path === 'data/runtime/service/mcpace-autostart.vbs'));

    createZipFromDirectory(tmp, zip, { rootName: 'fixture-root', date: new Date(0) });
    result = runNodeScript('scripts/verify-clean-archive.mjs', ['--json', '--archive', zip]);
    assert.notEqual(result.status, 0, result.stdout);
    report = JSON.parse(result.stdout);
    assert.equal(report.status, 'fail');
    assert.ok(report.checks[0].violations.some((issue) => issue.path === 'fixture-root/data/runtime/bin/mcpace.exe'));
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
    fs.rmSync(zip, { force: true });
  }
});

test('repository source tree has no generated runtime/state artifacts checked into the bundle', () => {
  const result = runNodeScript('scripts/verify-clean-archive.mjs', ['--json', '--source-tree', '--repo', repoRoot]);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'pass');
  assert.deepEqual(report.checks.flatMap((check) => check.violations), []);
});
