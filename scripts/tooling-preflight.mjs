#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { repoRoot } from './lib/project-metadata.mjs';
import { commandForPlatform, commandNeedsShell } from './lib/process.mjs';
import { childEnvForCommand } from './lib/safe-child-env.mjs';

const maxBuffer = 32 * 1024 * 1024;

function localBin(name) {
  const binaryName = process.platform === 'win32' ? `${name}.cmd` : name;
  return path.join(repoRoot, 'node_modules', '.bin', binaryName);
}

function hasLocalBin(name) {
  return fs.existsSync(localBin(name));
}

function spawnCommand(command, args, options) {
  if (commandNeedsShell(command)) {
    return spawnSync(process.env.COMSPEC || 'cmd.exe', ['/d', '/c', command, ...args], options);
  }
  return spawnSync(command, args, options);
}

function run(command, args, options = {}) {
  const resolved = options.localBin && hasLocalBin(command)
    ? localBin(command)
    : commandForPlatform(command, process.platform);
  const result = spawnCommand(resolved, args, {
    cwd: options.cwd || repoRoot,
    encoding: 'utf8',
    env: options.env || childEnvForCommand(command),
    maxBuffer,
    windowsHide: true,
  });
  return {
    command: [resolved, ...args].join(' '),
    status: result.status,
    signal: result.signal,
    error: result.error?.message || null,
    stdout: String(result.stdout || '').trim(),
    stderr: String(result.stderr || '').trim(),
  };
}

function commandExists(command) {
  const probe = process.platform === 'win32'
    ? run('where', [command])
    : run('sh', ['-c', `command -v ${JSON.stringify(command)} >/dev/null 2>&1`]);
  return probe.status === 0;
}

function resultStatus(result, { optional = false, warnOnOutputPattern = null } = {}) {
  if (result.error && /ENOENT/i.test(result.error)) return optional ? 'skipped' : 'fail';
  if (result.status !== 0) return optional ? 'warn' : 'fail';
  if (warnOnOutputPattern && warnOnOutputPattern.test(`${result.stdout}\n${result.stderr}`)) return 'warn';
  return 'pass';
}

const checks = [];

const publint = run('publint', ['packages/npm/cli'], { localBin: true });
checks.push({
  name: 'publint',
  required: true,
  status: resultStatus(publint),
  detail: publint.error || publint.stderr || publint.stdout,
  command: publint.command,
});

if (commandExists('check-jsonschema')) {
  const jsonschema = run('check-jsonschema', [
    '--schemafile', 'schemas/mcpace-hub.schema.json',
    'examples/mcpace-hub.minimal.json',
    'examples/mcpace-hub.workstation.json',
  ]);
  checks.push({
    name: 'check-jsonschema',
    required: false,
    status: resultStatus(jsonschema),
    detail: jsonschema.error || jsonschema.stderr || jsonschema.stdout,
    command: jsonschema.command,
  });
} else {
  checks.push({
    name: 'check-jsonschema',
    required: false,
    status: 'skipped',
    detail: 'Install with: python -m pip install check-jsonschema',
  });
}

if (commandExists('actionlint')) {
  const actionlint = run('actionlint', ['.github/workflows']);
  checks.push({
    name: 'actionlint',
    required: false,
    status: resultStatus(actionlint),
    detail: actionlint.error || actionlint.stderr || actionlint.stdout || 'ok',
    command: actionlint.command,
  });
} else {
  checks.push({
    name: 'actionlint',
    required: false,
    status: 'skipped',
    detail: 'Install actionlint from rhysd/actionlint or an OS/package-manager binary.',
  });
}

if (commandExists('zizmor')) {
  const zizmorArgs = ['--offline', '--no-exit-codes', '--color', 'never', '--format', 'plain'];
  const zizmorConfig = path.join(repoRoot, '.github', 'zizmor.yml');
  if (fs.existsSync(zizmorConfig)) zizmorArgs.push('--config', zizmorConfig);
  zizmorArgs.push('.github/workflows');
  const zizmor = run('zizmor', zizmorArgs);
  checks.push({
    name: 'zizmor',
    required: false,
    status: resultStatus(zizmor, { optional: true, warnOnOutputPattern: /\b(error|warning)\[/i }),
    detail: zizmor.error || zizmor.stderr || zizmor.stdout || 'ok',
    command: zizmor.command,
  });
} else {
  checks.push({
    name: 'zizmor',
    required: false,
    status: 'skipped',
    detail: 'Install with: python -m pip install zizmor',
  });
}

if (commandExists('gitleaks')) {
  const gitleaks = run('gitleaks', ['detect', '--no-git', '--source', '.', '--redact']);
  checks.push({
    name: 'gitleaks',
    required: false,
    status: resultStatus(gitleaks),
    detail: gitleaks.error || gitleaks.stderr || gitleaks.stdout || 'ok',
    command: gitleaks.command,
  });
} else {
  checks.push({
    name: 'gitleaks',
    required: false,
    status: 'skipped',
    detail: 'Install gitleaks from gitleaks/gitleaks to scan repository contents for secrets.',
  });
}

if (commandExists('osv-scanner')) {
  const osv = run('osv-scanner', ['scan', '--lockfile', 'package-lock.json']);
  checks.push({
    name: 'osv-scanner',
    required: false,
    status: resultStatus(osv),
    detail: osv.error || osv.stderr || osv.stdout || 'ok',
    command: osv.command,
  });
} else {
  checks.push({
    name: 'osv-scanner',
    required: false,
    status: 'skipped',
    detail: 'Install osv-scanner from google/osv-scanner for lockfile vulnerability scans.',
  });
}

const hardFailures = checks.filter((check) => check.required && check.status !== 'pass');
const installedFailures = checks.filter((check) => !check.required && check.status === 'fail');
const status = hardFailures.length > 0 || installedFailures.length > 0
  ? 'fail'
  : checks.some((check) => check.status === 'warn')
    ? 'warn'
    : 'pass';

const payload = {
  schema: 'mcpace.toolingPreflight.v1',
  generatedAt: new Date().toISOString(),
  status,
  checks,
};
console.log(JSON.stringify(payload, null, 2));

if (hardFailures.length > 0 || installedFailures.length > 0) process.exit(1);
