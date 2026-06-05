#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { resolveBinary } from '../lib/resolve-binary.js';
import { assertSupportedNodeVersion } from '../lib/runtime.js';

function isWindowsCommandScript(binary) {
  return process.platform === 'win32' && /\.(?:cmd|bat)$/i.test(binary);
}

function spawnResolvedBinary(binary, args) {
  if (isWindowsCommandScript(binary)) {
    const commandProcessor = process.env.ComSpec || process.env.COMSPEC || 'cmd.exe';
    return spawn(commandProcessor, ['/d', '/s', '/c', binary, ...args], {
      stdio: 'inherit',
      windowsHide: true,
    });
  }

  return spawn(binary, args, {
    stdio: 'inherit',
    windowsHide: true,
  });
}

function signalExitCode(signal) {
  const signalNumbers = { SIGHUP: 1, SIGINT: 2, SIGQUIT: 3, SIGTERM: 15 };
  return 128 + (signalNumbers[signal] ?? 0);
}

try {
  assertSupportedNodeVersion();

  const binary = resolveBinary();
  const child = spawnResolvedBinary(binary, process.argv.slice(2));
  let finished = false;
  const finish = (code) => {
    if (finished) return;
    finished = true;
    process.exit(code);
  };

  child.on('error', (error) => {
    console.error(`mcpace: failed to launch native binary: ${error.message}`);
    finish(1);
  });

  child.on('close', (code, signal) => {
    if (signal) {
      finish(signalExitCode(signal));
      return;
    }
    finish(code ?? 1);
  });
} catch (error) {
  console.error(error?.message || String(error));
  process.exit(1);
}
