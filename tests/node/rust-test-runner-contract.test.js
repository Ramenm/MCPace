const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { read, readJson, repoRoot } = require('./helpers');

test('Rust CI test runner executes suites one at a time with timeout cleanup', () => {
  const script = read(path.join('scripts', 'run-rust-tests.mjs'));
  assert.match(script, /discoverIntegrationSuites/);
  assert.match(script, /PRIORITY_INTEGRATION_SUITES/);
  assert.match(script, /orderedIntegrationSuites/);
  assert.match(script, /LIFECYCLE_SUITE_NAMES/);
  assert.match(script, /--profile/);
  assert.match(script, /non-lifecycle/);
  assert.match(script, /MCPACE_RUST_TEST_TIMEOUT_MS/);
  assert.match(script, /--timeout-ms/);
  assert.match(script, /cargo', \['test', '--lib', '--locked'/);
  assert.match(script, /cargo', \['test', '--test', suite, '--locked'/);
  assert.match(script, /cargo', \['test', '--doc', '--locked'/);
  assert.match(script, /killChildTree/);
  assert.match(script, /child\.on\('exit'/);
  assert.match(script, /forceResolveTimer/);
  assert.match(script, /process\.kill\(-child\.pid, 'SIGTERM'\)/);
});

test('npm scripts expose the suite-isolated Rust runner for CI', () => {
  const pkg = readJson('package.json');
  assert.equal(pkg.scripts['test:rust'], 'node scripts/run-rust-tests.mjs');
  assert.equal(pkg.scripts['test:rust:ci'], 'node scripts/run-rust-tests.mjs --json --profile non-lifecycle');
  assert.equal(pkg.scripts['test:rust:full'], 'node scripts/run-rust-tests.mjs --json --profile full');
  assert.equal(pkg.scripts['test:linux-npm-install:docker'], 'node scripts/verify-linux-npm-install-docker.mjs --json');
  assert.equal(pkg.scripts['verify:macos-proof-lanes'], 'node scripts/verify-macos-proof-lanes.mjs --json');
  assert.equal(pkg.scripts['prove:rust-host'], 'npm run verify:rust-quality');
  assert.equal(pkg.scripts['verify:rust-quality'], 'node scripts/verify-rust-quality.mjs --json --write reports/rust-quality-latest.json');
  assert.equal(pkg.scripts['lint:npm'], 'node scripts/check-node-syntax.mjs --json');
  for (const script of [
    'scripts/run-rust-tests.mjs',
    'scripts/verify-rust-quality.mjs',
    'scripts/verify-linux-npm-install-docker.mjs',
    'scripts/verify-macos-proof-lanes.mjs',
  ]) {
    assert.equal(fs.existsSync(path.join(repoRoot, script)), true, `${script} should exist`);
  }
});
