#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { repoRoot, deriveProjectVersion } from './lib/project-metadata.mjs';
import { cleanChildEnv } from './lib/safe-child-env.mjs';

const DEFAULT_WORKSPACE = '@mcpace/cli';
const DEFAULT_NPM_PACK_TIMEOUT_MS = 120000;
const NPM_PACK_TIMEOUT_MS = parseTimeoutEnv('MCPACE_NPM_PACK_TIMEOUT_MS', DEFAULT_NPM_PACK_TIMEOUT_MS);
const NPM_COMMAND = process.platform === 'win32' ? 'cmd.exe' : 'npm';
const REQUIRED_FILES = [
  'README.md',
  'LICENSE',
  'package.json',
  'bin/mcpace.js',
  'lib/platform.js',
  'lib/resolve-binary.js',
  'lib/runtime.js'
];

function parseTimeoutEnv(name, fallback) {
  const parsed = Number.parseInt(process.env[name] || '', 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}


function npmCommandArgs(args) {
  return process.platform === 'win32' ? ['/d', '/s', '/c', 'npm', ...args] : args;
}

function normalizeReportPath(filePath) {
  const absolute = path.resolve(filePath);
  const relative = path.relative(repoRoot, absolute);
  return relative && !relative.startsWith('..') ? relative.split(path.sep).join('/') : absolute;
}

function summarizeOutput(stdout = '', stderr = '') {
  const combined = [stdout, stderr].filter(Boolean).join('\n').trim();
  if (!combined) {
    return null;
  }
  return combined.split(/\r?\n/).slice(0, 20).join('\n');
}

function listRepoVendoredBinaryFiles() {
  const vendorRoot = path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor');
  const files = [];

  if (!fs.existsSync(vendorRoot)) {
    return files;
  }

  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(fullPath);
      } else {
        files.push(path.relative(path.join(repoRoot, 'packages', 'npm', 'cli'), fullPath).split(path.sep).join('/'));
      }
    }
  };

  walk(vendorRoot);
  return files.sort();
}

function parsePackJson(output) {
  const parsed = JSON.parse(output);
  if (!Array.isArray(parsed) || parsed.length !== 1 || typeof parsed[0] !== 'object' || !parsed[0]) {
    throw new Error('npm pack --json must return a single package entry');
  }
  return parsed[0];
}

export function parseArgs(argv) {
  const parsed = {
    json: false,
    workspace: DEFAULT_WORKSPACE
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--workspace':
        parsed.workspace = argv[++index] || DEFAULT_WORKSPACE;
        break;
      default:
        throw new Error(`unsupported verify-npm-pack argument: ${token}`);
    }
  }

  return parsed;
}

export function verifyNpmPack(options = {}) {
  const workspace = options.workspace || DEFAULT_WORKSPACE;
  const command = ['pack', '--workspace', workspace, '--json', '--dry-run'];
  const result = spawnSync(NPM_COMMAND, npmCommandArgs(command), {
    cwd: repoRoot,
    encoding: 'utf8',
    env: cleanChildEnv(),
    timeout: NPM_PACK_TIMEOUT_MS,
    windowsHide: true
  });

  const report = {
    status: 'fail',
    workspace,
    command: `npm ${command.join(' ')}`,
    packageMode: 'unknown',
    requiredFiles: [...REQUIRED_FILES],
    repoVendoredBinaryFiles: listRepoVendoredBinaryFiles()
  };

  if (result.error?.code === 'ETIMEDOUT') {
    report.reason = `npm pack timed out after ${NPM_PACK_TIMEOUT_MS}ms`;
    return report;
  }

  if (result.status !== 0) {
    report.reason = summarizeOutput(result.stdout, result.stderr) || result.error?.message || 'npm pack failed';
    return report;
  }

  let packInfo;
  try {
    packInfo = parsePackJson(result.stdout);
  } catch (error) {
    report.reason = `failed to parse npm pack output: ${error instanceof Error ? error.message : String(error)}`;
    return report;
  }

  const filePaths = Array.isArray(packInfo.files)
    ? packInfo.files
        .map((entry) => (entry && typeof entry.path === 'string' ? entry.path : null))
        .filter(Boolean)
        .sort()
    : [];
  const filePathSet = new Set(filePaths);
  const missingFiles = REQUIRED_FILES.filter((relativePath) => !filePathSet.has(relativePath));
  const packedVendoredBinaryFiles = filePaths.filter((relativePath) => relativePath.startsWith('vendor/'));
  const missingVendoredBinaryFiles = report.repoVendoredBinaryFiles.filter(
    (relativePath) => !filePathSet.has(relativePath)
  );
  const packageVersion = typeof packInfo.version === 'string' ? packInfo.version : null;
  const expectedVersion = deriveProjectVersion();

  report.packageName = packInfo.name || workspace;
  report.packageVersion = packageVersion;
  report.expectedVersion = expectedVersion;
  report.packFilename = packInfo.filename || null;
  report.entryCount = Number(packInfo.entryCount || filePaths.length || 0);
  report.unpackedSize = Number(packInfo.unpackedSize || 0);
  report.files = filePaths;
  report.missingFiles = missingFiles;
  report.packedVendoredBinaryFiles = packedVendoredBinaryFiles;
  report.missingVendoredBinaryFiles = missingVendoredBinaryFiles;
  report.packageMode = packedVendoredBinaryFiles.length > 0 ? 'vendored-binary-bundle' : 'thin-launcher';

  if (!packageVersion || packageVersion !== expectedVersion) {
    report.reason = `npm pack version drift: expected ${expectedVersion}, got ${packageVersion || 'missing version'}`;
    return report;
  }

  if (missingFiles.length > 0) {
    report.reason = `npm pack is missing required files: ${missingFiles.join(', ')}`;
    return report;
  }

  if (missingVendoredBinaryFiles.length > 0) {
    report.reason = `npm pack omitted staged vendored binaries: ${missingVendoredBinaryFiles.join(', ')}`;
    return report;
  }

  report.status = 'pass';
  return report;
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
    const report = verifyNpmPack(parsed);
    if (parsed.json) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    } else if (report.status === 'pass') {
      process.stdout.write(`${report.packFilename || report.workspace}\n`);
    } else {
      process.stderr.write(`${report.reason}\n`);
    }

    if (report.status !== 'pass') {
      process.exit(1);
    }
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) {
  main();
}
