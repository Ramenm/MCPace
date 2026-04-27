import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import {
  binaryNameForPlatform,
  binaryNameForTarget,
  currentTargetKey,
  describeSupportedTargets,
  detectTarget,
  packageNamesForTarget
} from './platform.js';

const require = createRequire(import.meta.url);
const BIN_NAME = binaryNameForPlatform();

function isExecutable(filePath) {
  try {
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function packageRootFromHere() {
  const currentFile = fileURLToPath(import.meta.url);
  return path.resolve(path.dirname(currentFile), '..');
}

function repoRootFromHere() {
  const currentFile = fileURLToPath(import.meta.url);
  return path.resolve(path.dirname(currentFile), '..', '..', '..', '..');
}

function candidateDevBinaryPaths(repoRoot) {
  return [
    path.join(repoRoot, 'target', 'release', BIN_NAME),
    path.join(repoRoot, 'target', 'debug', BIN_NAME),
    path.join(repoRoot, 'dist', BIN_NAME)
  ];
}

function candidateVendoredBinaryPaths(repoRoot, packageRoot, target) {
  if (!target) {
    return [];
  }

  const binName = binaryNameForTarget(target);
  const unique = new Set();
  return [
    path.join(packageRoot, 'vendor', target.key, binName),
    path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor', target.key, binName)
  ].filter((candidate) => {
    const normalized = path.normalize(candidate);
    if (unique.has(normalized)) {
      return false;
    }
    unique.add(normalized);
    return true;
  });
}

function resolveExplicitEnvPath() {
  const fromEnv = process.env.MCPACE_BINARY_PATH || process.env.MCPACE_DEV_BINARY;
  if (!fromEnv) {
    return null;
  }
  const absolute = path.resolve(fromEnv);
  if (!fs.existsSync(absolute)) {
    throw new Error(`MCPACE binary path does not exist: ${absolute}`);
  }
  if (process.platform !== 'win32' && !isExecutable(absolute)) {
    throw new Error(`MCPACE binary path is not executable: ${absolute}`);
  }
  return absolute;
}

function resolveDevBinary(repoRoot) {
  for (const candidate of candidateDevBinaryPaths(repoRoot)) {
    if (fs.existsSync(candidate) && (process.platform === 'win32' || isExecutable(candidate))) {
      return candidate;
    }
  }
  return null;
}

function resolveVendoredBinary(repoRoot, packageRoot, target) {
  for (const candidate of candidateVendoredBinaryPaths(repoRoot, packageRoot, target)) {
    if (fs.existsSync(candidate) && (process.platform === 'win32' || isExecutable(candidate))) {
      return candidate;
    }
  }
  return null;
}

function resolveFromInstalledBinaryPackage(target) {
  const binName = binaryNameForTarget(target);
  for (const pkgName of packageNamesForTarget(target)) {
    try {
      const pkgJsonPath = require.resolve(`${pkgName}/package.json`);
      const dir = path.dirname(pkgJsonPath);
      const candidate = path.join(dir, 'bin', binName);
      if (fs.existsSync(candidate) && (process.platform === 'win32' || isExecutable(candidate))) {
        return candidate;
      }
    } catch {
      // future optional package not installed yet
    }
  }
  return null;
}

export function resolveBinary(options = {}) {
  const explicit = resolveExplicitEnvPath();
  if (explicit) {
    return explicit;
  }

  const repoRoot = options.repoRoot ? path.resolve(options.repoRoot) : repoRootFromHere();
  const packageRoot = options.packageRoot ? path.resolve(options.packageRoot) : packageRootFromHere();
  if (!options.ignoreDevBinary) {
    const devBinary = resolveDevBinary(repoRoot);
    if (devBinary) {
      return devBinary;
    }
  }

  const target = options.target ?? detectTarget();
  if (!options.ignoreVendoredBinary) {
    const vendoredBinary = resolveVendoredBinary(repoRoot, packageRoot, target);
    if (vendoredBinary) {
      return vendoredBinary;
    }
  }

  const packagedBinary = resolveFromInstalledBinaryPackage(target);
  if (packagedBinary) {
    return packagedBinary;
  }

  const supported = describeSupportedTargets();
  const targetKey = target?.key ?? currentTargetKey();
  throw new Error(
    `Unable to resolve the mcpace binary for target ${targetKey}. ` +
      `Set MCPACE_BINARY_PATH, build the Rust binary locally, stage a vendored binary, or install a supported package. ` +
      `Supported targets: ${supported}.`
  );
}

export function createExecutableFixture(filePath, contents = `#!/usr/bin/env sh\necho fixture\n`) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, contents, 'utf8');
  if (process.platform !== 'win32') {
    fs.chmodSync(filePath, 0o755);
  }
  return filePath;
}
