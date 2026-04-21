import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { currentTargetKey, describeSupportedTargets, detectTarget, packageNamesForTarget } from './platform.js';

const require = createRequire(import.meta.url);
const BIN_NAME = process.platform === 'win32' ? 'mcpace.exe' : 'mcpace';

function isExecutable(filePath) {
  try {
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
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

function resolveFromInstalledBinaryPackage(target) {
  for (const pkgName of packageNamesForTarget(target)) {
    try {
      const pkgJsonPath = require.resolve(`${pkgName}/package.json`);
      const dir = path.dirname(pkgJsonPath);
      const candidate = path.join(dir, 'bin', BIN_NAME);
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
  if (!options.ignoreDevBinary) {
    const devBinary = resolveDevBinary(repoRoot);
    if (devBinary) {
      return devBinary;
    }
  }

  const target = detectTarget();
  const packagedBinary = resolveFromInstalledBinaryPackage(target);
  if (packagedBinary) {
    return packagedBinary;
  }

  const supported = describeSupportedTargets();
  const targetKey = target?.key ?? currentTargetKey();
  throw new Error(
    `Unable to resolve the mcpace binary for target ${targetKey}. ` +
      `Set MCPACE_BINARY_PATH, build the Rust binary locally, or install a supported package. ` +
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
