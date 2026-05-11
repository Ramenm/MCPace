#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { detectLibc } from '../packages/npm/cli/lib/platform.js';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_STEP_TIMEOUT_MS = 120000;
const DEFAULT_DOCKER_TIMEOUT_MS = 1200000;
const VALID_PROFILES = new Set(['host', 'standard', 'full', 'release']);
const DANGEROUS_RELEASE_SEGMENTS = new Set([
  '.git',
  '.claude',
  '.codex',
  '.omc',
  '%SystemDrive%',
  'node_modules',
  'target',
  'logs',
  'backups',
  '__MACOSX'
]);
const NPX_BASELINE_ENV_VARS = [
  'NPM_CONFIG_REGISTRY',
  'NPM_CONFIG_USERCONFIG',
  'NPM_CONFIG_GLOBALCONFIG',
  'NPM_CONFIG_CACHE',
  'NODE_EXTRA_CA_CERTS',
  'SSL_CERT_FILE',
  'REQUESTS_CA_BUNDLE',
  'HTTP_PROXY',
  'HTTPS_PROXY',
  'NO_PROXY',
  'http_proxy',
  'https_proxy',
  'no_proxy',
  'CI'
];

function parseArgs(argv) {
  const parsed = {
    profile: process.env.MCPACE_LINUX_CHECK_PROFILE || 'standard',
    json: false,
    write: null,
    markdown: null,
    noDocker: false,
    dryRun: false,
    strict: false,
    createDirs: false,
    help: false,
    root: process.env.MCPACE_ROOT || repoRoot,
    bin: process.env.MCPACE_BIN || process.env.MCPACE_BINARY_PATH || '',
    timeoutMs: parsePositiveInt(process.env.MCPACE_LINUX_CHECK_TIMEOUT_MS, DEFAULT_STEP_TIMEOUT_MS),
    dockerTimeoutMs: parsePositiveInt(process.env.MCPACE_LINUX_CHECK_DOCKER_TIMEOUT_MS, DEFAULT_DOCKER_TIMEOUT_MS)
  };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--profile': parsed.profile = argv[++index] || parsed.profile; break;
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; break;
      case '--markdown': parsed.markdown = argv[++index] || null; break;
      case '--no-docker': parsed.noDocker = true; break;
      case '--dry-run': parsed.dryRun = true; break;
      case '--strict': parsed.strict = true; break;
      case '--create-dirs': parsed.createDirs = true; break;
      case '--root': parsed.root = argv[++index] || parsed.root; break;
      case '--bin': parsed.bin = argv[++index] || parsed.bin; break;
      case '--timeout-ms': parsed.timeoutMs = parsePositiveInt(argv[++index], parsed.timeoutMs); break;
      case '--docker-timeout-ms': parsed.dockerTimeoutMs = parsePositiveInt(argv[++index], parsed.dockerTimeoutMs); break;
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported linux-auto-check argument: ${token}`);
    }
  }
  if (parsed.help) return parsed;
  if (!VALID_PROFILES.has(parsed.profile)) {
    throw new Error(`unsupported --profile '${parsed.profile}', expected ${[...VALID_PROFILES].join('|')}`);
  }
  parsed.root = path.resolve(parsed.root);
  if (parsed.bin) parsed.bin = path.resolve(parsed.bin);
  return parsed;
}

function printHelp() {
  process.stdout.write(`Usage: node scripts/linux-auto-check.mjs [--profile host|standard|full|release] [--json] [--write <path>] [--markdown <path>] [--root <path>] [--bin <path>] [--no-docker] [--dry-run]\n`);
}

function parsePositiveInt(value, fallback) {
  const parsed = Number.parseInt(String(value || ''), 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function npmCommand() {
  return process.platform === 'win32' ? 'npm.cmd' : 'npm';
}

function commandOutput(command, args = [], options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd || repoRoot,
    encoding: 'utf8',
    env: options.env || process.env,
    timeout: options.timeoutMs || DEFAULT_STEP_TIMEOUT_MS,
    windowsHide: true
  });
  return {
    status: result.status,
    signal: result.signal,
    timedOut: result.error?.code === 'ETIMEDOUT',
    error: result.error ? result.error.message : null,
    stdout: trimText(result.stdout),
    stderr: trimText(result.stderr)
  };
}

function trimText(value, limit = 12000) {
  const text = String(value || '').trim();
  if (text.length <= limit) return text;
  return `${text.slice(0, limit)}\n...[trimmed ${text.length - limit} chars]`;
}

function toolVersion(command, args = ['--version']) {
  const result = commandOutput(command, args, { timeoutMs: 15000 });
  if (result.status !== 0) {
    return { present: false, command, version: null, error: result.error || result.stderr || result.stdout || `exit ${result.status}` };
  }
  return { present: true, command, version: (result.stdout || result.stderr || '').split(/\r?\n/)[0].trim() || null, error: null };
}

function dockerAvailable() {
  const docker = toolVersion('docker', ['--version']);
  if (!docker.present) return { ...docker, daemon: false };
  const info = commandOutput('docker', ['info', '--format', '{{json .ServerVersion}}'], { timeoutMs: 30000 });
  return { ...docker, daemon: info.status === 0, daemonVersion: info.status === 0 ? trimText(info.stdout).replace(/^"|"$/g, '') : null, daemonError: info.status === 0 ? null : info.error || info.stderr || info.stdout || `exit ${info.status}` };
}

function readJson(relativeOrAbsolutePath) {
  const filePath = path.isAbsolute(relativeOrAbsolutePath) ? relativeOrAbsolutePath : path.join(repoRoot, relativeOrAbsolutePath);
  try { return JSON.parse(fs.readFileSync(filePath, 'utf8')); } catch { return null; }
}

function readText(relativeOrAbsolutePath) {
  const filePath = path.isAbsolute(relativeOrAbsolutePath) ? relativeOrAbsolutePath : path.join(repoRoot, relativeOrAbsolutePath);
  try { return fs.readFileSync(filePath, 'utf8'); } catch { return null; }
}

function fileExists(relativePath) {
  return fs.existsSync(path.join(repoRoot, relativePath));
}

function safeReaddir(dir) {
  try { return fs.readdirSync(dir); } catch { return []; }
}

function linuxDistroInfo() {
  if (process.platform !== 'linux') return null;
  const text = readText('/etc/os-release') || readText('/usr/lib/os-release') || '';
  const entries = Object.fromEntries(text.split(/\r?\n/).filter((line) => line.includes('=')).map((line) => {
    const [key, ...rest] = line.split('=');
    return [key, rest.join('=').replace(/^"|"$/g, '')];
  }));
  return { id: entries.ID || null, versionId: entries.VERSION_ID || null, name: entries.PRETTY_NAME || entries.NAME || null };
}

function pathSegments(relativePath) {
  return String(relativePath).split(/[\\/]+/).filter(Boolean);
}

function containsDangerousSegment(relativePath) {
  return pathSegments(relativePath).some((segment) => DANGEROUS_RELEASE_SEGMENTS.has(segment));
}

function canWriteDirectory(directory, createDirs = false) {
  const resolved = path.resolve(directory);
  try {
    if (createDirs) fs.mkdirSync(resolved, { recursive: true, mode: 0o700 });
    fs.accessSync(resolved, fs.constants.W_OK);
    return { status: 'pass', message: `writable: ${resolved}`, path: resolved };
  } catch (error) {
    return { status: 'warn', message: `not writable or missing: ${resolved}`, path: resolved, error: error instanceof Error ? error.message : String(error) };
  }
}

function checkReleaseManifestHygiene() {
  const manifest = readJson('release-manifest.json');
  if (!manifest) return { status: 'fail', message: 'release-manifest.json is missing or invalid', findings: {} };
  const includePaths = Array.isArray(manifest.includePaths) ? manifest.includePaths : [];
  const optionalIncludePaths = Array.isArray(manifest.optionalIncludePaths) ? manifest.optionalIncludePaths : [];
  const allPaths = [...includePaths, ...optionalIncludePaths];
  const dangerous = allPaths.filter(containsDangerousSegment);
  const missing = includePaths.filter((entry) => !fs.existsSync(path.join(repoRoot, entry)));
  const rootScreenshots = safeReaddir(repoRoot).filter((entry) => /^screenshot.*\.(png|jpe?g|webp)$/i.test(entry));
  const status = dangerous.length || missing.length ? 'fail' : rootScreenshots.length ? 'warn' : 'pass';
  return {
    status,
    message: status === 'pass'
      ? 'release manifest excludes local/private machine-state paths'
      : dangerous.length || missing.length
        ? 'release manifest contains paths that should not ship'
        : 'root-level screenshots are present; remove them from source snapshots even when release manifest excludes them',
    findings: { dangerous, missing, rootScreenshots }
  };
}

function checkArchiveReleaseExclusions() {
  const source = readText('scripts/archive-release.mjs') || '';
  const missing = ['.claude', '.codex', '.omc', '%SystemDrive%'].filter((segment) => !source.includes(segment));
  return { status: missing.length === 0 ? 'pass' : 'warn', message: missing.length === 0 ? 'archive-release excludes known local machine-state directories' : 'archive-release does not explicitly exclude all known local machine-state directories', missing };
}

function checkVendoredExecutableBits() {
  const vendorRoot = path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor');
  const findings = [];
  walk(vendorRoot, (filePath) => {
    if (path.basename(filePath) !== 'mcpace') return;
    const mode = fs.statSync(filePath).mode & 0o777;
    if ((mode & 0o111) === 0) findings.push({ path: path.relative(repoRoot, filePath).split(path.sep).join('/'), mode: `0${mode.toString(8)}` });
  });
  return { status: findings.length === 0 ? 'pass' : 'fail', message: findings.length === 0 ? 'vendored Unix binaries are executable' : `${findings.length} vendored Unix binary file(s) are not executable`, findings };
}

function walk(root, visit) {
  if (!fs.existsSync(root)) return;
  const stack = [root];
  while (stack.length) {
    const current = stack.pop();
    const stat = fs.statSync(current);
    if (stat.isDirectory()) {
      for (const entry of fs.readdirSync(current)) stack.push(path.join(current, entry));
    } else {
      visit(current);
    }
  }
}

function resolveMcpaceBinary(explicitBin) {
  const candidates = [];
  if (explicitBin) candidates.push(explicitBin);
  const fromPath = commandPath('mcpace');
  if (fromPath) candidates.push(fromPath);
  candidates.push(path.join(repoRoot, 'target', 'release', 'mcpace'));
  candidates.push(path.join(repoRoot, 'target', 'debug', 'mcpace'));
  for (const candidate of candidates) {
    try {
      if (fs.existsSync(candidate)) {
        fs.accessSync(candidate, fs.constants.X_OK);
        return candidate;
      }
    } catch {}
  }
  return null;
}

function commandPath(name) {
  const result = commandOutput('sh', ['-lc', `command -v '${String(name).replaceAll("'", "'\\''")}'`], { timeoutMs: 3000 });
  return result.status === 0 ? result.stdout.split(/\r?\n/)[0].trim() || null : null;
}

function listSettingsFiles(root) {
  const files = [];
  const main = path.join(root, 'mcp_settings.json');
  if (fs.existsSync(main)) files.push(main);
  const dir = path.join(root, 'mcp_settings.d');
  try {
    for (const entry of fs.readdirSync(dir).sort()) {
      if (entry.endsWith('.json')) files.push(path.join(dir, entry));
    }
  } catch {}
  return files;
}

function loadMergedServers(root) {
  const servers = new Map();
  for (const file of listSettingsFiles(root)) {
    const json = readJson(file);
    const rawServers = json?.mcpServers && typeof json.mcpServers === 'object' ? json.mcpServers : {};
    for (const [name, config] of Object.entries(rawServers)) servers.set(name, { name, source: file, config });
  }
  return [...servers.values()].sort((a, b) => a.name.localeCompare(b.name));
}

function isNpxCommand(command) {
  return /(^|[\\/])npx(?:\.cmd)?$/i.test(String(command || '')) || /^npx(?:\.cmd)?$/i.test(String(command || ''));
}

function serverEnabled(config) {
  return config?.enabled !== false && config?.disabled !== true;
}

function serverEnvVars(config) {
  if (Array.isArray(config?.env_vars)) return config.env_vars.map(String);
  if (Array.isArray(config?.envVars)) return config.envVars.map(String);
  return [];
}

function checkNpxUpstreamEnv(root) {
  const servers = loadMergedServers(root);
  const findings = servers.filter((server) => serverEnabled(server.config) && isNpxCommand(server.config?.command)).map((server) => {
    const envVars = new Set(serverEnvVars(server.config));
    return { name: server.name, source: path.relative(repoRoot, server.source), missingBaselineEnvVars: NPX_BASELINE_ENV_VARS.filter((name) => !envVars.has(name)), hasInlineEnv: Boolean(server.config?.env && Object.keys(server.config.env).length > 0) };
  });
  const missing = findings.filter((finding) => finding.missingBaselineEnvVars.length > 0);
  return { status: missing.length ? 'warn' : 'pass', message: missing.length ? `${missing.length} npx upstream(s) miss baseline env_vars` : 'npx upstream env_vars look configured or no npx upstreams enabled', findings, baselineEnvVars: NPX_BASELINE_ENV_VARS };
}

function hostIsLocal(host) {
  return ['127.0.0.1', 'localhost', '::1', '[::1]'].includes(String(host || '').trim().toLowerCase());
}

function hostFromConfig(root) {
  const value = readJson(path.join(root, 'mcpace.config.json'))?.serve?.host;
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

function systemdUserStatus(timeoutMs) {
  if (process.platform !== 'linux') return { available: false, reason: 'not-linux' };
  if (!commandPath('systemctl')) return { available: false, reason: 'systemctl-not-found' };
  const result = commandOutput('systemctl', ['--user', 'show-environment'], { timeoutMs });
  if (result.status === 0) return { available: true, reason: 'systemctl --user works' };
  return { available: false, reason: result.stderr || result.stdout || result.error || 'systemctl --user failed' };
}

function runNpmGate(name, args, options, required = true, timeoutMs = DEFAULT_STEP_TIMEOUT_MS) {
  if (options.dryRun) return { name, status: 'skip', message: `dry-run: npm ${args.join(' ')}`, details: { command: ['npm', ...args].join(' ') } };
  if (!commandPath('npm')) return { name, status: required ? 'fail' : 'warn', message: 'npm is not available', details: { command: ['npm', ...args].join(' ') } };
  const result = commandOutput(npmCommand(), args, { timeoutMs });
  return { name, status: result.status === 0 ? 'pass' : required ? 'fail' : 'warn', message: result.status === 0 ? `npm ${args.join(' ')} passed` : `npm ${args.join(' ')} failed`, details: { status: result.status, timedOut: result.timedOut, stdout: result.stdout, stderr: result.stderr } };
}

function buildChecks(options) {
  const checks = [];
  const add = (name, status, message, details = {}) => checks.push({ name, status, message, details });
  const libc = process.platform === 'linux' ? detectLibc() : null;
  add('linux-platform', process.platform === 'linux' ? 'pass' : 'fail', `platform=${process.platform}`, { platform: process.platform, arch: process.arch, release: os.release(), distro: linuxDistroInfo() });
  add('linux-architecture', ['x64', 'arm64'].includes(process.arch) ? 'pass' : 'fail', `arch=${process.arch}`, { supported: ['x64', 'arm64'] });
  add('linux-libc', libc === 'gnu' ? 'pass' : libc === 'musl' ? (options.profile === 'release' ? 'fail' : 'warn') : 'warn', libc ? `libc=${libc}` : 'could not determine libc', { libc });
  add('node-runtime', Number.parseInt(process.versions.node.split('.')[0], 10) >= 22 ? 'pass' : 'fail', `node=${process.versions.node}`, { required: '>=22' });
  for (const command of ['npm', 'npx', 'sh']) {
    const found = commandPath(command);
    add(`command-${command}`, found ? 'pass' : 'warn', found ? found : `${command} not found`, { path: found });
  }
  for (const command of ['cargo', 'rustc', 'docker', 'systemctl']) {
    const found = commandPath(command);
    add(`optional-command-${command}`, found ? 'pass' : 'warn', found ? found : `${command} not found`, { path: found });
  }
  add('package-json', readJson('package.json') ? 'pass' : 'fail', readJson('package.json') ? 'package.json present and parseable' : 'package.json missing or invalid');
  add('cargo-manifest', fileExists('Cargo.toml') ? 'pass' : 'warn', fileExists('Cargo.toml') ? 'Cargo.toml present' : 'Cargo.toml missing');
  add('release-targets', fileExists('release-targets.json') ? 'pass' : 'fail', fileExists('release-targets.json') ? 'release-targets.json present' : 'release-targets.json missing');

  for (const gate of [
    runNpmGate('npm-lint', ['run', 'lint:npm', '--', '--json'], options, true, 120000),
    runNpmGate('npm-cli-tests', ['run', 'test:npm'], options, true, 120000),
    runNpmGate('release-targets-gate', ['run', 'verify:release-targets'], options, true, 120000),
    runNpmGate('platform-packages-gate', ['run', 'verify:platform-packages'], options, true, 120000),
    runNpmGate('npm-pack-gate', ['run', 'verify:npm-pack'], options, true, 180000)
  ]) checks.push(gate);
  if (['standard', 'full', 'release'].includes(options.profile)) {
    checks.push(runNpmGate('repo-smoke-tests', ['run', 'test:repo:smoke'], options, true, 180000));
    checks.push(runNpmGate('secret-scan-gate', ['run', 'verify:secrets', '--', '--json'], options, true, 180000));
    checks.push(runNpmGate('source-audit-gate', ['run', 'audit:source', '--', '--json'], options, true, 180000));
  }
  if (['full', 'release'].includes(options.profile)) {
    checks.push(runNpmGate('defect-gates', ['run', 'verify:defect-gates'], options, true, 180000));
    checks.push(runNpmGate('bug-sweep', ['run', 'verify:bug-sweep'], options, true, 180000));
    checks.push(runNpmGate('supply-chain-audit', ['run', 'verify:supply-chain'], options, options.profile === 'release', 240000));
  }

  const configWritable = canWriteDirectory(path.dirname(options.root), options.createDirs);
  add('xdg-config-root-parent-writable', configWritable.status, configWritable.message, { root: options.root, ...configWritable });
  const stateRoot = process.env.MCPACE_STATE_ROOT || path.join(process.env.XDG_STATE_HOME || path.join(os.homedir(), '.local', 'state'), 'mcpace');
  const stateWritable = canWriteDirectory(path.dirname(stateRoot), options.createDirs);
  add('xdg-state-root-parent-writable', stateWritable.status, stateWritable.message, { stateRoot, ...stateWritable });
  const effectiveHost = process.env.MCPACE_SERVE_HOST || hostFromConfig(options.root) || '127.0.0.1';
  add('serve-host-local-only', hostIsLocal(effectiveHost) ? 'pass' : 'fail', `serve host=${effectiveHost}`, { allowedLocalHosts: ['127.0.0.1', 'localhost', '::1'] });
  const systemd = systemdUserStatus(options.timeoutMs);
  add('systemd-user', systemd.available ? 'pass' : 'warn', systemd.available ? 'systemd user manager is available' : systemd.reason, systemd);
  const npx = checkNpxUpstreamEnv(options.root);
  add('npx-upstream-env-vars', npx.status, npx.message, npx);
  const inline = npx.findings.filter((finding) => finding.hasInlineEnv);
  add('inline-server-env-secrets', inline.length ? 'warn' : 'pass', inline.length ? 'some upstreams use inline env; prefer env_vars names' : 'no inline env blocks detected in enabled npx upstreams', { servers: inline.map((finding) => finding.name) });
  const bin = resolveMcpaceBinary(options.bin);
  add('mcpace-binary', bin ? 'pass' : 'warn', bin ? bin : 'mcpace binary not found in --bin/PATH/target', { bin });

  const manifest = checkReleaseManifestHygiene();
  add('release-manifest-hygiene', manifest.status, manifest.message, manifest.findings);
  const archive = checkArchiveReleaseExclusions();
  add('archive-release-exclusions', archive.status, archive.message, { missing: archive.missing });
  const bits = checkVendoredExecutableBits();
  add('vendored-executable-bits', bits.status, bits.message, { findings: bits.findings });
  const pkg = readJson('package.json');
  const dockerScript = pkg?.scripts?.['test:linux-npm-install:docker'];
  add('linux-npm-install-docker-script', dockerScript?.includes('verify-linux-npm-install-docker.mjs') ? 'pass' : 'fail', dockerScript ? `test:linux-npm-install:docker => ${dockerScript}` : 'test:linux-npm-install:docker script is missing', { script: dockerScript });
  if (options.noDocker || options.profile === 'host' || options.profile === 'standard') {
    add('linux-npm-install-docker-proof', 'skip', 'Docker proof not executed for this profile', { profile: options.profile, noDocker: options.noDocker });
  } else if (!dockerAvailable().daemon) {
    add('linux-npm-install-docker-proof', options.profile === 'release' ? 'fail' : 'warn', 'Docker daemon is not available', { docker: dockerAvailable() });
  } else {
    const dockerProof = options.dryRun ? { status: 0, stdout: 'dry-run', stderr: '' } : commandOutput(npmCommand(), ['run', 'test:linux-npm-install:docker'], { timeoutMs: options.dockerTimeoutMs });
    add('linux-npm-install-docker-proof', dockerProof.status === 0 ? (options.dryRun ? 'skip' : 'pass') : 'fail', dockerProof.status === 0 ? (options.dryRun ? 'dry-run: Docker proof skipped' : 'clean Linux npm install proof passed') : 'clean Linux npm install proof failed', dockerProof);
  }
  return checks;
}

function summarizeStatus(checks, options) {
  const failCount = checks.filter((check) => check.status === 'fail').length;
  const warnCount = checks.filter((check) => check.status === 'warn').length;
  if (failCount > 0) return 'fail';
  if ((options.strict || options.profile === 'release') && warnCount > 0) return 'fail';
  if (warnCount > 0) return 'warn';
  return 'pass';
}

function markdownReport(report) {
  const lines = ['# MCPace Linux auto-check report', '', `Status: **${report.status}**`, `Profile: **${report.profile}**`, `Generated: ${report.generatedAt}`, `Root: \`${report.root}\``, '', '| Status | Check | Message |', '|---|---|---|'];
  for (const check of report.checks) lines.push(`| ${check.status} | ${check.name} | ${escapeMarkdown(check.message)} |`);
  lines.push('', '## Next actions');
  const todo = report.checks.filter((check) => check.status === 'fail' || check.status === 'warn');
  if (todo.length === 0) lines.push('- No blockers found by this checker.');
  else for (const check of todo.slice(0, 25)) lines.push(`- ${check.status.toUpperCase()} ${check.name}: ${check.message}`);
  lines.push('');
  return `${lines.join('\n')}\n`;
}

function escapeMarkdown(value) {
  return String(value).replaceAll('|', '\\|').replaceAll('\n', ' ');
}

function writeOutputFile(filePath, content) {
  const absolute = path.resolve(repoRoot, filePath);
  fs.mkdirSync(path.dirname(absolute), { recursive: true });
  fs.writeFileSync(absolute, content, 'utf8');
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) { printHelp(); return; }
  const checks = buildChecks(options);
  const report = { schema: 'mcpace.linuxAutoCheck.v1', status: summarizeStatus(checks, options), profile: options.profile, strict: options.strict, dryRun: options.dryRun, generatedAt: new Date().toISOString(), root: options.root, bin: resolveMcpaceBinary(options.bin), checks };
  if (options.write) writeOutputFile(options.write, `${JSON.stringify(report, null, 2)}\n`);
  if (options.markdown) writeOutputFile(options.markdown, markdownReport(report));
  if (options.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  else process.stdout.write(markdownReport(report));
  process.exit(report.status === 'fail' ? 1 : 0);
}

if (isCliInvocation()) {
  try { main(); } catch (error) { process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`); process.exit(1); }
}
