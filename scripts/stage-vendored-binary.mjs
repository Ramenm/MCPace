#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import {
  SUPPORTED_TARGETS,
  binaryNameForTarget,
  currentTargetKey,
  describeSupportedTargets,
  detectTarget
} from '../packages/npm/cli/lib/platform.js';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_OUTPUT_DIR = path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor');

export function parseArgs(argv) {
  const parsed = {
    json: false,
    binaryPath: null,
    outputDir: DEFAULT_OUTPUT_DIR,
    targetKey: null,
    clearTargetDir: false
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--binary-path':
        parsed.binaryPath = path.resolve(argv[++index] || '');
        break;
      case '--output-dir':
        parsed.outputDir = path.resolve(argv[++index] || '');
        break;
      case '--target-key':
        parsed.targetKey = argv[++index] || null;
        break;
      case '--clear-target-dir':
        parsed.clearTargetDir = true;
        break;
      default:
        throw new Error(`unsupported stage-vendored-binary argument: ${token}`);
    }
  }

  return parsed;
}

function resolveTarget(targetKey = null) {
  if (targetKey) {
    const explicit = SUPPORTED_TARGETS.find((entry) => entry.key === targetKey);
    if (!explicit) {
      throw new Error(
        `unsupported target key '${targetKey}'. Supported targets: ${describeSupportedTargets()}.`
      );
    }
    return explicit;
  }

  const detected = detectTarget();
  if (!detected) {
    throw new Error(
      `unable to detect a supported target for ${currentTargetKey()}. ` +
        `Pass --target-key explicitly. Supported targets: ${describeSupportedTargets()}.`
    );
  }
  return detected;
}

export function stageVendoredBinary(options = {}) {
  const target = resolveTarget(options.targetKey || null);
  const binaryName = binaryNameForTarget(target);
  const sourcePath = options.binaryPath
    ? path.resolve(options.binaryPath)
    : path.join(repoRoot, 'target', 'release', binaryName);

  if (!fs.existsSync(sourcePath)) {
    throw new Error(`vendored binary source does not exist: ${sourcePath}`);
  }

  const stat = fs.statSync(sourcePath);
  if (!stat.isFile()) {
    throw new Error(`vendored binary source is not a file: ${sourcePath}`);
  }

  const outputDir = path.resolve(options.outputDir || DEFAULT_OUTPUT_DIR);
  const targetDir = path.join(outputDir, target.key);
  if (options.clearTargetDir) {
    fs.rmSync(targetDir, { recursive: true, force: true });
  }
  fs.mkdirSync(targetDir, { recursive: true });

  const destinationPath = path.join(targetDir, binaryName);
  fs.copyFileSync(sourcePath, destinationPath);
  if (target.platform !== 'win32') {
    fs.chmodSync(destinationPath, 0o755);
  }

  return {
    targetKey: target.key,
    binaryName,
    sourcePath,
    destinationPath,
    outputDir,
    destinationRelative: path.relative(repoRoot, destinationPath).split(path.sep).join('/')
  };
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
    const staged = stageVendoredBinary(parsed);
    if (parsed.json) {
      process.stdout.write(`${JSON.stringify(staged, null, 2)}\n`);
      return;
    }
    process.stdout.write(`${staged.destinationPath}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) {
  main();
}
