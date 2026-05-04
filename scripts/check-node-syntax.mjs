#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { repoRoot } from './lib/project-metadata.mjs';
import { cleanChildEnv } from './lib/safe-child-env.mjs';

export const SOURCE_ROOTS = Object.freeze(['packages/npm/cli', 'scripts', 'tests/node', 'tests/fixtures', 'examples']);
const NODE_EXTENSIONS = new Set(['.js', '.mjs']);
const SKIP_DIRS = new Set(['.git', 'node_modules', 'target', 'dist', 'vendor', 'data', 'logs', 'backups']);
const SKIP_PREFIXES = ['.tmp-', 'tmp-'];
const DEFAULT_MAX_AUTO_JOBS = 6;

function parseArgs(argv) {
  const parsed = {
    json: false,
    write: null,
    list: false,
    failFast: false,
    root: repoRoot,
    jobs: process.env.MCPACE_NODE_SYNTAX_JOBS || 'auto',
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; if (!parsed.write) throw new Error('check-node-syntax requires a path after --write'); break;
      case '--list': parsed.list = true; break;
      case '--fail-fast': parsed.failFast = true; break;
      case '--jobs': parsed.jobs = argv[++index] || null; if (!parsed.jobs) throw new Error('check-node-syntax requires a value after --jobs'); break;
      case '--root': {
        const value = argv[++index];
        if (!value) throw new Error('check-node-syntax requires a path after --root');
        parsed.root = path.resolve(value);
        break;
      }
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported check-node-syntax argument: ${token}`);
    }
  }
  return parsed;
}

function printHelp() {
  process.stdout.write('Usage: node scripts/check-node-syntax.mjs [--json] [--write <path>] [--list] [--fail-fast] [--jobs <n|auto>] [--root <path>]\n\nAuto-discovers project JavaScript/MJS files and runs `node --check` on each one. Parallel jobs default to auto and can be capped with MCPACE_NODE_SYNTAX_JOBS. This keeps package.json from hardcoding every Node source file.\n');
}

function shouldSkipDir(entryName) {
  return SKIP_DIRS.has(entryName) || SKIP_PREFIXES.some((prefix) => entryName.startsWith(prefix));
}

function normalizeRelative(root, filePath) {
  return path.relative(root, filePath).split(path.sep).join('/');
}

function walkNodeFiles(root, relativeRoot, files) {
  const absoluteRoot = path.join(root, relativeRoot);
  if (!fs.existsSync(absoluteRoot)) return;
  const stack = [absoluteRoot];
  while (stack.length > 0) {
    const current = stack.pop();
    const entries = fs.readdirSync(current, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const fullPath = path.join(current, entry.name);
      if (entry.isDirectory()) {
        if (!shouldSkipDir(entry.name)) stack.push(fullPath);
        continue;
      }
      if (!entry.isFile()) continue;
      if (!NODE_EXTENSIONS.has(path.extname(entry.name).toLowerCase())) continue;
      const relativePath = normalizeRelative(root, fullPath);
      if (relativePath.endsWith('.min.js')) continue;
      files.push({ relativePath, absolutePath: fullPath });
    }
  }
}

export function discoverNodeSourceFiles(options = {}) {
  const root = options.root || repoRoot;
  const files = [];
  for (const relativeRoot of SOURCE_ROOTS) walkNodeFiles(root, relativeRoot, files);
  const seen = new Set();
  return files
    .filter((file) => {
      if (seen.has(file.relativePath)) return false;
      seen.add(file.relativePath);
      return true;
    })
    .sort((left, right) => left.relativePath.localeCompare(right.relativePath));
}

function resolveJobs(options, fileCount) {
  if (fileCount <= 1 || options.failFast) return 1;
  const raw = String(options.jobs || process.env.MCPACE_NODE_SYNTAX_JOBS || 'auto').trim().toLowerCase();
  let requested;
  if (raw === 'auto') {
    const available = typeof os.availableParallelism === 'function'
      ? os.availableParallelism()
      : Math.max(1, os.cpus().length || 1);
    requested = Math.max(1, Math.min(DEFAULT_MAX_AUTO_JOBS, available));
  } else {
    requested = Number.parseInt(raw, 10);
  }
  if (!Number.isFinite(requested) || requested < 1) {
    throw new Error(`invalid check-node-syntax jobs value '${raw}', expected positive integer or auto`);
  }
  return Math.max(1, Math.min(fileCount, requested));
}

function baseCheckResult(file, startedAt) {
  return {
    file: file.relativePath,
    durationMs: Date.now() - startedAt,
  };
}

function checkOne(file, root) {
  const startedAt = Date.now();
  const result = spawnSync(process.execPath, ['--check', file.relativePath], {
    cwd: root,
    encoding: 'utf8',
    env: cleanChildEnv(),
    maxBuffer: 2 * 1024 * 1024,
    windowsHide: true,
  });
  return {
    ...baseCheckResult(file, startedAt),
    status: result.status === 0 ? 'pass' : 'fail',
    code: result.status,
    signal: result.signal || null,
    stdout: (result.stdout || '').trim(),
    stderr: (result.stderr || '').trim(),
    error: result.error ? result.error.message || String(result.error) : null,
  };
}

function checkOneAsync(file, root) {
  const startedAt = Date.now();
  return new Promise((resolve) => {
    const child = spawn(process.execPath, ['--check', file.relativePath], {
      cwd: root,
      encoding: 'utf8',
      env: cleanChildEnv(),
      windowsHide: true,
    });
    let stdout = '';
    let stderr = '';
    let finished = false;
    const finish = (result) => {
      if (finished) return;
      finished = true;
      resolve(result);
    };
    child.stdout?.on('data', (chunk) => { stdout += String(chunk); });
    child.stderr?.on('data', (chunk) => { stderr += String(chunk); });
    child.on('error', (error) => finish({
      ...baseCheckResult(file, startedAt),
      status: 'fail',
      code: null,
      signal: null,
      stdout: stdout.trim(),
      stderr: stderr.trim(),
      error: error.message || String(error),
    }));
    child.on('close', (code, signal) => finish({
      ...baseCheckResult(file, startedAt),
      status: code === 0 ? 'pass' : 'fail',
      code,
      signal: signal || null,
      stdout: stdout.trim(),
      stderr: stderr.trim(),
      error: null,
    }));
  });
}

async function runChecksParallel(files, root, jobs) {
  const results = new Array(files.length);
  let nextIndex = 0;
  async function worker() {
    while (nextIndex < files.length) {
      const index = nextIndex;
      nextIndex += 1;
      results[index] = await checkOneAsync(files[index], root);
    }
  }
  const workers = Array.from({ length: jobs }, () => worker());
  await Promise.all(workers);
  return results;
}

function buildReport({ root, files, results, jobs, options }) {
  const failures = results.filter((result) => result.status !== 'pass');
  return {
    schema: 'mcpace.nodeSyntaxCheck.v1',
    generatedAt: new Date().toISOString(),
    root,
    sourceRoots: SOURCE_ROOTS,
    status: failures.length === 0 && results.length === files.length ? 'pass' : 'fail',
    fileCount: files.length,
    checkedCount: results.length,
    failureCount: failures.length,
    jobs,
    failures: failures.map((result) => ({ file: result.file, code: result.code, signal: result.signal, stderr: result.stderr, error: result.error })),
    files: options.list ? files.map((file) => file.relativePath) : undefined,
  };
}

export function runSyntaxCheck(options = {}) {
  const root = options.root || repoRoot;
  const files = discoverNodeSourceFiles({ root });
  const jobs = 1;
  const results = [];
  for (const file of files) {
    const result = checkOne(file, root);
    results.push(result);
    if (options.failFast && result.status !== 'pass') break;
  }
  return buildReport({ root, files, results, jobs, options });
}

export async function runSyntaxCheckAsync(options = {}) {
  const root = options.root || repoRoot;
  const files = discoverNodeSourceFiles({ root });
  const jobs = resolveJobs(options, files.length);
  if (jobs === 1) return runSyntaxCheck({ ...options, root });
  const results = await runChecksParallel(files, root, jobs);
  return buildReport({ root, files, results, jobs, options });
}

function writeFileEnsuringDir(filePath, contents) {
  const target = path.resolve(filePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, contents, 'utf8');
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

async function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    if (parsed.help) { printHelp(); return; }
    const report = await runSyntaxCheckAsync(parsed);
    if (parsed.write) writeFileEnsuringDir(parsed.write, `${JSON.stringify(report, null, 2)}\n`);
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else {
      process.stdout.write(`[mcpace node-lint] ${report.status}: ${report.checkedCount}/${report.fileCount} files checked (jobs=${report.jobs})\n`);
      for (const failure of report.failures) process.stdout.write(`  fail: ${failure.file}\n`);
    }
    if (report.status !== 'pass') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
