import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { resolveBinary } from './resolve-binary.js';
import { currentTargetKey, describeSupportedTargets, detectTarget, packageNamesForTarget } from './platform.js';
import { parseNodeMajor, isSupportedNodeMajor, MINIMUM_NODE_MAJOR } from './runtime.js';

const require = createRequire(import.meta.url);
const NPM_PROBE_TIMEOUT_MS = 2500;

export const CANONICAL_COMMAND = 'mcpace';
export const CLI_COMMAND_ALIASES = Object.freeze(['mcpace']);
export const TERMINAL_DIAGNOSTIC_FLAGS = Object.freeze([
  '--mcpace-npm-diagnostics',
  '--npm-diagnostics'
]);

function pathEnvKey(env = process.env) {
  return Object.keys(env).find((key) => key.toLowerCase() === 'path') || 'PATH';
}

function pathEntries(env = process.env) {
  const raw = env[pathEnvKey(env)] || '';
  return String(raw).split(path.delimiter).filter(Boolean);
}

function isExecutableLike(filePath, platform = process.platform) {
  try {
    const stat = fs.statSync(filePath);
    if (!stat.isFile() && !stat.isSymbolicLink?.()) return false;
    if (platform === 'win32') return true;
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function commandFileNames(commandName, platform = process.platform) {
  if (platform !== 'win32') return [commandName];
  return [commandName, `${commandName}.cmd`, `${commandName}.ps1`, `${commandName}.exe`, `${commandName}.bat`];
}

function findCommandOnPath(commandName, env = process.env, platform = process.platform) {
  const matches = [];
  const names = commandFileNames(commandName, platform);
  for (const dir of pathEntries(env)) {
    for (const name of names) {
      const candidate = path.join(dir, name);
      if (isExecutableLike(candidate, platform)) {
        matches.push(candidate);
      }
    }
  }
  return matches;
}

function npmProbe(args) {
  const npmExecPath = process.env.npm_execpath;
  const candidates = [];
  if (npmExecPath && fs.existsSync(npmExecPath)) {
    candidates.push({ command: process.execPath, args: [npmExecPath, ...args], shell: false });
  }
  candidates.push({
    command: process.platform === 'win32' ? 'npm.cmd' : 'npm',
    args,
    shell: process.platform === 'win32'
  });

  for (const candidate of candidates) {
    const result = spawnSync(candidate.command, candidate.args, {
      encoding: 'utf8',
      timeout: NPM_PROBE_TIMEOUT_MS,
      shell: candidate.shell,
      windowsHide: true,
      env: process.env
    });
    if (result.status === 0) {
      return String(result.stdout || '').trim();
    }
  }
  return null;
}

function npmMetadata() {
  return {
    version: npmProbe(['--version']),
    globalPrefix: npmProbe(['prefix', '-g']),
    localPrefix: npmProbe(['prefix'])
  };
}

function explicitPathStatus(name, value) {
  const text = String(value || '').trim();
  if (!text) {
    return { name, set: false };
  }
  const unquoted = text.length >= 2 && ((text[0] === '"' && text.at(-1) === '"') || (text[0] === "'" && text.at(-1) === "'"))
    ? text.slice(1, -1)
    : text;
  const absolute = path.resolve(unquoted);
  let exists = false;
  let isFile = false;
  let executable = false;
  try {
    const stat = fs.statSync(absolute);
    exists = true;
    isFile = stat.isFile();
    executable = isExecutableLike(absolute);
  } catch {
    // absent paths are reported below
  }
  return { name, set: true, value: unquoted, absolute, exists, isFile, executable };
}

function optionalPackageStatuses(target) {
  return packageNamesForTarget(target).map((packageName) => {
    try {
      const packageJsonPath = require.resolve(`${packageName}/package.json`);
      return { packageName, installed: true, packageJsonPath };
    } catch {
      return { packageName, installed: false };
    }
  });
}

function binaryResolutionStatus() {
  try {
    const binaryPath = resolveBinary();
    return { status: 'resolved', binaryPath };
  } catch (error) {
    return {
      status: 'unresolved',
      error: error?.message || String(error),
      code: error?.code || null
    };
  }
}

function buildHints(payload) {
  const hints = [];
  if (!isSupportedNodeMajor(payload.node.major)) {
    hints.push(`Use Node.js ${MINIMUM_NODE_MAJOR}+; different terminals may pick different node versions from PATH.`);
  }
  const anyShim = CLI_COMMAND_ALIASES.some((name) => payload.path.commands[name]?.length > 0);
  if (!anyShim) {
    hints.push('No mcpace shim was found on PATH. Add the npm global bin directory to PATH, reopen the terminal, clear the shell command cache (`hash -r` in bash or `rehash` in zsh), or run through npm exec/npx.');
  }
  if (payload.resolution.status !== 'resolved') {
    const optionalInstalled = payload.optionalPackages.some((entry) => entry.installed);
    if (!optionalInstalled) {
      hints.push('No native optional platform package was found. Reinstall without --omit=optional, install the platform package, build locally, or set MCPACE_BINARY_PATH.');
    }
    if (payload.target.key.includes('musl')) {
      hints.push('This looks like a musl/Alpine Linux target; publish-enabled npm packages currently cover glibc Linux, macOS, and Windows.');
    }
    if (payload.env.MCPACE_BINARY_PATH.set && !payload.env.MCPACE_BINARY_PATH.executable) {
      hints.push('MCPACE_BINARY_PATH is set but does not point to an executable file visible from this terminal.');
    }
  }
  return hints;
}

export function isTerminalDiagnosticsRequest(args = []) {
  return args.some((arg) => TERMINAL_DIAGNOSTIC_FLAGS.includes(arg));
}

export function terminalDiagnosticsWantsJson(args = []) {
  return args.includes('--json') || args.includes('--json-output') || process.env.MCPACE_NPM_DIAGNOSTICS_JSON === '1';
}

export function collectTerminalDiagnostics(options = {}) {
  const target = detectTarget();
  const commands = Object.fromEntries(
    CLI_COMMAND_ALIASES.map((name) => [name, findCommandOnPath(name).slice(0, 12)])
  );
  const payload = {
    schema: 'mcpace.npmTerminalDiagnostics.v1',
    status: 'ok',
    canonicalCommand: CANONICAL_COMMAND,
    commandAliases: CLI_COMMAND_ALIASES,
    invokedAs: options.invokedAs || path.basename(process.argv[1] || ''),
    node: {
      version: process.versions.node,
      major: parseNodeMajor(process.versions.node),
      supported: isSupportedNodeMajor(parseNodeMajor(process.versions.node)),
      minimumMajor: MINIMUM_NODE_MAJOR,
      execPath: process.execPath
    },
    npm: npmMetadata(),
    platform: {
      nodePlatform: process.platform,
      nodeArch: process.arch,
      cwd: process.cwd()
    },
    target: {
      key: target?.key ?? currentTargetKey(),
      detected: Boolean(target),
      supportedTargets: describeSupportedTargets()
    },
    env: {
      pathKey: pathEnvKey(),
      pathEntryCount: pathEntries().length,
      MCPACE_BINARY_PATH: explicitPathStatus('MCPACE_BINARY_PATH', process.env.MCPACE_BINARY_PATH),
      MCPACE_DEV_BINARY: explicitPathStatus('MCPACE_DEV_BINARY', process.env.MCPACE_DEV_BINARY)
    },
    path: { commands },
    optionalPackages: optionalPackageStatuses(target),
    resolution: binaryResolutionStatus()
  };
  payload.hints = buildHints(payload);
  return payload;
}

export function formatTerminalDiagnostics(payload) {
  const lines = [
    `MCPace npm terminal diagnostics (${payload.schema})`,
    `canonical command: ${payload.canonicalCommand}`,
    `aliases: ${payload.commandAliases.join(', ')}`,
    `node: ${payload.node.version} (${payload.node.supported ? 'supported' : `requires ${payload.node.minimumMajor}+`})`,
    `npm: ${payload.npm.version || 'not found'}`,
    `target: ${payload.target.key}${payload.target.detected ? '' : ' (unsupported)'}`,
    `resolution: ${payload.resolution.status}${payload.resolution.binaryPath ? ` -> ${payload.resolution.binaryPath}` : ''}`
  ];
  if (payload.resolution.error) {
    lines.push(`resolution error: ${payload.resolution.error}`);
  }
  for (const name of payload.commandAliases) {
    const matches = payload.path.commands[name] || [];
    lines.push(`${name} on PATH: ${matches.length ? matches.join(', ') : 'not found'}`);
  }
  if (payload.npm.globalPrefix) {
    lines.push(`npm global prefix: ${payload.npm.globalPrefix}`);
  }
  if (payload.hints.length) {
    lines.push('hints:');
    for (const hint of payload.hints) lines.push(`- ${hint}`);
  }
  return `${lines.join('\n')}\n`;
}

export function formatTerminalResolutionHelp(error) {
  const message = error?.message || String(error || 'unknown error');
  return [
    'MCPace npm launcher could not start the native binary.',
    `Reason: ${message}`,
    'Run `mcpace --mcpace-npm-diagnostics` for PATH/Node/native-package diagnostics.',
    'Canonical command is `mcpace`.',
    'Common fixes: reopen the terminal after install, clear the shell command cache (`hash -r` in bash or `rehash` in zsh), ensure the npm global bin directory is on PATH, reinstall with optional dependencies, or set MCPACE_BINARY_PATH to a built native binary.'
  ].join('\n');
}
