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

function executableMode(stat) {
  return (Number(stat.mode) & 0o111) !== 0;
}

function binaryPathProblem(filePath) {
  let stat;
  try {
    stat = fs.lstatSync(filePath);
  } catch (error) {
    if (error?.code === 'ENOENT' || error?.code === 'ENOTDIR') return 'does not exist';
    return error?.message ?? String(error);
  }
  if (stat.isSymbolicLink()) return 'is a symbolic link';
  if (!stat.isFile()) return 'is not a file';
  if (process.platform !== 'win32' && !executableMode(stat)) return 'is not executable';
  return null;
}

function isUsableBinaryFile(filePath) {
  return binaryPathProblem(filePath) === null;
}

function unquoteExplicitEnvPath(value) {
  const trimmed = String(value || '').trim();
  if (trimmed.length >= 2) {
    const first = trimmed[0];
    const last = trimmed[trimmed.length - 1];
    if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
      return trimmed.slice(1, -1);
    }
  }
  return trimmed;
}

function packageRootFromHere() {
  const currentFile = fileURLToPath(import.meta.url);
  return path.resolve(path.dirname(currentFile), '..');
}

function repoRootFromHere() {
  const currentFile = fileURLToPath(import.meta.url);
  return path.resolve(path.dirname(currentFile), '..', '..', '..', '..');
}

function stableStatFingerprint(stat) {
  return {
    dev: stat.dev,
    ino: stat.ino,
    size: stat.size,
    mtimeMs: stat.mtimeMs,
    ctimeMs: stat.ctimeMs,
  };
}

function sameStableFingerprint(left, right) {
  return (
    left.dev === right.dev &&
    left.ino === right.ino &&
    left.size === right.size &&
    left.mtimeMs === right.mtimeMs &&
    left.ctimeMs === right.ctimeMs
  );
}

function readRegularTextFileStable(filePath, label = 'file') {
  let linkStat;
  try {
    linkStat = fs.lstatSync(filePath);
  } catch (error) {
    throw new Error(`${label} cannot be inspected: ${error?.message || error}`);
  }
  if (linkStat.isSymbolicLink()) {
    throw new Error(`${label} must not be a symbolic link: ${filePath}`);
  }
  if (!linkStat.isFile()) {
    throw new Error(`${label} must be a regular file: ${filePath}`);
  }

  const openFlags = fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW ?? 0);
  const fd = fs.openSync(filePath, openFlags);
  try {
    const before = stableStatFingerprint(fs.fstatSync(fd));
    if (before.size > 1024 * 1024) {
      throw new Error(`${label} is unexpectedly large: ${filePath}`);
    }
    const text = fs.readFileSync(fd, 'utf8');
    const after = stableStatFingerprint(fs.fstatSync(fd));
    if (!sameStableFingerprint(before, after)) {
      throw new Error(`${label} changed while being read: ${filePath}`);
    }
    return text;
  } finally {
    fs.closeSync(fd);
  }
}

function readJsonFile(filePath, label = 'JSON file') {
  return JSON.parse(readRegularTextFileStable(filePath, label));
}

function packageVersion(packageRoot) {
  try {
    return readJsonFile(path.join(packageRoot, 'package.json'), 'main package.json').version ?? null;
  } catch {
    return null;
  }
}

function isMCPaceSourceWorkspace(repoRoot) {
  try {
    const cargoToml = fs.readFileSync(path.join(repoRoot, 'Cargo.toml'), 'utf8');
    const rootPackage = readJsonFile(path.join(repoRoot, 'package.json'), 'workspace package.json');
    return /^\s*name\s*=\s*"mcpace"/m.test(cargoToml) && rootPackage.name === 'mcpace-workspace';
  } catch {
    return false;
  }
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
  const envName = process.env.MCPACE_BINARY_PATH ? 'MCPACE_BINARY_PATH' : process.env.MCPACE_DEV_BINARY ? 'MCPACE_DEV_BINARY' : null;
  if (!envName) {
    return null;
  }
  const raw = process.env[envName];
  const explicitPath = unquoteExplicitEnvPath(raw);
  if (!explicitPath) {
    throw new Error(`${envName} must not be empty`);
  }
  if (explicitPath.includes('\0') || /[\r\n]/.test(explicitPath)) {
    throw new Error(`${envName} must not contain control characters or newlines`);
  }
  if (!path.isAbsolute(explicitPath)) {
    throw new Error(`${envName} must be an absolute path, not a cwd-relative binary override: ${explicitPath}`);
  }
  const absolute = path.resolve(explicitPath);
  const problem = binaryPathProblem(absolute);
  if (problem) {
    throw new Error(`MCPACE binary path ${problem}: ${absolute}`);
  }
  return absolute;
}

function resolveDevBinary(repoRoot) {
  for (const candidate of candidateDevBinaryPaths(repoRoot)) {
    if (isUsableBinaryFile(candidate)) {
      return validateContainedBinary(candidate, [repoRoot], 'MCPace development binary');
    }
  }
  return null;
}

function resolveVendoredBinary(repoRoot, packageRoot, target) {
  for (const candidate of candidateVendoredBinaryPaths(repoRoot, packageRoot, target)) {
    if (isUsableBinaryFile(candidate)) {
      return validateContainedBinary(candidate, [packageRoot, repoRoot], 'vendored MCPace binary');
    }
  }
  return null;
}

function realpathOrNull(filePath) {
  try {
    return fs.realpathSync(filePath);
  } catch {
    return null;
  }
}

function pathInside(parent, child) {
  const relative = path.relative(parent, child);
  return relative === '' || (!relative.startsWith('..') && !path.isAbsolute(relative));
}

function validateContainedBinary(filePath, allowedRoots, label) {
  const problem = binaryPathProblem(filePath);
  if (problem) {
    throw new Error(`${label} ${problem}: ${filePath}`);
  }

  const realFile = realpathOrNull(filePath);
  const realRoots = allowedRoots
    .map((root) => realpathOrNull(root))
    .filter(Boolean);
  if (!realFile || realRoots.length === 0 || !realRoots.some((root) => pathInside(root, realFile))) {
    throw new Error(`${label} escapes expected package or workspace root: ${filePath}`);
  }
  return filePath;
}

function packageRootCandidates(packageRoot, pkgName) {
  const parts = pkgName.split('/');
  const nodeModulesRoot = path.basename(path.dirname(packageRoot))?.startsWith('@')
    ? path.dirname(path.dirname(packageRoot))
    : path.dirname(packageRoot);
  return [...new Set([path.join(nodeModulesRoot, ...parts)].map((candidate) => path.resolve(candidate)))];
}

function validateInstalledBinaryPackage(pkgName, pkgJsonPath, candidate, target, mainVersion) {
  const packageJson = readJsonFile(pkgJsonPath, `installed MCPace binary package.json for ${pkgName}`);
  if (packageJson.name !== pkgName) {
    throw new Error(`installed MCPace binary package name mismatch: expected ${pkgName}, got ${packageJson.name ?? '<missing>'}`);
  }
  if (!mainVersion || packageJson.version !== mainVersion) {
    throw new Error(`installed MCPace binary package version mismatch for ${pkgName}: expected ${mainVersion ?? '<unknown>'}, got ${packageJson.version ?? '<missing>'}`);
  }
  if (packageJson.mcpace?.target !== target.key) {
    throw new Error(`installed MCPace binary package target mismatch for ${pkgName}: expected ${target.key}, got ${packageJson.mcpace?.target ?? '<missing>'}`);
  }

  const problem = binaryPathProblem(candidate);
  if (problem) {
    throw new Error(`installed MCPace binary ${problem}: ${candidate}`);
  }

  const packageRoot = path.dirname(pkgJsonPath);
  const realPackageRoot = realpathOrNull(packageRoot);
  const realCandidate = realpathOrNull(candidate);
  if (!realPackageRoot || !realCandidate || !pathInside(realPackageRoot, realCandidate)) {
    throw new Error(`installed MCPace binary escapes package root: ${candidate}`);
  }
  return candidate;
}

function optionalPackageJsonPaths(packageRoot, pkgName) {
  const paths = [];
  for (const root of packageRootCandidates(packageRoot, pkgName)) {
    paths.push(path.join(root, 'package.json'));
  }
  try {
    paths.push(require.resolve(`${pkgName}/package.json`));
  } catch {
    // future optional package not installed yet
  }
  return [...new Set(paths.map((entry) => path.resolve(entry)))];
}

function resolveFromInstalledBinaryPackage(target, packageRoot) {
  const binName = binaryNameForTarget(target);
  const mainVersion = packageVersion(packageRoot);
  for (const pkgName of packageNamesForTarget(target)) {
    for (const pkgJsonPath of optionalPackageJsonPaths(packageRoot, pkgName)) {
      if (!fs.existsSync(pkgJsonPath)) continue;
      const dir = path.dirname(pkgJsonPath);
      const candidate = path.join(dir, 'bin', binName);
      return validateInstalledBinaryPackage(pkgName, pkgJsonPath, candidate, target, mainVersion);
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
  if (!options.ignoreDevBinary && isMCPaceSourceWorkspace(repoRoot)) {
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

  const packagedBinary = target && !options.ignoreInstalledBinaryPackage
    ? resolveFromInstalledBinaryPackage(target, packageRoot)
    : null;
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
