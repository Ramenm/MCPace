#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';
import {
  PLATFORM_PACKAGE_TARGETS,
  expectedOptionalDependencies,
  platformPackageBinPath,
  platformPackageDir,
  platformPackageJson,
  targetByKey
} from './lib/npm-platform-packages.mjs';

const NPM_COMMAND = process.platform === 'win32' ? 'cmd.exe' : 'npm';

function parseArgs(argv) {
  const parsed = { json: false, targetKey: null, requireBinaries: false, packDryRun: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--target-key': parsed.targetKey = argv[++index] || null; break;
      case '--require-binaries': parsed.requireBinaries = true; break;
      case '--pack-dry-run': parsed.packDryRun = true; break;
      default: throw new Error(`unsupported verify-platform-packages argument: ${token}`);
    }
  }
  return parsed;
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function sameJson(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function cleanChildEnv() {
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;
  return env;
}

function npmCommandArgs(args) {
  return process.platform === 'win32' ? ['/d', '/s', '/c', 'npm', ...args] : args;
}

function isExecutable(filePath, target) {
  if (target.os[0] === 'win32') {
    return true;
  }
  try {
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function npmPackDryRun(target) {
  const packageName = target.packageName;
  const packagePath = path.relative(repoRoot, platformPackageDir(target));
  const result = spawnSync(NPM_COMMAND, npmCommandArgs(['pack', packagePath, '--json', '--dry-run']), {
    cwd: repoRoot,
    encoding: 'utf8',
    env: cleanChildEnv(),
    timeout: 120000,
    windowsHide: true
  });
  if (result.status !== 0) {
    return { status: 'fail', reason: result.stderr || result.stdout || result.error?.message || `npm pack failed with ${result.status}` };
  }
  try {
    const parsed = JSON.parse(result.stdout);
    const entry = Array.isArray(parsed) ? parsed[0] : parsed;
    return { status: 'pass', filename: entry?.filename || null, files: Array.isArray(entry?.files) ? entry.files.map((file) => file.path) : [] };
  } catch (error) {
    return { status: 'fail', reason: `failed to parse npm pack output: ${error instanceof Error ? error.message : String(error)}` };
  }
}

function verifyOneTarget(target, version, options) {
  const issues = [];
  const packageDir = platformPackageDir(target);
  const packageJsonPath = path.join(packageDir, 'package.json');
  const readmePath = path.join(packageDir, 'README.md');
  const licensePath = path.join(packageDir, 'LICENSE');
  const binaryPath = platformPackageBinPath(target);

  if (!fs.existsSync(packageJsonPath)) {
    issues.push(`missing ${path.relative(repoRoot, packageJsonPath)}`);
  } else {
    const actual = readJson(packageJsonPath);
    const expected = platformPackageJson(target, version);
    for (const key of ['name', 'version', 'description', 'license', 'os', 'cpu', 'libc', 'files', 'engines', 'publishConfig']) {
      if (!sameJson(actual[key] ?? null, expected[key] ?? null)) {
        issues.push(`${target.key} package.json ${key} drift`);
      }
    }
  }

  if (!fs.existsSync(readmePath)) issues.push(`missing ${path.relative(repoRoot, readmePath)}`);
  if (!fs.existsSync(licensePath)) issues.push(`missing ${path.relative(repoRoot, licensePath)}`);

  const binaryExists = fs.existsSync(binaryPath);
  if (options.requireBinaries && !binaryExists) issues.push(`missing platform binary ${path.relative(repoRoot, binaryPath)}`);
  if (binaryExists && !fs.statSync(binaryPath).isFile()) issues.push(`platform binary is not a file ${path.relative(repoRoot, binaryPath)}`);
  if (binaryExists && !isExecutable(binaryPath, target)) issues.push(`platform binary is not executable ${path.relative(repoRoot, binaryPath)}`);

  let pack = null;
  if (options.packDryRun || options.requireBinaries) {
    pack = npmPackDryRun(target);
    if (pack.status !== 'pass') {
      issues.push(`npm pack dry-run failed for ${target.packageName}: ${pack.reason}`);
    } else if (binaryExists) {
      const expectedBinary = `bin/${target.binaryName}`;
      if (!pack.files.includes(expectedBinary)) issues.push(`npm pack omitted ${expectedBinary} for ${target.packageName}`);
    }
  }

  return {
    targetKey: target.key,
    packageName: target.packageName,
    packageDir: path.relative(repoRoot, packageDir).split(path.sep).join('/'),
    binaryPresent: binaryExists,
    binaryPath: path.relative(repoRoot, binaryPath).split(path.sep).join('/'),
    pack,
    status: issues.length === 0 ? 'pass' : 'fail',
    issues
  };
}

export function verifyPlatformPackages(options = {}) {
  const version = deriveProjectVersion();
  const targets = options.targetKey ? [targetByKey(options.targetKey)].filter((target) => target?.publishEnabled !== false) : PLATFORM_PACKAGE_TARGETS;
  if (targets.length === 0) throw new Error(`unsupported target key '${options.targetKey}'`);

  const issues = [];
  const cliPackage = readJson(path.join(repoRoot, 'packages', 'npm', 'cli', 'package.json'));
  const expectedOptional = expectedOptionalDependencies(version);
  if (!sameJson(cliPackage.optionalDependencies || {}, expectedOptional)) {
    issues.push('packages/npm/cli/package.json optionalDependencies drift');
  }

  const packages = targets.map((target) => verifyOneTarget(target, version, options));
  for (const packageReport of packages) issues.push(...packageReport.issues);

  return {
    version,
    status: issues.length === 0 ? 'pass' : 'fail',
    requireBinaries: Boolean(options.requireBinaries),
    checkedTargets: packages.map((entry) => entry.targetKey),
    issues,
    packages
  };
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = verifyPlatformPackages(parsed);
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else if (report.status === 'pass') process.stdout.write(`platform packages verified for ${report.checkedTargets.join(', ')}\n`);
    else process.stderr.write(`${report.issues.join('\n')}\n`);
    if (report.status !== 'pass') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
