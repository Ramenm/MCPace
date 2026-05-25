import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';
import { runChecked } from '../../scripts/lib/process.mjs';

const cliBin = path.join(repoRoot, 'packages', 'npm', 'cli', 'bin', 'mcpace.js');

test('npm package bin entry exists, is executable, and is included by npm pack', () => {
  const cliPackage = JSON.parse(fs.readFileSync(path.join(repoRoot, 'packages', 'npm', 'cli', 'package.json'), 'utf8'));
  assert.equal(cliPackage.bin?.mcpace, 'bin/mcpace.js');
  assert.equal(fs.existsSync(cliBin), true, 'package.json bin target is missing');
  assert.match(fs.readFileSync(cliBin, 'utf8'), /^#!\/usr\/bin\/env node\n/);
  if (process.platform !== 'win32') {
    assert.notEqual(fs.statSync(cliBin).mode & 0o111, 0, 'bin/mcpace.js must be executable on Unix');
  }

  const pack = runChecked('npm', ['pack', '--workspace', '@mcpace/cli', '--json', '--dry-run'], {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  assert.equal(pack.status, 0, pack.stderr || pack.stdout);
  const [manifest] = JSON.parse(pack.stdout);
  const packedFiles = new Set(manifest.files.map((entry) => entry.path));
  assert.equal(packedFiles.has('bin/mcpace.js'), true, 'npm pack omitted the executable bin shim');
});

test('npm bin shim launches the resolved native binary with user arguments', () => {
  if (process.platform === 'win32') {
    return;
  }
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-bin-shim-'));
  const native = path.join(tmp, 'mcpace-native-fixture');
  const out = path.join(tmp, 'argv.txt');
  fs.writeFileSync(native, `#!/usr/bin/env sh\nprintf '%s\\n' "$@" > ${JSON.stringify(out)}\n`, 'utf8');
  fs.chmodSync(native, 0o755);

  const env = { ...process.env, MCPACE_BINARY_PATH: native };
  const result = spawnSync(process.execPath, [cliBin, 'serve', '--port', '0'], {
    cwd: repoRoot,
    env,
    encoding: 'utf8',
    windowsHide: true,
  });
  try {
    assert.equal(result.status, 0, result.stderr || result.stdout);
    assert.equal(fs.readFileSync(out, 'utf8'), 'serve\n--port\n0\n');
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});
