import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const packageRoot = path.resolve(import.meta.dirname, '..');
const shimPath = path.join(packageRoot, 'bin', 'mcpace.js');

test('published npm bin shim exists, is executable, and delegates args without shell composition', () => {
  const stat = fs.statSync(shimPath);
  assert.equal(stat.isFile(), true);
  if (process.platform !== 'win32') {
    assert.notEqual(stat.mode & 0o111, 0, 'mcpace bin shim must keep executable bits');
  }

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-bin-shim-'));
  const out = path.join(tmp, 'args.json');
  const native = path.join(tmp, process.platform === 'win32' ? 'native-bin.cmd' : 'native-bin');
  const nativeBody = process.platform === 'win32'
    ? `@echo off\nnode -e "require('fs').writeFileSync(process.env.MCPACE_SHIM_OUT, JSON.stringify(process.argv.slice(1)))" %*\n`
    : `#!/usr/bin/env node\nimport fs from 'node:fs';\nfs.writeFileSync(process.env.MCPACE_SHIM_OUT, JSON.stringify(process.argv.slice(2)));\n`;
  fs.writeFileSync(native, nativeBody, 'utf8');
  if (process.platform !== 'win32') fs.chmodSync(native, 0o755);

  try {
    const trickyArg = 'name & rm -rf / "quoted"';
    const result = spawnSync(process.execPath, [shimPath, 'server', 'list', trickyArg], {
      cwd: packageRoot,
      encoding: 'utf8',
      env: {
        ...process.env,
        MCPACE_BINARY_PATH: native,
        MCPACE_SHIM_OUT: out,
      },
      windowsHide: true,
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    assert.deepEqual(JSON.parse(fs.readFileSync(out, 'utf8')), ['server', 'list', trickyArg]);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});
