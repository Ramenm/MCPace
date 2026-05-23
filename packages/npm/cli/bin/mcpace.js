#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { resolveBinary } from '../lib/resolve-binary.js';
import { assertSupportedNodeVersion } from '../lib/runtime.js';

function fail(error) {
  const message = error && error.stack ? error.stack : String(error);
  process.stderr.write(`${message}\n`);
  process.exitCode = typeof error?.exitCode === 'number' ? error.exitCode : 1;
}

try {
  assertSupportedNodeVersion();
  const binary = resolveBinary();
  const child = spawn(binary, process.argv.slice(2), {
    stdio: 'inherit',
    windowsHide: false,
  });

  child.on('error', fail);
  child.on('exit', (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exitCode = code ?? 1;
  });
} catch (error) {
  fail(error);
}
