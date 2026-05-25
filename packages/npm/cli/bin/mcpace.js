#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import process from 'node:process';
import { resolveBinary } from '../lib/resolve-binary.js';
import { assertSupportedNodeVersion, formatUnsupportedNodeMessage } from '../lib/runtime.js';

try {
  assertSupportedNodeVersion();
} catch (error) {
  process.stderr.write(`${error?.message || formatUnsupportedNodeMessage()}\n`);
  process.exit(1);
}

let binaryPath;
try {
  binaryPath = resolveBinary();
} catch (error) {
  process.stderr.write(`${error?.message || error}\n`);
  process.exit(1);
}

const result = spawnSync(binaryPath, process.argv.slice(2), {
  stdio: 'inherit',
  windowsHide: true,
});

if (result.error) {
  process.stderr.write(`failed to launch mcpace native binary '${binaryPath}': ${result.error.message}\n`);
  process.exit(1);
}

if (result.signal) {
  process.stderr.write(`mcpace native binary terminated by signal ${result.signal}\n`);
  process.exit(1);
}

process.exit(result.status ?? 0);
