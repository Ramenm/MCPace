#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawn } from 'node:child_process';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
import { childEnvForCommand } from './lib/safe-child-env.mjs';

const DEFAULT_TIMEOUT_MS = 120_000;
const KILL_GRACE_MS = 2_000;
const DEFAULT_HEARTBEAT_MS = 0;
const DEFAULT_MAX_AUTO_JOBS = 4;
const DEFAULT_MAX_CAPTURE_BYTES = 512 * 1024;
const NODE_MAJOR = Number.parseInt(process.versions.node.split('.')[0], 10);
const SUPPORTS_TEST_FORCE_EXIT = Number.isSafeInteger(NODE_MAJOR) && NODE_MAJOR >= 20;
const ISOLATED_TEST_FILE_BASENAMES = new Set([
  'dashboard-chaos-contract.test.js',
  'lifecycle-blast-radius-contract.test.js',
  'mcp-install-scenarios-contract.test.js',
  'performance-smoke-contract.test.js',
  'product-practice-contract.test.js',
  'publish-npm-artifacts-contract.test.js',
  'rust-quality-contract.test.js',
  'stage-vendored-binary.test.js',
  'system-lifecycle-contract.test.js',
  'tool-exposure-call-safety-contract.test.js',
  'tool-message-integrity-contract.test.js',
  'tool-scale-contract.test.js',
  'upstream-failsafe-contract.test.js',
  'verify-npm-pack.test.js',
  'verify-vendored-binary.test.js',
]);

function positiveInt(value, label) {
  if (!/^\d+$/.test(String(value || ''))) throw new Error(`${label} must be a positive integer`);
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}


function parseJobs(value, label = '--jobs') {
  if (value === undefined || value === null || value === '' || value === 'auto') return 'auto';
  return positiveInt(value, label);
}

function availableJobCount() {
  if (typeof os.availableParallelism === 'function') return os.availableParallelism();
  const cpus = os.cpus();
  return Array.isArray(cpus) && cpus.length > 0 ? cpus.length : 1;
}

function resolveBatchSize(value, fileCount) {
  if (fileCount <= 1) return Math.max(1, fileCount);
  if (value === 'auto') return Math.min(fileCount, Math.max(1, Math.min(availableJobCount(), DEFAULT_MAX_AUTO_JOBS)));
  return Math.min(fileCount, Math.max(1, value));
}

function parseShard(value) {
  const match = String(value || '').match(/^(\d+)\/(\d+)$/);
  if (!match) throw new Error('--shard must use the form index/total, for example 1/3');
  const index = Number(match[1]);
  const total = Number(match[2]);
  if (!Number.isSafeInteger(index) || !Number.isSafeInteger(total) || index < 1 || total < 1 || index > total) {
    throw new Error('--shard index/total must be positive and index must be <= total');
  }
  return { index, total };
}

function parseArgs(argv) {
  const parsed = {
    dir: null,
    ext: null,
    json: false,
    write: null,
    progress: false,
    timeoutMs: process.env.MCPACE_NODE_TEST_FILE_TIMEOUT_MS
      ? positiveInt(process.env.MCPACE_NODE_TEST_FILE_TIMEOUT_MS, 'MCPACE_NODE_TEST_FILE_TIMEOUT_MS')
      : DEFAULT_TIMEOUT_MS,
    heartbeatMs: process.env.MCPACE_NODE_TEST_HEARTBEAT_MS
      ? positiveInt(process.env.MCPACE_NODE_TEST_HEARTBEAT_MS, 'MCPACE_NODE_TEST_HEARTBEAT_MS')
      : DEFAULT_HEARTBEAT_MS,
    maxCaptureBytes: process.env.MCPACE_NODE_TEST_MAX_CAPTURE_BYTES
      ? positiveInt(process.env.MCPACE_NODE_TEST_MAX_CAPTURE_BYTES, 'MCPACE_NODE_TEST_MAX_CAPTURE_BYTES')
      : DEFAULT_MAX_CAPTURE_BYTES,
    batchSize: process.env.MCPACE_NODE_TEST_JOBS
      ? parseJobs(process.env.MCPACE_NODE_TEST_JOBS, 'MCPACE_NODE_TEST_JOBS')
      : 'auto',
    only: [],
    shard: null,
    failFast: true,
    help: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--dir': parsed.dir = argv[++index] || null; break;
      case '--ext': parsed.ext = argv[++index] || null; break;
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; break;
      case '--progress': parsed.progress = true; break;
      case '--timeout-ms': parsed.timeoutMs = positiveInt(argv[++index], '--timeout-ms'); break;
      case '--heartbeat-ms': parsed.heartbeatMs = positiveInt(argv[++index], '--heartbeat-ms'); break;
      case '--max-capture-bytes': parsed.maxCaptureBytes = positiveInt(argv[++index], '--max-capture-bytes'); break;
      case '--batch-size': parsed.batchSize = positiveInt(argv[++index], '--batch-size'); break;
      case '--jobs': parsed.batchSize = parseJobs(argv[++index], '--jobs'); break;
      case '--only': parsed.only.push(String(argv[++index] || '')); break;
      case '--shard': parsed.shard = parseShard(argv[++index]); break;
      case '--no-fail-fast': parsed.failFast = false; break;
      case '--fail-fast': parsed.failFast = true; break;
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported run-node-test-files argument: ${token}`);
    }
  }

  if (parsed.help) return parsed;
  if (!parsed.dir) throw new Error('--dir is required');
  if (!parsed.ext || !parsed.ext.startsWith('.')) throw new Error('--ext is required and must start with a dot');
  if (parsed.write === '') throw new Error('--write requires a value');
  parsed.only = parsed.only.filter(Boolean);
  return parsed;
}

function discoverTests(dir, ext) {
  const absoluteDir = path.resolve(repoRoot, dir);
  if (!fs.existsSync(absoluteDir)) throw new Error(`test directory does not exist: ${dir}`);
  return fs
    .readdirSync(absoluteDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(ext))
    .map((entry) => path.join(dir, entry.name).split(path.sep).join('/'))
    .sort();
}

function applyFilters(files, options) {
  let selected = files;
  if (options.only.length) {
    selected = selected.filter((file) => {
      const base = path.basename(file);
      return options.only.some((needle) => file.includes(needle) || base.includes(needle));
    });
  }
  if (options.shard) {
    selected = selected.filter((_, idx) => (idx % options.shard.total) + 1 === options.shard.index);
  }
  return selected;
}

function chunk(files, size) {
  const chunks = [];
  let current = [];
  const flushCurrent = () => {
    if (current.length) {
      chunks.push(current);
      current = [];
    }
  };

  for (const file of files) {
    if (ISOLATED_TEST_FILE_BASENAMES.has(path.basename(file))) {
      flushCurrent();
      chunks.push([file]);
      continue;
    }
    current.push(file);
    if (current.length >= size) flushCurrent();
  }
  flushCurrent();
  return chunks;
}

function terminateChild(child) {
  if (!child.pid) return;
  if (process.platform === 'win32') {
    try { child.kill('SIGTERM'); } catch { /* ignore */ }
    setTimeout(() => {
      try { child.kill('SIGKILL'); } catch { /* ignore */ }
    }, KILL_GRACE_MS).unref();
    return;
  }

  try {
    process.kill(-child.pid, 'SIGTERM');
  } catch {
    try { child.kill('SIGTERM'); } catch { /* ignore */ }
  }
  setTimeout(() => {
    try { process.kill(-child.pid, 'SIGKILL'); }
    catch { try { child.kill('SIGKILL'); } catch { /* ignore */ } }
  }, KILL_GRACE_MS).unref();
}

function appendBounded(current, chunkValue, limit) {
  const next = current + String(chunkValue || '');
  if (next.length <= limit) return next;
  const marker = '\n…<mcpace output truncated>…\n';
  return marker + next.slice(Math.max(0, next.length - limit));
}

function summarize(text, maxLines = 40) {
  const lines = String(text || '').split(/\r?\n/).filter(Boolean);
  if (lines.length <= maxLines) return lines.join('\n');
  return ['…', ...lines.slice(-maxLines)].join('\n');
}



function runTestFile(file, options) {
  return new Promise((resolve) => {
    const startedAt = Date.now();
    const childArgs = SUPPORTS_TEST_FORCE_EXIT ? ['--test', '--test-force-exit', file] : ['--test', file];
    const child = spawn(process.execPath, childArgs, {
      cwd: repoRoot,
      env: childEnvForCommand('node'),
      detached: process.platform !== 'win32',
      stdio: ['ignore', 'pipe', 'pipe'],
      windowsHide: true,
    });

    let stdout = '';
    let stderr = '';
    let timedOut = false;
    let settled = false;
    let heartbeat = null;
    let forceResolveTimer = null;

    child.stdout?.on('data', (data) => {
      stdout = appendBounded(stdout, data, options.maxCaptureBytes);
      if (!options.json) process.stdout.write(data);
    });
    child.stderr?.on('data', (data) => {
      stderr = appendBounded(stderr, data, options.maxCaptureBytes);
      if (!options.json) process.stderr.write(data);
    });

    const finish = (status) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (heartbeat) clearInterval(heartbeat);
      if (forceResolveTimer) clearTimeout(forceResolveTimer);
      try { child.stdout?.destroy(); } catch { /* ignore */ }
      try { child.stderr?.destroy(); } catch { /* ignore */ }
      resolve({
        file,
        status: status.ok ? 'pass' : timedOut ? 'timeout' : 'fail',
        ok: status.ok,
        code: status.code,
        signal: status.signal,
        durationMs: Date.now() - startedAt,
        timeoutMs: options.timeoutMs,
        error: status.error || null,
        stdoutSummary: summarize(stdout),
        stderrSummary: summarize(stderr),
      });
    };

    const timer = setTimeout(() => {
      timedOut = true;
      terminateChild(child);
      forceResolveTimer = setTimeout(() => {
        finish({ ok: false, code: null, signal: 'timeout', error: `timed out after ${options.timeoutMs}ms` });
      }, KILL_GRACE_MS + 500);
    }, options.timeoutMs);

    if (options.heartbeatMs > 0 && options.progress) {
      heartbeat = setInterval(() => {
        process.stderr.write(`[mcpace node-test] still running ${file} (${Date.now() - startedAt}ms)\n`);
      }, options.heartbeatMs);
      heartbeat.unref();
    }

    child.on('error', (error) => finish({ ok: false, code: null, signal: null, error: String(error.message || error) }));
    child.on('exit', (code, signal) => finish({ ok: code === 0 && !timedOut, code, signal, error: null }));
  });
}

async function runBatch(files, batchIndex, batchCount, options) {
  if (options.progress) process.stderr.write(`[mcpace node-test] start batch ${batchIndex + 1}/${batchCount} (${files.length} files)\n`);
  const startedAt = Date.now();
  const results = await Promise.all(files.map((file) => runTestFile(file, options)));
  const ok = results.every((result) => result.ok);
  if (options.progress) process.stderr.write(`[mcpace node-test] batch ${batchIndex + 1}/${batchCount}: ${ok ? 'pass' : 'fail'} (${Date.now() - startedAt}ms)\n`);
  return results.map((result) => ({ batch: batchIndex + 1, files, ...result }));
}

function writeJson(file, report) {
  if (!file) return;
  const target = path.resolve(repoRoot, file);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write('Usage: node scripts/run-node-test-files.mjs --dir <dir> --ext <.ext> [--json] [--write <path>] [--progress] [--batch-size <n>] [--jobs <n|auto>] [--only <substring>] [--shard <i/n>] [--timeout-ms <ms>]\n');
    return;
  }
  const allFiles = discoverTests(options.dir, options.ext);
  const selected = applyFilters(allFiles, options);
  const resolvedBatchSize = resolveBatchSize(options.batchSize, selected.length);
  const batches = chunk(selected, resolvedBatchSize);
  const results = [];

  if (options.progress) process.stderr.write(`[mcpace node-test] selected ${selected.length}/${allFiles.length} files, batchSize=${resolvedBatchSize}\n`);

  for (let index = 0; index < batches.length; index += 1) {
    const batchResults = await runBatch(batches[index], index, batches.length, options);
    results.push(...batchResults);
    if (options.failFast && batchResults.some((result) => !result.ok)) break;
  }

  const completed = results.length;
  const passed = results.filter((result) => result.ok).length;
  const report = {
    schema: 'mcpace.nodeTestFiles.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: completed === selected.length && results.every((result) => result.ok) ? 'pass' : 'fail',
    total: allFiles.length,
    selected: selected.length,
    batchSize: resolvedBatchSize,
    batchCount: batches.length,
    shard: options.shard,
    completedBatches: new Set(results.map((result) => result.batch)).size,
    completed,
    passed,
    results: results.map((result) => ({
      batch: result.batch,
      files: result.files,
      file: result.file,
      status: result.status,
      code: result.code,
      signal: result.signal,
      durationMs: result.durationMs,
      timeoutMs: result.timeoutMs,
      error: result.error,
      stdoutSummary: result.stdoutSummary,
      stderrSummary: result.stderrSummary,
    })),
  };

  writeJson(options.write, report);
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  if (report.status !== 'pass') process.exitCode = 1;
}

main().catch((error) => {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
