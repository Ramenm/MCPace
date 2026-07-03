import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { extractTomlPackageName, extractTomlVersion, readCliPackageJson, readRootPackageJson, repoRoot } from '../../scripts/lib/project-metadata.mjs';

const REQUIRED_DOCS = new Set([
  'README.md',
  'architecture.md',
  'architecture-simplification.md',
  'configuration.md',
  'dashboard-base.md',
  'frontend.md',
  'holistic-runtime-model.md',
  'security.md',
  'supported-clients.md',
  'troubleshooting.md',
  'lab-harness.md',
  'main-logic-runtime-check.md',
  'mcp-lifecycle-hardening.md',
  'platform-testing.md',
  'release-completion.md',
  'signing-and-notarization.md',
  'supply-chain.md'
]);
const REMOVED_ROOT_DOCS = [
  'CITATION.cff',
  'CODE_OF_CONDUCT.md',
  'CONTRIBUTING.md',
  'SOURCE_ARCHIVE_NOTE.txt'
];
const REMOVED_LEGACY_CONFIGS = [
  'manager.settings.json'
];
const FORBIDDEN_DIRS = [
  '.git',
  'node_modules',
  'target',
  'dist',
  '.cache',
  '.pytest_cache',
  '__pycache__',
  '.DS_Store'
];

function readText(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function walkDirs(root, relativeRoot = '.') {
  const found = [];
  const base = path.join(root, relativeRoot);
  const stack = [base];
  if (!fs.existsSync(base)) return found;
  if (!fs.statSync(base).isDirectory()) return found;
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const fullPath = path.join(current, entry.name);
      if (entry.isDirectory()) {
        const relativeDir = path.relative(root, fullPath).split(path.sep).join('/');
        if (hasForbiddenPart(relativeDir)) continue;
        found.push(relativeDir);
        stack.push(fullPath);
      }
    }
  }
  return found;
}

function hasForbiddenPart(relativePath) {
  return relativePath.split('/').some((part) => FORBIDDEN_DIRS.includes(part));
}

test('package versions stay aligned across Rust and npm metadata', () => {
  const cargoToml = readText('Cargo.toml');
  const rootPackage = readRootPackageJson();
  const cliPackage = readCliPackageJson();
  const cargoLock = readText('Cargo.lock');
  const mcpaceConfig = JSON.parse(readText('mcpace.config.json'));
  const expectedVersion = extractTomlVersion(cargoToml);

  assert.equal(extractTomlPackageName(cargoToml), 'mcpace');
  assert.match(expectedVersion, /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/);
  assert.equal(rootPackage.version, expectedVersion);
  assert.equal(cliPackage.version, expectedVersion);
  assert.match(cargoLock, new RegExp(`name = "mcpace"\\nversion = "${expectedVersion.replaceAll(".", "\\.")}"`));
  assert.equal(mcpaceConfig.version, expectedVersion);
  for (const [name, version] of Object.entries(cliPackage.optionalDependencies ?? {})) {
    assert.equal(version, expectedVersion, `${name} optional dependency version drifted`);
  }
});

test('root README is a short landing page and detailed docs live under docs/', () => {
  const readme = readText('README.md');
  const lines = readme.trimEnd().split(/\r?\n/);
  assert.ok(lines.length <= 60, `README should stay compact, got ${lines.length} lines`);
  assert.match(readme, /^# MCPace$/m);
  assert.match(readme, /MCPace runs MCP servers at the right concurrency\./);
  assert.match(readme, /mcpace up/);
  assert.match(readme, /server set-policy/);
  assert.match(readme, /does \*\*not\*\* add a filesystem server/);
  assert.doesNotMatch(readme, /UpstreamSessionPool|proof gate|release harness|operator manual/i);
});

test('docs directory contains the normalized user-facing set only', () => {
  const docsDir = path.join(repoRoot, 'docs');
  const docs = fs.readdirSync(docsDir).filter((name) => name.endsWith('.md')).sort();
  assert.deepEqual(docs, [...REQUIRED_DOCS].sort());
  for (const doc of docs) {
    const text = readText(path.join('docs', doc));
    assert.match(text, /^# /, `${doc} must start with a markdown title`);
    assert.doesNotMatch(text, /TODO|TBD|docs\/toolchain-policy|product-truth-and-beta-gate/i, `${doc} contains stale internal wording`);
  }
});

test('source bundle excludes stale public-repo docs and heavyweight artifacts', () => {
  const manifest = JSON.parse(readText('release-manifest.json'));
  for (const relativePath of [...REMOVED_ROOT_DOCS, ...REMOVED_LEGACY_CONFIGS]) {
    assert.equal(fs.existsSync(path.join(repoRoot, relativePath)), false, `${relativePath} should not be in the source bundle`);
  }

  for (const includedPath of manifest.includePaths) {
    assert.equal(hasForbiddenPart(includedPath), false, `manifest includes forbidden path: ${includedPath}`);
    assert.equal(/^packages\/npm\/cli-(darwin|linux|win32)/.test(includedPath), false, `manifest includes platform package scaffolding: ${includedPath}`);
    for (const relativeDir of walkDirs(repoRoot, includedPath)) {
      assert.equal(hasForbiddenPart(relativeDir), false, `forbidden directory found in source bundle path: ${relativeDir}`);
      assert.equal(/^packages\/npm\/cli-(darwin|linux|win32)/.test(relativeDir), false, `empty platform package scaffolding should stay out: ${relativeDir}`);
    }
  }
});

test('release manifest matches the normalized bundle contract', () => {
  const manifest = JSON.parse(readText('release-manifest.json'));
  for (const required of ['README.md', 'docs/README.md', 'docs/lab-harness.md', 'reports/summary.md', 'tests/node', 'packages/npm/cli', 'catalog', 'manifests', 'eval', 'scripts/check-node-syntax.mjs']) {
    assert.ok(manifest.includePaths.includes(required), `manifest missing ${required}`);
  }
  for (const removed of [...REMOVED_ROOT_DOCS, ...REMOVED_LEGACY_CONFIGS]) {
    assert.equal(manifest.includePaths.includes(removed), false, `manifest still includes ${removed}`);
  }
  assert.equal(manifest.includePaths.some((item) => item.startsWith('packages/npm/cli-')), false, 'manifest still includes platform package scaffolding');
});

test('bundled hub examples align with the hub schema empty-manual-profile contract', () => {
  const schema = JSON.parse(readText('schemas/mcpace-hub.schema.json'));
  const schemaProperties = schema.properties || {};

  assert.equal(schemaProperties.compatibility, undefined, 'retired legacy bridge flags must not stay in the hub schema');
  assert.equal(schemaProperties.servers.minItems ?? 0, 0, 'manual hub examples must be allowed to start with zero upstream servers');
  assert.equal(
    schemaProperties.profiles.properties.definitions.additionalProperties.properties.serverIds.minItems ?? 0,
    0,
    'manual profile examples must be allowed to start with zero serverIds'
  );

  for (const relativePath of ['examples/mcpace-hub.minimal.json', 'examples/mcpace-hub.workstation.json']) {
    const example = JSON.parse(readText(relativePath));
    assert.equal(example.compatibility, undefined, `${relativePath} must not ship retired legacy bridge flags`);
    for (const key of Object.keys(example)) {
      assert.ok(schemaProperties[key], `${relativePath} has top-level key not described by schema: ${key}`);
    }
    assert.deepEqual(example.servers, [], `${relativePath} should remain safe-by-default with no bundled upstream servers`);
    for (const [profileName, profile] of Object.entries(example.profiles.definitions)) {
      assert.ok(Array.isArray(profile.serverIds), `${relativePath} profile ${profileName} must declare serverIds`);
    }
  }
});
