#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { readRegularFileStableSync } from './lib/atomic-fs.mjs';
import { deriveProjectVersion, readCliPackageJson, readJson, repoRoot } from './lib/project-metadata.mjs';

const args = new Set(process.argv.slice(2));
const jsonOutput = args.has('--json');
const enforce = args.has('--enforce');

function readTextIfExists(relativePath) {
  const fullPath = path.join(repoRoot, relativePath);
  return fs.existsSync(fullPath) ? fs.readFileSync(fullPath, 'utf8') : '';
}

function walkPackageJsonFiles(root) {
  if (!fs.existsSync(root)) return [];
  const results = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      if (entry.name === 'node_modules' || entry.name === '.git' || entry.name === 'dist' || entry.name === 'target') {
        continue;
      }
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(full);
      } else if (entry.isFile() && entry.name === 'package.json') {
        results.push(full);
      }
    }
  }
  return results.sort();
}

function discoverPackages() {
  const packageDir = path.join(repoRoot, 'packages', 'npm');
  const packagesByName = new Map();
  for (const packageJsonPath of walkPackageJsonFiles(packageDir)) {
    try {
      const parsed = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
      if (typeof parsed.name === 'string') {
        packagesByName.set(parsed.name, {
          name: parsed.name,
          version: parsed.version ?? null,
          relativeDir: path.relative(repoRoot, path.dirname(packageJsonPath)).split(path.sep).join('/'),
          private: parsed.private === true,
          mcpaceTarget: parsed.mcpace?.target ?? null,
        });
      }
    } catch {
      // Let package syntax checks report malformed package files; this script only reports publish contract shape.
    }
  }
  return packagesByName;
}

function targetPackageName(target) {
  return target.packageName ?? target.npmPackage ?? `@mcpace/cli-${target.key}`;
}

function tarballNameFragments(packageName, version) {
  const unscoped = packageName.replace(/^@[^/]+\//, '');
  return [
    `${unscoped}-${version}.tgz`,
    `${packageName.replace('@', '').replace('/', '-')}-${version}.tgz`,
  ];
}

function tarballCandidatesFor(packageName, version) {
  const candidates = [];
  for (const dir of ['dist', 'dist/npm', '.artifacts', '.artifacts/npm']) {
    for (const fragment of tarballNameFragments(packageName, version)) {
      candidates.push(path.join(repoRoot, dir, fragment));
    }
  }
  return candidates;
}

function trimNullPaddedAscii(buffer, start, length) {
  return buffer
    .subarray(start, start + length)
    .toString('utf8')
    .replace(/\0.*$/s, '')
    .trim();
}

function tarOctal(buffer, start, length) {
  const raw = trimNullPaddedAscii(buffer, start, length).replace(/\s/g, '');
  if (!raw) return 0;
  if (!/^[0-7]+$/.test(raw)) {
    throw new Error(`invalid tar octal field '${raw}'`);
  }
  return Number.parseInt(raw, 8);
}

function tarPathIsUnsafe(name) {
  return !name
    || name.startsWith('/')
    || name.includes('\\')
    || name.split('/').some((part) => part === '' || part === '.' || part === '..');
}

function listTarGzEntries(tarballPath) {
  const { data: compressed } = readRegularFileStableSync(tarballPath, { maxBytes: 256 * 1024 * 1024 });
  const buffer = zlib.gunzipSync(compressed);
  const entries = [];
  let offset = 0;
  while (offset + 512 <= buffer.length) {
    const header = buffer.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) break;
    const name = trimNullPaddedAscii(header, 0, 100);
    const prefix = trimNullPaddedAscii(header, 345, 155);
    const fullName = prefix ? `${prefix}/${name}` : name;
    const mode = tarOctal(header, 100, 8);
    const size = tarOctal(header, 124, 12);
    const type = String.fromCharCode(header[156] || 0);
    const dataStart = offset + 512;
    const dataEnd = dataStart + size;
    if (dataEnd > buffer.length) {
      throw new Error(`tar entry '${fullName}' extends beyond archive size`);
    }
    entries.push({
      path: fullName,
      mode,
      size,
      type: type === '\0' ? '0' : type,
      data: buffer.subarray(dataStart, dataEnd),
    });
    offset = dataStart + Math.ceil(size / 512) * 512;
  }
  return entries;
}

function targetArrayMatches(actual, expected) {
  if (!expected || expected.length === 0) return true;
  return Array.isArray(actual)
    && expected.length === actual.length
    && expected.every((value) => actual.includes(value));
}

function verifyNativePackageTarball(tarballPath, target, version) {
  const packageName = target.packageName;
  const binaryName = target.binaryName;
  const relativePath = path.relative(repoRoot, tarballPath).split(path.sep).join('/');
  const issues = [];
  let entries = [];
  try {
    entries = listTarGzEntries(tarballPath);
  } catch (error) {
    return {
      path: relativePath,
      status: 'failed',
      issues: [`failed to parse tgz: ${error?.message ?? error}`],
    };
  }

  const duplicateEntries = [];
  const seen = new Set();
  for (const entry of entries) {
    if (seen.has(entry.path)) duplicateEntries.push(entry.path);
    seen.add(entry.path);
    if (tarPathIsUnsafe(entry.path)) issues.push(`unsafe tar entry path: ${entry.path}`);
    if (entry.type === '1' || entry.type === '2') issues.push(`link entries are not allowed: ${entry.path}`);
  }
  if (duplicateEntries.length > 0) {
    issues.push(`duplicate tar entries: ${duplicateEntries.sort().join(', ')}`);
  }

  const packageJsonEntry = entries.find((entry) => entry.path === 'package/package.json');
  let packageJson = null;
  if (!packageJsonEntry) {
    issues.push('missing package/package.json');
  } else {
    try {
      packageJson = JSON.parse(packageJsonEntry.data.toString('utf8'));
    } catch (error) {
      issues.push(`package/package.json is not valid JSON: ${error?.message ?? error}`);
    }
  }

  const binaryEntryPath = `package/bin/${binaryName}`;
  const binaryEntry = entries.find((entry) => entry.path === binaryEntryPath);
  if (!binaryEntry) {
    issues.push(`missing ${binaryEntryPath}`);
  } else if (binaryEntry.type !== '0') {
    issues.push(`${binaryEntryPath} must be a regular file entry`);
  } else if (target.platform !== 'win32' && (binaryEntry.mode & 0o111) === 0) {
    issues.push(`${binaryEntryPath} must be executable for ${target.key}`);
  }

  if (packageJson) {
    if (packageJson.name !== packageName) issues.push(`package name mismatch: expected ${packageName}, got ${packageJson.name ?? null}`);
    if (packageJson.version !== version) issues.push(`package version mismatch: expected ${version}, got ${packageJson.version ?? null}`);
    if (packageJson.private === true) issues.push('native package tarball must not be private');
    if (packageJson.mcpace?.target !== target.key) issues.push(`mcpace.target mismatch: expected ${target.key}, got ${packageJson.mcpace?.target ?? null}`);
    if (packageJson.mcpace?.binaryName !== binaryName) issues.push(`mcpace.binaryName mismatch: expected ${binaryName}, got ${packageJson.mcpace?.binaryName ?? null}`);
    if (packageJson.bin?.mcpace !== `bin/${binaryName}`) issues.push(`bin.mcpace must point to bin/${binaryName}`);
    if (!targetArrayMatches(packageJson.os, target.os)) issues.push(`os metadata mismatch for ${target.key}`);
    if (!targetArrayMatches(packageJson.cpu, target.cpu)) issues.push(`cpu metadata mismatch for ${target.key}`);
    if (!targetArrayMatches(packageJson.libc, target.libc)) issues.push(`libc metadata mismatch for ${target.key}`);
  }

  return {
    path: relativePath,
    status: issues.length === 0 ? 'pass' : 'failed',
    issues,
    entryCount: entries.length,
    packageName: packageJson?.name ?? null,
    packageVersion: packageJson?.version ?? null,
    packageTargetMetadata: packageJson?.mcpace?.target ?? null,
    binaryEntryPath: binaryEntry ? binaryEntry.path : null,
    binaryMode: binaryEntry ? binaryEntry.mode : null,
  };
}

function tarballProofFor(target, version) {
  for (const candidate of tarballCandidatesFor(target.packageName, version)) {
    if (fs.existsSync(candidate)) {
      return verifyNativePackageTarball(candidate, target, version);
    }
  }
  return null;
}

function sourcePackageBinaryPath(packageInfo, binaryName) {
  if (!packageInfo) return null;
  const packageDir = path.join(repoRoot, packageInfo.relativeDir);
  for (const candidate of [
    path.join(packageDir, 'bin', binaryName),
    path.join(packageDir, binaryName),
  ]) {
    if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
      return path.relative(repoRoot, candidate).split(path.sep).join('/');
    }
  }
  return null;
}

function check(id, ok, message, details = {}) {
  return {
    id,
    status: ok ? 'pass' : 'failed',
    message,
    ...details,
  };
}

function buildReport() {
  const version = deriveProjectVersion();
  const releaseTargets = readJson('release-targets.json');
  const cliPackage = readCliPackageJson();
  const workflow = readTextIfExists('.github/workflows/publish-npm.yml');
  const packagesByName = discoverPackages();
  const enabledTargets = (releaseTargets.targets ?? []).filter((target) => target.publishEnabled !== false);
  const requiredBinaryPackages = enabledTargets.map((target) => ({
    ...target,
    packageName: targetPackageName(target),
    binaryName: target.binaryName ?? (target.platform === 'win32' ? 'mcpace.exe' : 'mcpace'),
  }));
  const optionalDependencies = cliPackage.optionalDependencies ?? {};
  const optionalDependencyNames = new Set(Object.keys(optionalDependencies));
  const requiredNames = new Set(requiredBinaryPackages.map((target) => target.packageName));
  const missingOptionalDependencies = requiredBinaryPackages
    .filter((target) => !optionalDependencyNames.has(target.packageName))
    .map((target) => target.packageName);
  const extraOptionalDependencies = [...optionalDependencyNames]
    .filter((name) => name.startsWith('@mcpace/cli-') && !requiredNames.has(name))
    .sort();
  const optionalDependencyVersionDrift = Object.entries(optionalDependencies)
    .filter(([name, depVersion]) => requiredNames.has(name) && depVersion !== version)
    .map(([name, depVersion]) => ({ name, expected: version, actual: depVersion }));

  const binaryPackageGaps = [];
  const binaryPackageProof = [];
  const binaryPackageMetadataDrift = [];
  for (const target of requiredBinaryPackages) {
    const packageInfo = packagesByName.get(target.packageName) ?? null;
    const tarballProof = tarballProofFor(target, version);
    const sourceBinaryPath = sourcePackageBinaryPath(packageInfo, target.binaryName);
    const targetMetadataMatches = packageInfo?.mcpaceTarget === target.key;
    const hasPublishableSource = Boolean(
      packageInfo
        && packageInfo.private !== true
        && packageInfo.version === version
        && targetMetadataMatches
        && sourceBinaryPath,
    );
    const hasTarball = tarballProof?.status === 'pass';
    binaryPackageProof.push({
      ...target,
      packageSourceDir: packageInfo?.relativeDir ?? null,
      packageVersion: packageInfo?.version ?? null,
      packageTargetMetadata: packageInfo?.mcpaceTarget ?? null,
      sourceBinaryPath,
      tarballPath: tarballProof?.path ?? null,
      tarballStatus: tarballProof?.status ?? 'missing',
      tarballIssues: tarballProof?.issues ?? [],
      tarballEntryCount: tarballProof?.entryCount ?? null,
      publishReady: Boolean(hasPublishableSource || hasTarball),
    });
    if (packageInfo && packageInfo.private !== true && packageInfo.version === version && !targetMetadataMatches) {
      binaryPackageMetadataDrift.push({
        ...target,
        expected: target.key,
        actual: packageInfo.mcpaceTarget ?? null,
      });
    }
    if (!hasPublishableSource && !hasTarball) {
      let reason = 'No publishable platform package source with the expected native binary or prebuilt npm tarball was found for this target.';
      if (tarballProof?.status === 'failed') {
        reason = `Prebuilt native npm tarball exists, but failed verification: ${tarballProof.issues.join('; ')}`;
      } else if (packageInfo && packageInfo.private !== true && packageInfo.version === version && !targetMetadataMatches) {
        reason = `Platform package source exists, but package.json mcpace.target does not match '${target.key}'.`;
      } else if (packageInfo && packageInfo.private !== true && packageInfo.version === version) {
        reason = `Platform package source exists, but the expected native binary '${target.binaryName}' was not found in the package.`;
      }
      binaryPackageGaps.push({ ...target, reason });
    }
  }

  const pinnedPublishPattern = /npm exec --yes --package=npm@11\.13\.0 -- npm publish(?:\s|$)/;
  const workflowUsesPinnedNpmForPublish = pinnedPublishPattern.test(workflow);
  const workflowEnforcesContract = /verify-npm-publish-contract\.mjs --enforce/.test(workflow);
  const checks = [
    check('optional-dependencies-cover-enabled-targets', missingOptionalDependencies.length === 0, 'Main npm package must depend on every enabled platform package.', { missingOptionalDependencies }),
    check('optional-dependencies-match-project-version', optionalDependencyVersionDrift.length === 0, 'Platform optionalDependencies must match the project version.', { optionalDependencyVersionDrift }),
    check('optional-dependencies-do-not-advertise-disabled-targets', extraOptionalDependencies.length === 0, 'Main npm package must not advertise platform packages outside enabled release targets.', { extraOptionalDependencies }),
    check('binary-package-target-metadata-matches-release-targets', binaryPackageMetadataDrift.length === 0, 'Platform package package.json must declare mcpace.target matching release-targets.json.', { binaryPackageMetadataDrift }),
    check('binary-packages-or-tarballs-exist', binaryPackageGaps.length === 0, 'Every enabled target must have a publishable platform package source containing matching target metadata and the expected native binary, or a prebuilt tarball before npm publish.', { binaryPackageGaps }),
    check('publish-workflow-uses-pinned-npm-for-publish', workflowUsesPinnedNpmForPublish, 'The publish workflow must use the verified npm executable for npm publish, not the ambient npm binary.'),
    check('publish-workflow-enforces-native-package-contract', workflowEnforcesContract, 'The publish workflow must enforce this contract before publishing the main launcher.'),
  ];
  const failedChecks = checks.filter((entry) => entry.status !== 'pass');
  return {
    schema: 'mcpace.npmPublishContract.v1',
    generatedAt: new Date().toISOString(),
    status: failedChecks.length === 0 ? 'pass' : 'blocked',
    enforce,
    version,
    mainPackageName: cliPackage.name,
    enabledTargetCount: enabledTargets.length,
    requiredBinaryPackages,
    binaryPackageProof,
    binaryPackageGaps,
    binaryPackageMetadataDrift,
    checks,
    failedChecks,
    publishable: failedChecks.length === 0,
  };
}

const report = buildReport();
if (jsonOutput || enforce) {
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} else {
  const summary = report.publishable
    ? 'npm publish contract: pass'
    : `npm publish contract: blocked (${report.failedChecks.length} failed checks)`;
  process.stdout.write(`${summary}\n`);
  for (const failed of report.failedChecks) {
    process.stdout.write(`- ${failed.id}: ${failed.message}\n`);
  }
}

if (enforce && !report.publishable) {
  process.exitCode = 1;
}
