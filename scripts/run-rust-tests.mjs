#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..');
const DEFAULT_TIMEOUT_MS = 180_000;
const KILL_GRACE_MS = 5_000;
const PRIORITY_INTEGRATION_SUITES = ['hub_runtime', 'mcp_server', 'stdio_shim'];
const LIFECYCLE_SUITE_NAMES = new Set(PRIORITY_INTEGRATION_SUITES.map((suite) => `test:${suite}`));
const VALID_PROFILES = new Set(['full', 'non-lifecycle', 'lifecycle']);

function parsePositiveInteger(value, label) {
  if (!/^\d+$/.test(String(value || ''))) {
    throw new Error(`${label} must be a positive integer`);
  }
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${label} must be a positive integer`);
  }
  return parsed;
}

function discoverIntegrationSuites() {
  const testsDir = path.join(repoRoot, 'tests');
  if (!fs.existsSync(testsDir)) {
    return [];
  }
  return fs
    .readdirSync(testsDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith('.rs'))
    .map((entry) => path.basename(entry.name, '.rs'))
    .sort();
}

function orderedIntegrationSuites() {
  const discovered = discoverIntegrationSuites();
  const discoveredSet = new Set(discovered);
  const priority = PRIORITY_INTEGRATION_SUITES.filter((suite) => discoveredSet.has(suite));
  const rest = discovered.filter((suite) => !PRIORITY_INTEGRATION_SUITES.includes(suite));
  return [...priority, ...rest];
}

function defaultSuites() {
  return [
    ...orderedIntegrationSuites().map((suite) => ({
      name: `test:${suite}`,
      command: ['cargo', ['test', '--test', suite, '--locked', '--', '--test-threads=1']]
    })),
    {
      name: 'lib',
      command: ['cargo', ['test', '--lib', '--locked', '--', '--test-threads=1']]
    },
    {
      name: 'doc',
      command: ['cargo', ['test', '--doc', '--locked', '--', '--test-threads=1']]
    }
  ];
}

function parseArgs(argv) {
  const parsed = {
    json: false,
    timeoutMs: process.env.MCPACE_RUST_TEST_TIMEOUT_MS
      ? parsePositiveInteger(process.env.MCPACE_RUST_TEST_TIMEOUT_MS, 'MCPACE_RUST_TEST_TIMEOUT_MS')
      : DEFAULT_TIMEOUT_MS,
    suites: [],
    profile: 'full',
    list: false
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--timeout-ms':
        parsed.timeoutMs = parsePositiveInteger(argv[++index], '--timeout-ms');
        break;
      case '--suite':
        parsed.suites.push(argv[++index] || '');
        break;
      case '--profile':
        parsed.profile = argv[++index] || '';
        break;
      case '--list':
        parsed.list = true;
        break;
      case '--help':
      case '-h':
        writeHelp(process.stdout);
        process.exit(0);
        break;
      default:
        throw new Error(`unsupported run-rust-tests argument: ${token}`);
    }
  }

  if (parsed.suites.some((suite) => !suite.trim())) {
    throw new Error('--suite requires a non-empty suite name');
  }
  if (!VALID_PROFILES.has(parsed.profile)) {
    throw new Error(`--profile must be one of: ${Array.from(VALID_PROFILES).join(', ')}`);
  }

  return parsed;
}

function writeHelp(stream) {
  stream.write([
    'Usage: node scripts/run-rust-tests.mjs [--json] [--timeout-ms <ms>] [--profile full|non-lifecycle|lifecycle] [--suite <name>...] [--list]',
    '',
    'Runs Rust tests one suite at a time so CI can report the exact suite that failed or hung.',
    'Suite names are lib, doc, and test:<integration-test-name>. Explicit --suite values override --profile.',
    'The per-suite timeout can also be set with MCPACE_RUST_TEST_TIMEOUT_MS.',
    ''
  ].join('\n'));
}

function writeLog(json, chunk) {
  if (json) {
    process.stderr.write(chunk);
  } else {
    process.stdout.write(chunk);
  }
}

function killChildTree(child) {
  if (!child.pid) {
    return;
  }

  if (process.platform === 'win32') {
    spawn('taskkill', ['/pid', String(child.pid), '/t', '/f'], { stdio: 'ignore' });
    return;
  }

  try {
    process.kill(-child.pid, 'SIGTERM');
  } catch {
    try {
      child.kill('SIGTERM');
    } catch {
      // Ignore cleanup failures; the result will still be reported as timed out.
    }
  }

  setTimeout(() => {
    try {
      process.kill(-child.pid, 'SIGKILL');
    } catch {
      try {
        child.kill('SIGKILL');
      } catch {
        // Ignore final cleanup failures.
      }
    }
  }, KILL_GRACE_MS).unref();
}

function runCommand(suite, options) {
  return new Promise((resolve) => {
    const [bin, args] = suite.command;
    const startedAt = Date.now();
    const child = spawn(bin, args, {
      cwd: repoRoot,
      env: process.env,
      detached: process.platform !== 'win32',
      windowsHide: true,
      stdio: ['ignore', 'pipe', 'pipe']
    });

    let timedOut = false;
    let stdoutBytes = 0;
    let stderrBytes = 0;

    writeLog(options.json, `\n[mcpace rust-test] ${suite.name}: ${bin} ${args.join(' ')}\n`);

    let settled = false;
    let forceResolveTimer = null;
    const finish = (result) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      if (forceResolveTimer) {
        clearTimeout(forceResolveTimer);
      }
      resolve({
        ...result,
        durationMs: Date.now() - startedAt,
        stdoutBytes,
        stderrBytes
      });
    };

    const timeout = setTimeout(() => {
      timedOut = true;
      writeLog(options.json, `[mcpace rust-test] ${suite.name}: timed out after ${options.timeoutMs}ms\n`);
      killChildTree(child);
      forceResolveTimer = setTimeout(() => {
        finish({
          name: suite.name,
          status: 'timeout',
          code: null,
          signal: 'timeout',
          timedOut
        });
      }, KILL_GRACE_MS + 500);
    }, options.timeoutMs);

    child.stdout.on('data', (chunk) => {
      stdoutBytes += chunk.length;
      writeLog(options.json, chunk);
    });

    child.stderr.on('data', (chunk) => {
      stderrBytes += chunk.length;
      process.stderr.write(chunk);
    });

    child.on('error', (error) => {
      finish({
        name: suite.name,
        status: 'fail',
        code: null,
        signal: null,
        timedOut,
        error: error.message
      });
    });

    child.on('exit', (code, signal) => {
      const status = code === 0 && !timedOut ? 'pass' : timedOut ? 'timeout' : 'fail';
      finish({
        name: suite.name,
        status,
        code,
        signal,
        timedOut
      });
    });
  });
}

function suitesForProfile(allSuites, profile) {
  switch (profile) {
    case 'full':
      return allSuites;
    case 'non-lifecycle':
      return allSuites.filter((suite) => !LIFECYCLE_SUITE_NAMES.has(suite.name));
    case 'lifecycle':
      return allSuites.filter((suite) => LIFECYCLE_SUITE_NAMES.has(suite.name));
    default:
      throw new Error(`unsupported Rust test profile: ${profile}`);
  }
}

function selectSuites(allSuites, requested, profile) {
  if (!requested.length) {
    return suitesForProfile(allSuites, profile);
  }
  const byName = new Map(allSuites.map((suite) => [suite.name, suite]));
  const missing = requested.filter((suite) => !byName.has(suite));
  if (missing.length) {
    throw new Error(`unknown Rust test suite(s): ${missing.join(', ')}`);
  }
  return requested.map((suite) => byName.get(suite));
}

export async function runRustTests(options) {
  const allSuites = defaultSuites();
  const suites = selectSuites(allSuites, options.suites, options.profile);
  const results = [];
  const startedAt = Date.now();

  for (const suite of suites) {
    const result = await runCommand(suite, options);
    results.push(result);
    if (result.status !== 'pass') {
      break;
    }
  }

  return {
    status: results.every((result) => result.status === 'pass') && results.length === suites.length ? 'pass' : 'fail',
    timeoutMs: options.timeoutMs,
    suiteCount: suites.length,
    durationMs: Date.now() - startedAt,
    suites: results
  };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const suites = defaultSuites();

  if (options.list) {
    const names = selectSuites(suites, options.suites, options.profile).map((suite) => suite.name);
    if (options.json) {
      process.stdout.write(`${JSON.stringify({ suites: names }, null, 2)}\n`);
    } else {
      process.stdout.write(`${names.join('\n')}\n`);
    }
    return;
  }

  const report = await runRustTests(options);
  if (options.json) {
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } else if (report.status !== 'pass') {
    process.stderr.write(`Rust test runner failed: ${JSON.stringify(report, null, 2)}\n`);
  }

  if (report.status !== 'pass') {
    process.exitCode = 1;
  }
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

if (isCliInvocation()) {
  main().catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
