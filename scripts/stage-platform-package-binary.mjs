#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import {
  PLATFORM_PACKAGE_TARGETS,
  defaultCargoBinaryPath,
  platformPackageBinPath,
  platformPackageDir,
  targetByKey
} from './lib/npm-platform-packages.mjs';
import { repoRoot } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const parsed = { json: false, targetKey: null, binaryPath: null, clearBinDir: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--target-key':
        parsed.targetKey = argv[++index] || null;
        break;
      case '--binary-path':
        parsed.binaryPath = path.resolve(argv[++index] || '');
        break;
      case '--clear-bin-dir':
        parsed.clearBinDir = true;
        break;
      default:
        throw new Error(`unsupported stage-platform-package-binary argument: ${token}`);
    }
  }
  return parsed;
}

function requireTarget(targetKey) {
  const target = targetByKey(targetKey || '');
  if (!target || target.publishEnabled === false) {
    const supported = PLATFORM_PACKAGE_TARGETS.map((entry) => entry.key).join(', ');
    throw new Error(`unsupported or missing target key '${targetKey || ''}'. Supported targets: ${supported}.`);
  }
  return target;
}

export function stagePlatformPackageBinary(options = {}) {
  const target = requireTarget(options.targetKey);
  const sourcePath = options.binaryPath ? path.resolve(options.binaryPath) : defaultCargoBinaryPath(target);
  if (!fs.existsSync(sourcePath)) {
    throw new Error(`platform package binary source does not exist: ${sourcePath}`);
  }
  if (!fs.statSync(sourcePath).isFile()) {
    throw new Error(`platform package binary source is not a file: ${sourcePath}`);
  }

  const packageDir = platformPackageDir(target);
  const binDir = path.join(packageDir, 'bin');
  if (options.clearBinDir) {
    fs.rmSync(binDir, { recursive: true, force: true });
  }
  fs.mkdirSync(binDir, { recursive: true });

  const destinationPath = platformPackageBinPath(target);
  fs.copyFileSync(sourcePath, destinationPath);
  if (target.os[0] !== 'win32') {
    fs.chmodSync(destinationPath, 0o755);
  }

  return {
    targetKey: target.key,
    packageName: target.packageName,
    sourcePath,
    destinationPath,
    destinationRelative: path.relative(repoRoot, destinationPath).split(path.sep).join('/')
  };
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    const report = stagePlatformPackageBinary(parsed);
    if (parsed.json) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
      return;
    }
    process.stdout.write(`${report.destinationPath}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) {
  main();
}
