import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { commandExists, commandTokenIsSafe, runCommand } from '../../scripts/lib/command-runner.mjs';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function manifestIncludesPath(relativePath) {
  const manifest = JSON.parse(read('release-manifest.json'));
  return manifest.includePaths.some((entry) => relativePath === entry || relativePath.startsWith(`${entry}/`));
}

test('proof and tooling preflight share one command discovery and spawn helper', () => {
  assert.equal(commandTokenIsSafe('cargo'), true);
  assert.equal(commandTokenIsSafe('rustc.exe'), true);
  assert.equal(commandTokenIsSafe('cargo;rm'), false);
  assert.equal(commandTokenIsSafe('../cargo'), false);

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-command-runner-'));
  try {
    const toolName = process.platform === 'win32' ? 'mcpace-helper.cmd' : 'mcpace-helper';
    const toolPath = path.join(tmp, toolName);
    fs.writeFileSync(toolPath, process.platform === 'win32' ? '@echo ok\r\n' : '#!/bin/sh\necho ok\n');
    if (process.platform !== 'win32') fs.chmodSync(toolPath, 0o755);
    assert.equal(commandExists('mcpace-helper', { pathValue: tmp }), true);
    assert.equal(commandExists('mcpace-helper;rm', { pathValue: tmp }), false);

    const result = runCommand(process.execPath, ['-e', 'console.log("ok")'], { cwd: repoRoot, timeoutMs: 30_000 });
    assert.equal(result.status, 'pass');
    assert.equal(result.stdout.trim(), 'ok');
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }

  for (const relativePath of ['scripts/local-proof.mjs', 'scripts/tooling-preflight.mjs']) {
    const source = read(relativePath);
    assert.doesNotMatch(source, /function commandTokenIsSafe/);
    assert.doesNotMatch(source, /function executableCandidateNames/);
    assert.doesNotMatch(source, /function isExecutableFile/);
    assert.match(source, /command-runner\.mjs/);
  }
});

test('generated proof reports use the shared atomic writer', () => {
  for (const relativePath of ['scripts/project-assurance.mjs', 'scripts/platform-proof.mjs', 'scripts/project-inventory.mjs']) {
    const source = read(relativePath);
    assert.match(source, /writeFileAtomicSync/);
    assert.doesNotMatch(source, /fs\.writeFileSync\(path\.join\([^\n]*(?:assurance|platform-proof|internal-inventory)/);
  }
});

test('architecture simplification helpers are included in the source release manifest', () => {
  assert.equal(manifestIncludesPath('scripts/lib/command-runner.mjs'), true);
  assert.equal(manifestIncludesPath('docs/architecture-simplification.md'), true);
});
