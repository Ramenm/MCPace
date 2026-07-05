#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { cargoLockRefreshFindings, cargoLockRefreshMessage } from './lib/cargo-policy.mjs';

function parseArgs(argv) {
  const args = { json: false, repoRoot: process.cwd(), enforceCargoLock: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--json') args.json = true;
    else if (arg === '--enforce-cargo-lock') args.enforceCargoLock = true;
    else if (arg === '--repo') args.repoRoot = path.resolve(argv[++index]);
    else if (arg === '--help' || arg === '-h') {
      console.log('Usage: node scripts/verify-dependency-policy.mjs [--json] [--enforce-cargo-lock] [--repo DIR]');
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return args;
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function finding(id, status, detail, extra = {}) {
  return { id, status, detail, ...extra };
}

function readTextIfExists(filePath) {
  return fs.existsSync(filePath) ? fs.readFileSync(filePath, 'utf8') : '';
}

function hasIgnoreScripts(text) {
  return text.split(/\r?\n/).some((line) => /^\s*ignore-scripts\s*=\s*true\s*(?:#.*)?$/i.test(line));
}

function isExternalRegistryUrl(value) {
  return typeof value === 'string' && /^https:\/\/registry\.npmjs\.org\//.test(value);
}

function isLocalWorkspaceResolution(value) {
  return typeof value === 'string' && /^packages\//.test(value);
}

function packageDisplayName(entryKey, meta) {
  return meta.name ?? entryKey.replace(/^node_modules\//, '');
}

function checkLockfile(repoRoot) {
  const lockPath = path.join(repoRoot, 'package-lock.json');
  const lock = readJson(lockPath);
  const findings = [];
  findings.push(finding(
    'lockfile-version-3',
    lock.lockfileVersion === 3 ? 'pass' : 'fail',
    `lockfileVersion is ${lock.lockfileVersion}`,
  ));

  const packages = lock.packages && typeof lock.packages === 'object' ? lock.packages : {};
  findings.push(finding(
    'lockfile-packages-present',
    Object.keys(packages).length > 0 ? 'pass' : 'fail',
    `${Object.keys(packages).length} package entries`,
  ));

  const missingIntegrity = [];
  const badResolved = [];
  const lifecycleScripts = [];
  for (const [entryKey, meta] of Object.entries(packages)) {
    if (entryKey === '') continue;
    if (meta.link === true) continue;
    if (entryKey.startsWith('packages/')) continue;
    if (isLocalWorkspaceResolution(meta.resolved)) continue;

    if (!meta.integrity) missingIntegrity.push(packageDisplayName(entryKey, meta));
    if (meta.resolved && !isExternalRegistryUrl(meta.resolved)) {
      badResolved.push({ package: packageDisplayName(entryKey, meta), resolved: meta.resolved });
    }
    if (meta.hasInstallScript === true) lifecycleScripts.push(packageDisplayName(entryKey, meta));
  }
  findings.push(finding(
    'external-packages-have-integrity',
    missingIntegrity.length === 0 ? 'pass' : 'fail',
    missingIntegrity.length === 0 ? 'all external locked packages have integrity' : 'external packages missing integrity',
    { packages: missingIntegrity },
  ));
  findings.push(finding(
    'external-packages-use-npm-registry',
    badResolved.length === 0 ? 'pass' : 'fail',
    badResolved.length === 0 ? 'external resolved URLs are npm registry URLs' : 'unexpected external resolved URLs',
    { packages: badResolved },
  ));
  findings.push(finding(
    'external-install-scripts-disabled-by-lockfile',
    lifecycleScripts.length === 0 ? 'pass' : 'fail',
    lifecycleScripts.length === 0 ? 'no external locked package has hasInstallScript=true' : 'external locked packages declare install scripts',
    { packages: lifecycleScripts },
  ));

  return { lock, findings };
}

function checkWorkspacePackage(repoRoot, lock) {
  const findings = [];
  const packageJson = readJson(path.join(repoRoot, 'packages/npm/cli/package.json'));
  const rootPackage = readJson(path.join(repoRoot, 'package.json'));
  const version = packageJson.version;
  const optional = packageJson.optionalDependencies ?? {};
  const optionalEntries = Object.entries(optional);
  const badOptional = optionalEntries.filter(([name, range]) => {
    return !name.startsWith('@mcpace/cli-') || range !== version;
  });
  findings.push(finding(
    'native-optional-deps-exact-version',
    badOptional.length === 0 ? 'pass' : 'fail',
    badOptional.length === 0
      ? `${optionalEntries.length} native optional dependencies are exact ${version}`
      : 'native optional dependencies must be exact-version @mcpace/cli-* packages',
    { packages: badOptional.map(([name, range]) => ({ name, range })) },
  ));
  findings.push(finding(
    'workspace-version-consistency',
    rootPackage.version === undefined || rootPackage.version === version ? 'pass' : 'fail',
    `root version ${rootPackage.version ?? '<unset>'}; package version ${version}`,
  ));

  const lockWorkspace = lock.packages?.['packages/npm/cli'];
  const lockedOptional = lockWorkspace?.optionalDependencies ?? {};
  const lockMismatch = [];
  for (const [name, range] of optionalEntries) {
    if (lockedOptional[name] !== range) lockMismatch.push({ name, packageJson: range, packageLock: lockedOptional[name] });
  }
  findings.push(finding(
    'native-optional-deps-lockfile-synced',
    lockMismatch.length === 0 ? 'pass' : 'fail',
    lockMismatch.length === 0 ? 'package-lock optional dependency metadata is synced' : 'package-lock optional dependency metadata mismatch',
    { packages: lockMismatch },
  ));
  return findings;
}

function checkNpmrc(repoRoot) {
  const text = readTextIfExists(path.join(repoRoot, '.npmrc'));
  return [
    finding(
      'npmrc-ignore-scripts',
      hasIgnoreScripts(text) ? 'pass' : 'fail',
      hasIgnoreScripts(text) ? '.npmrc sets ignore-scripts=true' : '.npmrc must set ignore-scripts=true',
    ),
  ];
}

function checkCargoDependencyPolicy(repoRoot) {
  const cargoToml = readTextIfExists(path.join(repoRoot, 'Cargo.toml'));
  const localCompat = [...cargoToml.matchAll(/^\s*([A-Za-z0-9_-]+)\s*=\s*\{\s*path\s*=\s*"crates\/compat\/[^"]+"/gm)]
    .map((match) => match[1]);
  const compatTreeExists = fs.existsSync(path.join(repoRoot, 'crates', 'compat'));
  return [
    finding(
      'cargo-uses-upstream-standard-crates',
      localCompat.length === 0 ? 'pass' : 'fail',
      localCompat.length === 0
        ? 'Cargo.toml does not redirect standard crates to local compat shims'
        : 'Cargo.toml still redirects standard crates to crates/compat shims',
      { crates: localCompat },
    ),
    finding(
      'cargo-compat-crate-tree-removed',
      compatTreeExists ? 'fail' : 'pass',
      compatTreeExists ? 'crates/compat still exists' : 'crates/compat is absent',
    ),
  ];
}

function checkCargoLockPolicy(repoRoot, { enforceCargoLock = false } = {}) {
  const issues = cargoLockRefreshFindings(repoRoot);
  const status = issues.length === 0 ? 'pass' : (enforceCargoLock ? 'fail' : 'warn');
  return [finding(
    'cargo-lock-standard-crates-synced',
    status,
    cargoLockRefreshMessage(issues),
    { crates: issues },
  )];
}

function run() {
  const args = parseArgs(process.argv.slice(2));
  const repoRoot = args.repoRoot;
  const findings = [];
  findings.push(...checkNpmrc(repoRoot));
  const { lock, findings: lockFindings } = checkLockfile(repoRoot);
  findings.push(...lockFindings);
  findings.push(...checkWorkspacePackage(repoRoot, lock));
  findings.push(...checkCargoDependencyPolicy(repoRoot));
  findings.push(...checkCargoLockPolicy(repoRoot, { enforceCargoLock: args.enforceCargoLock }));

  const failures = findings.filter((item) => item.status === 'fail');
  const warnings = findings.filter((item) => item.status === 'warn');
  const report = {
    status: failures.length === 0 ? 'pass' : 'fail',
    checkedAt: new Date().toISOString(),
    repoRoot: '.',
    failures: failures.length,
    warnings: warnings.length,
    findings,
  };
  if (args.json) console.log(JSON.stringify(report, null, 2));
  else {
    console.log(`${report.status}: ${findings.length} dependency policy checks, ${failures.length} failures, ${warnings.length} warnings`);
    for (const item of findings) console.log(`- ${item.status}: ${item.id} — ${item.detail}`);
  }
  process.exitCode = failures.length === 0 ? 0 : 1;
}

try {
  run();
} catch (error) {
  const message = error?.stack ?? String(error);
  console.error(message);
  process.exitCode = 1;
}
