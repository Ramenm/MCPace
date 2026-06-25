import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { childEnvForCommand } from './safe-child-env.mjs';

const NPM_CLI_NAMES = Object.freeze({
  npm: 'npm-cli.js',
  npx: 'npx-cli.js',
});

export function commandForPlatform(command, platform = process.platform) {
  if (platform !== 'win32') return command;
  if (command === 'npm' || command === 'npx') return `${command}.cmd`;
  return command;
}

export function commandNeedsShell(command, platform = process.platform) {
  return platform === 'win32' && /\.(?:cmd|bat)$/i.test(command);
}

export function windowsCommandShell(env = process.env) {
  const systemRoot = env.SystemRoot || env.WINDIR;
  if (systemRoot) {
    const candidate = path.join(systemRoot, 'System32', 'cmd.exe');
    if (isRegularFile(candidate)) return candidate;
  }
  return 'cmd.exe';
}

function realpathOrNull(filePath) {
  try {
    return fs.realpathSync(filePath);
  } catch {
    return null;
  }
}

function isRegularFile(filePath) {
  try {
    return fs.statSync(filePath).isFile();
  } catch {
    return false;
  }
}

function pathInside(parent, child) {
  const relative = path.relative(parent, child);
  return relative === '' || (!relative.startsWith('..') && !path.isAbsolute(relative));
}

function nodeInstallPrefixes() {
  const execReal = realpathOrNull(process.execPath) ?? path.resolve(process.execPath);
  const execDir = path.dirname(execReal);
  return [...new Set([
    execDir,
    path.dirname(execDir),
    path.resolve(path.dirname(process.execPath)),
    path.resolve(path.dirname(process.execPath), '..'),
  ])]
    .map((candidate) => realpathOrNull(candidate) ?? candidate)
    .filter(Boolean);
}

function npmCliCandidates(command) {
  const cliName = NPM_CLI_NAMES[command];
  if (!cliName) return [];
  const execDir = path.resolve(path.dirname(process.execPath));
  const prefix = path.resolve(execDir, '..');
  return [...new Set([
    path.join(execDir, 'node_modules', 'npm', 'bin', cliName),
    path.join(prefix, 'lib', 'node_modules', 'npm', 'bin', cliName),
    path.join(prefix, 'share', 'nodejs', 'npm', 'bin', cliName),
  ])];
}

function isTrustedNpmCliPath(candidate, command) {
  const cliName = NPM_CLI_NAMES[command];
  if (!cliName || !candidate || path.basename(candidate) !== cliName) return false;
  const realCandidate = realpathOrNull(candidate);
  if (!realCandidate || !isRegularFile(realCandidate)) return false;
  return nodeInstallPrefixes().some((prefix) => pathInside(prefix, realCandidate));
}

export function trustedNpmCliPath(command, env = process.env) {
  const envKey = command === 'npx' ? 'npx_execpath' : command === 'npm' ? 'npm_execpath' : null;
  if (!envKey) return null;

  const envPath = env[envKey];
  if (envPath && isTrustedNpmCliPath(path.resolve(envPath), command)) {
    return path.resolve(envPath);
  }

  for (const candidate of npmCliCandidates(command)) {
    if (isTrustedNpmCliPath(candidate, command)) {
      return candidate;
    }
  }
  return null;
}

function npmCliPath(command) {
  const trusted = trustedNpmCliPath(command);
  if (trusted) return trusted;
  const [fallback] = npmCliCandidates(command);
  return fallback;
}

export function runChecked(command, args = [], options = {}) {
  const platform = options.platform || process.platform;
  const isWindowsNpmShim = platform === 'win32' && (command === 'npm' || command === 'npx');
  const platformCommand = commandForPlatform(command, platform);
  const isWindowsCommandScript = !isWindowsNpmShim && commandNeedsShell(platformCommand, platform);
  const resolvedCommand = isWindowsNpmShim
    ? process.execPath
    : isWindowsCommandScript
      ? windowsCommandShell(options.env || process.env)
      : platformCommand;
  const resolvedArgs = isWindowsNpmShim
    ? [npmCliPath(command), ...args]
    : isWindowsCommandScript
      ? ['/d', '/s', '/c', platformCommand, ...args]
      : args;
  const result = spawnSync(resolvedCommand, resolvedArgs, {
    cwd: options.cwd,
    encoding: options.encoding || 'utf8',
    env: options.env || childEnvForCommand(command),
    maxBuffer: options.maxBuffer || 16 * 1024 * 1024,
    shell: false,
    windowsHide: true,
  });
  if (result.status !== 0) {
    const output = String(result.stderr || result.stdout || result.error?.message || '').trim();
    throw new Error(`${resolvedCommand} ${resolvedArgs.join(' ')} failed${output ? `\n${output}` : ''}`);
  }
  return result;
}
