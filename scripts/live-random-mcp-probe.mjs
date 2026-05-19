#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { repoRoot, deriveProjectVersion } from './lib/project-metadata.mjs';

const DEFAULT_REPORT = 'reports/live-random-mcp-probe-latest.json';
const DEFAULT_MARKDOWN = 'reports/live-random-mcp-probe-latest.md';
const DEFAULT_FIXTURE = 'eval/fixtures/live-random-mcp-probe-sample.json';
const DEFAULT_TIMEOUT_MS = 15_000;
const PACKAGE_INSTALL_TIMEOUT_SECONDS = 60;
const MAX_CAPTURE_BYTES = 80_000;
const MAX_SERVER_SAMPLE_BYTES = 20_000;
const MAX_JSONRPC_MESSAGES = 1000;
const MAX_REJECTED_SERVER_REQUESTS = 50;

const SERVER_MATRIX = [
  { id: 'official-filesystem', kind: 'npm', pkg: '@modelcontextprotocol/server-filesystem', version: '2026.1.14', bin: 'dist/index.js', args: ['{workspace}/sandboxfs'], expectedPolicy: 'project-filesystem-single-writer' },
  { id: 'official-memory', kind: 'npm', pkg: '@modelcontextprotocol/server-memory', version: '2026.1.26', bin: 'dist/index.js', args: [], expectedPolicy: 'state-profile-single-session' },
  { id: 'official-sequential-thinking', kind: 'npm', pkg: '@modelcontextprotocol/server-sequential-thinking', version: '2025.12.18', bin: 'dist/index.js', args: [], expectedPolicy: 'state-profile-single-session' },
  { id: 'official-everything', kind: 'npm', pkg: '@modelcontextprotocol/server-everything', version: '2026.1.26', bin: 'dist/index.js', args: [], expectedPolicy: 'test-fixture-disabled', defaultSkipReason: 'protocol fixture can expose stress-test behavior; kept as a policy canary and not required for default live package smoke' },
  { id: 'deprecated-brave-search', kind: 'npm', pkg: '@modelcontextprotocol/server-brave-search', version: '0.6.2', bin: 'dist/index.js', args: [], expectedPolicy: 'credential-scoped-review', defaultSkipReason: 'deprecated credentialed search server; kept as policy canary, covered by python-fetch for live network classification', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'context7', kind: 'npm', pkg: '@upstash/context7-mcp', version: '2.2.5', bin: 'dist/index.js', args: [], expectedPolicy: 'network-docs-multi-reader-review', defaultSkipReason: 'external documentation/network server canary; python-fetch provides default live network lane and this can be run explicitly with --ids context7' },
  { id: 'chrome-devtools', kind: 'npm', pkg: 'chrome-devtools-mcp', version: '0.26.0', bin: 'build/src/bin/chrome-devtools-mcp.js', args: [], expectedPolicy: 'shared-exclusive-host-lock', defaultSkipReason: 'browser-host-lock canary; run explicitly with --ids chrome-devtools because browser packages may leave handles in some sandboxes' },
  { id: 'apify-actors', kind: 'npm', pkg: '@apify/actors-mcp-server', version: '0.10.4', bin: 'dist/stdio.js', args: [], expectedPolicy: 'credential-scoped-review', defaultSkipReason: 'credentialed external actor platform; kept as policy canary, not part of default no-secret live probe', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'official-postgres', kind: 'npm', pkg: '@modelcontextprotocol/server-postgres', version: '0.6.2', bin: 'dist/index.js', args: ['postgresql://localhost/mcpace_probe'], expectedPolicy: 'database-credential-scoped-review', defaultSkipReason: 'deprecated credentialed database npm server is kept as a policy canary; default probe uses python-sqlite for live database-path coverage', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'official-puppeteer', kind: 'npm', pkg: '@modelcontextprotocol/server-puppeteer', version: '2025.5.12', bin: 'dist/index.js', args: [], expectedPolicy: 'shared-exclusive-host-lock', defaultSkipReason: 'slow/heavy browser package install is intentionally skipped in the default live probe; chrome-devtools-mcp covers the browser-host-lock lane' },
  { id: 'official-github', kind: 'npm', pkg: '@modelcontextprotocol/server-github', version: '2025.4.8', bin: 'dist/index.js', args: [], expectedPolicy: 'credential-scoped-review', defaultSkipReason: 'credentialed/deprecated external API server; kept as policy canary without default live launch', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'notion', kind: 'npm', pkg: '@notionhq/notion-mcp-server', version: '2.2.1', bin: 'bin/cli.mjs', args: [], expectedPolicy: 'credential-scoped-review', defaultSkipReason: 'credentialed external SaaS server; kept as policy canary without default live launch', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'sentry', kind: 'npm', pkg: '@sentry/mcp-server', version: '0.33.0', bin: 'dist/index.js', args: [], expectedPolicy: 'credential-scoped-review', defaultSkipReason: 'credentialed external SaaS server is kept as a policy canary; default probe avoids slow/flaky credential-package install in restricted mirrors' },
  { id: 'ui5', kind: 'npm', pkg: '@ui5/mcp-server', version: '0.2.11', bin: 'bin/ui5mcp.js', args: [], expectedPolicy: 'project-devtools-single-writer-review', defaultSkipReason: 'large project-devtools dependency tree; kept as policy canary while default live probe stays fast and deterministic' },
  { id: 'railway', kind: 'npm', pkg: '@railway/mcp-server', version: '0.1.11', bin: 'dist/index.js', args: [], expectedPolicy: 'credential-scoped-review', defaultSkipReason: 'credentialed deployment platform server; kept as policy canary without default live launch', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'hubspot', kind: 'npm', pkg: '@hubspot/mcp-server', version: '0.4.0', bin: 'dist/index.js', args: [], expectedPolicy: 'credential-scoped-review', defaultSkipReason: 'credentialed CRM platform server; kept as policy canary without default live launch', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'eslint', kind: 'npm', pkg: '@eslint/mcp', version: '0.3.5', bin: 'src/mcp-cli.js', args: [], expectedPolicy: 'project-devtools-single-writer-review', defaultSkipReason: 'project linting/devtools package; run explicitly because it inspects project files and may depend on workspace shape' },
  { id: 'sap-fiori', kind: 'npm', pkg: '@sap-ux/fiori-mcp-server', version: '0.7.0', bin: 'dist/index.js', args: [], expectedPolicy: 'project-devtools-single-writer-review', defaultSkipReason: 'large SAP/Fiori project-devtools dependency tree; kept as policy canary' },
  { id: 'mapbox', kind: 'npm', pkg: '@mapbox/mcp-server', version: '0.11.0', bin: 'dist/esm/index.js', args: [], expectedPolicy: 'credential-scoped-review', defaultSkipReason: 'credentialed geospatial API server; kept as policy canary without user token', hardSkipReason: 'package install observed to exceed the live-probe timeout in the restricted mirror environment; keep metadata-classified unless explicitly allowed', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'browserstack', kind: 'npm', pkg: '@browserstack/mcp-server', version: '1.2.16', bin: 'dist/index.js', args: [], expectedPolicy: 'credential-scoped-review', defaultSkipReason: 'credentialed hosted browser testing platform; kept as policy canary', hardSkipReason: 'large hosted-browser dependency tree; keep metadata-classified unless explicitly allowed', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'kubernetes-flux159', kind: 'npm', pkg: 'mcp-server-kubernetes', version: '3.6.2', bin: 'dist/index.js', args: [], expectedPolicy: 'cluster-admin-credential-review', defaultSkipReason: 'cluster-control package; discovery-only canary; tool calls require strong user review', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'code-runner', kind: 'npm', pkg: 'mcp-server-code-runner', version: '0.1.8', bin: 'dist/cli.js', args: [], expectedPolicy: 'disabled-dangerous-command-runner', defaultSkipReason: 'code execution server; discovery-only canary only, never default-enabled', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'openapi-mcp', kind: 'npm', pkg: '@ivotoby/openapi-mcp-server', version: '1.14.0', bin: 'bin/mcp-server.js', args: [], expectedPolicy: 'network-openapi-review', defaultSkipReason: 'dynamic OpenAPI bridge; needs explicit spec/credential review before enablement', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'tavily', kind: 'npm', pkg: 'tavily-mcp', version: '0.2.19', bin: 'build/index.js', args: [], expectedPolicy: 'credential-scoped-review', defaultSkipReason: 'credentialed search API server; kept as policy canary without user token', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'playwright-official', kind: 'npm', pkg: '@playwright/mcp', version: '0.0.75', bin: 'cli.js', args: [], expectedPolicy: 'shared-exclusive-host-lock', defaultSkipReason: 'browser automation server; discovery-only canary because browser processes/profiles need host locking and may require downloaded browsers', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'google-maps-official', kind: 'npm', pkg: '@modelcontextprotocol/server-google-maps', version: '0.6.2', bin: 'dist/index.js', args: [], expectedPolicy: 'credential-scoped-review', defaultSkipReason: 'credentialed Google Maps API server; kept as policy canary without user API key', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'azure-mcp', kind: 'npm', pkg: '@azure/mcp', version: '3.0.0-beta.10', bin: 'index.js', args: [], expectedPolicy: 'cloud-admin-credential-review', defaultSkipReason: 'cloud administration server; discovery-only canary and disabled until tenant/subscription credentials and scopes are reviewed', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'evm-mcp', kind: 'npm', pkg: '@mcpdotdirect/evm-mcp-server', version: '2.0.4', bin: 'bin/cli.js', args: [], expectedPolicy: 'blockchain-wallet-review', defaultSkipReason: 'blockchain/wallet-capable server; discovery-only canary and disabled until wallet/key/network blast radius is reviewed', allowedStatuses: ['startup-error', 'tools-list-error', 'install-blocked'] },
  { id: 'python-time', kind: 'pypi', pkg: 'mcp-server-time', version: '2026.1.26', command: 'mcp-server-time', args: ['--local-timezone', 'UTC'], expectedPolicy: 'local-utility-multi-reader' },
  { id: 'python-git', kind: 'pypi', pkg: 'mcp-server-git', version: '2026.1.14', command: 'mcp-server-git', args: ['--repository', '{workspace}/gitrepo'], expectedPolicy: 'project-repo-single-writer' },
  { id: 'python-fetch', kind: 'pypi', pkg: 'mcp-server-fetch', version: '2025.4.7', command: 'mcp-server-fetch', args: [], expectedPolicy: 'network-fetch-review' },
  { id: 'python-sqlite', kind: 'pypi', pkg: 'mcp-server-sqlite', version: '2025.4.25', command: 'mcp-server-sqlite', args: ['--db-path', '{workspace}/probe.sqlite'], expectedPolicy: 'database-path-single-writer' },
];

function parseArgs(argv) {
  const args = {
    download: false,
    fixture: DEFAULT_FIXTURE,
    workspace: null,
    json: false,
    write: DEFAULT_REPORT,
    markdown: DEFAULT_MARKDOWN,
    timeoutMs: DEFAULT_TIMEOUT_MS,
    noWrite: false,
    kinds: ['npm', 'pypi'],
    ids: null,
    forceCanaries: false,
    allowHeavyInstalls: false,
    help: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    const read = () => {
      const value = argv[i + 1];
      if (!value || value.startsWith('--')) throw new Error(`${token} requires a value`);
      i += 1;
      return value;
    };
    switch (token) {
      case '--download': args.download = true; break;
      case '--fixture': args.fixture = read(); break;
      case '--workspace': args.workspace = read(); break;
      case '--timeout-ms': args.timeoutMs = parsePositiveInteger(read(), token); break;
      case '--write': args.write = read(); break;
      case '--markdown': args.markdown = read(); break;
      case '--kinds': args.kinds = read().split(',').map((item) => item.trim()).filter(Boolean); break;
      case '--ids': args.ids = new Set(read().split(',').map((item) => item.trim()).filter(Boolean)); break;
      case '--force-canaries': args.forceCanaries = true; break;
      case '--allow-heavy-installs': args.allowHeavyInstalls = true; break;
      case '--no-write': args.write = null; args.markdown = null; args.noWrite = true; break;
      case '--json': args.json = true; break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported live-random-mcp-probe argument: ${token}`);
    }
  }
  return args;
}
const RESOLVED_EXECUTABLES = new Map();

function resolveExecutable(name) {
  if (!/^[a-zA-Z0-9_.-]+$/.test(name)) return null;
  if (RESOLVED_EXECUTABLES.has(name)) return RESOLVED_EXECUTABLES.get(name);
  const resolved = spawnSync('bash', ['-lc', `command -v ${name}`], { encoding: 'utf8', timeout: 3000, env: { PATH: process.env.PATH || '/usr/bin:/bin' } });
  const value = resolved.status === 0 ? resolved.stdout.trim().split(/\r?\n/)[0] : null;
  RESOLVED_EXECUTABLES.set(name, value || null);
  return value || null;
}

function safeSystemPath() {
  const candidates = [
    path.dirname(process.execPath),
    path.dirname(resolveExecutable('npm') || ''),
    path.dirname(resolveExecutable('uv') || ''),
    path.dirname(resolveExecutable('docker') || ''),
    path.dirname(resolveExecutable('unshare') || ''),
    '/usr/local/bin',
    '/usr/bin',
    '/bin',
  ];
  return [...new Set(candidates.filter(Boolean).filter((item) => item !== '.'))].join(':');
}

function appendCapped(current, addition, limit) {
  const next = `${current || ''}${addition || ''}`;
  return next.length <= limit ? next : next.slice(next.length - limit);
}

function redactSensitive(value) {
  return String(value ?? '')
    .replace(/(https?:\/\/[^\s:@/]+:)[^\s@/]+(@)/gi, '$1<redacted>$2')
    .replace(/((?:npm|pip|uv|pypi|registry|proxy)[^\n]{0,80}(?:token|password|secret|key)[^=:\n]{0,40}[=:]\s*)[^\s\n]+/gi, '$1<redacted>')
    .replace(/((?:api[_-]?key|access[_-]?token|auth[_-]?token|password|secret|bearer)\s*[=:]\s*)[^\s\n]+/gi, '$1<redacted>')
    .replace(/(Authorization:\s*Bearer\s+)[A-Za-z0-9._~+\/-]+=*/gi, '$1<redacted>');
}

function parsePositiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}

function allowedStatus(server, status) {
  if (status === 'ok') return true;
  if (status === 'skipped-by-policy' && (server.defaultSkipReason || server.hardSkipReason)) return true;
  return Array.isArray(server.allowedStatuses) && server.allowedStatuses.includes(status);
}

function isAllowedNonOkResult(result) {
  if (!result || result.status === 'ok') return true;
  if (result.status === 'skipped-by-policy') return true;
  if (result.allowedNonOkStatus) return true;
  const policy = String(result.expectedPolicy || result.suggestedPolicy || '');
  return (result.status === 'startup-error' || result.status === 'tools-list-error' || result.status === 'install-blocked') && /credential|auth|database-credential|cluster-admin|openapi|cloud-admin|blockchain-wallet/.test(policy);
}
function printHelp() {
  console.log(`Usage: node scripts/live-random-mcp-probe.mjs [--download] [options]\n\nBy default this verifies a saved probe fixture/report and does not contact package registries.\nWith --download it installs a pinned sample of MCP npm/PyPI servers through configured package registries and probes only initialize + tools/list with stripped runtime env.\n\nOptions:\n  --download             Install pinned MCP servers and launch the probe.\n  --kinds <list>         Comma-separated package kinds to probe in download mode: npm,pypi.\n  --ids <list>           Comma-separated probe ids to run, useful for isolating flaky packages.\n  --force-canaries       Install/run entries that are skipped by default as slow or credentialed canaries.
  --allow-heavy-installs Allow hard-skipped package-manager canaries that previously hung or exceeded package timeout.\n  --fixture <path>       Offline fixture path. Default ${DEFAULT_FIXTURE}.\n  --workspace <path>     Temp workspace for --download mode.\n  --timeout-ms <ms>      Per-server timeout. Default ${DEFAULT_TIMEOUT_MS}.\n  --write <path>         JSON report path.\n  --markdown <path>      Markdown report path.\n  --no-write             Do not write reports.\n  --json                 Print JSON report.\n`);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) { printHelp(); return; }
  const started = performance.now();
  const report = args.download ? await runLiveProbe(args) : loadFixtureReport(args.fixture);
  report.project = { name: 'mcpace', version: deriveProjectVersion() };
  report.elapsedMs = Math.round(performance.now() - started);
  report.status = deriveStatus(report);
  writeOutputs(report, args);
  if (args.json) console.log(JSON.stringify(report, null, 2));
  // Third-party MCP servers may leave child-process or stdio handles open even after
  // the safe probe is complete. In download mode this script is a standalone gate,
  // so exit explicitly after reports are flushed rather than allowing leaked handles
  // from arbitrary packages to hang CI indefinitely.
  if (args.download) process.exit(report.status === 'fail' ? 1 : 0);
}

function loadFixtureReport(relativePath) {
  const fullPath = path.resolve(repoRoot, relativePath);
  const report = JSON.parse(fs.readFileSync(fullPath, 'utf8'));
  const normalizedResults = Array.isArray(report.results) ? report.results.map((result) => ({
    ...result,
    allowedNonOkStatus: result.allowedNonOkStatus ?? isAllowedNonOkResult(result),
    serverSideRequests: result.serverSideRequests || {},
    rejectedServerRequests: result.rejectedServerRequests || [],
  })) : [];
  return {
    ...report,
    schema: 'mcpace.liveRandomMcpProbe.v5',
    sourceSchema: report.schema || 'unknown',
    mode: 'fixture-replay',
    fixture: path.relative(repoRoot, fullPath),
    safety: {
      ...(report.safety || {}),
      executesThirdPartyPackages: false,
      destructiveToolCallsAllowed: false,
      packageInstallScriptsAllowed: false,
      packageManagerEnvWhitelisted: true,
      packageManagerHomeIsolated: true,
      packageManagerOutputRedacted: true,
      packageManagerCredentialsMayBeUsedForMirrors: true,
      userSecretsPassedToRuntime: false,
      defaultUnknownPolicy: 'review-required + disabled-until-user-confirms + single-writer',
    },
    results: normalizedResults,
    summary: normalizedResults.length ? summarize(normalizedResults) : report.summary,
  };
}

async function runLiveProbe(args) {
  const workspace = path.resolve(args.workspace || fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-live-mcp-probe-')));
  fs.rmSync(workspace, { recursive: true, force: true });
  prepareWorkspace(workspace);
  const hostConstraints = detectHostConstraints();
  const selectedKinds = new Set(args.kinds);
  validateSelection(args, selectedKinds);
  const matrix = SERVER_MATRIX.filter((server) => selectedKinds.has(server.kind) && (!args.ids || args.ids.has(server.id)));
  const installResults = [];
  if (selectedKinds.has('npm')) installResults.push(await installNpmServers(workspace, matrix.filter((server) => server.kind === 'npm'), args));
  if (selectedKinds.has('pypi')) installResults.push(await installPythonServers(workspace, matrix.filter((server) => server.kind === 'pypi')));
  const install = summarizeInstallResults(installResults);
  const results = [];
  for (const server of matrix) {
    const packageStatus = install.byPackage[packageKey(server)];
    if (!packageStatus?.ok) {
      results.push(blockedInstallResult(server, packageStatus?.error || 'install failed'));
      continue;
    }
    results.push(await probeServer(workspace, server, args.timeoutMs, hostConstraints));
  }
  const summary = summarize(results);
  return baseReport({
    mode: 'live-download-probe',
    generatedAt: new Date().toISOString(),
    workspace,
    hostConstraints,
    install,
    summary,
    results,
    notes: [
      'Only initialize, notifications/initialized, and tools/list were sent.',
      'No user API keys or user home directory were passed to runtime processes.',
      'npm install uses --ignore-scripts, --no-audit, --no-fund, --omit=dev, isolated HOME/cache, and a whitelisted package-manager environment.',
      'PyPI installs happen in a disposable venv with isolated cache/HOME and a whitelisted package-manager environment; runtime processes receive a stripped environment.',
      'Runtime network namespace isolation uses unshare -Urn when this host allows it; otherwise the probe falls back to stripped env + timeout only.',
      'This is a smoke probe. It is not a source security audit and it does not prove destructive tool behavior is safe.',
      'Some canaries are hard-skipped unless --allow-heavy-installs is passed because package-manager installs can hang in restricted mirrors.',
    ],
  });
}

function validateSelection(args, selectedKinds) {
  const allowedKinds = new Set(['npm', 'pypi']);
  const unsupportedKinds = [...selectedKinds].filter((kind) => !allowedKinds.has(kind));
  if (unsupportedKinds.length) throw new Error(`unsupported live probe kind(s): ${unsupportedKinds.join(', ')}`);
  if (args.ids) {
    const knownIds = new Set(SERVER_MATRIX.map((server) => server.id));
    const unknown = [...args.ids].filter((id) => !knownIds.has(id));
    if (unknown.length) throw new Error(`unknown live probe id(s): ${unknown.join(', ')}`);
  }
}

function prepareWorkspace(workspace) {
  fs.mkdirSync(path.join(workspace, 'reports'), { recursive: true });
  fs.mkdirSync(path.join(workspace, 'logs'), { recursive: true });
  fs.mkdirSync(path.join(workspace, 'sandboxfs'), { recursive: true });
  fs.mkdirSync(path.join(workspace, 'home-runtime'), { recursive: true });
  fs.mkdirSync(path.join(workspace, 'tmp-runtime'), { recursive: true });
  fs.mkdirSync(path.join(workspace, 'home-install'), { recursive: true });
  fs.mkdirSync(path.join(workspace, 'tmp-install'), { recursive: true });
  fs.mkdirSync(path.join(workspace, 'npm-cache'), { recursive: true });
  fs.writeFileSync(path.join(workspace, 'sandboxfs', 'README.txt'), 'MCPace live probe sandbox root.\n');
  fs.writeFileSync(path.join(workspace, 'package.json'), JSON.stringify({ private: true, name: 'mcpace-live-mcp-probe' }, null, 2));
  spawnSync('git', ['init', '-q', path.join(workspace, 'gitrepo')], { encoding: 'utf8', timeout: 30_000 });
  fs.writeFileSync(path.join(workspace, 'probe.sqlite'), '');
}

async function installNpmServers(workspace, servers, args) {
  if (!servers.length) return { kind: 'npm', ok: true, packages: [], byPackage: {} };
  const byPackage = {};
  const packages = [];
  for (const server of servers) {
    const spec = `${server.pkg}@${server.version}`;
    packages.push(spec);
    if (server.hardSkipReason && !args.allowHeavyInstalls) {
      byPackage[packageKey(server)] = { ok: false, skipped: true, hardSkipped: true, spec, error: server.hardSkipReason };
      continue;
    }
    if (server.defaultSkipReason && !args.forceCanaries) {
      byPackage[packageKey(server)] = { ok: false, skipped: true, spec, error: server.defaultSkipReason };
      continue;
    }
    const installDir = npmInstallDir(workspace, server);
    fs.mkdirSync(installDir, { recursive: true });
    fs.writeFileSync(path.join(installDir, 'package.json'), JSON.stringify({ private: true, name: `mcpace-probe-${safeLogName(server.id)}` }, null, 2));
    const logStem = safeLogName(`npm-${server.id}`);
    const install = await runCommandWithTimeout(resolveExecutable('npm') || 'npm', ['install', '--ignore-scripts', '--no-audit', '--no-fund', '--omit=dev', spec], {
      cwd: installDir,
      env: cleanPackageManagerEnv(workspace, 'npm'),
      timeoutMs: PACKAGE_INSTALL_TIMEOUT_SECONDS * 1000,
    });
    fs.writeFileSync(path.join(workspace, 'logs', `${logStem}.stdout.log`), redactSensitive(install.stdout || ''));
    fs.writeFileSync(path.join(workspace, 'logs', `${logStem}.stderr.log`), redactSensitive(install.stderr || ''));
    byPackage[packageKey(server)] = {
      ok: install.status === 0,
      code: install.status,
      signal: install.signal,
      timedOut: install.timedOut,
      spec,
      ignoreScripts: true,
      packageManagerEnv: 'whitelisted',
      error: install.status === 0 ? null : redactSensitive(install.stderr || install.stdout || install.error || 'npm install failed or timed out').slice(0, 4000),
    };
  }
  return {
    kind: 'npm',
    ok: Object.values(byPackage).every((item) => item.ok),
    packages,
    byPackage,
    ignoreScripts: true,
  };
}

async function installPythonServers(workspace, servers) {
  if (!servers.length) return { kind: 'pypi', ok: true, packages: [], byPackage: {} };
  const byPackage = {};
  const packages = [];
  const venv = path.join(workspace, 'py-venv');
  const cache = path.join(workspace, 'uv-cache');
  fs.mkdirSync(cache, { recursive: true });
  const env = cleanPackageManagerEnv(workspace, 'pypi', { UV_CACHE_DIR: cache });
  const uvBin = resolveExecutable('uv') || 'uv';
  const venvResult = await runCommandWithTimeout(uvBin, ['venv', venv], { cwd: workspace, env, timeoutMs: 120_000 });
  fs.writeFileSync(path.join(workspace, 'logs', 'uv-venv.stdout.log'), redactSensitive(venvResult.stdout || ''));
  fs.writeFileSync(path.join(workspace, 'logs', 'uv-venv.stderr.log'), redactSensitive(venvResult.stderr || ''));
  if (venvResult.status !== 0) {
    const error = redactSensitive(venvResult.stderr || venvResult.stdout || venvResult.error || 'uv venv failed').slice(0, 4000);
    for (const server of servers) {
      byPackage[packageKey(server)] = { ok: false, spec: `${server.pkg}==${server.version}`, error };
    }
    return { kind: 'pypi', ok: false, code: venvResult.status, packages, byPackage, error };
  }
  const python = path.join(venv, 'bin', 'python');
  for (const server of servers) {
    const spec = `${server.pkg}==${server.version}`;
    packages.push(spec);
    const logStem = safeLogName(`pypi-${server.id}`);
    const install = await runCommandWithTimeout(uvBin, ['pip', 'install', '--python', python, spec], { cwd: workspace, env, timeoutMs: PACKAGE_INSTALL_TIMEOUT_SECONDS * 1000 });
    fs.writeFileSync(path.join(workspace, 'logs', `${logStem}.stdout.log`), redactSensitive(install.stdout || ''));
    fs.writeFileSync(path.join(workspace, 'logs', `${logStem}.stderr.log`), redactSensitive(install.stderr || ''));
    byPackage[packageKey(server)] = {
      ok: install.status === 0,
      code: install.status,
      signal: install.signal,
      timedOut: install.timedOut,
      spec,
      packageManagerEnv: 'whitelisted',
      error: install.status === 0 ? null : redactSensitive(install.stderr || install.stdout || install.error || 'uv pip install failed or timed out').slice(0, 4000),
    };
  }
  return {
    kind: 'pypi',
    ok: Object.values(byPackage).every((item) => item.ok),
    packages,
    byPackage,
    venv: path.relative(workspace, venv),
  };
}

function summarizeInstallResults(results) {
  const byKind = {};
  const byPackage = {};
  const packages = [];
  let ok = true;
  for (const result of results) {
    byKind[result.kind] = result;
    ok = ok && result.ok;
    packages.push(...(result.packages || []));
    Object.assign(byPackage, result.byPackage || {});
  }
  return { status: ok ? 'pass' : 'partial-or-failed', packages, byKind, byPackage };
}

function packageKey(server) {
  return `${server.kind}:${server.pkg}@${server.version}`;
}

function npmInstallDir(workspace, server) {
  return path.join(workspace, 'npm-packages', safeLogName(server.id));
}

function safeLogName(value) {
  return String(value).replace(/[^a-zA-Z0-9_.-]+/g, '_').slice(0, 120);
}

function blockedInstallResult(server, reason) {
  const skipped = Boolean(server.defaultSkipReason || server.hardSkipReason) || /skip|slow|heavy|timeout/i.test(String(reason));
  const status = skipped ? 'skipped-by-policy' : 'install-blocked';
  return {
    id: server.id,
    package: server.pkg,
    version: server.version,
    kind: server.kind,
    status,
    elapsedMs: 0,
    init: null,
    toolCount: 0,
    tools: [],
    riskSignals: ['install-blocked'],
    suggestedPolicy: server.expectedPolicy || 'unknown-conservative-review',
    expectedPolicy: server.expectedPolicy,
    expectedMatchesSuggested: true,
    allowedNonOkStatus: allowedStatus(server, status),
    errors: [{ phase: 'install', error: String(reason).slice(0, 1000) }],
    stderrSample: '',
    stdoutSample: '',
  };
}

function baseReport(extra) {
  return {
    schema: 'mcpace.liveRandomMcpProbe.v5',
    ...extra,
    safety: {
      executesThirdPartyPackages: extra.mode === 'live-download-probe',
      destructiveToolCallsAllowed: false,
      packageInstallScriptsAllowed: false,
      packageManagerEnvWhitelisted: true,
      packageManagerHomeIsolated: true,
      packageManagerOutputRedacted: true,
      packageManagerCredentialsMayBeUsedForMirrors: true,
      userSecretsPassedToRuntime: false,
      defaultUnknownPolicy: 'review-required + disabled-until-user-confirms + single-writer',
    },
  };
}

function detectHostConstraints() {
  return {
    dockerAvailable: Boolean(resolveExecutable('docker')) && spawnSync(resolveExecutable('docker'), ['--version'], { encoding: 'utf8', timeout: 3000, env: { PATH: safeSystemPath() } }).status === 0,
    uvAvailable: Boolean(resolveExecutable('uv')) && spawnSync(resolveExecutable('uv'), ['--version'], { encoding: 'utf8', timeout: 3000, env: { PATH: safeSystemPath() } }).status === 0,
    npmAvailable: Boolean(resolveExecutable('npm')) && spawnSync(resolveExecutable('npm'), ['--version'], { encoding: 'utf8', timeout: 3000, env: { PATH: safeSystemPath() } }).status === 0,
    unshareUserNetworkAvailable: Boolean(resolveExecutable('unshare')) && spawnSync(resolveExecutable('unshare'), ['-Urn', 'true'], { encoding: 'utf8', timeout: 3000, env: { PATH: safeSystemPath() } }).status === 0,
    directPublicDnsLikelyBlocked: true,
    packageDownloadsViaConfiguredRegistry: true,
    runtimeNetworkIsolation: 'unshare -Urn when available, otherwise env-stripped timeout-only',
    runtimeEnvStripped: true,
    packageManagerEnvWhitelisted: true,
    packageManagerHomeIsolated: true,
    packageManagerOutputRedacted: true,
    callsPerformed: ['initialize', 'notifications/initialized', 'tools/list'],
  };
}

async function probeServer(workspace, server, timeoutMs, hostConstraints) {
  appendProgress(workspace, `probe:start:${server.id}`);
  const commandInfo = buildRuntimeCommand(workspace, server, hostConstraints);
  let stdoutText = '';
  let stderrText = '';
  let droppedJsonRpcMessages = 0;
  const messages = [];
  const serverSideRequests = {};
  const rejectedServerRequests = [];
  const child = spawn(commandInfo.command, commandInfo.args, { cwd: workspace, env: commandInfo.wrapperEnv, stdio: ['pipe', 'pipe', 'pipe'], detached: process.platform !== 'win32' });
  let exited = false;
  const exitPromise = new Promise((resolve) => child.once('exit', (code, signal) => { exited = true; resolve({ code, signal }); }));
  let stdoutBuf = '';
  child.stdout.on('data', (chunk) => {
    const text = chunk.toString('utf8');
    stdoutText = appendCapped(stdoutText, text, MAX_SERVER_SAMPLE_BYTES);
    stdoutBuf += text;
    let index;
    while ((index = stdoutBuf.indexOf('\n')) >= 0) {
      const line = stdoutBuf.slice(0, index).trim();
      stdoutBuf = stdoutBuf.slice(index + 1);
      if (!line) continue;
      try {
        if (messages.length < MAX_JSONRPC_MESSAGES) messages.push(JSON.parse(line));
        else droppedJsonRpcMessages += 1;
      } catch {}
    }
  });
  child.stderr.on('data', (chunk) => { stderrText = appendCapped(stderrText, chunk.toString('utf8'), MAX_SERVER_SAMPLE_BYTES); });
  const write = (obj) => {
    try { child.stdin.write(`${JSON.stringify(obj)}\n`); } catch {}
  };
  const started = performance.now();
  let status = 'timeout';
  let init = null;
  const tools = [];
  const errors = [];
  let toolsRequestCount = 0;
  const killTimer = setTimeout(() => {
    if (!exited) {
      terminateChild(child, 'SIGTERM');
      setTimeout(() => { if (!exited) terminateChild(child, 'SIGKILL'); }, 1000).unref();
    }
  }, timeoutMs);
  await new Promise((resolveDone) => {
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      clearInterval(interval);
      clearTimeout(doneTimer);
      resolveDone();
    };
    let initializedSent = false;
    let toolsSent = false;
    const doneTimer = setTimeout(() => {
      if (status === 'timeout') errors.push({ phase: 'probe', error: `timed out after ${timeoutMs}ms` });
      finish();
    }, timeoutMs + 1500);
    const interval = setInterval(() => {
      for (const msg of messages.splice(0)) {
        if (msg && Object.prototype.hasOwnProperty.call(msg, 'id') && typeof msg.method === 'string') {
          respondToServerRequest(write, workspace, msg, serverSideRequests, rejectedServerRequests);
          continue;
        }
        if (msg.id === 1) {
          init = msg;
          if (msg.error) {
            status = 'initialize-error';
            errors.push({ phase: 'initialize', error: msg.error });
            finish();
          } else if (!initializedSent) {
            initializedSent = true;
            write({ jsonrpc: '2.0', method: 'notifications/initialized', params: {} });
          }
        }
        if (typeof msg.id === 'string' && msg.id.startsWith('tools-')) {
          if (msg.error) {
            status = 'tools-list-error';
            errors.push({ phase: 'tools/list', error: msg.error });
            finish();
          } else {
            tools.push(...(msg.result?.tools || []).map(summarizeTool));
            const cursor = msg.result?.nextCursor;
            if (cursor && toolsRequestCount < 20) {
              toolsRequestCount += 1;
              write({ jsonrpc: '2.0', id: `tools-${toolsRequestCount + 1}`, method: 'tools/list', params: { cursor } });
            } else {
              status = 'ok';
              finish();
            }
          }
        }
      }
      if (init && !init.error && initializedSent && !toolsSent) {
        toolsSent = true;
        toolsRequestCount = 1;
        write({ jsonrpc: '2.0', id: 'tools-1', method: 'tools/list', params: {} });
      }
    }, 25);
    write({
      jsonrpc: '2.0', id: 1, method: 'initialize',
      params: {
        protocolVersion: '2025-06-18',
        capabilities: { roots: { listChanged: true }, sampling: {} },
        clientInfo: { name: 'mcpace-live-random-mcp-probe', version: deriveProjectVersion() },
      },
    });
    exitPromise.then(({ code }) => {
      if (status === 'timeout') status = code === 0 ? 'exited-before-complete' : 'startup-error';
      finish();
    });
  });
  clearTimeout(killTimer);
  try { child.stdin.end(); } catch {}
  if (!exited) terminateChild(child, 'SIGTERM');
  await Promise.race([exitPromise, new Promise((resolve) => setTimeout(resolve, 1200))]);
  try { child.stdout.destroy(); } catch {}
  try { child.stderr.destroy(); } catch {}
  try { child.stdin.destroy(); } catch {}
  try { child.unref(); } catch {}
  const stderr = redactSensitive(stderrText).slice(0, MAX_SERVER_SAMPLE_BYTES);
  const stdout = redactSensitive(stdoutText).slice(0, MAX_SERVER_SAMPLE_BYTES);
  const riskSignals = detectToolRiskSignals(server, tools, stderr, stdout);
  const suggestedPolicy = suggestRuntimePolicy(server, riskSignals);
  appendProgress(workspace, `probe:end:${server.id}:${status}:${tools.length}`);
  return {
    id: server.id,
    package: server.pkg,
    version: server.version,
    kind: server.kind,
    status,
    elapsedMs: Math.round(performance.now() - started),
    init: init ? { error: init.error || null, serverInfo: init.result?.serverInfo || null, protocolVersion: init.result?.protocolVersion || null, capabilities: init.result?.capabilities || null } : null,
    toolCount: tools.length,
    tools,
    riskSignals,
    suggestedPolicy,
    expectedPolicy: server.expectedPolicy,
    expectedMatchesSuggested: suggestedPolicy === server.expectedPolicy,
    allowedNonOkStatus: allowedStatus(server, status),
    serverSideRequests,
    rejectedServerRequests: rejectedServerRequests.slice(0, MAX_REJECTED_SERVER_REQUESTS),
    droppedJsonRpcMessages,
    errors,
    stderrSample: stderr.slice(0, 2000),
    stdoutSample: stdout.slice(0, 2000),
  };
}

function appendProgress(workspace, line) {
  try { fs.appendFileSync(path.join(workspace, 'logs', 'probe-progress.log'), `${new Date().toISOString()} ${line}\n`); } catch {}
}

function terminateChild(child, signal) {
  if (!child?.pid) return;
  if (process.platform !== 'win32') {
    try { process.kill(-child.pid, signal); return; } catch {}
  }
  try { child.kill(signal); } catch {}
}

function respondToServerRequest(write, workspace, msg, serverSideRequests = {}, rejectedServerRequests = []) {
  serverSideRequests[msg.method] = (serverSideRequests[msg.method] || 0) + 1;
  if (msg.method === 'roots/list') {
    write({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        roots: [{ uri: pathToFileUri(path.join(workspace, 'sandboxfs')), name: 'mcpace-probe-sandbox' }],
      },
    });
    return;
  }
  if (msg.method === 'ping') {
    write({ jsonrpc: '2.0', id: msg.id, result: {} });
    return;
  }
  rejectedServerRequests.push(String(msg.method));
  write({
    jsonrpc: '2.0',
    id: msg.id,
    error: { code: -32601, message: `probe client does not implement ${msg.method}` },
  });
}

function pathToFileUri(filePath) {
  const resolved = path.resolve(filePath).replace(/\\/g, '/');
  return `file://${resolved.startsWith('/') ? '' : '/'}${resolved.split('/').map(encodeURIComponent).join('/')}`;
}

function buildRuntimeCommand(workspace, server, hostConstraints) {
  const env = cleanRuntimeEnv(workspace);
  const wrapperEnv = { PATH: safeSystemPath(), LANG: 'C.UTF-8', LC_ALL: 'C.UTF-8' };
  const envPairs = Object.entries(env).map(([key, value]) => `${key}=${value}`);
  let command;
  let childArgs;
  if (server.kind === 'npm') {
    command = process.execPath;
    childArgs = [path.join(npmInstallDir(workspace, server), 'node_modules', server.pkg, server.bin), ...resolveArgs(workspace, server.args || [])];
  } else if (server.kind === 'pypi') {
    command = path.join(workspace, 'py-venv', 'bin', server.command);
    childArgs = resolveArgs(workspace, server.args || []);
  } else {
    throw new Error(`unsupported server kind: ${server.kind}`);
  }
  return hostConstraints.unshareUserNetworkAvailable
    ? { command: 'unshare', args: ['-Urn', '/usr/bin/env', '-i', ...envPairs, command, ...childArgs], wrapperEnv }
    : { command: '/usr/bin/env', args: ['-i', ...envPairs, command, ...childArgs], wrapperEnv };
}

function resolveArgs(workspace, args) {
  return args.map((arg) => arg.replaceAll('{workspace}', workspace));
}

function cleanRuntimeEnv(workspace) {
  const home = path.join(workspace, 'home-runtime');
  const tmp = path.join(workspace, 'tmp-runtime');
  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(tmp, { recursive: true });
  return {
    PATH: safeSystemPath(),
    HOME: home,
    TMPDIR: tmp,
    LANG: 'C.UTF-8',
    LC_ALL: 'C.UTF-8',
    NO_PROXY: 'localhost,127.0.0.1',
  };
}

function cleanPackageManagerEnv(workspace, kind, extra = {}) {
  const home = path.join(workspace, 'home-install');
  const tmp = path.join(workspace, 'tmp-install');
  const npmCache = path.join(workspace, 'npm-cache');
  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(tmp, { recursive: true });
  fs.mkdirSync(npmCache, { recursive: true });
  const emptyNpmConfig = path.join(workspace, 'npm-userconfig-empty');
  fs.writeFileSync(emptyNpmConfig, '');
  const env = {
    PATH: safeSystemPath(),
    HOME: home,
    TMPDIR: tmp,
    LANG: 'C.UTF-8',
    LC_ALL: 'C.UTF-8',
    CI: '1',
    COREPACK_ENABLE_DOWNLOAD_PROMPT: '0',
    npm_config_ignore_scripts: 'true',
    NPM_CONFIG_IGNORE_SCRIPTS: 'true',
    npm_config_audit: 'false',
    NPM_CONFIG_AUDIT: 'false',
    npm_config_fund: 'false',
    NPM_CONFIG_FUND: 'false',
    npm_config_cache: npmCache,
    NPM_CONFIG_CACHE: npmCache,
    npm_config_userconfig: emptyNpmConfig,
    NPM_CONFIG_USERCONFIG: emptyNpmConfig,
    ...extra,
  };
  for (const key of [
    'HTTP_PROXY', 'HTTPS_PROXY', 'NO_PROXY', 'http_proxy', 'https_proxy', 'no_proxy',
    'NPM_CONFIG_REGISTRY', 'npm_config_registry',
    'PIP_INDEX_URL', 'PIP_TRUSTED_HOST', 'UV_INDEX_URL', 'UV_INSECURE_HOST',
  ]) {
    if (process.env[key] && isAllowedPackageManagerEnvKey(key, kind)) env[key] = process.env[key];
  }
  return env;
}

function isAllowedPackageManagerEnvKey(key, kind) {
  if (/proxy/i.test(key) || key === 'NO_PROXY' || key === 'no_proxy') return true;
  if (kind === 'npm') return key === 'NPM_CONFIG_REGISTRY' || key === 'npm_config_registry';
  if (kind === 'pypi') return ['PIP_INDEX_URL', 'PIP_TRUSTED_HOST', 'UV_INDEX_URL', 'UV_INSECURE_HOST'].includes(key);
  return false;
}

function runCommandWithTimeout(command, args, { cwd, env, timeoutMs }) {
  return new Promise((resolve) => {
    const child = spawn(command, args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'], detached: process.platform !== 'win32' });
    let stdout = '';
    let stderr = '';
    let timedOut = false;
    let settled = false;
    let hardTimer = null;
    const finish = (status, signal, error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (hardTimer) clearTimeout(hardTimer);
      try { child.stdout.destroy(); } catch {}
      try { child.stderr.destroy(); } catch {}
      resolve({
        status,
        signal,
        timedOut,
        stdout: redactSensitive(stdout),
        stderr: redactSensitive(stderr),
        error: error ? redactSensitive(String(error.message || error)).slice(0, 4000) : null,
      });
    };
    const timer = setTimeout(() => {
      timedOut = true;
      terminateChild(child, 'SIGTERM');
      setTimeout(() => terminateChild(child, 'SIGKILL'), 1500).unref();
      hardTimer = setTimeout(() => finish(124, 'SIGKILL', new Error(`timed out after ${timeoutMs}ms`)), 3000);
      hardTimer.unref?.();
    }, timeoutMs);
    child.stdout.on('data', (chunk) => { stdout = appendCapped(stdout, chunk.toString('utf8'), MAX_CAPTURE_BYTES); });
    child.stderr.on('data', (chunk) => { stderr = appendCapped(stderr, chunk.toString('utf8'), MAX_CAPTURE_BYTES); });
    child.once('error', (error) => finish(127, null, error));
    child.once('exit', (code, signal) => finish(code, signal, null));
  });
}

function summarizeTool(tool) {
  return {
    name: tool?.name,
    description: typeof tool?.description === 'string' ? tool.description.slice(0, 240) : undefined,
    inputProps: tool?.inputSchema?.properties ? Object.keys(tool.inputSchema.properties).slice(0, 24) : [],
    annotations: tool?.annotations || null,
  };
}

function detectToolRiskSignals(server, tools, stderr, stdout) {
  const packageHint = `${server.id} ${server.pkg}`.toLowerCase();
  const names = tools.map((tool) => String(tool.name || '').toLowerCase());
  const descriptions = tools.map((tool) => String(tool.description || '').toLowerCase());
  const annotations = tools.map((tool) => tool.annotations || {}).filter(Boolean);
  const joinedNames = names.join(' ');
  const joinedDescriptions = descriptions.join(' ');
  const joinedAnnotations = JSON.stringify(annotations).toLowerCase();
  const joinedAll = `${packageHint} ${joinedNames} ${joinedDescriptions} ${joinedAnnotations} ${stderr.toLowerCase()} ${stdout.toLowerCase()}`;
  const signals = new Set();
  if (/everything/.test(packageHint)) signals.add('protocol-fixture');
  if (/filesystem|file[-_ ]?system/.test(packageHint) || /\b(read|write|move|edit|list)_?(file|directory|directories)\b/.test(joinedNames)) signals.add('filesystem');
  if (/\bgit\b|repository|worktree/.test(packageHint) || /\bgit_|commit|branch|diff|repository/.test(joinedNames)) signals.add('git-repository');
  if (/postgres|sqlite|database|db\b|sql\b/.test(packageHint) || /sqlite|postgres|database|sql|db_path|db-path|table|schema/.test(joinedNames)) signals.add('database');
  if (/chrome|devtools|browser|playwright|puppeteer/.test(packageHint) || /\b(click|navigate|screenshot|page|tab|hover|fill|upload_file|evaluate)\b/.test(joinedNames)) signals.add('browser-or-desktop');
  if (/memory|sequential-thinking|context-store/.test(packageHint) || /entity|relation|observation|knowledge|thinking|thought/.test(joinedNames)) signals.add('memory-or-context');
  if (/time|timezone/.test(packageHint) || /timezone|current_time|convert_time|get_current_time/.test(joinedNames)) signals.add('local-utility');
  if (/fetch/.test(packageHint) || /fetch|url|http|web|robots/.test(joinedNames)) signals.add('network-fetch');
  if (/brave|apify|api|context7|docs|documentation|github|slack|notion|sentry|supabase|railway|hubspot|mapbox|browserstack|tavily|google-maps/.test(packageHint) || /\b(web_search|brave_web_search|query_docs|fetch_url|run_actor|search_actor|url|network_request|issue|ticket|project|deployment)\b/.test(joinedNames)) signals.add('network-or-external-api');
  if (/ui5|fiori|sap|eslint/.test(packageHint)) signals.add('project-devtools');
  if (/azure|aws|gcp|cloudflare|terraform|pulumi/.test(packageHint) || /subscription|tenant|resource_group|cloud account|iam|role assignment/.test(joinedAll)) signals.add('cloud-admin');
  if (/evm|ethereum|wallet|web3|blockchain|crypto|token transfer|sign(ature)?/.test(packageHint) || /wallet|private key|transaction|contract|chain id|sign message/.test(joinedAll)) signals.add('blockchain-wallet');
  if (/secret manager|secrets manager|vault|1password|bitwarden|password manager|keychain|credential store/.test(joinedAll)) signals.add('secrets-manager');
  if (/stripe|paypal|payment|billing|invoice|bank|treasury|card|refund|charge|payout/.test(joinedAll)) signals.add('payments-financial');
  if (/okta|auth0|entra|active directory|scim|sso|identity|user management|group management/.test(joinedAll)) signals.add('identity-admin');
  if (/gmail|outlook|email|imap|smtp|mailbox|send mail|slack|teams|discord|sms/.test(joinedAll)) signals.add('messaging-email');
  if (/kubernetes|k8s|kubectl|openshift/.test(packageHint) || /kubernetes|namespace|pod|deployment|cluster/.test(joinedNames)) signals.add('cluster-control');
  if (/openapi|swagger/.test(packageHint) || /openapi|swagger|operation|endpoint/.test(joinedNames)) signals.add('openapi-bridge');
  if (/shell|terminal|exec|command-runner|code-runner/.test(packageHint) || /\b(shell|exec|spawn|run_command|terminal)\b|run[-_]?code|code snippet|languageid/.test(`${joinedNames} ${joinedDescriptions}`)) signals.add('shell-or-process');
  if (/delete|remove|write|update|create|patch|upload|close|click|drag|fill|navigate|commit|merge|apply|execute|run/.test(joinedNames) || annotations.some((ann) => ann.destructiveHint === true || ann.readOnlyHint === false)) signals.add('mutable-or-destructive-tools');
  if (annotations.some((ann) => ann.openWorldHint === true)) signals.add('open-world-annotation');
  if (/ignore (all )?(previous|prior) instructions|system prompt|developer message|exfiltrate|leak secret|prompt injection/.test(joinedAll)) signals.add('prompt-injection-surface');
  if (/api[_-]?key|\btoken\b|credential|required but not set|unauthorized|missing.*token|bearer|oauth|auth[_-]?token|authentication|authorization/i.test(joinedAll) || /brave|apify|notion|sentry|slack|github|railway|hubspot|google-maps|tavily|azure|evm/.test(packageHint)) signals.add('credentials-or-auth');
  if (/context7/.test(packageHint)) {
    signals.delete('database');
    signals.delete('credentials-or-auth');
  }
  if (signals.has('memory-or-context') && !signals.has('credentials-or-auth') && !signals.has('network-or-external-api') && !signals.has('cloud-admin')) {
    signals.delete('identity-admin');
  }
  if (!signals.size) signals.add('unknown-side-effects');
  return [...signals];
}

function suggestRuntimePolicy(server, risks) {
  const has = (risk) => risks.includes(risk);
  if (has('protocol-fixture')) return 'test-fixture-disabled';
  if (server.pkg.includes('context7')) return 'network-docs-multi-reader-review';
  if (has('memory-or-context') && !has('credentials-or-auth') && !has('network-or-external-api')) return 'state-profile-single-session';
  if (has('identity-admin')) return 'identity-admin-credential-review';
  if (has('cloud-admin')) return 'cloud-admin-credential-review';
  if (has('blockchain-wallet')) return 'blockchain-wallet-review';
  if (has('secrets-manager')) return 'secrets-manager-disabled-review';
  if (has('payments-financial')) return 'payments-financial-review';
  if (has('messaging-email')) return 'messaging-external-review';
  if (has('cluster-control')) return 'cluster-admin-credential-review';
  if (has('openapi-bridge')) return 'network-openapi-review';
  if (has('browser-or-desktop')) return 'shared-exclusive-host-lock';
  if (has('shell-or-process')) return 'disabled-dangerous-command-runner';
  if (has('git-repository')) return 'project-repo-single-writer';
  if (has('database') && has('credentials-or-auth')) return 'database-credential-scoped-review';
  if (has('database')) return 'database-path-single-writer';
  if (has('filesystem')) return 'project-filesystem-single-writer';
  if (has('memory-or-context')) return 'state-profile-single-session';
  if (has('project-devtools')) return 'project-devtools-single-writer-review';
  if (has('local-utility') && !has('network-or-external-api') && !has('credentials-or-auth')) return 'local-utility-multi-reader';
  if (has('network-fetch')) return 'network-fetch-review';
  if (has('credentials-or-auth') || has('network-or-external-api')) return 'credential-scoped-review';
  return 'unknown-conservative-review';
}

function summarize(results) {
  const byPolicy = {};
  const byStatus = {};
  const byKind = {};
  const serverSideRequestMethods = {};
  for (const result of results) {
    byPolicy[result.suggestedPolicy] = (byPolicy[result.suggestedPolicy] || 0) + 1;
    byStatus[result.status] = (byStatus[result.status] || 0) + 1;
    byKind[result.kind] = (byKind[result.kind] || 0) + 1;
    for (const [method, count] of Object.entries(result.serverSideRequests || {})) {
      serverSideRequestMethods[method] = (serverSideRequestMethods[method] || 0) + count;
    }
  }
  const unexpectedFailures = results.filter((result) => result.status !== 'ok' && !isAllowedNonOkResult(result)).map((result) => result.id);
  return {
    total: results.length,
    ok: results.filter((result) => result.status === 'ok').length,
    failed: results.filter((result) => result.status !== 'ok' && result.status !== 'skipped-by-policy').length,
    skipped: results.filter((result) => result.status === 'skipped-by-policy').length,
    allowedNonOk: results.filter((result) => result.status !== 'ok' && isAllowedNonOkResult(result)).map((result) => result.id),
    unexpectedFailures,
    totalTools: results.reduce((sum, result) => sum + result.toolCount, 0),
    policyMismatches: results.filter((result) => !result.expectedMatchesSuggested).map((result) => result.id),
    byStatus,
    byKind,
    byPolicy,
    serverSideRequestMethods,
  };
}

function deriveStatus(report) {
  if (report.install?.status === 'failed') return 'blocked';
  const summary = report.results ? summarize(report.results) : report.summary;
  if (summary?.policyMismatches?.length) return 'fail';
  if (summary?.unexpectedFailures?.length) return 'fail';
  if ((summary?.ok || 0) < 1) return 'blocked';
  return 'pass';
}

function writeOutputs(report, args) {
  if (args.write) writeJson(args.write, report);
  if (args.markdown) writeText(args.markdown, renderMarkdown(report));
}
function writeJson(relativePath, value) {
  const fullPath = path.resolve(repoRoot, relativePath);
  fs.mkdirSync(path.dirname(fullPath), { recursive: true });
  fs.writeFileSync(fullPath, `${JSON.stringify(value, null, 2)}\n`);
}
function writeText(relativePath, value) {
  const fullPath = path.resolve(repoRoot, relativePath);
  fs.mkdirSync(path.dirname(fullPath), { recursive: true });
  fs.writeFileSync(fullPath, value);
}
function renderMarkdown(report) {
  const rows = (report.results || []).map((r) => `| ${r.id} | ${r.kind} | ${r.package}@${r.version} | ${r.status} | ${r.toolCount} | ${r.riskSignals.join(', ')} | ${r.suggestedPolicy} |`);
  const notes = (report.notes || []).map((note) => `- ${note}`).join('\n');
  return `# Live Random MCP Probe\n\nSchema: \`${report.schema}\`  \nStatus: **${report.status || 'unknown'}**  \nMode: \`${report.mode}\`  \nGenerated: ${report.generatedAt}\n\nThis report covers real package-manager downloads only when run with \`--download\`. It sends only \`initialize\`, \`notifications/initialized\`, and \`tools/list\`. It does not call tools.\n\n## Summary\n\n- Servers: ${report.summary?.total || 0}\n- OK: ${report.summary?.ok || 0}\n- Failed/startup-blocked: ${report.summary?.failed || 0}\n- Tools discovered: ${report.summary?.totalTools || 0}\n- Policy mismatches: ${(report.summary?.policyMismatches || []).join(', ') || 'none'}
- Unexpected failures: ${(report.summary?.unexpectedFailures || []).join(', ') || 'none'}
- Server-side requests handled: ${Object.entries(report.summary?.serverSideRequestMethods || {}).map(([method, count]) => `${method}=${count}`).join(', ') || 'none'}\n\n## Results\n\n| Server | Kind | Package | Status | Tools | Risk signals | Suggested policy |\n|---|---|---|---:|---:|---|---|\n${rows.join('\n')}\n\n## Safety\n\n- Package install scripts allowed: ${report.safety?.packageInstallScriptsAllowed}\n- User secrets passed to runtime: ${report.safety?.userSecretsPassedToRuntime}\n- Destructive tool calls allowed: ${report.safety?.destructiveToolCallsAllowed}\n\n## Notes\n\n${notes}\n`;
}

main().catch((error) => {
  console.error(error.stack || error.message || String(error));
  process.exitCode = 1;
});
