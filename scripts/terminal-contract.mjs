#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { repoRoot } from './lib/project-metadata.mjs';

const args = process.argv.slice(2);
const jsonOutput = args.includes('--json');

function writeJson(payload) {
  process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
}

function executableFixture(tmpDir) {
  const file = path.join(tmpDir, process.platform === 'win32' ? 'mcpace-native.cmd' : 'mcpace-native');
  const body = process.platform === 'win32'
    ? '@echo off\r\nexit /b 0\r\n'
    : '#!/usr/bin/env sh\nexit 0\n';
  fs.writeFileSync(file, body, 'utf8');
  if (process.platform !== 'win32') fs.chmodSync(file, 0o755);
  return file;
}

function runNode(commandArgs, env = process.env) {
  const result = spawnSync(process.execPath, commandArgs, {
    cwd: repoRoot,
    encoding: 'utf8',
    env,
    maxBuffer: 16 * 1024 * 1024,
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.doesNotMatch(result.stderr, /DeprecationWarning/, 'terminal diagnostics should not emit Node deprecation warnings');
  return result;
}

function runTerminalContract() {
  const cliPackagePath = path.join(repoRoot, 'packages', 'npm', 'cli', 'package.json');
  const shimPath = path.join(repoRoot, 'packages', 'npm', 'cli', 'bin', 'mcpace.js');
  const cliPackage = JSON.parse(fs.readFileSync(cliPackagePath, 'utf8'));

  assert.deepEqual(cliPackage.bin, {
    mcpace: 'bin/mcpace.js',
  });
  assert.equal(cliPackage.exports['./terminal-diagnostics'], './lib/terminal-diagnostics.js');
  assert.equal(fs.statSync(shimPath).isFile(), true);
  if (process.platform !== 'win32') {
    assert.notEqual(fs.statSync(shimPath).mode & 0o111, 0, 'npm shim must be executable');
  }

  const withoutNative = JSON.parse(runNode([shimPath, '--mcpace-npm-diagnostics', '--json'], {
    ...process.env,
    MCPACE_BINARY_PATH: '',
    MCPACE_DEV_BINARY: '',
  }).stdout);
  assert.equal(withoutNative.schema, 'mcpace.npmTerminalDiagnostics.v1');
  assert.deepEqual(withoutNative.commandAliases, ['mcpace']);
  assert.equal(withoutNative.canonicalCommand, 'mcpace');
  assert.equal(typeof withoutNative.node.version, 'string');
  assert.equal(typeof withoutNative.resolution.status, 'string');

  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-terminal-contract-'));
  try {
    const native = executableFixture(tmpDir);
    const withNative = JSON.parse(runNode([shimPath, '--mcpace-npm-diagnostics', '--json'], {
      ...process.env,
      MCPACE_BINARY_PATH: native,
      MCPACE_DEV_BINARY: '',
    }).stdout);
    assert.equal(withNative.resolution.status, 'resolved');
    assert.equal(path.resolve(withNative.resolution.binaryPath), path.resolve(native));
    assert.equal(withNative.env.MCPACE_BINARY_PATH.executable, true);

    return {
      schema: 'mcpace.terminalContract.v1',
      status: 'pass',
      canonicalCommand: cliPackage.bin.mcpace,
      aliases: Object.keys(cliPackage.bin),
      diagnosticsSchema: withNative.schema,
      nativeFixture: path.basename(native),
    };
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
}

try {
  const payload = runTerminalContract();
  if (jsonOutput) writeJson(payload);
  else process.stdout.write(`PASS terminal contract (${payload.aliases.join(', ')})\n`);
} catch (error) {
  const payload = {
    schema: 'mcpace.terminalContract.v1',
    status: 'fail',
    error: error?.message || String(error),
  };
  if (jsonOutput) writeJson(payload);
  else process.stderr.write(`${error?.stack || error}\n`);
  process.exitCode = 1;
}
