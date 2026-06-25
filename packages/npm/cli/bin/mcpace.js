#!/usr/bin/env node
import { spawn } from 'node:child_process';
import process from 'node:process';
import { resolveBinary } from '../lib/resolve-binary.js';
import { assertSupportedNodeVersion } from '../lib/runtime.js';
import {
  collectTerminalDiagnostics,
  formatTerminalDiagnostics,
  isTerminalDiagnosticsRequest,
  terminalDiagnosticsWantsJson,
} from '../lib/terminal-diagnostics.js';

function isWindowsCommandScript(filePath) {
  return process.platform === 'win32' && /\.(?:cmd|bat)$/i.test(String(filePath || ''));
}

function spawnResolvedBinary(binaryPath, args) {
  if (isWindowsCommandScript(binaryPath)) {
    return spawn('cmd.exe', ['/d', '/s', '/c', binaryPath, ...args], {
      stdio: 'inherit',
      windowsHide: false,
    });
  }

  return spawn(binaryPath, args, {
    stdio: 'inherit',
    windowsHide: false,
  });
}

function startupMessage(error) {
  const message = error?.message || String(error);
  return message.startsWith('mcpace:') ? message : `mcpace: ${message}`;
}

function reportStartupError(error) {
  process.stderr.write(`${startupMessage(error)}\n`);
  process.exitCode = 1;
}

function writeTerminalDiagnostics(args) {
  const diagnostics = collectTerminalDiagnostics({ invokedAs: process.argv[1] });
  if (terminalDiagnosticsWantsJson(args)) {
    process.stdout.write(`${JSON.stringify(diagnostics, null, 2)}\n`);
  } else {
    process.stdout.write(formatTerminalDiagnostics(diagnostics));
  }
}

function main() {
  const cliArgs = process.argv.slice(2);
  if (isTerminalDiagnosticsRequest(cliArgs)) {
    writeTerminalDiagnostics(cliArgs);
    return;
  }

  let child;
  try {
    assertSupportedNodeVersion();
    const binaryPath = resolveBinary();
    child = spawnResolvedBinary(binaryPath, cliArgs);
  } catch (error) {
    reportStartupError(error);
    return;
  }

  child.on('error', (error) => {
    process.stderr.write(`mcpace: failed to launch native binary: ${error?.message || error}\n`);
    process.exitCode = 1;
  });

  child.on('close', (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 1);
  });
}

main();
