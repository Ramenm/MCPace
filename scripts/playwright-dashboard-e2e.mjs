#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { repoRoot, deriveProjectName, deriveProjectVersion } from './lib/project-metadata.mjs';
import { cleanChildEnv } from './lib/safe-child-env.mjs';

const PLAYWRIGHT_PACKAGE = '@playwright/test@1.60.0';
const DEFAULT_TIMEOUT_MS = 180_000;

function parseArgs(argv) {
  const args = {
    json: false,
    write: 'reports/playwright-dashboard-e2e-latest.json',
    markdown: 'reports/playwright-dashboard-e2e-latest.md',
    timeoutMs: DEFAULT_TIMEOUT_MS,
    chromium: process.env.MCPACE_PLAYWRIGHT_CHROMIUM || findChromiumExecutable(),
    npm: process.env.MCPACE_NPM || 'npm',
    useExistingNodeModules: false,
    help: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    const readValue = () => {
      const value = argv[index + 1];
      if (!value || value.startsWith('--')) throw new Error(`${token} requires a value`);
      index += 1;
      return value;
    };
    switch (token) {
      case '--json': args.json = true; break;
      case '--write': args.write = readValue(); break;
      case '--markdown': args.markdown = readValue(); break;
      case '--no-write': args.write = null; args.markdown = null; break;
      case '--timeout-ms': args.timeoutMs = parsePositiveInteger(readValue(), token); break;
      case '--chromium': args.chromium = readValue(); break;
      case '--npm': args.npm = readValue(); break;
      case '--use-existing-node-modules': args.useExistingNodeModules = true; break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported playwright-dashboard-e2e argument: ${token}`);
    }
  }
  return args;
}

function parsePositiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}

function printHelp() {
  console.log(`Usage: node scripts/playwright-dashboard-e2e.mjs [options]

Runs the real Chromium dashboard E2E lane through Playwright. The wrapper creates
a temporary npm prefix, installs ${PLAYWRIGHT_PACKAGE} there, copies the E2E spec,
and runs it against system Chromium. The repository archive still does not vendor
node_modules or Playwright browser binaries.

Options:
  --chromium <path>              Chromium/Chrome executable. Auto-detected by default.
  --timeout-ms <ms>              Wrapper timeout. Default ${DEFAULT_TIMEOUT_MS}
  --use-existing-node-modules    Run from repository node_modules instead of temp install.
  --write <path>                 JSON report path.
  --markdown <path>              Markdown report path.
  --no-write                     Do not write reports.
  --json                         Print JSON report.
`);
}

function findChromiumExecutable() {
  const candidates = [
    process.env.CHROME_BIN,
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    '/usr/bin/google-chrome',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
    'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe'
  ].filter(Boolean);
  return candidates.find((candidate) => fs.existsSync(candidate)) || null;
}

function redact(text) {
  return String(text || '')
    .replace(/Bearer\s+[A-Za-z0-9._~+/-]+=*/gi, 'Bearer <redacted>')
    .replace(/(api[_-]?key|token|secret|password)=([^\s&]+)/gi, '$1=<redacted>');
}


function npmInstallEnv(overrides = {}) {
  const env = cleanChildEnv(overrides);
  for (const key of [
    'NPM_CONFIG_REGISTRY',
    'npm_config_registry',
    'NPM_CONFIG_USERCONFIG',
    'npm_config_userconfig',
    'NPM_CONFIG_CACHE',
    'npm_config_cache',
    'HTTP_PROXY',
    'HTTPS_PROXY',
    'ALL_PROXY',
    'NO_PROXY',
    'http_proxy',
    'https_proxy',
    'all_proxy',
    'no_proxy',
    'ENV_HTTP_PROXY',
    'ENV_HTTPS_PROXY',
    'ENV_ALL_PROXY',
    'ENV_NO_PROXY'
  ]) {
    if (process.env[key]) env[key] = process.env[key];
  }
  return env;
}

function copyE2EFiles(tempRoot) {
  const destination = path.join(tempRoot, 'tests', 'e2e');
  fs.mkdirSync(destination, { recursive: true });
  for (const entry of fs.readdirSync(path.join(repoRoot, 'tests', 'e2e'))) {
    if (!entry.endsWith('.mjs')) continue;
    fs.copyFileSync(path.join(repoRoot, 'tests', 'e2e', entry), path.join(destination, entry));
  }
  return destination;
}

function readParallelState(stateDir) {
  if (!stateDir || !fs.existsSync(stateDir)) {
    return {
      clientCount: 0,
      workerCount: 0,
      workerIndexes: [],
      clients: [],
      conflicts: [],
      maxDurationMs: 0
    };
  }

  const entries = fs.readdirSync(stateDir)
    .filter((entry) => entry.endsWith('.json'))
    .map((entry) => JSON.parse(fs.readFileSync(path.join(stateDir, entry), 'utf8')));
  const clients = entries.map((entry) => entry.clientId);
  const workerIndexes = [...new Set(entries.map((entry) => entry.workerIndex))].sort((a, b) => a - b);
  const conflicts = [];
  const localSessions = new Map();
  for (const entry of entries) {
    const session = entry.snapshot?.clientSession;
    const rootPath = entry.snapshot?.rootPath || '';
    if (session !== entry.clientId) conflicts.push(`${entry.clientId}: local session mismatch`);
    if (!rootPath.includes(`/tmp/mcpace-parallel-${entry.clientId}-`)) {
      conflicts.push(`${entry.clientId}: root path mismatch`);
    }
    if (localSessions.has(session) && localSessions.get(session) !== entry.clientId) {
      conflicts.push(`${entry.clientId}: local session collides with ${localSessions.get(session)}`);
    }
    localSessions.set(session, entry.clientId);
  }
  return {
    clientCount: entries.length,
    workerCount: workerIndexes.length,
    workerIndexes,
    clients,
    conflicts,
    maxDurationMs: entries.reduce((max, entry) => Math.max(max, entry.durationMs || 0), 0)
  };
}

function installPlaywright(tempRoot, args, remainingTimeoutMs) {
  if (args.useExistingNodeModules) {
    const cli = path.join(repoRoot, 'node_modules', '.bin', process.platform === 'win32' ? 'playwright.cmd' : 'playwright');
    return fs.existsSync(cli)
      ? { ok: true, cli, stdout: '', stderr: '', elapsedMs: 0 }
      : { ok: false, reason: 'repository node_modules does not contain playwright CLI', stdout: '', stderr: '', elapsedMs: 0 };
  }

  fs.writeFileSync(path.join(tempRoot, 'package.json'), JSON.stringify({ private: true, type: 'module' }, null, 2));
  const started = performance.now();
  const install = spawnSync(args.npm, ['install', '--no-save', '--no-audit', '--no-fund', PLAYWRIGHT_PACKAGE], {
    cwd: tempRoot,
    encoding: 'utf8',
    timeout: remainingTimeoutMs,
    env: npmInstallEnv({
      PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD: '1',
      npm_config_loglevel: 'error',
      CI: '1'
    }),
    windowsHide: true,
  });
  const elapsedMs = performance.now() - started;
  const cli = path.join(tempRoot, 'node_modules', '.bin', process.platform === 'win32' ? 'playwright.cmd' : 'playwright');
  if (install.error || install.status !== 0 || !fs.existsSync(cli)) {
    return {
      ok: false,
      reason: install.error?.message || `npm install exited ${install.status}`,
      stdout: redact(install.stdout),
      stderr: redact(install.stderr),
      elapsedMs
    };
  }
  return {
    ok: true,
    cli,
    stdout: redact(install.stdout),
    stderr: redact(install.stderr),
    elapsedMs
  };
}

function runPlaywright(args) {
  if (!args.chromium || !fs.existsSync(args.chromium)) {
    return {
      status: 'blocked',
      exitCode: null,
      elapsedMs: 0,
      install: null,
      reason: 'Chromium executable was not found. Set MCPACE_PLAYWRIGHT_CHROMIUM or pass --chromium.',
      stdout: '',
      stderr: '',
      parallelState: null,
    };
  }

  const started = performance.now();
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-playwright-e2e-'));
  const stateDir = path.join(tempRoot, 'parallel-state');
  try {
    copyE2EFiles(tempRoot);
    const install = installPlaywright(tempRoot, args, Math.ceil(Math.max(30_000, args.timeoutMs - 20_000)));
    if (!install.ok) {
      return {
        status: 'blocked',
        exitCode: null,
        elapsedMs: performance.now() - started,
        install,
        reason: `Playwright package install failed: ${install.reason}`,
        stdout: install.stdout,
        stderr: install.stderr,
        parallelState: readParallelState(stateDir),
      };
    }

    const remaining = Math.ceil(Math.max(30_000, args.timeoutMs - (performance.now() - started)));
    const result = spawnSync(install.cli, [
      'test',
      '--config', path.join(tempRoot, 'tests', 'e2e', 'playwright.config.mjs'),
      '--reporter', 'list'
    ], {
      cwd: tempRoot,
      encoding: 'utf8',
      timeout: remaining,
      env: cleanChildEnv({
        MCPACE_PLAYWRIGHT_CHROMIUM: args.chromium,
        MCPACE_REPO_ROOT: repoRoot,
        MCPACE_PLAYWRIGHT_STATE_DIR: stateDir,
        MCPACE_PLAYWRIGHT_WORKERS: process.env.MCPACE_PLAYWRIGHT_WORKERS || '2',
        MCPACE_PLAYWRIGHT_PARALLEL_CLIENTS: process.env.MCPACE_PLAYWRIGHT_PARALLEL_CLIENTS || '4',
        PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD: '1',
        CI: '1'
      }),
      windowsHide: true,
    });
    const elapsedMs = performance.now() - started;
    const parallelState = readParallelState(stateDir);
    if (result.error) {
      return {
        status: result.error.name === 'ETIMEDOUT' ? 'blocked' : 'fail',
        exitCode: result.status,
        elapsedMs,
        install,
        parallelState,
        reason: result.error.message,
        stdout: redact(result.stdout),
        stderr: redact(result.stderr),
      };
    }
    return {
      status: result.status === 0 ? 'pass' : 'fail',
      exitCode: result.status,
      elapsedMs,
      install,
      parallelState,
      reason: result.status === 0 ? null : 'Playwright dashboard E2E returned a non-zero exit code.',
      stdout: redact(result.stdout),
      stderr: redact(result.stderr),
    };
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
}

function makeReport(args, outcome) {
  const specText = fs.readdirSync(path.join(repoRoot, 'tests/e2e'))
    .filter((entry) => entry.endsWith('.mjs'))
    .map((entry) => fs.readFileSync(path.join(repoRoot, 'tests/e2e', entry), 'utf8'))
    .join('\n');
  const configText = fs.readFileSync(path.join(repoRoot, 'tests/e2e/playwright.config.mjs'), 'utf8');
  const parallelState = outcome.parallelState || { clientCount: 0, workerCount: 0, conflicts: [] };
  const checks = [
    {
      id: 'chromium-executable-found',
      ok: Boolean(args.chromium && fs.existsSync(args.chromium)),
      evidence: args.chromium || 'not found'
    },
    {
      id: 'playwright-package-available-in-temp-prefix',
      ok: Boolean(outcome.install?.ok),
      evidence: PLAYWRIGHT_PACKAGE
    },
    {
      id: 'real-playwright-invoked',
      ok: /Running\s+\d+\s+test/.test(outcome.stdout) || /passed/.test(outcome.stdout),
      evidence: 'Playwright CLI output observed'
    },
    {
      id: 'multiple-tabs-and-network-degradation-covered',
      ok: specText.includes('real Chromium tabs') && specText.includes('synthetic logs outage'),
      evidence: 'tests/e2e/dashboard.playwright.spec.mjs'
    },
    {
      id: 'multi-worker-parallel-configured',
      ok: configText.includes('fullyParallel: true') && configText.includes('MCPACE_PLAYWRIGHT_WORKERS'),
      evidence: 'tests/e2e/playwright.config.mjs uses configurable workers and fullyParallel'
    },
    {
      id: 'parallel-client-session-spec-covered',
      ok: specText.includes("test.describe.configure({ mode: 'parallel' })") && specText.includes('browser.newContext') && specText.includes('__mcpaceClientSession'),
      evidence: 'tests/e2e/dashboard.parallel.playwright.spec.mjs'
    },
    {
      id: 'parallel-client-sessions-isolated-at-runtime',
      ok: parallelState.clientCount >= 4 && parallelState.workerCount >= 2 && parallelState.conflicts.length === 0,
      evidence: `${parallelState.clientCount} clients across ${parallelState.workerCount} workers; conflicts=${parallelState.conflicts.length}`
    },
    {
      id: 'console-errors-fail-test',
      ok: specText.includes("message.type() === 'error'"),
      evidence: 'browser console errors are captured'
    },
    {
      id: 'playwright-execution-pass',
      ok: outcome.status === 'pass',
      evidence: outcome.status === 'pass' ? `elapsed ${Math.round(outcome.elapsedMs)}ms` : (outcome.reason || `exit ${outcome.exitCode}`)
    }
  ];

  const status = outcome.status === 'pass' && checks.every((check) => check.ok) ? 'pass' : outcome.status === 'blocked' ? 'blocked' : 'fail';
  return {
    schema: 'mcpace.playwrightDashboardE2E.v2',
    status,
    generatedAt: new Date().toISOString(),
    project: deriveProjectName(),
    version: deriveProjectVersion(),
    tool: {
      package: PLAYWRIGHT_PACKAGE,
      chromium: args.chromium,
      command: `temporary npm install ${PLAYWRIGHT_PACKAGE} && playwright test --config tests/e2e/playwright.config.mjs --reporter list`
    },
    summary: {
      elapsedMs: Number(outcome.elapsedMs.toFixed(2)),
      installElapsedMs: outcome.install?.elapsedMs ? Number(outcome.install.elapsedMs.toFixed(2)) : null,
      exitCode: outcome.exitCode,
      reason: outcome.reason,
      stdoutTail: outcome.stdout.split('\n').slice(-30).join('\n'),
      stderrTail: outcome.stderr.split('\n').slice(-30).join('\n'),
      parallelState
    },
    checks
  };
}

function writeReport(report, args) {
  if (args.write) {
    const output = path.join(repoRoot, args.write);
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n');
  }
  if (args.markdown) {
    const output = path.join(repoRoot, args.markdown);
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, renderMarkdown(report));
  }
}

function renderMarkdown(report) {
  return `# Playwright dashboard E2E smoke

- Status: ${report.status}
- Generated: ${report.generatedAt}
- Project: ${report.project} ${report.version}
- Tool: ${report.tool.package}
- Chromium: ${report.tool.chromium || 'not found'}
- Elapsed: ${report.summary.elapsedMs}ms
- Install elapsed: ${report.summary.installElapsedMs ?? 'n/a'}ms
- Parallel clients: ${report.summary.parallelState?.clientCount ?? 0}
- Parallel workers observed: ${report.summary.parallelState?.workerCount ?? 0}
- Parallel conflicts: ${(report.summary.parallelState?.conflicts || []).length}

## Checks

| Check | OK | Evidence |
|---|---:|---|
${report.checks.map((check) => `| ${check.id} | ${check.ok ? 'yes' : 'no'} | ${String(check.evidence || '').replace(/\n/g, ' ')} |`).join('\n')}

## Output tail

\`\`\`text
${report.summary.stdoutTail || '(empty)'}
${report.summary.stderrTail ? `\nSTDERR:\n${report.summary.stderrTail}` : ''}
\`\`\`
`;
}

function main() {
  try {
    const args = parseArgs(process.argv.slice(2));
    if (args.help) {
      printHelp();
      return;
    }
    const outcome = runPlaywright(args);
    const report = makeReport(args, outcome);
    writeReport(report, args);
    if (args.json) console.log(JSON.stringify(report, null, 2));
    if (report.status !== 'pass') process.exitCode = report.status === 'blocked' ? 2 : 1;
  } catch (error) {
    console.error(error.message || error);
    process.exitCode = 1;
  }
}

main();
