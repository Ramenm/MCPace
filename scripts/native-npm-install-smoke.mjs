#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { commandForPlatform, runChecked } from './lib/process.mjs';
import { readJson, repoRoot } from './lib/project-metadata.mjs';

const args = process.argv.slice(2);

function usage() {
  return 'Usage: node scripts/native-npm-install-smoke.mjs --target <release-target-key> --main-tarball <@mcpace-cli.tgz> --native-tarball <native-package.tgz> [--json] [--keep-temp]';
}

function parseArgs(argv) {
  const parsed = { target: null, mainTarball: null, nativeTarball: null, json: false, keepTemp: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--target') parsed.target = argv[++index] ?? null;
    else if (arg === '--main-tarball') parsed.mainTarball = argv[++index] ?? null;
    else if (arg === '--native-tarball') parsed.nativeTarball = argv[++index] ?? null;
    else if (arg === '--json') parsed.json = true;
    else if (arg === '--keep-temp') parsed.keepTemp = true;
    else if (arg === '--help' || arg === '-h') parsed.help = true;
    else throw new Error(`unsupported argument: ${arg}`);
  }
  if (!parsed.help) {
    for (const [name, value] of Object.entries({ '--target': parsed.target, '--main-tarball': parsed.mainTarball, '--native-tarball': parsed.nativeTarball })) {
      if (!value) throw new Error(`${name} is required`);
    }
  }
  return parsed;
}

function regularFile(label, inputPath) {
  const filePath = path.resolve(inputPath);
  const stat = fs.lstatSync(filePath);
  if (!stat.isFile() || stat.isSymbolicLink()) throw new Error(`${label} must be a regular non-symlink file: ${filePath}`);
  return filePath;
}

function parseJson(label, stdout) {
  try {
    return JSON.parse(stdout);
  } catch (error) {
    throw new Error(`${label} did not return JSON: ${error.message}`);
  }
}

function pathInside(parent, child) {
  const relative = path.relative(parent, child);
  return relative === '' || (!relative.startsWith('..') && !path.isAbsolute(relative));
}

function installedLauncher(appDir) {
  const launcher = installedBin(appDir);
  const stat = fs.lstatSync(launcher);
  if (!stat.isFile() && !stat.isSymbolicLink()) throw new Error(`installed launcher command must be a file or symlink: ${launcher}`);
  const resolved = fs.realpathSync(launcher);
  const resolvedPrefix = fs.realpathSync(appDir);
  if (!fs.statSync(resolved).isFile() || !pathInside(resolvedPrefix, resolved)) {
    throw new Error(`installed launcher must resolve to a regular file inside its npm prefix: ${launcher}`);
  }
  return launcher;
}

function readPackageJson(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (error) {
    throw new Error(`invalid installed package JSON at ${filePath}: ${error?.message ?? String(error)}`, { cause: error });
  }
}

function requireCondition(condition, message) {
  if (!condition) throw new Error(message);
}

function installedBin(appDir) {
  return path.join(appDir, 'node_modules', '.bin', process.platform === 'win32' ? 'mcpace.cmd' : 'mcpace');
}

function runSmoke(parsed) {
  const releaseTargets = readJson('release-targets.json');
  const target = (releaseTargets.targets ?? []).find((candidate) => candidate.key === parsed.target && candidate.publishEnabled !== false);
  if (!target) throw new Error(`unknown or disabled release target: ${parsed.target}`);

  const mainTarball = regularFile('main tarball', parsed.mainTarball);
  const nativeTarball = regularFile('native tarball', parsed.nativeTarball);
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-native-npm-install-'));
  const appDir = path.join(tempDir, 'app');
  fs.mkdirSync(appDir, { recursive: true });
  fs.writeFileSync(path.join(appDir, 'package.json'), '{"private":true}\n', 'utf8');

  try {
    // Both tarballs are explicit and the registry is unreachable: this proves the launcher
    // resolves the just-built native optional package rather than an ambient global install.
    runChecked('npm', [
      'install', '--prefix', appDir, '--ignore-scripts', '--no-audit', '--no-fund',
      '--offline', '--registry', 'http://127.0.0.1:9', '--fetch-retries=0', '--fetch-timeout=1000',
      mainTarball, nativeTarball,
    ], { cwd: repoRoot, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });

    const mainPackage = readPackageJson(path.join(appDir, 'node_modules', '@mcpace', 'cli', 'package.json'));
    const nativePackagePath = path.join(appDir, 'node_modules', ...target.packageName.split('/'), 'package.json');
    const nativePackage = readPackageJson(nativePackagePath);
    requireCondition(mainPackage.optionalDependencies?.[target.packageName] === mainPackage.version, `launcher optional dependency does not pin ${target.packageName} to ${mainPackage.version}`);
    requireCondition(nativePackage.name === target.packageName, `installed native package name mismatch: ${nativePackage.name}`);
    requireCondition(nativePackage.version === mainPackage.version, `native package version ${nativePackage.version} did not match launcher ${mainPackage.version}`);
    requireCondition(nativePackage.mcpace?.target === target.key, `native package target mismatch: ${nativePackage.mcpace?.target}`);
    requireCondition(nativePackage.mcpace?.binaryName === target.binaryName, `native package binaryName mismatch: ${nativePackage.mcpace?.binaryName}`);

    const nativeRoot = path.dirname(nativePackagePath);
    const nativeBinary = path.join(nativeRoot, 'bin', target.binaryName);
    regularFile('installed native binary', nativeBinary);
    const launcher = installedLauncher(appDir);
    const launcherVersion = runChecked(commandForPlatform(launcher), ['--version'], {
      cwd: appDir,
      encoding: 'utf8',
      maxBuffer: 16 * 1024 * 1024,
    }).stdout.trim();
    requireCondition(launcherVersion === mainPackage.version, `launcher --version ${launcherVersion} did not match ${mainPackage.version}`);

    const runtime = parseJson('installed runtime smoke', runChecked(process.execPath, [
      'scripts/installer-runtime-smoke.mjs', '--binary', nativeBinary, '--command', launcher, '--json',
    ], {
      cwd: repoRoot,
      encoding: 'utf8',
      maxBuffer: 32 * 1024 * 1024,
    }).stdout);
    requireCondition(runtime.status === 'pass', `installed runtime smoke failed: ${runtime.error ?? 'unknown error'}`);

    return {
      schema: 'mcpace.nativeNpmInstallSmoke.v1',
      status: 'pass',
      platform: process.platform,
      arch: process.arch,
      target: target.key,
      mainPackage: { name: mainPackage.name, version: mainPackage.version },
      nativePackage: { name: nativePackage.name, version: nativePackage.version, binaryName: target.binaryName },
      launcher: { path: launcher, version: launcherVersion },
      runtime: { endpoint: runtime.up?.endpoint ?? null, toolCount: runtime.up?.toolCount ?? null },
      tempDir: parsed.keepTemp ? tempDir : null,
    };
  } finally {
    if (!parsed.keepTemp) fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

let parsed;
try {
  parsed = parseArgs(args);
  if (parsed.help) {
    process.stdout.write(`${usage()}\n`);
    process.exit(0);
  }
  const report = runSmoke(parsed);
  if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  else process.stdout.write(`PASS native npm install smoke: ${report.target} ${report.mainPackage.version}\n`);
} catch (error) {
  const report = {
    schema: 'mcpace.nativeNpmInstallSmoke.v1',
    status: 'failed',
    error: error?.message ?? String(error),
  };
  if (parsed?.json || args.includes('--json')) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  else process.stderr.write(`${report.error}\n`);
  process.exitCode = 1;
}
