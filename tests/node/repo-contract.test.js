const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');

function walk(dir, files = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === '.git' || entry.name === 'node_modules' || entry.name === 'target') continue;
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(fullPath, files);
    else files.push(fullPath);
  }
  return files;
}

function extractTomlVersion(text) {
  const match = text.match(/^version\s*=\s*"([^"]+)"/m);
  return match ? match[1] : null;
}

test('repo contains no ps1 files or shell bridge wrappers', () => {
  const files = walk(repoRoot);
  assert.equal(files.filter((file) => file.endsWith('.ps1')).length, 0);
  assert.equal(fs.existsSync(path.join(repoRoot, 'manager.sh')), false);
  assert.equal(fs.existsSync(path.join(repoRoot, 'manager.cmd')), false);
  assert.equal(fs.existsSync(path.join(repoRoot, 'src', 'psbridge.rs')), false);
});

test('release manifest excludes removed shell artifacts and keeps current roots', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(repoRoot, 'release-manifest.json'), 'utf8'));
  const includePaths = manifest.includePaths;
  for (const forbidden of ['manager.ps1', 'manager.sh', 'manager.cmd', 'verify-manager.ps1', 'build-release.ps1', 'lib']) {
    assert.equal(includePaths.includes(forbidden), false, forbidden);
  }
  for (const required of ['src', 'packages', 'schemas', 'tests', 'scripts', 'TODO.md', 'STATE.md', 'DECISIONS.md']) {
    assert.equal(includePaths.includes(required), true, required);
  }
});

test('versions stay aligned across manifests and reports', () => {
  const cargoVersion = extractTomlVersion(fs.readFileSync(path.join(repoRoot, 'Cargo.toml'), 'utf8'));
  const rootPkgVersion = JSON.parse(fs.readFileSync(path.join(repoRoot, 'package.json'), 'utf8')).version;
  const npmPkgVersion = JSON.parse(fs.readFileSync(path.join(repoRoot, 'packages', 'npm', 'cli', 'package.json'), 'utf8')).version;
  const configVersion = JSON.parse(fs.readFileSync(path.join(repoRoot, 'mcpace.config.json'), 'utf8')).version;
  const coverageVersion = JSON.parse(fs.readFileSync(path.join(repoRoot, 'reports', 'rust-command-coverage.json'), 'utf8')).version;
  assert.equal(cargoVersion, rootPkgVersion);
  assert.equal(npmPkgVersion, rootPkgVersion);
  assert.equal(configVersion, rootPkgVersion);
  assert.equal(coverageVersion, rootPkgVersion);
});

test('Cargo manifest stays dependency-light for offline Linux proof', () => {
  const cargoToml = fs.readFileSync(path.join(repoRoot, 'Cargo.toml'), 'utf8');
  assert.match(cargoToml, /\[dependencies\]/);
  assert.doesNotMatch(cargoToml, /serde/);
  assert.doesNotMatch(cargoToml, /serde_json/);
  assert.doesNotMatch(cargoToml, /assert_cmd/);
  assert.doesNotMatch(cargoToml, /predicates/);
  assert.doesNotMatch(cargoToml, /tempfile/);
});

test('CI workflow includes Rust build and test validation', () => {
  const workflow = fs.readFileSync(path.join(repoRoot, '.github', 'workflows', 'ci.yml'), 'utf8');
  assert.match(workflow, /cargo test/);
  assert.match(workflow, /cargo build --release/);
  assert.match(workflow, /ubuntu-latest/);
  assert.match(workflow, /windows-latest/);
});
