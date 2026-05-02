#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { repoRoot } from './lib/project-metadata.mjs';
import { cleanChildEnv } from './lib/safe-child-env.mjs';

export const SOURCE_ROOTS = Object.freeze(['packages/npm/cli', 'scripts', 'tests/node', 'tests/fixtures', 'examples']);
const NODE_EXTENSIONS = new Set(['.js', '.mjs']);
const SKIP_DIRS = new Set(['.git', 'node_modules', 'target', 'dist', 'vendor', 'data', 'logs', 'backups']);
const SKIP_PREFIXES = ['.tmp-', 'tmp-'];

function parseArgs(argv) {
  const parsed = { json: false, write: null, list: false, failFast: false, root: repoRoot, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; if (!parsed.write) throw new Error('check-node-syntax requires a path after --write'); break;
      case '--list': parsed.list = true; break;
      case '--fail-fast': parsed.failFast = true; break;
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
  process.stdout.write('Usage: node scripts/check-node-syntax.mjs [--json] [--write <path>] [--list] [--fail-fast] [--root <path>]\n\nAuto-discovers project JavaScript/MJS files and runs `node --check` on each one. This keeps package.json from hardcoding every Node source file.\n');
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
    file: file.relativePath,
    status: result.status === 0 ? 'pass' : 'fail',
    code: result.status,
    durationMs: Date.now() - startedAt,
    stdout: (result.stdout || '').trim(),
    stderr: (result.stderr || '').trim(),
    error: result.error ? result.error.message || String(result.error) : null,
  };
}

export function runSyntaxCheck(options = {}) {
  const root = options.root || repoRoot;
  const files = discoverNodeSourceFiles({ root });
  const results = [];
  for (const file of files) {
    const result = checkOne(file, root);
    results.push(result);
    if (options.failFast && result.status !== 'pass') break;
  }
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
    failures: failures.map((result) => ({ file: result.file, code: result.code, stderr: result.stderr, error: result.error })),
    files: options.list ? files.map((file) => file.relativePath) : undefined,
  };
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

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    if (parsed.help) { printHelp(); return; }
    const report = runSyntaxCheck(parsed);
    if (parsed.write) writeFileEnsuringDir(parsed.write, `${JSON.stringify(report, null, 2)}\n`);
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else {
      process.stdout.write(`[mcpace node-lint] ${report.status}: ${report.checkedCount}/${report.fileCount} files checked\n`);
      for (const failure of report.failures) process.stdout.write(`  fail: ${failure.file}\n`);
    }
    if (report.status !== 'pass') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
