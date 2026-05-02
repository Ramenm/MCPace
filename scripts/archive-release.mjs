#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import {
  deriveProjectName,
  deriveProjectVersion,
  readJson,
  repoRoot
} from './lib/project-metadata.mjs';
import { cleanChildEnv } from './lib/safe-child-env.mjs';
const DEFAULT_OUTPUT_DIR = path.join(repoRoot, 'dist');
const DEFAULT_ARCHIVE_COMMAND_TIMEOUT_MS = 120000;
const ARCHIVE_COMMAND_TIMEOUT_MS = parseTimeoutEnv(
  'MCPACE_ARCHIVE_COMMAND_TIMEOUT_MS',
  DEFAULT_ARCHIVE_COMMAND_TIMEOUT_MS
);
const FORBIDDEN_SEGMENTS = new Set([
  '.git',
  'node_modules',
  'target',
  '.DS_Store',
  'Thumbs.db',
  '__MACOSX'
]);

function parseTimeoutEnv(name, fallback) {
  const parsed = Number.parseInt(process.env[name] || '', 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
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

function normalizeManifestPaths(value, fieldName) {
  if (value == null) {
    return [];
  }
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== 'string' || !entry.trim())) {
    throw new Error(`release manifest ${fieldName} must be an array of non-empty strings`);
  }
  return value;
}

function collectManifestIncludePaths(manifest) {
  const includePaths = normalizeManifestPaths(manifest.includePaths, 'includePaths');
  const optionalIncludePaths = normalizeManifestPaths(
    manifest.optionalIncludePaths,
    'optionalIncludePaths'
  );
  const includedOptionalPaths = optionalIncludePaths.filter((relative) =>
    fs.existsSync(path.join(repoRoot, relative))
  );
  return {
    includePaths: [...includePaths, ...includedOptionalPaths],
    includedOptionalPaths
  };
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
  const manifestPaths = collectManifestIncludePaths(manifest);
  const projectName = parsed.projectName || deriveProjectName();
  const version = parsed.version || deriveProjectVersion();
  const stamp = formatStamp(parsed.stamp);
  const rootName = `${projectName}-v${version}-${stamp}`;
  const archiveName = `${rootName}.zip`;
  const outputDir = parsed.outputDir;
  const archivePath = path.join(outputDir, archiveName);

  fs.mkdirSync(outputDir, { recursive: true });
  if (fs.existsSync(archivePath)) {
    fs.rmSync(archivePath, { force: true });
  }

  const { stagingParent } = stageArchive(rootName, manifestPaths.includePaths);
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
    includeCount: manifestPaths.includePaths.length,
    includedOptionalPaths: manifestPaths.includedOptionalPaths
  };
}

function createArchive(stagingParent, rootName, archivePath) {
  if (process.platform === 'win32') {
    return createArchiveWithPowerShell(stagingParent, rootName, archivePath);
  }

  const zipResult = spawnSync('zip', ['-qr', archivePath, rootName], {
    cwd: stagingParent,
    encoding: 'utf8',
    env: cleanChildEnv(),
    timeout: ARCHIVE_COMMAND_TIMEOUT_MS,
    windowsHide: true
  });

  return zipResult;
}

function createArchiveWithPowerShell(stagingParent, rootName, archivePath) {
  const escapedStagingParent = stagingParent.replace(/'/g, "''");
  const escapedRoot = rootName.replace(/'/g, "''");
  const escapedArchivePath = archivePath.replace(/'/g, "''");
  return spawnSync(
    'powershell.exe',
    [
      '-NoProfile',
      '-Command',
      [
        'Add-Type -AssemblyName System.IO.Compression',
        'Add-Type -AssemblyName System.IO.Compression.FileSystem',
        `$staging = '${escapedStagingParent}'`,
        `$rootName = '${escapedRoot}'`,
        `$archivePath = '${escapedArchivePath}'`,
        '$root = Join-Path $staging $rootName',
        'if (Test-Path -LiteralPath $archivePath) { Remove-Item -LiteralPath $archivePath -Force }',
        '$archive = [System.IO.Compression.ZipFile]::Open($archivePath, [System.IO.Compression.ZipArchiveMode]::Create)',
        'try {',
        '  $prefix = $staging.TrimEnd([char[]]@([char]92, [char]47)) + [System.IO.Path]::DirectorySeparatorChar',
        '  Get-ChildItem -LiteralPath $root -Recurse -File | ForEach-Object {',
        '    $relative = $_.FullName.Substring($prefix.Length)',
        "    $entry = $relative -replace '\\\\', '/'",
        '    [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $_.FullName, $entry, [System.IO.Compression.CompressionLevel]::Optimal) | Out-Null',
        '  }',
        '} finally {',
        '  $archive.Dispose()',
        '}'
      ].join('; ')
    ],
    {
      cwd: stagingParent,
      encoding: 'utf8',
      env: cleanChildEnv(),
      timeout: ARCHIVE_COMMAND_TIMEOUT_MS,
      windowsHide: true
    }
  );
}

function readProcessFailure(result, fallbackMessage) {
  if (result.error instanceof Error) {
    return result.error.code === 'ETIMEDOUT'
      ? `archive command timed out after ${ARCHIVE_COMMAND_TIMEOUT_MS}ms`
      : result.error.message;
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
