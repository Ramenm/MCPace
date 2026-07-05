import fs from 'node:fs';
import path from 'node:path';

const DEFAULT_STANDARD_CRATES = ['auto-launch', 'clap', 'getrandom', 'serde', 'serde_json', 'which'];

function readTextIfExists(filePath) {
  return fs.existsSync(filePath) ? fs.readFileSync(filePath, 'utf8') : '';
}

function dependencySection(text) {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim() === '[dependencies]');
  if (start < 0) return '';
  const body = [];
  for (const line of lines.slice(start + 1)) {
    if (/^\s*\[[^\]]+\]\s*$/.test(line)) break;
    body.push(line);
  }
  return body.join('\n');
}


function normalizeRequirement(value) {
  return value.trim().replace(/^=/, '').replace(/^\^/, '').replace(/^~/, '');
}

function parseSemverPrefix(value) {
  const normalized = normalizeRequirement(value);
  const match = normalized.match(/^(\d+)\.(\d+)(?:\.(\d+))?/);
  if (!match) return null;
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: match[3] === undefined ? null : Number(match[3]),
  };
}

function requirementSatisfiedByLockedVersion(requirement, lockedVersion) {
  const req = parseSemverPrefix(requirement);
  const locked = parseSemverPrefix(lockedVersion);
  if (!req || !locked) return true;
  if (req.major === 0) {
    return locked.major === 0 && locked.minor === req.minor;
  }
  return locked.major === req.major;
}

export function readCargoDependencySpecs(repoRoot) {
  const cargoToml = readTextIfExists(path.join(repoRoot, 'Cargo.toml'));
  const deps = dependencySection(cargoToml);
  const specs = new Map();
  for (const line of deps.split(/\r?\n/)) {
    const trimmed = line.replace(/#.*$/, '').trim();
    if (!trimmed) continue;
    const simple = trimmed.match(/^([A-Za-z0-9_-]+)\s*=\s*"([^"]+)"/);
    if (simple) {
      specs.set(simple[1], { name: simple[1], version: simple[2], path: null, raw: trimmed });
      continue;
    }
    const table = trimmed.match(/^([A-Za-z0-9_-]+)\s*=\s*\{(.+)\}\s*$/);
    if (table) {
      const body = table[2];
      const version = body.match(/\bversion\s*=\s*"([^"]+)"/)?.[1] ?? null;
      const depPath = body.match(/\bpath\s*=\s*"([^"]+)"/)?.[1] ?? null;
      specs.set(table[1], { name: table[1], version, path: depPath, raw: trimmed });
    }
  }
  return specs;
}

export function readCargoLockPackages(repoRoot) {
  const lockText = readTextIfExists(path.join(repoRoot, 'Cargo.lock'));
  const packages = new Map();
  for (const block of lockText.split(/\n\[\[package\]\]\n/)) {
    const name = block.match(/^name\s*=\s*"([^"]+)"/m)?.[1];
    const version = block.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
    if (name && version) packages.set(name, { name, version });
  }
  return packages;
}

export function cargoLockRefreshFindings(repoRoot, crateNames = DEFAULT_STANDARD_CRATES) {
  const specs = readCargoDependencySpecs(repoRoot);
  const locked = readCargoLockPackages(repoRoot);
  const issues = [];
  for (const crateName of crateNames) {
    const spec = specs.get(crateName);
    if (!spec || spec.path || !spec.version) continue;
    const lockPackage = locked.get(crateName);
    if (!lockPackage) {
      issues.push({ crate: crateName, dependency: spec.version, lock: null, reason: 'missing from Cargo.lock' });
      continue;
    }
    if (!requirementSatisfiedByLockedVersion(spec.version, lockPackage.version)) {
      issues.push({
        crate: crateName,
        dependency: spec.version,
        lock: lockPackage.version,
        reason: 'locked version does not satisfy the Cargo.toml semver line',
      });
    }
  }
  return issues;
}

export function cargoLockRefreshMessage(issues) {
  if (issues.length === 0) return 'Cargo.lock standard-crate entries are consistent with Cargo.toml';
  return `Cargo.lock needs refresh for ${issues.map((item) => `${item.crate} (${item.lock ?? 'missing'} vs ${item.dependency})`).join(', ')}`;
}
