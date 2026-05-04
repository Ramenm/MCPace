const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const repoRoot = path.resolve(__dirname, '..', '..');

function runNode(args) {
  const result = spawnSync('node', args, {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: 60_000,
    maxBuffer: 4 * 1024 * 1024,
  });
  assert.equal(result.status, 0, `${args.join(' ')} failed\nSTDOUT:\n${result.stdout}\nSTDERR:\n${result.stderr}`);
  return result;
}

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

test('product-practice proof lanes separate source health, runtime proof, and scripts', () => {
  {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-runtime-trace-'));
    const jsonPath = path.join(tmpDir, 'runtime-trace.json');
    const mdPath = path.join(tmpDir, 'runtime-trace.md');
    const result = runNode([
      'scripts/runtime-trace-harness.mjs',
      '--json',
      '--binary',
      path.join(tmpDir, 'missing-mcpace'),
      '--write',
      jsonPath,
      '--markdown',
      mdPath,
    ]);
    const report = JSON.parse(result.stdout);

    assert.equal(report.schema, 'mcpace.runtimeTraceHarness.v1');
    assert.equal(report.status, 'blocked');
    assert.match(report.blockers.join('\n'), /binary/i);
    assert.ok(report.nextCommands.some((command) => command.includes('cargo build --release --locked')));
    assert.equal(JSON.parse(fs.readFileSync(jsonPath, 'utf8')).schema, 'mcpace.runtimeTraceHarness.v1');
    assert.match(fs.readFileSync(mdPath, 'utf8'), /runtime trace harness/i);
  }

  {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-product-practice-'));
    const jsonPath = path.join(tmpDir, 'practice.json');
    const mdPath = path.join(tmpDir, 'practice.md');
    const result = runNode(['scripts/product-practice-harness.mjs', '--json', '--write', jsonPath, '--markdown', mdPath]);
    const report = JSON.parse(result.stdout);

    assert.equal(report.schema, 'mcpace.productPractice.v1');
    assert.equal(report.canClaim.sourceTreeHealthy, true);
    assert.equal(report.canClaim.universalRemoteMcpBroker, false);
    const rustGate = report.gates.find((gate) => gate.id === 'rust-build');
    assert.ok(rustGate, 'rust-build gate must stay explicit');
    const runtimeGate = report.gates.find((gate) => gate.id === 'runtime-trace');
    assert.ok(runtimeGate, 'runtime-trace gate must stay explicit');
    assert.equal(report.canClaim.runtimeBeta, rustGate.status === 'pass' && runtimeGate.status === 'pass');
    const binaryGate = report.gates.find((gate) => gate.id === 'published-binary-install');
    assert.ok(binaryGate, 'published-binary-install gate must stay explicit');
    assert.equal(report.canClaim.publishedBinaryInstall, binaryGate.status === 'pass');
    assert.ok(report.proofValidity.vendoredBinary, 'published binary claims must be backed by a vendored-binary proof report');
    assert.ok(report.freshness.vendoredBinary, 'vendored-binary proof freshness must stay visible');
    assert.match(binaryGate.evidence, /vendored-binary|binary|missing|stale|version|target/i);
    if (report.status === 'ready-for-release-candidate-review') {
      assert.deepEqual(report.wrongPracticeRisks, []);
    } else {
      assert.match(report.wrongPracticeRisks.join('\n'), /feel done|not the same|stale proof/i);
    }
    assert.ok(report.nextMoves.some((move) => /cargo check|runtime|binary|Stage|Refresh/i.test(move)) || report.status === 'ready-for-release-candidate-review');
    if (Object.values(report.freshness).some((entry) => entry.status === 'stale')) {
      assert.ok(report.nextMoves.some((move) => /Refresh .*stale/i.test(move)), 'stale proof warnings should produce an actionable next move');
    }
    assert.equal(JSON.parse(fs.readFileSync(jsonPath, 'utf8')).schema, 'mcpace.productPractice.v1');
    assert.match(fs.readFileSync(mdPath, 'utf8'), /product-practice harness/i);
  }

  {
    const pkg = readJson('package.json');
    assert.equal(pkg.scripts['lint:npm'], 'node scripts/check-node-syntax.mjs --json');
    assert.equal(
      pkg.scripts['verify:runtime-trace'],
      'node scripts/runtime-trace-harness.mjs --json --write reports/runtime-trace-latest.json --markdown reports/runtime-trace-latest.md'
    );
    assert.equal(
      pkg.scripts['verify:product-practice'],
      'node scripts/product-practice-harness.mjs --json --write reports/product-practice-latest.json --markdown reports/product-practice-latest.md'
    );

    assert.ok(fs.existsSync(path.join(repoRoot, 'scripts/runtime-trace-harness.mjs')));
    assert.ok(fs.existsSync(path.join(repoRoot, 'scripts/product-practice-harness.mjs')));
    assert.ok(fs.existsSync(path.join(repoRoot, 'tests/node/product-practice-contract.test.js')));
  }
});
