const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const repoRoot = path.resolve(__dirname, '..', '..');

function runNode(args) {
  const result = spawnSync('node', args, { cwd: repoRoot, encoding: 'utf8', timeout: 60_000, maxBuffer: 4 * 1024 * 1024 });
  assert.equal(result.status, 0, `${args.join(' ')} failed\nSTDOUT:\n${result.stdout}\nSTDERR:\n${result.stderr}`);
  return result;
}

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

test('runtime trace harness is explicit when a compiled binary is not staged', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-runtime-trace-'));
  const jsonPath = path.join(tmpDir, 'runtime-trace.json');
  const mdPath = path.join(tmpDir, 'runtime-trace.md');
  const result = runNode(['scripts/runtime-trace-harness.mjs', '--json', '--binary', path.join(tmpDir, 'missing-mcpace'), '--write', jsonPath, '--markdown', mdPath]);
  const report = JSON.parse(result.stdout);

  assert.equal(report.schema, 'mcpace.runtimeTraceHarness.v1');
  assert.equal(report.status, 'blocked');
  assert.match(report.blockers.join('\n'), /binary/i);
  assert.ok(report.nextCommands.some((command) => command.includes('cargo build --release --locked')));
  assert.equal(JSON.parse(fs.readFileSync(jsonPath, 'utf8')).schema, 'mcpace.runtimeTraceHarness.v1');
  assert.match(fs.readFileSync(mdPath, 'utf8'), /runtime trace harness/i);
});

test('product practice harness separates source health from runtime and published install claims', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-product-practice-'));
  const jsonPath = path.join(tmpDir, 'practice.json');
  const mdPath = path.join(tmpDir, 'practice.md');
  const result = runNode(['scripts/product-practice-harness.mjs', '--json', '--write', jsonPath, '--markdown', mdPath]);
  const report = JSON.parse(result.stdout);

  assert.equal(report.schema, 'mcpace.productPractice.v1');
  assert.equal(report.canClaim.sourceTreeHealthy, true);
  assert.equal(report.canClaim.universalRemoteMcpBroker, false);
  const runtimeGate = report.gates.find((gate) => gate.id === 'runtime-trace');
  assert.ok(runtimeGate, 'runtime-trace gate must stay explicit');
  assert.equal(report.canClaim.runtimeBeta, runtimeGate.status === 'pass');
  assert.ok(report.gates.some((gate) => gate.id === 'published-binary-install' && gate.status === 'blocked'));
  assert.match(report.wrongPracticeRisks.join('\n'), /feel done|not the same/i);
  assert.ok(report.nextMoves.some((move) => /cargo check|runtime|binary|Stage/i.test(move)));
  assert.equal(JSON.parse(fs.readFileSync(jsonPath, 'utf8')).schema, 'mcpace.productPractice.v1');
  assert.match(fs.readFileSync(mdPath, 'utf8'), /product-practice harness/i);
});

test('package scripts expose compact source lint and product-practice proof lanes', () => {
  const pkg = readJson('package.json');
  assert.equal(pkg.scripts['lint:npm'], 'node scripts/check-node-syntax.mjs --json');
  assert.equal(pkg.scripts['verify:runtime-trace'], 'node scripts/runtime-trace-harness.mjs --json --write reports/runtime-trace-latest.json --markdown reports/runtime-trace-latest.md');
  assert.equal(pkg.scripts['verify:product-practice'], 'node scripts/product-practice-harness.mjs --json --write reports/product-practice-latest.json --markdown reports/product-practice-latest.md');

  const syntax = runNode(['scripts/check-node-syntax.mjs', '--json', '--list']);
  const report = JSON.parse(syntax.stdout);
  assert.ok(report.files.includes('scripts/runtime-trace-harness.mjs'));
  assert.ok(report.files.includes('scripts/product-practice-harness.mjs'));
  assert.ok(report.files.includes('tests/node/product-practice-contract.test.js'));
});
