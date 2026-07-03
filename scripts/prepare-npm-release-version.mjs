#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { repoRoot } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const args = { version: process.env.MCPACE_RELEASE_VERSION ?? null, json: argv.includes('--json') };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--version') args.version = argv[++index] ?? null;
    else if (arg === '--json') args.json = true;
    else if (arg === '--help' || arg === '-h') {
      console.log('Usage: node scripts/prepare-npm-release-version.mjs --version <semver>');
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!args.version) throw new Error('missing --version <semver>');
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(args.version)) {
    throw new Error(`invalid npm release version '${args.version}'`);
  }
  return args;
}

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

function writeJson(relativePath, value) {
  fs.writeFileSync(path.join(repoRoot, relativePath), `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function replaceTomlPackageVersion(relativePath, version) {
  const filePath = path.join(repoRoot, relativePath);
  const text = fs.readFileSync(filePath, 'utf8');
  const updated = text.replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`);
  if (updated === text) throw new Error(`${relativePath} does not contain a top-level package version`);
  fs.writeFileSync(filePath, updated, 'utf8');
}

function replaceCargoLockPackageVersion(version) {
  const relativePath = 'Cargo.lock';
  const filePath = path.join(repoRoot, relativePath);
  const text = fs.readFileSync(filePath, 'utf8');
  const updated = text.replace(/(name = "mcpace"\r?\nversion = )"[^"]+"/, `$1"${version}"`);
  if (updated === text) throw new Error(`${relativePath} does not contain the mcpace package version`);
  fs.writeFileSync(filePath, updated, 'utf8');
}

function updateOptionalDependencies(packageJson, version) {
  for (const name of Object.keys(packageJson.optionalDependencies ?? {})) {
    if (name.startsWith('@mcpace/cli-')) packageJson.optionalDependencies[name] = version;
  }
}

function updatePackageLock(version) {
  const lock = readJson('package-lock.json');
  if (lock.packages?.['']) lock.packages[''].version = version;
  const workspace = lock.packages?.['packages/npm/cli'];
  if (workspace) {
    workspace.version = version;
    updateOptionalDependencies(workspace, version);
  }
  writeJson('package-lock.json', lock);
}

function updateMcpaceConfig(version) {
  const config = readJson('mcpace.config.json');
  config.version = version;
  writeJson('mcpace.config.json', config);
}

function run() {
  const args = parseArgs(process.argv.slice(2));
  const version = args.version;

  const rootPackage = readJson('package.json');
  rootPackage.version = version;
  writeJson('package.json', rootPackage);

  const cliPackage = readJson('packages/npm/cli/package.json');
  cliPackage.version = version;
  updateOptionalDependencies(cliPackage, version);
  writeJson('packages/npm/cli/package.json', cliPackage);

  replaceTomlPackageVersion('Cargo.toml', version);
  replaceCargoLockPackageVersion(version);
  updatePackageLock(version);
  updateMcpaceConfig(version);

  const report = {
    schema: 'mcpace.releaseVersionPreparation.v1',
    version,
    updated: [
      'package.json',
      'packages/npm/cli/package.json',
      'Cargo.toml',
      'Cargo.lock',
      'package-lock.json',
      'mcpace.config.json',
    ],
  };
  if (args.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  else process.stdout.write(`Prepared MCPace release version ${version}\n`);
}

try {
  run();
} catch (error) {
  console.error(error?.stack ?? String(error));
  process.exitCode = 1;
}
