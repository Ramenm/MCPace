#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..');
const manifestPath = path.join(repoRoot, 'release-targets.json');
const targetsPath = path.join(repoRoot, 'packages', 'npm', 'cli', 'lib', 'targets.js');
const cliPackagePath = path.join(repoRoot, 'packages', 'npm', 'cli', 'package.json');
const cargoPath = path.join(repoRoot, 'Cargo.toml');
const args = new Set(process.argv.slice(2));
const checkOnly = args.has('--check');

function readManifest() {
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  if (!Array.isArray(manifest.targets)) {
    throw new Error('release-targets.json must contain a targets array');
  }
  if (!Array.isArray(manifest.plannedTargets)) {
    throw new Error('release-targets.json must contain a plannedTargets array');
  }
  return manifest;
}

function normalizeTarget(target, publishEnabled) {
  const normalized = { ...target };
  if (!normalized.key) {
    throw new Error(`target is missing key: ${JSON.stringify(target)}`);
  }
  if (!normalized.platform || !normalized.arch) {
    throw new Error(`target ${normalized.key} is missing platform/arch`);
  }
  if (!normalized.binaryName) {
    normalized.binaryName = normalized.platform === 'win32' ? 'mcpace.exe' : 'mcpace';
  }
  normalized.publishEnabled = publishEnabled;
  normalized.nodePlatform = normalized.nodePlatform ?? normalized.platform;
  normalized.nodeArch = normalized.nodeArch ?? normalized.arch;
  return normalized;
}

function projectVersion() {
  const cargo = fs.readFileSync(cargoPath, 'utf8');
  const match = cargo.match(/^version\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error('Cargo.toml package version not found');
  }
  return match[1];
}

function packageNamesForOptionalDeps(manifest) {
  return manifest.targets
    .map((target) => target.packageName ?? target.npmPackage)
    .filter(Boolean)
    .sort();
}

function renderCliPackageJson(manifest) {
  const version = projectVersion();
  const pkg = JSON.parse(fs.readFileSync(cliPackagePath, 'utf8'));
  pkg.version = version;
  const existing = pkg.optionalDependencies ?? {};
  const next = {};
  for (const name of packageNamesForOptionalDeps(manifest)) {
    next[name] = version;
  }
  pkg.optionalDependencies = next;
  return `${JSON.stringify(pkg, null, 2)}\n`;
}

function renderTargetsModule(manifest) {
  const releaseTargets = [
    ...manifest.targets.map((target) => normalizeTarget(target, true)),
    ...manifest.plannedTargets.map((target) => normalizeTarget(target, false))
  ];
  return `// Generated from release-targets.json by scripts/sync-platform-packages.mjs.\n` +
    `// Do not edit by hand.\n` +
    `export const RELEASE_TARGETS = ${JSON.stringify(releaseTargets, null, 2)};\n\n` +
    `export const SUPPORTED_TARGETS = RELEASE_TARGETS.filter((target) => target.publishEnabled !== false);\n\n` +
    `export const PLANNED_TARGETS = RELEASE_TARGETS.filter((target) => target.publishEnabled === false);\n`;
}

function main() {
  const manifest = readManifest();
  const renderedTargets = renderTargetsModule(manifest);
  const renderedCliPackage = renderCliPackageJson(manifest);
  const currentTargets = fs.existsSync(targetsPath) ? fs.readFileSync(targetsPath, 'utf8') : '';
  const currentCliPackage = fs.existsSync(cliPackagePath) ? fs.readFileSync(cliPackagePath, 'utf8') : '';
  if (checkOnly) {
    const drifted = [];
    if (currentTargets !== renderedTargets) {
      drifted.push(path.relative(repoRoot, targetsPath));
    }
    if (currentCliPackage !== renderedCliPackage) {
      drifted.push(path.relative(repoRoot, cliPackagePath));
    }
    if (drifted.length > 0) {
      process.stderr.write(`${drifted.join(', ')} out of date. Run: npm run sync:targets\n`);
      process.exitCode = 1;
      return;
    }
    process.stdout.write('release target metadata and optionalDependencies are in sync\n');
    return;
  }
  fs.mkdirSync(path.dirname(targetsPath), { recursive: true });
  fs.writeFileSync(targetsPath, renderedTargets, 'utf8');
  fs.writeFileSync(cliPackagePath, renderedCliPackage, 'utf8');
  process.stdout.write(`wrote ${path.relative(repoRoot, targetsPath)} and ${path.relative(repoRoot, cliPackagePath)}\n`);
}

main();
