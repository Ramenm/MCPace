#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { assertSupportedNodeVersion } from '../lib/runtime.js';
import { resolveBinary } from '../lib/resolve-binary.js';

try {
  assertSupportedNodeVersion();
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}

const binary = resolveBinary();
const child = spawn(binary, process.argv.slice(2), {
  stdio: 'inherit',
  env: process.env
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});

child.on('error', (error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
