#!/usr/bin/env node
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_OUTPUT_DIR = path.join(repoRoot, 'dist');
const DEFAULT_OUTPUT_FILENAME = 'SHA256SUMS.txt';

function normalizeReportPath(filePath) {
  const absolute = path.resolve(filePath);
  const relative = path.relative(repoRoot, absolute);
  return relative && !relative.startsWith('..') ? relative.split(path.sep).join('/') : absolute;
}

function sha256ForFile(filePath) {
  const hash = crypto.createHash('sha256');
  hash.update(fs.readFileSync(filePath));
  return hash.digest('hex');
}

function listRegularFiles(dir, outputPath, recursive = false, files = []) {
  if (!fs.existsSync(dir)) {
    return files;
  }

  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (recursive) {
        listRegularFiles(fullPath, outputPath, recursive, files);
      }
      continue;
    }
    if (!entry.isFile()) {
      continue;
    }
    if (path.resolve(fullPath) === path.resolve(outputPath)) {
      continue;
    }
    files.push(fullPath);
  }

  return files.sort((left, right) => left.localeCompare(right));
}

export function parseArgs(argv) {
  const parsed = {
    json: false,
    outputDir: DEFAULT_OUTPUT_DIR,
    outputPath: null,
    include: [],
    recursive: false
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--output-dir':
        parsed.outputDir = path.resolve(argv[++index] || '');
        break;
      case '--output-path':
        parsed.outputPath = path.resolve(argv[++index] || '');
        break;
      case '--include':
        parsed.include.push(path.resolve(argv[++index] || ''));
        break;
      case '--recursive':
        parsed.recursive = true;
        break;
      default:
        throw new Error(`unsupported generate-checksums argument: ${token}`);
    }
  }

  return parsed;
}

export function generateChecksums(options = {}) {
  const outputDir = path.resolve(options.outputDir || DEFAULT_OUTPUT_DIR);
  const outputPath = path.resolve(options.outputPath || path.join(outputDir, DEFAULT_OUTPUT_FILENAME));
  const include = Array.isArray(options.include) ? options.include.map((filePath) => path.resolve(filePath)) : [];
  const recursive = Boolean(options.recursive);
  const files = include.length > 0 ? include : listRegularFiles(outputDir, outputPath, recursive);

  if (files.length === 0) {
    throw new Error(`no files available for checksum generation under ${outputDir}`);
  }

  const entries = files.map((filePath) => {
    if (!fs.existsSync(filePath)) {
      throw new Error(`checksum source does not exist: ${filePath}`);
    }
    const stat = fs.statSync(filePath);
    if (!stat.isFile()) {
      throw new Error(`checksum source is not a file: ${filePath}`);
    }
    return {
      file: normalizeReportPath(filePath),
      name: path.relative(outputDir, filePath).split(path.sep).join('/'),
      sha256: sha256ForFile(filePath)
    };
  });

  const body = `${entries.map((entry) => `${entry.sha256}  ${entry.name}`).join('\n')}\n`;
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, body, 'utf8');

  return {
    outputPath: normalizeReportPath(outputPath),
    fileCount: entries.length,
    recursive,
    entries
  };
}

function isCliInvocation() {
  const entry = process.argv[1];
  if (!entry) {
    return false;
  }
  return pathToFileURL(path.resolve(entry)).href === import.meta.url;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = generateChecksums(parsed);
    if (parsed.json) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
      return;
    }
    process.stdout.write(`${report.outputPath}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) {
  main();
}
