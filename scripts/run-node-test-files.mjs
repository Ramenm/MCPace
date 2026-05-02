#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { spawn } from 'node:child_process';
import { repoRoot } from './lib/project-metadata.mjs';

const DEFAULT_TIMEOUT_MS = 120_000;
const NODE_MAJOR = Number.parseInt(process.versions.node.split('.')[0], 10);
const SUPPORTS_TEST_FORCE_EXIT = Number.isSafeInteger(NODE_MAJOR) && NODE_MAJOR >= 20;

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

function parseArgs(argv) {
  const parsed = {
    dir: null,
    ext: null,
    json: false,
    timeoutMs: process.env.MCPACE_NODE_TEST_FILE_TIMEOUT_MS
      ? parsePositiveInteger(process.env.MCPACE_NODE_TEST_FILE_TIMEOUT_MS, 'MCPACE_NODE_TEST_FILE_TIMEOUT_MS')
      : DEFAULT_TIMEOUT_MS
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--dir':
        parsed.dir = argv[++index] || null;
        break;
      case '--ext':
        parsed.ext = argv[++index] || null;
        break;
      case '--json':
        parsed.json = true;
        break;
      case '--timeout-ms':
        parsed.timeoutMs = parsePositiveInteger(argv[++index], '--timeout-ms');
        break;
      default:
        throw new Error(`unsupported run-node-test-files argument: ${token}`);
    }
  }

  if (!parsed.dir) {
    throw new Error('--dir is required');
  }
  if (!parsed.ext || !parsed.ext.startsWith('.')) {
    throw new Error('--ext is required and must start with a dot');
  }

  return parsed;
}

function discoverTests(dir, ext) {
  const absoluteDir = path.resolve(repoRoot, dir);
  if (!fs.existsSync(absoluteDir)) {
    throw new Error(`test directory does not exist: ${dir}`);
  }
  return fs
    .readdirSync(absoluteDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(ext))
    .map((entry) => path.join(dir, entry.name).split(path.sep).join('/'))
    .sort();
}

function terminateChild(child) {
  if (!child.pid) return;
  try {
    child.kill('SIGTERM');
  } catch {
    // Ignore cleanup errors; timeout status is still reported.
  }
  setTimeout(() => {
    try {
      child.kill('SIGKILL');
    } catch {
      // Ignore final cleanup errors.
    }
  }, 2000).unref();
}

function runTestFile(file, options) {
  return new Promise((resolve) => {
    const startedAt = Date.now();
    const childArgs = SUPPORTS_TEST_FORCE_EXIT
      ? ['--test', '--test-force-exit', file]
      : ['--test', file];
    const child = spawn(process.execPath, childArgs, {
      cwd: repoRoot,
      stdio: options.json ? ['ignore', 'pipe', 'pipe'] : 'inherit',
      windowsHide: true
    });

    let stdout = '';
    let stderr = '';
    let timedOut = false;
    let settled = false;

    if (options.json) {
      child.stdout?.on('data', (chunk) => { stdout += chunk; });
      child.stderr?.on('data', (chunk) => { stderr += chunk; });
    }

    const finish = (status) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve({
        file,
        status: status.ok ? 'pass' : timedOut ? 'timeout' : 'fail',
        ok: status.ok,
        code: status.code,
        signal: status.signal,
        durationMs: Date.now() - startedAt,
        timeoutMs: options.timeoutMs,
        stdout,
        stderr,
        error: status.error || null
      });
    };

    const timer = setTimeout(() => {
      timedOut = true;
      terminateChild(child);
      finish({ ok: false, code: null, signal: 'timeout', error: `timed out after ${options.timeoutMs}ms` });
    }, options.timeoutMs);

    child.on('error', (error) => {
      finish({ ok: false, code: null, signal: null, error: String(error.message || error) });
    });

    child.on('exit', (code, signal) => {
      finish({ ok: code === 0 && !timedOut, code, signal, error: null });
    });
  });
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const files = discoverTests(options.dir, options.ext);
  const results = [];

  for (const file of files) {
    if (!options.json) {
      process.stdout.write(`\n[mcpace node-test] ${file}\n`);
    }
    const result = await runTestFile(file, options);
    results.push(result);
    if (options.json) {
      // Keep stdout/stderr in the JSON report only.
    } else {
      process.stdout.write(`[mcpace node-test] ${file}: ${result.status} (${result.durationMs}ms)\n`);
    }
    if (!result.ok) {
      break;
    }
  }

  const report = {
    status: results.every((result) => result.ok) && results.length === files.length ? 'pass' : 'fail',
    total: files.length,
    completed: results.length,
    passed: results.filter((result) => result.ok).length,
    results: results.map((result) => ({
      file: result.file,
      status: result.status,
      code: result.code,
      signal: result.signal,
      durationMs: result.durationMs,
      timeoutMs: result.timeoutMs,
      error: result.error
    }))
  };

  if (options.json) {
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } else {
    process.stdout.write(`\n[mcpace node-test] ${report.status}: ${report.passed}/${report.total} files passed\n`);
  }

  if (report.status !== 'pass') {
    process.exit(1);
  }
}

main().catch((error) => {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exit(1);
});
