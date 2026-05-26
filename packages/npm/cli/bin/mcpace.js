#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { assertSupportedNodeVersion } from '../lib/runtime.js';
import { resolveBinary } from '../lib/resolve-binary.js';

function main() {
  assertSupportedNodeVersion();
  const binary = resolveBinary();
  const result = spawnSync(binary, process.argv.slice(2), {
    stdio: 'inherit',
    windowsHide: true
  });
  if (result.error) {
    throw result.error;
  }
  if (typeof result.status === 'number') {
    process.exit(result.status);
  }
  if (result.signal) {
    console.error(`mcpace terminated by signal ${result.signal}`);
    process.exit(1);
  }
}

try {
  main();
} catch (error) {
  console.error(error?.message || String(error));
  process.exit(1);
}
