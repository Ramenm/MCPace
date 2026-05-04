const test = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { cleanChildEnv, repoRoot, read } = require('./helpers.js');

const CHILD_OPTIONS = {
  cwd: repoRoot,
  encoding: 'utf8',
  env: cleanChildEnv(),
  timeout: 30_000,
  maxBuffer: 4 * 1024 * 1024,
};

function runRustQuality(args) {
  return spawnSync(process.execPath, ['scripts/verify-rust-quality.mjs', ...args], CHILD_OPTIONS);
}

test('Rust quality gate exposes fmt, check, clippy, full tests, and release-build lanes in order', () => {
  const result = runRustQuality(['--json', '--plan-only']);

  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'planned');
  assert.deepEqual(report.policy.laneOrder, ['fmt', 'check', 'clippy', 'rust-tests', 'release-build']);
  assert.equal(report.policy.testsProfile, 'full');
  assert.deepEqual(report.lanes.map((lane) => lane.name), report.policy.laneOrder);
  assert.match(report.lanes.find((lane) => lane.name === 'fmt').command, /cargo fmt --all -- --check/);
  assert.match(report.lanes.find((lane) => lane.name === 'check').command, /cargo check --all-targets --locked/);
  assert.match(report.lanes.find((lane) => lane.name === 'clippy').command, /cargo clippy --all-targets --locked -- -D warnings/);
  assert.match(report.lanes.find((lane) => lane.name === 'rust-tests').command, /run-rust-tests\.mjs --json --profile full/);
  assert.match(report.lanes.find((lane) => lane.name === 'release-build').command, /cargo build --release --locked/);
});

test('Rust quality gate can intentionally narrow the Rust test profile in plan-only mode', () => {
  const result = runRustQuality(['--json', '--plan-only', '--test-profile', 'non-lifecycle']);

  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.policy.testsProfile, 'non-lifecycle');
  assert.match(report.lanes.find((lane) => lane.name === 'rust-tests').command, /--profile non-lifecycle/);
});

test('Rust quality gate can write an honest partial report when Cargo is unavailable', () => {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-rust-quality-'));
  const outputPath = path.join(tempDir, 'rust-quality.json');
  const result = spawnSync(
    process.execPath,
    ['scripts/verify-rust-quality.mjs', '--json', '--write', outputPath, '--allow-missing-cargo'],
    { ...CHILD_OPTIONS, env: cleanChildEnv({ PATH: tempDir }) },
  );

  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.equal(fs.existsSync(outputPath), true);
  const report = JSON.parse(fs.readFileSync(outputPath, 'utf8'));
  assert.equal(report.status, 'partial');
  assert.equal(report.toolchain.cargoAvailable, false);
  assert.deepEqual([...new Set(report.lanes.map((lane) => lane.status))], ['skipped']);
  assert.match(JSON.stringify(report.lanes), /cargo is not available/);
});

test('Rust quality gate is wired into package scripts, CI, and docs', () => {
  const packageJson = JSON.parse(read('package.json'));
  const ci = read('.github/workflows/ci.yml');
  const testStrategy = read('docs/test-strategy.md');
  const verification = read('docs/verification-matrix.md');
  const sourceQuality = read('docs/source-quality.md');

  assert.equal(packageJson.scripts['verify:rust-quality'], 'node scripts/verify-rust-quality.mjs --json --write reports/rust-quality-latest.json');
  assert.equal(packageJson.scripts['verify:rust-quality:plan'], 'node scripts/verify-rust-quality.mjs --json --plan-only');
  assert.equal(packageJson.scripts['prove:rust-host'], 'npm run verify:rust-quality');
  assert.equal(packageJson.scripts['lint:npm'], 'node scripts/check-node-syntax.mjs --json');
  assert.equal(fs.existsSync(path.join(repoRoot, 'scripts/verify-rust-quality.mjs')), true);
  assert.equal(fs.existsSync(path.join(repoRoot, 'tests/node/rust-quality-contract.test.js')), true);
  assert.match(ci, /verify:rust-quality/);
  assert.match(testStrategy, /verify:rust-quality/);
  assert.match(verification, /rust-quality-latest\.json/);
  assert.match(sourceQuality, /unsafe/i);
});
