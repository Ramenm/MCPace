import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { childEnvForCommand } from './safe-child-env.mjs';
import { commandForPlatform, commandNeedsShell, windowsCommandShell } from './process.mjs';
import { repoRoot } from './project-metadata.mjs';

export const DEFAULT_MAX_BUFFER = 32 * 1024 * 1024;

export function localBinPath(name, root = repoRoot, platform = process.platform) {
  const binaryName = platform === 'win32' ? `${name}.cmd` : name;
  return path.join(root, 'node_modules', '.bin', binaryName);
}

export function isExecutableFile(filePath, platform = process.platform) {
  try {
    const stat = fs.statSync(filePath);
    if (!stat.isFile()) return false;
    return platform === 'win32' || (stat.mode & 0o111) !== 0;
  } catch {
    return false;
  }
}

export function hasLocalBin(name, options = {}) {
  return isExecutableFile(localBinPath(name, options.root ?? repoRoot, options.platform ?? process.platform), options.platform ?? process.platform);
}

export function commandTokenIsSafe(command) {
  return /^[A-Za-z0-9_.-]+(?:\.exe)?$/i.test(command);
}

export function executableCandidateNames(command, options = {}) {
  const platform = options.platform ?? process.platform;
  if (platform !== 'win32') return [command];
  const pathext = String(options.pathext ?? process.env.PATHEXT ?? '.COM;.EXE;.BAT;.CMD')
    .split(';')
    .filter(Boolean);
  if (path.extname(command)) return [command];
  return pathext.map((ext) => `${command}${ext.toLowerCase()}`);
}

export function commandExists(command, options = {}) {
  if (!commandTokenIsSafe(command)) return false;
  const platform = options.platform ?? process.platform;
  if (options.includeLocalBin && hasLocalBin(command, { root: options.root ?? repoRoot, platform })) return true;
  const pathValue = String(options.pathValue ?? process.env.PATH ?? process.env.Path ?? '');
  for (const directory of pathValue.split(path.delimiter).filter(Boolean)) {
    for (const candidateName of executableCandidateNames(command, { platform, pathext: options.pathext })) {
      if (isExecutableFile(path.join(directory, candidateName), platform)) return true;
    }
  }
  return false;
}

function spawnCommand(command, args, options) {
  const { platform, ...spawnOptions } = options;
  if (commandNeedsShell(command, platform)) {
    return spawnSync(windowsCommandShell(spawnOptions.env), ['/d', '/c', command, ...args], spawnOptions);
  }
  return spawnSync(command, args, spawnOptions);
}

export function runCommand(command, args = [], options = {}) {
  const platform = options.platform ?? process.platform;
  const started = Date.now();
  const resolvedCommand = options.localBin && hasLocalBin(command, { root: options.root ?? repoRoot, platform })
    ? localBinPath(command, options.root ?? repoRoot, platform)
    : commandForPlatform(command, platform);
  const env = options.env ?? childEnvForCommand(command, options.envOverrides ?? {});
  const result = spawnCommand(resolvedCommand, args, {
    cwd: options.cwd ?? repoRoot,
    encoding: options.encoding ?? 'utf8',
    env,
    maxBuffer: options.maxBuffer ?? DEFAULT_MAX_BUFFER,
    shell: false,
    stdio: options.stdio,
    timeout: options.timeoutMs,
    windowsHide: true,
    platform,
  });
  const stdout = options.stdio === 'inherit' ? '' : String(result.stdout ?? '');
  const stderr = options.stdio === 'inherit' ? '' : String(result.stderr ?? '');
  return {
    command: [resolvedCommand, ...args].join(' '),
    status: result.error ? 'failed' : result.status === 0 ? 'pass' : 'fail',
    exitCode: result.status,
    signal: result.signal,
    durationMs: Date.now() - started,
    stdout,
    stderr,
    stdoutTail: stdout.slice(-4000),
    stderrTail: stderr.slice(-4000),
    error: result.error ? String(result.error.message ?? result.error) : null,
  };
}

export function runCommandInherited(command, args = [], options = {}) {
  return runCommand(command, args, { ...options, stdio: 'inherit' });
}

export function resultStatus(result, { optional = false, warnOnOutputPattern = null } = {}) {
  if (result.error && /ENOENT/i.test(result.error)) return optional ? 'skipped' : 'fail';
  if (result.status !== 'pass' && result.exitCode !== 0) return optional ? 'warn' : 'fail';
  if (warnOnOutputPattern && warnOnOutputPattern.test(`${result.stdout}\n${result.stderr}`)) return 'warn';
  return 'pass';
}
