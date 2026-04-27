#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
import {
  PLATFORM_PACKAGE_TARGETS,
  ensurePlatformPackageScaffold,
  expectedOptionalDependencies
} from './lib/npm-platform-packages.mjs';
import { allReleaseTargets, releaseTargetsManifest } from './lib/release-targets.mjs';

function parseArgs(argv) {
  const parsed = { json: false, repositoryUrl: undefined };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--repository-url':
        parsed.repositoryUrl = normalizeRepositoryUrl(argv[++index] || '');
        break;
      default:
        throw new Error(`unsupported sync-platform-packages argument: ${token}`);
    }
  }
  return parsed;
}

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

function writeJson(relativePath, value) {
  fs.writeFileSync(path.join(repoRoot, relativePath), `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function normalizeRepositoryUrl(value) {
  const trimmed = String(value || '').trim();
  if (!trimmed) {
    return null;
  }
  return trimmed.replace(/^git\+/, '').replace(/\.git$/i, '');
}

function applyRepository(manifest, repositoryUrl, directory) {
  if (repositoryUrl) {
    manifest.repository = {
      type: 'git',
      url: repositoryUrl,
      directory
    };
  } else {
    delete manifest.repository;
  }
}

function targetsModuleText() {
  const manifest = releaseTargetsManifest();
  const targets = allReleaseTargets(manifest);
  return [
    '// Generated from release-targets.json by scripts/sync-platform-packages.mjs.',
    '// Do not edit by hand.',
    `export const RELEASE_TARGETS = ${JSON.stringify(targets, null, 2)};`,
    '',
    'export const SUPPORTED_TARGETS = RELEASE_TARGETS.filter((target) => target.publishEnabled !== false);',
    '',
    'export const PLANNED_TARGETS = RELEASE_TARGETS.filter((target) => target.publishEnabled === false);',
    ''
  ].join('\n');
}

export function syncPlatformPackages(options = {}) {
  const version = deriveProjectVersion();
  const cliPackagePath = path.join('packages', 'npm', 'cli', 'package.json');
  const cliPackageJson = readJson(cliPackagePath);
  const repositoryUrl = Object.prototype.hasOwnProperty.call(options, 'repositoryUrl')
    ? normalizeRepositoryUrl(options.repositoryUrl)
    : null;
  const packageDirs = PLATFORM_PACKAGE_TARGETS.map((target) =>
    ensurePlatformPackageScaffold(target, version, { repositoryUrl })
  );

  cliPackageJson.optionalDependencies = expectedOptionalDependencies(version);
  cliPackageJson.exports = {
    ...cliPackageJson.exports,
    './targets': './lib/targets.js'
  };
  cliPackageJson.mcpace = {
    targetManifest: 'release-targets.json'
  };
  applyRepository(cliPackageJson, repositoryUrl, 'packages/npm/cli');
  writeJson(cliPackagePath, cliPackageJson);

  const targetsModulePath = path.join('packages', 'npm', 'cli', 'lib', 'targets.js');
  fs.writeFileSync(path.join(repoRoot, targetsModulePath), targetsModuleText(), 'utf8');

  return {
    version,
    platformPackageCount: PLATFORM_PACKAGE_TARGETS.length,
    platformPackages: PLATFORM_PACKAGE_TARGETS.map((target) => ({
      key: target.key,
      packageName: target.packageName,
      triple: target.triple,
      runner: target.runner,
      packageRepositoryUrl: repositoryUrl,
      directory: path.relative(repoRoot, packageDirs.find((dir) => dir.endsWith(`cli-${target.key}`))).split(path.sep).join('/')
    })),
    mainPackage: cliPackagePath,
    targetsModule: targetsModulePath
  };
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = syncPlatformPackages(parsed);
    if (parsed.json) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
      return;
    }
    process.stdout.write(`synced ${report.platformPackageCount} platform packages for ${report.version}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) {
  main();
}
