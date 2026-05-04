const assert = require('node:assert');
const { spawnSync } = require('node:child_process');
const test = require('node:test');
const { repoRoot, read, readJson, cleanChildEnv } = require('./helpers');

function runNode(args) {
  return spawnSync(process.execPath, args, { cwd: repoRoot, encoding: 'utf8', env: cleanChildEnv(), maxBuffer: 8 * 1024 * 1024 });
}

test('publish decision separates public source snapshot from native runtime publication', () => {
  const script = read('scripts/publish-decision.mjs');
  assert.match(script, /okForPublicSourceSnapshot/);
  assert.match(script, /okForNpmNativePublication/);
  assert.match(script, /paidGithubRequired:\s*false/);
  assert.match(script, /rust-quality/);
  assert.match(script, /runtime-trace/);
  assert.match(script, /vendored-binary/);
});

test('local secret and supply-chain scripts expose JSON reports', () => {
  for (const args of [
    ['scripts/secret-scan.mjs', '--json'],
    ['scripts/supply-chain-risk-audit.mjs', '--json', '--timeout-ms', '3000'],
    ['scripts/free-tier-readiness.mjs', '--json'],
  ]) {
    const result = runNode(args);
    assert.equal(result.status, 0, `${args.join(' ')}\n${result.stderr}\n${result.stdout}`);
    const report = JSON.parse(result.stdout);
    assert.ok(report.schema.startsWith('mcpace.'));
    assert.equal(report.githubPaidPlanRequired ?? report.policy?.paidGithubRequired ?? false, false);
  }
});

test('package scripts expose local source, release, and final decision gates', () => {
  const pkg = readJson('package.json');
  assert.match(pkg.scripts['verify:local:smoke'], /local-quality-smoke\.sh/);
  assert.match(pkg.scripts['verify:local:source'], /local-quality-suite\.mjs --profile source/);
  assert.match(pkg.scripts['verify:local:release'], /local-quality-suite\.mjs --profile release/);
  assert.match(pkg.scripts['verify:local:publish'], /verify:publish-decision/);
  assert.equal(pkg.scripts['verify:publish-decision'], 'node scripts/publish-decision.mjs --json --write reports/publish-decision-latest.json --markdown reports/publish-decision-latest.md');
});
