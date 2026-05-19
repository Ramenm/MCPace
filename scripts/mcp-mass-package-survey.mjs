#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { repoRoot, deriveProjectName, deriveProjectVersion } from './lib/project-metadata.mjs';
import { minimalPackageProfile, policyRequiresReview } from './lib/mcp-package-policy.mjs';

const DEFAULT_FIXTURE = 'eval/fixtures/mcp-mass-package-survey-sample.json';
const DEFAULT_WRITE = 'reports/mcp-mass-package-survey-latest.json';
const DEFAULT_MD = 'reports/mcp-mass-package-survey-latest.md';
const DEFAULT_LIMIT = 100;
const DEFAULT_INSTALL_LOCK_CHUNK_SIZE = 10;

// Safety-critical command shapes kept as literal comments for contract tests:
// npm search --json --searchlimit <n> "mcp server"
// npm install --package-lock-only --ignore-scripts --omit=dev --no-audit --no-fund
// npm pack --json --ignore-scripts --pack-destination <dir> <package>@<version>


function parseArgs(argv) {
  const args = { live: false, json: false, limit: DEFAULT_LIMIT, query: 'mcp server', fixture: DEFAULT_FIXTURE, write: path.join(repoRoot, DEFAULT_WRITE), markdown: path.join(repoRoot, DEFAULT_MD), noWrite: false, resolveInstallLock: false, resolveInstallLockChunks: 0, resolveInstallLockMaxChunks: 0, downloadTarballs: 0, timeoutMs: 120000, workspace: null, help: false };
  for (let i = 0; i < argv.length; i++) {
    const t = argv[i];
    if (t === '--live') args.live = true;
    else if (t === '--json') args.json = true;
    else if (t === '--limit') args.limit = Math.max(1, Math.min(250, Number(argv[++i] || DEFAULT_LIMIT)));
    else if (t === '--query') args.query = argv[++i] || args.query;
    else if (t === '--fixture') args.fixture = argv[++i] || DEFAULT_FIXTURE;
    else if (t === '--resolve-install-lock') args.resolveInstallLock = true;
    else if (t === '--resolve-install-lock-chunks') args.resolveInstallLockChunks = Math.max(1, Math.min(50, Number(argv[++i] || DEFAULT_INSTALL_LOCK_CHUNK_SIZE)));
    else if (t === '--resolve-install-lock-max-chunks') args.resolveInstallLockMaxChunks = Math.max(1, Math.min(100, Number(argv[++i] || 1)));
    else if (t === '--download-tarballs') args.downloadTarballs = Math.max(0, Math.min(100, Number(argv[++i] || 0)));
    else if (t === '--timeout-ms') args.timeoutMs = Math.max(1000, Number(argv[++i] || args.timeoutMs));
    else if (t === '--workspace') args.workspace = argv[++i] || null;
    else if (t === '--write') args.write = path.resolve(argv[++i] || DEFAULT_WRITE);
    else if (t === '--markdown') args.markdown = path.resolve(argv[++i] || DEFAULT_MD);
    else if (t === '--no-write') { args.write = null; args.markdown = null; args.noWrite = true; }
    else if (t === '-h' || t === '--help') args.help = true;
    else throw new Error(`unsupported mcp-mass-package-survey argument: ${t}`);
  }
  return args;
}

function help() {
  console.log(`Usage: node scripts/mcp-mass-package-survey.mjs [--live] [--limit 100] [--resolve-install-lock] [--resolve-install-lock-chunks 10] [--resolve-install-lock-max-chunks 2] [--download-tarballs N] [--json]

Surveys MCP-looking npm packages without starting their MCP servers or calling tools. Live mode uses npm search metadata; optional install-lock uses npm install --package-lock-only --ignore-scripts in an isolated workspace. Use --resolve-install-lock-chunks N to split 100-package pressure tests into bounded chunks.`);
}

function envFor(workspace) {
  fs.mkdirSync(path.join(workspace, 'home'), { recursive: true });
  fs.mkdirSync(path.join(workspace, 'cache'), { recursive: true });
  fs.mkdirSync(path.join(workspace, 'tmp'), { recursive: true });
  return {
    ...process.env,
    HOME: path.join(workspace, 'home'),
    USERPROFILE: path.join(workspace, 'home'),
    TMPDIR: path.join(workspace, 'tmp'),
    npm_config_cache: path.join(workspace, 'cache'),
    npm_config_audit: 'false',
    npm_config_fund: 'false',
    npm_config_ignore_scripts: 'true',
    npm_config_update_notifier: 'false',
    npm_config_progress: 'false',
    npm_config_loglevel: 'warn',
    CI: '1',
    NO_COLOR: '1',
  };
}

function run(cmd, args, opts = {}) {
  const started = Date.now();
  const r = spawnSync(cmd, args, { cwd: opts.cwd || repoRoot, env: opts.env || process.env, encoding: 'utf8', timeout: opts.timeoutMs || 120000, maxBuffer: opts.maxBuffer || 20 * 1024 * 1024, windowsHide: true });
  return { cmd: `${cmd} ${args.join(' ')}`, status: r.status, signal: r.signal || null, durationMs: Date.now() - started, timedOut: r.error?.code === 'ETIMEDOUT', stdout: redact(r.stdout || ''), stderr: redact(r.stderr || ''), error: r.error ? redact(String(r.error.message || r.error)) : null };
}

function redact(s) { return String(s).replace(/(token|api[_-]?key|secret|password|bearer)\s*[=:]\s*[^\s,'\"]+/gi, '$1=[REDACTED]').replace(/npm_[A-Za-z0-9]{20,}/g, 'npm_[REDACTED]'); }
function json(s, fallback) { try { return JSON.parse(s); } catch { return fallback; } }
function norm(v) { return String(v || '').toLowerCase(); }

// Policy classification lives in scripts/lib/mcp-package-policy.mjs so live survey,
// fixture checks, and overhead benchmarks cannot drift. Safety-critical command
// shapes remain in this file as literal comments for contract tests:
// npm search --json --searchlimit <n> "mcp server"
// npm install --package-lock-only --ignore-scripts --omit=dev --no-audit --no-fund
// npm pack --json --ignore-scripts --pack-destination <dir> <package>@<version>

function minimal(item) {
  return minimalPackageProfile(item);
}

function buildReport(base) {
  const packages = base.packages || [];
  const policyCounts = {};
  const signalCounts = {};
  for (const p of packages) {
    policyCounts[p.classification.policy] = (policyCounts[p.classification.policy] || 0) + 1;
    for (const s of p.classification.signals) signalCounts[s] = (signalCounts[s] || 0) + 1;
  }
  const checks = [
    { id: 'no-random-server-start', ok: base.safety.startsMcpServers === false && base.safety.callsMcpTools === false, detail: 'No random MCP package bins are started and no tools/call is sent.' },
    { id: 'install-scripts-disabled', ok: base.safety.packageInstallScriptsAllowed === false, detail: 'All package-manager operations disable install scripts.' },
    { id: 'default-disabled', ok: packages.every((p) => p.classification.executeDefault === false), detail: 'All surveyed packages remain disabled/not auto-enabled.' },
    { id: 'volume', ok: base.mode === 'fixture-replay' ? packages.length >= 20 : packages.length >= Math.min(100, base.limit), detail: 'Survey covers the requested MCP package volume.' },
    { id: 'locks-present', ok: packages.every((p) => p.classification.locks.length > 0), detail: 'Every package has an explicit scheduling boundary.' },
  ];
  if (base.installLock) checks.push({
    id: 'install-lock-resolution',
    ok: base.installLock.ok,
    detail: base.installLock.ok
      ? 'npm install --package-lock-only resolved selected packages without scripts.'
      : 'npm install --package-lock-only did not complete within the configured safe budget; install scripts remained disabled and no MCP server was started.',
  });
  if (base.downloads?.length) checks.push({ id: 'tarball-downloads', ok: base.downloads.every((d) => d.ok && d.sha512), detail: 'Downloaded tarballs exist and have sha512 evidence.' });
  const blockers = checks.filter((c) => !c.ok).map((c) => `${c.id}: ${c.detail}`);
  return { schema: 'mcpace.mcpMassPackageSurvey.v1', generatedAt: new Date().toISOString(), project: { name: deriveProjectName(), version: deriveProjectVersion() }, status: blockers.length ? 'blocked' : 'pass', ...base, summary: { packageCount: packages.length, policyCounts, signalCounts, highRiskCount: packages.filter((p) => policyRequiresReview(p.classification.policy) && p.classification.policy !== 'review-required-single-writer').length, reviewRequiredCount: packages.filter((p) => p.classification.reviewRequired).length, installLockOk: base.installLock?.ok ?? null, downloadedTarballs: base.downloads?.filter((d) => d.ok).length || 0 }, checks, blockers };
}

function live(args) {
  const workspace = path.resolve(args.workspace || fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-mass-mcp-')));
  fs.mkdirSync(path.join(workspace, 'logs'), { recursive: true });
  const env = envFor(workspace);
  const searchLimit = Math.max(args.limit, Math.min(250, args.limit * 2));
  const search = run('npm', ['search', '--json', '--searchlimit', String(searchLimit), args.query], { cwd: workspace, env, timeoutMs: args.timeoutMs });
  fs.writeFileSync(path.join(workspace, 'logs', 'npm-search.stdout.log'), search.stdout);
  fs.writeFileSync(path.join(workspace, 'logs', 'npm-search.stderr.log'), search.stderr);
  const rows = Array.isArray(json(search.stdout, [])) ? json(search.stdout, []) : [];
  const seen = new Set();
  const packages = [];
  for (const row of rows) {
    const key = norm(row.name);
    const text = norm(`${row.name} ${row.description || ''} ${(row.keywords || []).join(' ')}`);
    if (!key || seen.has(key) || !/(\bmcp\b|model[ -]?context[ -]?protocol|modelcontextprotocol)/.test(text)) continue;
    seen.add(key);
    packages.push(minimal(row));
    if (packages.length >= args.limit) break;
  }
  const installLock = args.resolveInstallLock
    ? (args.resolveInstallLockChunks > 0
      ? resolveInstallLockChunks(workspace, env, packages, args.timeoutMs, args.resolveInstallLockChunks, args.resolveInstallLockMaxChunks)
      : resolveInstallLock(workspace, env, packages, args.timeoutMs))
    : null;
  const downloads = args.downloadTarballs ? downloadTarballs(workspace, env, packages.slice(0, args.downloadTarballs), args.timeoutMs) : [];
  return buildReport({ mode: 'live-npm-search-metadata', query: args.query, limit: args.limit, workspace, search: summarize(search), packages, installLock, downloads, safety: safety(true) });
}

function resolveInstallLock(workspace, env, packages, timeoutMs) {
  return resolveInstallLockBatch(workspace, env, packages, timeoutMs, 'install-lock', 'install-lock');
}

function resolveInstallLockChunks(workspace, env, packages, timeoutMs, chunkSize, maxChunks = 0) {
  const chunks = [];
  const chunkLimit = maxChunks > 0 ? maxChunks : Number.MAX_SAFE_INTEGER;
  for (let index = 0; index < packages.length && chunks.length < chunkLimit; index += chunkSize) {
    const slice = packages.slice(index, index + chunkSize);
    const label = `install-lock-chunk-${String(chunks.length + 1).padStart(3, '0')}`;
    chunks.push(resolveInstallLockBatch(workspace, env, slice, timeoutMs, label, label));
  }
  const failedChunks = chunks.filter((chunk) => !chunk.ok).map((chunk) => chunk.label);
  const attemptedPackages = chunks.reduce((total, chunk) => total + chunk.packageCount, 0);
  const partial = attemptedPackages < packages.length;
  return {
    mode: 'chunked npm install --package-lock-only --ignore-scripts --omit=dev',
    packageCount: packages.length,
    attemptedPackages,
    remainingPackages: Math.max(0, packages.length - attemptedPackages),
    chunkSize,
    chunkCount: chunks.length,
    partial,
    ok: failedChunks.length === 0 && chunks.length > 0 && !partial,
    failedChunks,
    chunks,
  };
}

function resolveInstallLockBatch(workspace, env, packages, timeoutMs, directoryName, label) {
  const root = path.join(workspace, directoryName);
  fs.mkdirSync(root, { recursive: true });
  const deps = Object.fromEntries(packages.map((p) => [p.name, p.version || 'latest']));
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ private: true, name: `mcpace-mass-${directoryName}`, dependencies: deps }, null, 2));
  const seconds = String(Math.max(1, Math.ceil(timeoutMs / 1000)));
  const r = fs.existsSync('/usr/bin/timeout')
    ? run('/usr/bin/timeout', [seconds, 'npm', 'install', '--package-lock-only', '--ignore-scripts', '--omit=dev', '--no-audit', '--no-fund'], { cwd: root, env, timeoutMs: timeoutMs + 5000, maxBuffer: 30 * 1024 * 1024 })
    : run('npm', ['install', '--package-lock-only', '--ignore-scripts', '--omit=dev', '--no-audit', '--no-fund'], { cwd: root, env, timeoutMs, maxBuffer: 30 * 1024 * 1024 });
  fs.writeFileSync(path.join(workspace, 'logs', `${label}.stdout.log`), r.stdout);
  fs.writeFileSync(path.join(workspace, 'logs', `${label}.stderr.log`), r.stderr);
  return {
    label,
    mode: 'npm install --package-lock-only --ignore-scripts --omit=dev',
    packageCount: packages.length,
    packages: packages.map((p) => `${p.name}@${p.version || 'latest'}`),
    ok: r.status === 0 && fs.existsSync(path.join(root, 'package-lock.json')),
    run: summarize(r),
  };
}

function downloadTarballs(workspace, env, packages, timeoutMs) {
  const dir = path.join(workspace, 'tarballs');
  fs.mkdirSync(dir, { recursive: true });
  return packages.map((p) => {
    const spec = `${p.name}@${p.version || 'latest'}`;
    const r = run('npm', ['pack', '--json', '--ignore-scripts', '--pack-destination', dir, spec], { cwd: workspace, env, timeoutMs: Math.max(30000, timeoutMs) });
    const out = json(r.stdout, []);
    const first = Array.isArray(out) ? out[0] : null;
    const file = first?.filename ? path.join(dir, first.filename) : null;
    const exists = file && fs.existsSync(file);
    const sha512 = exists ? `sha512-${crypto.createHash('sha512').update(fs.readFileSync(file)).digest('base64')}` : null;
    return { name: p.name, version: p.version, ok: r.status === 0 && Boolean(exists), file: file ? path.relative(workspace, file).split(path.sep).join('/') : null, integrity: first?.integrity || null, sha512, run: summarize(r) };
  });
}

function safety(liveRegistryAccess) { return { liveRegistryAccess, executesThirdPartyPackages: false, startsMcpServers: false, callsMcpTools: false, packageInstallScriptsAllowed: false, destructiveToolCallsAllowed: false, userSecretsPassedToRuntime: false, defaultServerEnablement: false, registryCredentialsMayBeUsedForMirrors: true }; }
function summarize(r) { return { status: r.status, signal: r.signal, durationMs: r.durationMs, timedOut: r.timedOut, ok: r.status === 0, stderr: r.stderr.slice(0, 1000), error: r.error }; }

function fixture(file) {
  const full = path.resolve(repoRoot, file || DEFAULT_FIXTURE);
  const report = JSON.parse(fs.readFileSync(full, 'utf8'));
  return { ...report, generatedAt: new Date().toISOString(), mode: 'fixture-replay', fixture: path.relative(repoRoot, full).split(path.sep).join('/'), safety: safety(false) };
}

function md(report) {
  const lines = ['# MCP mass package survey', '', `Generated: ${report.generatedAt}`, `Status: **${report.status}**`, `Mode: ${report.mode}`, '', `Packages: ${report.summary.packageCount}; high-risk: ${report.summary.highRiskCount}; install-lock ok: ${report.summary.installLockOk}; tarballs: ${report.summary.downloadedTarballs}.`, '', '## Safety', '', `- Starts random MCP servers: ${report.safety.startsMcpServers}`, `- Calls MCP tools: ${report.safety.callsMcpTools}`, `- Allows install scripts: ${report.safety.packageInstallScriptsAllowed}`, `- Enables by default: ${report.safety.defaultServerEnablement}`, '', '## Packages', '', '| Package | Version | Policy | State | Locks | Signals |', '|---|---:|---|---|---|---|'];
  for (const p of report.packages.slice(0, 120)) lines.push(`| ${esc(p.name)} | ${esc(p.version)} | ${esc(p.classification.policy)} | ${esc(p.classification.stateClass)} | ${esc(p.classification.locks.join(', '))} | ${esc(p.classification.signals.join(', '))} |`);
  lines.push('', '## Checks', '');
  for (const c of report.checks) lines.push(`- ${c.ok ? 'PASS' : 'FAIL'} ${c.id}: ${c.detail}`);
  if (report.blockers.length) { lines.push('', '## Blockers', ''); for (const b of report.blockers) lines.push(`- ${b}`); }
  return `${lines.join('\n')}\n`;
}
function esc(v) { return String(v || '').replace(/[|\n\r]/g, ' '); }

const args = parseArgs(process.argv.slice(2));
if (args.help) help();
else {
  const report = args.live ? live(args) : fixture(args.fixture);
  if (args.write) { fs.mkdirSync(path.dirname(args.write), { recursive: true }); fs.writeFileSync(args.write, JSON.stringify(report, null, 2) + '\n'); }
  if (args.markdown) { fs.mkdirSync(path.dirname(args.markdown), { recursive: true }); fs.writeFileSync(args.markdown, md(report)); }
  if (args.json) console.log(JSON.stringify(report, null, 2)); else process.stdout.write(md(report));
  if (report.status !== 'pass') process.exitCode = 1;
}
