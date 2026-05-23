#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { extractTomlPackageName, extractTomlVersion, readRootPackageJson, repoRoot } from './lib/project-metadata.mjs';

const args = process.argv.slice(2);
const jsonOutput = args.includes('--json');
const dryRun = args.includes('--dry-run');

function argValue(name, fallback = null) {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] ?? fallback : fallback;
}

const outDir = path.resolve(argValue('--out-dir', path.join(repoRoot, '.artifacts')));
const timestampOverride = argValue('--timestamp', process.env.MCPACE_RELEASE_TIMESTAMP || null);
const forbiddenParts = new Set(['.git', 'node_modules', 'target', 'dist', '.cache', '.pytest_cache', '__pycache__']);
const forbiddenFiles = new Set(['.DS_Store', 'Thumbs.db']);

function kebabCase(value) {
  return String(value)
    .trim()
    .replace(/^@/, '')
    .replace(/[\\/]+/g, '-')
    .replace(/[^a-zA-Z0-9._-]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .toLowerCase();
}

function readText(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function projectMetadata() {
  const cargo = readText('Cargo.toml');
  const rootPackage = readRootPackageJson();
  const name = extractTomlPackageName(cargo) || rootPackage.name || 'mcpace';
  const version = extractTomlVersion(cargo) || rootPackage.version || '0.1.0';
  return { name: kebabCase(name), version };
}

function timestamp(now = new Date()) {
  if (timestampOverride) {
    if (!/^\d{6}-\d{6}$/.test(timestampOverride)) {
      throw new Error(`invalid --timestamp value '${timestampOverride}', expected ddmmyy-hhmmss`);
    }
    return timestampOverride;
  }
  const pad = (value) => String(value).padStart(2, '0');
  return `${pad(now.getDate())}${pad(now.getMonth() + 1)}${String(now.getFullYear()).slice(-2)}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
}

function readManifest() {
  const manifest = JSON.parse(readText('release-manifest.json'));
  if (!Array.isArray(manifest.includePaths)) {
    throw new Error('release-manifest.json must contain includePaths array');
  }
  return manifest;
}

function shouldSkip(relativePath) {
  const parts = relativePath.split(/[\\/]+/).filter(Boolean);
  return parts.some((part) => forbiddenParts.has(part)) || forbiddenFiles.has(path.basename(relativePath));
}

function copyPath(source, destination, relativePath) {
  if (shouldSkip(relativePath)) {
    return { skipped: [relativePath], copied: [] };
  }
  const stat = fs.statSync(source);
  if (stat.isDirectory()) {
    const copied = [];
    const skipped = [];
    fs.mkdirSync(destination, { recursive: true });
    for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
      const childRelative = path.posix.join(relativePath.split(path.sep).join('/'), entry.name);
      const child = copyPath(path.join(source, entry.name), path.join(destination, entry.name), childRelative);
      copied.push(...child.copied);
      skipped.push(...child.skipped);
    }
    return { copied, skipped };
  }
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.copyFileSync(source, destination);
  fs.chmodSync(destination, stat.mode & 0o777);
  return { copied: [relativePath.split(path.sep).join('/')], skipped: [] };
}

function walkFiles(root) {
  const files = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(full);
      } else {
        files.push(path.relative(root, full).split(path.sep).join('/'));
      }
    }
  }
  return files.sort();
}

function run(command, commandArgs, options = {}) {
  const result = spawnSync(command, commandArgs, {
    cwd: options.cwd ?? repoRoot,
    encoding: 'utf8',
    windowsHide: true,
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${commandArgs.join(' ')} failed\n${result.stderr || result.stdout}`.trim());
  }
  return result;
}

function build() {
  const { name, version } = projectMetadata();
  const stamp = timestamp();
  const rootName = `${name}-v${version}-${stamp}`;
  const archiveName = `${rootName}.zip`;
  const archivePath = path.join(outDir, archiveName);
  const manifestPath = path.join(outDir, `${rootName}.manifest.json`);
  const tempParent = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-release-'));
  const stagedRoot = path.join(tempParent, rootName);
  const manifest = readManifest();
  const copied = [];
  const skipped = [];
  const missing = [];

  for (const relativePath of manifest.includePaths) {
    const source = path.join(repoRoot, relativePath);
    if (!fs.existsSync(source)) {
      missing.push(relativePath);
      continue;
    }
    const result = copyPath(source, path.join(stagedRoot, relativePath), relativePath);
    copied.push(...result.copied);
    skipped.push(...result.skipped);
  }

  const required = ['README.md', 'docs/README.md', 'reports/summary.md', 'Cargo.toml', 'package.json'];
  const stagedFiles = walkFiles(stagedRoot);
  const missingRequired = required.filter((relativePath) => !stagedFiles.includes(relativePath));
  const forbiddenIncluded = stagedFiles.filter(shouldSkip);

  const verificationReport = {
    sourceProofStatus: missing.length === 0 && missingRequired.length === 0 && forbiddenIncluded.length === 0 ? 'pass' : 'failed',
    copiedFileCount: stagedFiles.length,
    skippedPaths: skipped.sort(),
    missingManifestPaths: missing.sort(),
    missingRequiredPaths: missingRequired.sort(),
    forbiddenIncludedPaths: forbiddenIncluded.sort(),
  };

  if (verificationReport.sourceProofStatus !== 'pass') {
    throw new Error(`source bundle verification failed: ${JSON.stringify(verificationReport, null, 2)}`);
  }

  fs.mkdirSync(outDir, { recursive: true });
  fs.writeFileSync(manifestPath, JSON.stringify({
    schema: 'mcpace.releaseArtifactManifest.v1',
    generatedAt: new Date().toISOString(),
    rootName,
    archiveName,
    sourceRoot: repoRoot,
    includePaths: manifest.includePaths,
    files: stagedFiles,
    verificationReport,
  }, null, 2) + '\n');

  if (!dryRun) {
    fs.rmSync(archivePath, { force: true });
    run('zip', ['-qr', archivePath, rootName], { cwd: tempParent });
    run('unzip', ['-tq', archivePath]);
  }

  fs.rmSync(tempParent, { recursive: true, force: true });
  return {
    schema: 'mcpace.releaseArtifactBuild.v1',
    status: 'pass',
    dryRun,
    rootName,
    archive: {
      name: archiveName,
      path: archivePath,
    },
    manifestPath,
    releaseProofStatus: dryRun ? 'dry-run' : 'pass',
    verificationReport,
  };
}

try {
  const result = build();
  if (jsonOutput) {
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  } else {
    process.stdout.write(`Built ${result.archive.path}\n`);
    process.stdout.write(`Manifest ${result.manifestPath}\n`);
  }
} catch (error) {
  if (jsonOutput) {
    process.stdout.write(`${JSON.stringify({
      schema: 'mcpace.releaseArtifactBuild.v1',
      status: 'failed',
      error: error?.message ?? String(error),
    }, null, 2)}\n`);
  } else {
    process.stderr.write(`${error?.stack ?? error}\n`);
  }
  process.exitCode = 1;
}
