import fs from 'node:fs';
import path from 'node:path';
import { repoRoot } from './project-metadata.mjs';

export const RELEASE_TARGETS_PATH = path.join(repoRoot, 'release-targets.json');
export const releaseTargetsPath = RELEASE_TARGETS_PATH;

function npmLibcToProbe(libc) {
  return libc === 'glibc' ? 'gnu' : libc || null;
}

function normalizeLibc(value) {
  if (Array.isArray(value)) {
    return value;
  }
  if (typeof value === 'string' && value) {
    return [value];
  }
  return undefined;
}

function normalizeTarget(target) {
  const platform = target.platform || target.nodePlatform;
  const arch = target.arch || target.nodeArch;
  const packageName = target.packageName || target.npmPackage;
  const libc = normalizeLibc(target.libc);
  const normalized = {
    publishEnabled: target.publishEnabled !== false,
    ...target,
    platform,
    arch,
    nodePlatform: target.nodePlatform || platform,
    nodeArch: target.nodeArch || arch,
    os: Array.isArray(target.os) ? target.os : platform ? [platform] : [],
    cpu: Array.isArray(target.cpu) ? target.cpu : arch ? [arch] : [],
    rustTarget: target.rustTarget || target.triple,
    triple: target.triple || target.rustTarget,
    packageName,
    npmPackage: target.npmPackage || packageName
  };
  if (libc) {
    normalized.libc = libc;
    normalized.libcProbe = target.libcProbe || npmLibcToProbe(libc[0]);
  }
  return normalized;
}

export function releaseTargetsManifest() {
  const manifest = JSON.parse(fs.readFileSync(RELEASE_TARGETS_PATH, 'utf8'));
  const targets = Array.isArray(manifest.targets) ? manifest.targets.map(normalizeTarget) : [];
  const plannedTargets = Array.isArray(manifest.plannedTargets)
    ? manifest.plannedTargets.map((target) => normalizeTarget({ ...target, publishEnabled: false }))
    : [];
  return { ...manifest, targets, plannedTargets };
}

export function readReleaseTargets() {
  return releaseTargetsManifest();
}

export function allReleaseTargets(manifest = releaseTargetsManifest()) {
  return [...manifest.targets, ...manifest.plannedTargets];
}

export function enabledReleaseTargets(manifest = releaseTargetsManifest()) {
  return manifest.targets.filter((target) => target.publishEnabled !== false);
}

export function supportedReleaseTargets(manifest = releaseTargetsManifest()) {
  return enabledReleaseTargets(manifest);
}

export function plannedReleaseTargets(manifest = releaseTargetsManifest()) {
  return manifest.plannedTargets;
}

export function releaseTargetByKey(targetKey, manifest = releaseTargetsManifest()) {
  return allReleaseTargets(manifest).find((target) => target.key === targetKey) ?? null;
}

export function findReleaseTarget(targetKey, manifest = releaseTargetsManifest()) {
  return releaseTargetByKey(targetKey, manifest);
}

export function releaseTargetByPackageName(packageName, manifest = releaseTargetsManifest()) {
  return allReleaseTargets(manifest).find((target) => target.packageName === packageName || target.npmPackage === packageName) ?? null;
}

export function githubMatrixInclude(manifest = releaseTargetsManifest()) {
  return enabledReleaseTargets(manifest).map((target) => ({
    target_key: target.key,
    package_name: target.packageName,
    npm_package: target.npmPackage,
    os: target.runner,
    runner: target.runner,
    rust_target: target.triple,
    binary_name: target.binaryName
  }));
}

export function describeEnabledTargetKeys(manifest = releaseTargetsManifest()) {
  return enabledReleaseTargets(manifest).map((target) => target.key).join(', ');
}

export function targetPackageDirectory(target) {
  return path.join(repoRoot, 'packages', 'npm', `cli-${target.key}`);
}

export function npmPackageDirectory(target) {
  return targetPackageDirectory(target);
}

export function targetBinaryPath(target) {
  return path.join(targetPackageDirectory(target), 'bin', target.binaryName);
}

export function binaryPathInPackage(target) {
  return targetBinaryPath(target);
}

export function assertReleaseTargetsManifest(manifest = releaseTargetsManifest()) {
  const errors = [];
  const seenKeys = new Set();
  const seenPackages = new Set();
  const enabled = enabledReleaseTargets(manifest);

  if (manifest.schemaVersion !== 1) {
    errors.push('release-targets.json schemaVersion must be 1');
  }
  if (manifest.mainPackageName !== '@mcpace/cli') {
    errors.push('release-targets.json mainPackageName must be @mcpace/cli');
  }

  for (const target of allReleaseTargets(manifest)) {
    for (const field of ['key', 'platform', 'arch', 'triple', 'packageName', 'binaryName', 'runner']) {
      if (!target[field]) {
        errors.push(`target ${target.key || '<unknown>'} is missing ${field}`);
      }
    }
    if (seenKeys.has(target.key)) {
      errors.push(`duplicate target key: ${target.key}`);
    }
    seenKeys.add(target.key);
    if (seenPackages.has(target.packageName)) {
      errors.push(`duplicate npm package: ${target.packageName}`);
    }
    seenPackages.add(target.packageName);
    if (!target.packageName?.startsWith('@mcpace/cli-')) {
      errors.push(`target ${target.key} uses unexpected packageName ${target.packageName}`);
    }
    if (!Array.isArray(target.os) || target.os.length !== 1) {
      errors.push(`target ${target.key} must declare one npm os filter`);
    }
    if (!Array.isArray(target.cpu) || target.cpu.length !== 1) {
      errors.push(`target ${target.key} must declare one npm cpu filter`);
    }
    if (target.platform === 'linux' && (!Array.isArray(target.libc) || target.libc.length !== 1)) {
      errors.push(`linux target ${target.key} must declare one npm libc filter`);
    }
    if (target.platform === 'linux' && !target.libcProbe) {
      errors.push(`linux target ${target.key} must declare libcProbe`);
    }
    if (target.platform === 'win32' && target.binaryName !== 'mcpace.exe') {
      errors.push(`windows target ${target.key} must use mcpace.exe`);
    }
    if (target.platform !== 'win32' && target.binaryName !== 'mcpace') {
      errors.push(`non-windows target ${target.key} must use mcpace`);
    }
    if (target.publishEnabled === false && typeof target.reason !== 'string') {
      errors.push(`planned target ${target.key} must explain why it is not published yet`);
    }
  }

  if (enabled.length === 0) {
    errors.push('release-targets.json must include at least one enabled target');
  }

  return errors;
}
