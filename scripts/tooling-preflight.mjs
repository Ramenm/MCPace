#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { commandExists, resultStatus, runCommand } from './lib/command-runner.mjs';
import { repoRoot } from './lib/project-metadata.mjs';


const cleanupDirs = [];
const generatedParts = new Set(['.git', 'node_modules', 'target', 'dist', '.cache', '.pytest_cache', '__pycache__']);

function shouldSkipScanPath(relativePath) {
  return relativePath.split(/[\/]+/).some((part) => generatedParts.has(part));
}

function copyForScan(source, destination, relativePath) {
  if (shouldSkipScanPath(relativePath)) return;
  const stat = fs.lstatSync(source);
  if (stat.isSymbolicLink() || !stat.isDirectory() && !stat.isFile()) return;
  if (stat.isDirectory()) {
    fs.mkdirSync(destination, { recursive: true });
    for (const entry of fs.readdirSync(source, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name))) {
      const childRelative = path.posix.join(relativePath, entry.name);
      copyForScan(path.join(source, entry.name), path.join(destination, entry.name), childRelative);
    }
    return;
  }
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.copyFileSync(source, destination);
}

function releaseManifestPaths() {
  try {
    const manifest = JSON.parse(fs.readFileSync(path.join(repoRoot, 'release-manifest.json'), 'utf8'));
    return Array.isArray(manifest.includePaths) ? manifest.includePaths : [];
  } catch {
    return [];
  }
}

function prepareGitleaksScanSource() {
  const scanRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-gitleaks-scan-'));
  cleanupDirs.push(scanRoot);
  for (const relativePath of releaseManifestPaths()) {
    if (typeof relativePath !== 'string' || relativePath.length === 0 || shouldSkipScanPath(relativePath)) continue;
    const absoluteSource = path.resolve(repoRoot, relativePath);
    if (!absoluteSource.startsWith(repoRoot + path.sep) && absoluteSource !== repoRoot) continue;
    if (!fs.existsSync(absoluteSource)) continue;
    copyForScan(absoluteSource, path.join(scanRoot, relativePath), relativePath);
  }
  return scanRoot;
}

function run(command, args, options = {}) {
  return runCommand(command, args, {
    cwd: options.cwd || repoRoot,
    env: options.env,
    localBin: options.localBin,
    timeoutMs: options.timeoutMs ?? 120_000,
  });
}

function detail(result) {
  return result.error || result.stderr.trim() || result.stdout.trim() || 'ok';
}

function checkFromResult(name, required, result, statusOptions = {}) {
  return { name, required, status: resultStatus(result, statusOptions), detail: detail(result), command: result.command };
}

function skippedCheck(name, detailText) {
  return { name, required: false, status: 'skipped', detail: detailText };
}

function optionalToolCheck({ name, args, missing, statusOptions, timeoutMs }) {
  if (!commandExists(name, { includeLocalBin: true })) return skippedCheck(name, missing);
  return checkFromResult(name, false, run(name, args, { timeoutMs }), statusOptions);
}

function workflowFileArgs() {
  const workflowDir = path.join(repoRoot, '.github', 'workflows');
  if (!fs.existsSync(workflowDir)) return [];
  return fs.readdirSync(workflowDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && /\.ya?ml$/i.test(entry.name))
    .map((entry) => path.join('.github', 'workflows', entry.name).split(path.sep).join('/'))
    .sort();
}


function actionlintArgs() {
  const args = [];
  const config = path.join(repoRoot, '.github', 'actionlint.yaml');
  if (fs.existsSync(config)) args.push('-config-file', config);
  args.push(...workflowFileArgs());
  return args;
}

function gitleaksArgs() {
  const args = ['detect', '--no-git', '--source', prepareGitleaksScanSource(), '--redact'];
  const config = path.join(repoRoot, '.gitleaks.toml');
  if (fs.existsSync(config)) args.push('--config', config);
  return args;
}

function zizmorArgs() {
  const args = ['--offline', '--no-exit-codes', '--color', 'never', '--format', 'plain'];
  const config = path.join(repoRoot, '.github', 'zizmor.yml');
  if (fs.existsSync(config)) args.push('--config', config);
  args.push('.github/workflows');
  return args;
}

const checks = [
  checkFromResult('publint', true, run('publint', ['packages/npm/cli'], { localBin: true })),
  optionalToolCheck({
    name: 'check-jsonschema',
    args: ['--schemafile', 'schemas/mcpace-hub.schema.json', 'examples/mcpace-hub.minimal.json', 'examples/mcpace-hub.workstation.json'],
    missing: 'Install with: python -m pip install check-jsonschema',
  }),
  optionalToolCheck({
    name: 'actionlint',
    args: actionlintArgs(),
    missing: 'Install actionlint from rhysd/actionlint or an OS/package-manager binary.',
  }),
  optionalToolCheck({
    name: 'zizmor',
    args: zizmorArgs(),
    missing: 'Install with: python -m pip install zizmor',
    statusOptions: { optional: true, warnOnOutputPattern: /\b(error|warning)\[/i },
    timeoutMs: 120_000,
  }),
  optionalToolCheck({
    name: 'gitleaks',
    args: gitleaksArgs(),
    missing: 'Install gitleaks from gitleaks/gitleaks to scan repository contents for secrets.',
    timeoutMs: 120_000,
  }),
  optionalToolCheck({
    name: 'osv-scanner',
    args: ['scan', '--experimental-offline', '--lockfile', 'package-lock.json'],
    missing: 'Install osv-scanner from google/osv-scanner for lockfile vulnerability scans.',
    statusOptions: { optional: true },
    timeoutMs: 30_000,
  }),
];

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

for (const dir of cleanupDirs) fs.rmSync(dir, { recursive: true, force: true });

if (hardFailures.length > 0 || installedFailures.length > 0) process.exit(1);
