#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_OUTPUT_DIR = path.join(repoRoot, 'dist');
const FORBIDDEN_SEGMENTS = new Set([
  '.git',
  'node_modules',
  'target',
  '.DS_Store',
  'Thumbs.db',
  '__MACOSX'
]);

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

function readText(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function parseArgs(argv) {
  const parsed = {
    json: false,
    outputDir: DEFAULT_OUTPUT_DIR,
    projectName: null,
    version: null,
    stamp: process.env.MCPACE_ARCHIVE_TIMESTAMP || null
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
      case '--project-name':
        parsed.projectName = argv[++index] || null;
        break;
      case '--version':
        parsed.version = argv[++index] || null;
        break;
      case '--stamp':
        parsed.stamp = argv[++index] || null;
        break;
      default:
        throw new Error(`unsupported archive-release argument: ${token}`);
    }
  }

  return parsed;
}

function extractTomlPackageName(text) {
  const match = text.match(/^name\s*=\s*"([^"]+)"/m);
  return match ? match[1] : null;
}

function extractTomlVersion(text) {
  const match = text.match(/^version\s*=\s*"([^"]+)"/m);
  return match ? match[1] : null;
}

function toKebabCase(value) {
  return String(value)
    .trim()
    .toLowerCase()
    .replace(/^@/, '')
    .replace(/\//g, '-')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .replace(/-workspace$/, '')
    .replace(/-cli$/, '') || 'mcpace';
}

function deriveProjectName() {
  const cargoName = extractTomlPackageName(readText('Cargo.toml'));
  if (cargoName) {
    return toKebabCase(cargoName);
  }

  const rootPkgName = readJson('package.json').name;
  if (rootPkgName) {
    return toKebabCase(rootPkgName);
  }

  const readme = readText('README.md');
  const heading = readme.match(/^#\s+(.+)$/m);
  return toKebabCase(heading ? heading[1] : 'mcpace');
}

function deriveVersion() {
  return (
    extractTomlVersion(readText('Cargo.toml')) ||
    readJson('package.json').version ||
    '0.1.0'
  );
}

function formatStamp(value) {
  if (value) {
    if (!/^\d{6}-\d{6}$/.test(value)) {
      throw new Error(`invalid archive timestamp '${value}', expected ddmmyy-hhmmss`);
    }
    return value;
  }

  const now = new Date();
  const pad = (number) => String(number).padStart(2, '0');
  const dd = pad(now.getDate());
  const mm = pad(now.getMonth() + 1);
  const yy = pad(now.getFullYear() % 100);
  const hh = pad(now.getHours());
  const mi = pad(now.getMinutes());
  const ss = pad(now.getSeconds());
  return `${dd}${mm}${yy}-${hh}${mi}${ss}`;
}

function shouldSkipPath(relativePath) {
  return relativePath
    .split(path.sep)
    .some((segment) => FORBIDDEN_SEGMENTS.has(segment));
}

function copyTree(sourcePath, destinationPath, relativePath = '') {
  const stat = fs.statSync(sourcePath);
  if (shouldSkipPath(relativePath)) {
    return;
  }

  if (stat.isDirectory()) {
    fs.mkdirSync(destinationPath, { recursive: true });
    for (const entry of fs.readdirSync(sourcePath, { withFileTypes: true })) {
      const childSource = path.join(sourcePath, entry.name);
      const childDestination = path.join(destinationPath, entry.name);
      const childRelative = relativePath ? path.join(relativePath, entry.name) : entry.name;
      copyTree(childSource, childDestination, childRelative);
    }
    return;
  }

  fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
  fs.copyFileSync(sourcePath, destinationPath);
}

function stageArchive(rootName, includePaths) {
  const stagingParent = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-release-'));
  const stagedRoot = path.join(stagingParent, rootName);
  fs.mkdirSync(stagedRoot, { recursive: true });

  for (const relative of includePaths) {
    const sourcePath = path.join(repoRoot, relative);
    if (!fs.existsSync(sourcePath)) {
      throw new Error(`release manifest path does not exist: ${relative}`);
    }
    copyTree(sourcePath, path.join(stagedRoot, relative), relative);
  }

  for (const required of ['docs/README.md', 'reports/summary.md']) {
    if (!fs.existsSync(path.join(stagedRoot, required))) {
      throw new Error(`staged archive is missing required file: ${required}`);
    }
  }

  return { stagedRoot, stagingParent };
}

function buildArchive(parsed) {
  const manifest = readJson('release-manifest.json');
  const projectName = parsed.projectName || deriveProjectName();
  const version = parsed.version || deriveVersion();
  const stamp = formatStamp(parsed.stamp);
  const rootName = `${projectName}-v${version}-${stamp}`;
  const archiveName = `${rootName}.zip`;
  const outputDir = parsed.outputDir;
  const archivePath = path.join(outputDir, archiveName);

  fs.mkdirSync(outputDir, { recursive: true });
  if (fs.existsSync(archivePath)) {
    fs.rmSync(archivePath, { force: true });
  }

  const { stagingParent } = stageArchive(rootName, manifest.includePaths);
  const zipResult = createArchive(stagingParent, rootName, archivePath);
  const cleanup = () => fs.rmSync(stagingParent, { recursive: true, force: true });

  if (zipResult.status !== 0) {
    cleanup();
    throw new Error(readProcessFailure(zipResult, 'archive command failed'));
  }

  cleanup();

  return {
    projectName,
    version,
    stamp,
    rootName,
    archiveName,
    archivePath,
    includeCount: manifest.includePaths.length
  };
}

function createArchive(stagingParent, rootName, archivePath) {
  const zipResult = spawnSync('zip', ['-qr', archivePath, rootName], {
    cwd: stagingParent,
    encoding: 'utf8'
  });

  if (zipResult.status === 0 || process.platform !== 'win32') {
    return zipResult;
  }

  if (zipResult.error?.code !== 'ENOENT') {
    return zipResult;
  }

  const escapedRoot = rootName.replace(/'/g, "''");
  const escapedArchivePath = archivePath.replace(/'/g, "''");
  return spawnSync(
    'powershell.exe',
    [
      '-NoProfile',
      '-Command',
      `Compress-Archive -Path '${escapedRoot}' -DestinationPath '${escapedArchivePath}' -Force`
    ],
    {
      cwd: stagingParent,
      encoding: 'utf8'
    }
  );
}

function readProcessFailure(result, fallbackMessage) {
  if (result.error instanceof Error) {
    return result.error.message;
  }

  const stderr =
    typeof result.stderr === 'string' ? result.stderr.trim() : '';
  if (stderr) {
    return stderr;
  }

  const stdout =
    typeof result.stdout === 'string' ? result.stdout.trim() : '';
  if (stdout) {
    return stdout;
  }

  return fallbackMessage;
}

function main() {
  try {
    const result = buildArchive(parseArgs(process.argv.slice(2)));
    if (process.argv.includes('--json')) {
      process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
      return;
    }
    process.stdout.write(`${result.archivePath}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

main();
