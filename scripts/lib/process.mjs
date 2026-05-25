import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { childEnvForCommand } from './safe-child-env.mjs';

export function commandForPlatform(command, platform = process.platform) {
  if (platform !== 'win32') return command;
  if (command === 'npm' || command === 'npx') return `${command}.cmd`;
  return command;
}

export function commandNeedsShell(command, platform = process.platform) {
  return platform === 'win32' && /\.(?:cmd|bat)$/i.test(command);
}

function npmCliPath(command) {
  const envKey = command === 'npx' ? 'npx_execpath' : 'npm_execpath';
  const envPath = process.env[envKey];
  if (envPath && fs.existsSync(envPath)) return envPath;
  const cliName = command === 'npx' ? 'npx-cli.js' : 'npm-cli.js';
  return path.join(path.dirname(process.execPath), 'node_modules', 'npm', 'bin', cliName);
}

export function runChecked(command, args = [], options = {}) {
  const platform = options.platform || process.platform;
  const isWindowsNpmShim = platform === 'win32' && (command === 'npm' || command === 'npx');
  const resolvedCommand = isWindowsNpmShim ? process.execPath : commandForPlatform(command, platform);
  const resolvedArgs = isWindowsNpmShim ? [npmCliPath(command), ...args] : args;
  const result = spawnSync(resolvedCommand, resolvedArgs, {
    cwd: options.cwd,
    encoding: options.encoding || 'utf8',
    env: options.env || childEnvForCommand(command),
    maxBuffer: options.maxBuffer || 16 * 1024 * 1024,
    shell: !isWindowsNpmShim && commandNeedsShell(resolvedCommand, platform),
    windowsHide: true,
  });
  if (result.status !== 0) {
    const output = String(result.stderr || result.stdout || result.error?.message || '').trim();
    throw new Error(`${resolvedCommand} ${resolvedArgs.join(' ')} failed${output ? `\n${output}` : ''}`);
  }
  return result;
}
