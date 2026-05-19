const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const repoRoot = path.resolve(__dirname, '..', '..');

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

test('eval fixture checker reports governance coverage without provider calls', () => {
  const result = spawnSync(process.execPath, ['scripts/eval-fixture-check.mjs', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: {
      PATH: process.env.PATH,
      HOME: process.env.HOME,
      TMPDIR: process.env.TMPDIR,
    },
    timeout: 15_000,
  });

  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace-eval-fixture-check.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.issues.length, 0);
  assert.ok(report.coverage.seedFixtureCount >= 25);
  assert.ok(report.coverage.runtimeFixtureCount >= 16);
  assert.ok(report.coverage.seedSplitCounts.typical >= 4);
  assert.ok(report.coverage.seedSplitCounts.adversarial >= 11);
  assert.ok(report.coverage.guardrailCases >= 6);
});

test('autonomous-agent workloop fixtures are first-class eval scenarios', () => {
  const matrix = readJson('eval/scenario-matrix.json');
  const family = matrix.families.find((value) => value.id === 'autonomous-agent-workloop');
  assert.ok(family, 'autonomous-agent-workloop family should exist');
  assert.equal(family.prevalence, 'high');
  assert.ok(family.seedTypical.includes('autonomous-agent-workloop-triage'));
  assert.ok(family.seedAdversarial.includes('endless-autonomy-overpromise'));

  for (const id of ['autonomous-agent-workloop-triage', 'endless-autonomy-overpromise']) {
    const fixture = readJson(path.join('eval', 'fixtures', 'seed', `${id}.json`));
    assert.equal(fixture.track, 'seed-prompt');
    assert.equal(fixture.bucket, 'autonomous-agent-workloop');
    assert.ok(fixture.expected.good.some((item) => /verify|провер/i.test(item)) || fixture.expected.good.some((item) => /blocker|блокер/i.test(item)));
    assert.ok(fixture.expected.unacceptable.some((item) => /invent|claim|secret|background|production/i.test(item)));
    assert.ok(Array.isArray(fixture.grounding.evidence));
    assert.ok(fixture.grounding.evidence.length >= 2);
  }
});
