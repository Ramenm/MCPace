#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { repoRoot } from './lib/project-metadata.mjs';

const TEST_ROOTS = Object.freeze([
  path.join('packages', 'npm', 'cli', 'test'),
  path.join('tests', 'node'),
]);
const here = path.dirname(fileURLToPath(import.meta.url));
const self = path.join(here, 'run-node-tests.mjs');

function collectTests() {
  const files = [];
  for (const relativeDir of TEST_ROOTS) {
    const dir = path.join(repoRoot, relativeDir);
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (entry.isFile() && entry.name.endsWith('.test.mjs')) {
        files.push(path.join(relativeDir, entry.name).split(path.sep).join('/'));
      }
    }
  }
  return files.sort();
}

function optionValue(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : '';
}

function integerOption(name, fallback) {
  const raw = optionValue(name);
  if (!raw) return fallback;
  const parsed = Number(raw);
  if (!Number.isSafeInteger(parsed) || parsed < 0) throw new Error(`${name} must be a non-negative integer`);
  return parsed;
}

const quiet = process.argv.includes('--quiet');
const noChunk = process.argv.includes('--no-chunk');
const chunkSize = integerOption('--chunk-size', 6);
const fromIndex = integerOption('--from-index', 0);
const toIndex = integerOption('--to-index', 0);
const allTests = collectTests();
const isChunkWorker = process.argv.includes('--from-index') || process.argv.includes('--to-index');

if (!isChunkWorker && !noChunk && allTests.length > chunkSize) {
  let passed = 0;
  for (let start = 0; start < allTests.length; start += chunkSize) {
    const end = Math.min(start + chunkSize, allTests.length);
    const args = [self, ...(quiet ? ['--quiet'] : []), '--no-chunk', '--from-index', String(start), '--to-index', String(end)];
    const result = spawnSync(process.execPath, args, {
      cwd: repoRoot,
      stdio: 'inherit',
      env: process.env,
      windowsHide: true,
    });
    if (result.status !== 0) process.exit(result.status || 1);
    passed += end - start;
  }
  process.stdout.write(`\nPASS node test files: ${passed}/${allTests.length}\n`);
  process.exit(0);
}

const tests = isChunkWorker ? allTests.slice(fromIndex, toIndex || allTests.length) : allTests;
let passed = 0;

function runTestFiles(files) {
  if (!quiet) {
    for (const file of files) process.stdout.write(`\n# MCPace node test file: ${file}\n`);
  }
  return spawnSync(process.execPath, ['--test', ...files], {
    cwd: repoRoot,
    encoding: quiet ? 'utf8' : undefined,
    stdio: quiet ? 'pipe' : 'inherit',
    env: process.env,
    maxBuffer: 16 * 1024 * 1024,
    windowsHide: true,
  });
}

if (quiet && tests.length > 1) {
  const result = runTestFiles(tests);
  if (result.status !== 0) {
    process.stderr.write(`\nFAIL node test chunk ${tests[0]}..${tests[tests.length - 1]} exited with ${result.status ?? result.signal}\n`);
    process.stderr.write(`${result.stdout || ''}${result.stderr || ''}`);
    process.exit(result.status || 1);
  }
  passed = tests.length;
  for (const file of tests) process.stdout.write(`PASS ${file}\n`);
} else {
  for (const file of tests) {
    const result = runTestFiles([file]);
    if (result.status !== 0) {
      process.stderr.write(`\nFAIL ${file} exited with ${result.status ?? result.signal}\n`);
      if (quiet) process.stderr.write(`${result.stdout || ''}${result.stderr || ''}`);
      process.exit(result.status || 1);
    }
    passed += 1;
    if (quiet) process.stdout.write(`PASS ${file}\n`);
  }
}

if (!isChunkWorker) process.stdout.write(`\nPASS node test files: ${passed}/${tests.length}\n`);
