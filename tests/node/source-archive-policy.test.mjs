import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';
import { createZipFromDirectory, listZipEntries } from '../../scripts/lib/zip-writer.mjs';
import { sourceArchivePolicyViolations } from '../../scripts/lib/source-archive-policy.mjs';

function runNode(args, options = {}) {
  return spawnSync(process.execPath, args, {
    cwd: options.cwd || repoRoot,
    encoding: 'utf8',
    windowsHide: true,
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

function unzipEntries(zipPath) {
  const entries = listZipEntries(zipPath);
  if (entries.length > 0 && entries.every((entry) => entry.includes('/'))) {
    const first = entries[0].split('/')[0];
    if (entries.every((entry) => entry.startsWith(`${first}/`))) {
      return entries.map((entry) => entry.split('/').slice(1).join('/'));
    }
  }
  return entries;
}

test('source archive policy rejects generated runtime state but allows review reports', () => {
  const entries = [
    'mcpace-v0.7.8/README.md',
    'mcpace-v0.7.8/reports/summary.md',
    'mcpace-v0.7.8/data/runtime/mcpace.sqlite',
    'mcpace-v0.7.8/data/runtime/service/mcpace-autostart.vbs',
    'mcpace-v0.7.8/data/runtime/bin/mcpace-serve-123.exe',
    'mcpace-v0.7.8/eval/random-100-npm-sweep.partial.jsonl',
  ];
  const violations = sourceArchivePolicyViolations(entries);
  assert.equal(violations.some((item) => item.path.endsWith('reports/summary.md')), false);
  assert.ok(violations.some((item) => item.path.endsWith('data/runtime/mcpace.sqlite')));
  assert.ok(violations.some((item) => item.path.endsWith('data/runtime/service/mcpace-autostart.vbs')));
  assert.ok(violations.some((item) => item.path.endsWith('data/runtime/bin/mcpace-serve-123.exe')));
  assert.ok(violations.some((item) => item.path.endsWith('eval/random-100-npm-sweep.partial.jsonl')));
});

test('verify-clean-archive fails closed for dirty source trees and clean ZIPs', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-clean-archive-policy-'));
  try {
    fs.writeFileSync(path.join(tmp, 'README.md'), '# Demo\n');
    fs.mkdirSync(path.join(tmp, 'reports'), { recursive: true });
    fs.writeFileSync(path.join(tmp, 'reports', 'summary.md'), '# Summary\n');
    fs.mkdirSync(path.join(tmp, 'data', 'runtime', 'service'), { recursive: true });
    fs.writeFileSync(path.join(tmp, 'data', 'runtime', 'service', 'mcpace-autostart.vbs'), 'WScript.Echo "no"\n');

    const dirty = runNode(['scripts/verify-clean-archive.mjs', '--json', '--repo', tmp, '--source-tree']);
    assert.notEqual(dirty.status, 0, dirty.stdout);
    const dirtyReport = JSON.parse(dirty.stdout);
    assert.equal(dirtyReport.status, 'fail');
    assert.ok(dirtyReport.checks[0].violations.some((item) => item.path === 'data/runtime/service/mcpace-autostart.vbs'));

    fs.rmSync(path.join(tmp, 'data'), { recursive: true, force: true });
    const clean = runNode(['scripts/verify-clean-archive.mjs', '--json', '--repo', tmp, '--source-tree']);
    assert.equal(clean.status, 0, clean.stderr || clean.stdout);
    assert.equal(JSON.parse(clean.stdout).status, 'pass');
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('cleanzip_fast strips project runtime directories from directory inputs', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-cleanzip-runtime-'));
  try {
    const source = path.join(tmp, 'source');
    const out = path.join(tmp, 'clean.zip');
    fs.mkdirSync(path.join(source, 'data', 'runtime', 'tool-list-cache'), { recursive: true });
    fs.mkdirSync(path.join(source, 'docs'), { recursive: true });
    fs.writeFileSync(path.join(source, 'README.md'), '# Demo\n');
    fs.writeFileSync(path.join(source, 'docs', 'README.md'), '# Docs\n');
    fs.writeFileSync(path.join(source, 'data', 'runtime', 'mcpace.sqlite'), '');
    fs.writeFileSync(path.join(source, 'data', 'runtime', 'tool-list-cache', 'browser.json'), '{}');

    const python = pythonRunner();
    const result = spawnSync(python.command, [...python.prefixArgs, path.join(repoRoot, 'cleanzip_fast.py'), source, out], {
      cwd: repoRoot,
      encoding: 'utf8',
      windowsHide: true,
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);

    const entries = unzipEntries(out);
    assert.ok(entries.includes('README.md'));
    assert.ok(entries.includes('docs/README.md'));
    assert.equal(entries.some((entry) => entry.startsWith('data/runtime/')), false, entries.join('\n'));
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('checked-in source tree has no generated runtime artifacts', () => {
  const result = runNode(['scripts/verify-clean-archive.mjs', '--json', '--source-tree']);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.equal(JSON.parse(result.stdout).status, 'pass');
});
