#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
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

function tarballExistsFor(packageName, version) {
  const candidates = [];
  for (const dir of ['dist', 'dist/npm', '.artifacts', '.artifacts/npm']) {
    for (const fragment of tarballNameFragments(packageName, version)) {
      candidates.push(path.join(repoRoot, dir, fragment));
    }
  }
  return candidates.find((candidate) => fs.existsSync(candidate)) ?? null;
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
    key: target.key,
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
  for (const target of requiredBinaryPackages) {
    const packageInfo = packagesByName.get(target.packageName) ?? null;
    const tarballPath = tarballExistsFor(target.packageName, version);
    const sourceBinaryPath = sourcePackageBinaryPath(packageInfo, target.binaryName);
    const hasPublishableSource = Boolean(
      packageInfo
        && packageInfo.private !== true
        && packageInfo.version === version
        && sourceBinaryPath,
    );
    const hasTarball = Boolean(tarballPath);
    binaryPackageProof.push({
      ...target,
      packageSourceDir: packageInfo?.relativeDir ?? null,
      packageVersion: packageInfo?.version ?? null,
      sourceBinaryPath,
      tarballPath: tarballPath ? path.relative(repoRoot, tarballPath).split(path.sep).join('/') : null,
      publishReady: Boolean(hasPublishableSource || hasTarball),
    });
    if (!hasPublishableSource && !hasTarball) {
      binaryPackageGaps.push({
        ...target,
        reason: packageInfo && packageInfo.private !== true && packageInfo.version === version
          ? `Platform package source exists, but the expected native binary '${target.binaryName}' was not found in the package.`
          : 'No publishable platform package source with the expected native binary or prebuilt npm tarball was found for this target.',
      });
    }
  }

  const pinnedPublishPattern = /npm exec --yes --package=npm@11\.13\.0 -- npm publish(?:\s|$)/;
  const workflowUsesPinnedNpmForPublish = pinnedPublishPattern.test(workflow);
  const workflowEnforcesContract = /verify-npm-publish-contract\.mjs --enforce/.test(workflow);
  const checks = [
    check('optional-dependencies-cover-enabled-targets', missingOptionalDependencies.length === 0, 'Main npm package must depend on every enabled platform package.', { missingOptionalDependencies }),
    check('optional-dependencies-match-project-version', optionalDependencyVersionDrift.length === 0, 'Platform optionalDependencies must match the project version.', { optionalDependencyVersionDrift }),
    check('optional-dependencies-do-not-advertise-disabled-targets', extraOptionalDependencies.length === 0, 'Main npm package must not advertise platform packages outside enabled release targets.', { extraOptionalDependencies }),
    check('binary-packages-or-tarballs-exist', binaryPackageGaps.length === 0, 'Every enabled target must have a publishable platform package source containing the expected native binary or a prebuilt tarball before npm publish.', { binaryPackageGaps }),
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
