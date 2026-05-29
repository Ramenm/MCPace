#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { resolveBinary } from '../lib/resolve-binary.js';
import { assertSupportedNodeVersion } from '../lib/runtime.js';

function main() {
  assertSupportedNodeVersion();
  const binary = resolveBinary();
  const result = spawnSync(binary, process.argv.slice(2), {
    stdio: 'inherit',
    windowsHide: true,
  });

  if (result.error) {
    throw result.error;
  }
  if (typeof result.status === 'number') {
    process.exitCode = result.status;
    return;
  }
  if (result.signal) {
    process.stderr.write(`mcpace: native binary terminated by ${result.signal}\n`);
    process.exitCode = 1;
  }
}

try {
  main();
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`mcpace: ${message}\n`);
  process.exitCode = 1;
}
