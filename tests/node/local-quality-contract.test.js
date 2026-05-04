const assert = require('node:assert');
const { spawnSync } = require('node:child_process');
const test = require('node:test');
const { repoRoot, read, readJson, cleanChildEnv } = require('./helpers');

function runNode(args) {
  return spawnSync(process.execPath, args, { cwd: repoRoot, encoding: 'utf8', env: cleanChildEnv(), maxBuffer: 8 * 1024 * 1024 });
}

test('local-first quality scripts are wired for workstation or self-hosted verification', () => {
  const pkg = readJson('package.json');
  const readme = read('README.md');
  const offlineDocs = read('docs/offline-quality-and-publish-gates.md');
  const localDocs = read('docs/local-quality-without-paid-github.md');

  assert.match(pkg.scripts['verify:toolbox'], /scripts\/toolbox-doctor\.mjs/);
  assert.match(pkg.scripts['verify:local:smoke'], /scripts\/local-quality-smoke\.sh/);
  assert.match(pkg.scripts['verify:local:source'], /scripts\/local-quality-suite\.mjs --profile source/);
  assert.match(pkg.scripts['verify:local:full'], /scripts\/local-quality-suite\.mjs --profile full/);
  assert.match(pkg.scripts['verify:local:release'], /scripts\/local-quality-suite\.mjs --profile release/);
  assert.match(pkg.scripts['verify:secrets'], /scripts\/secret-scan\.mjs/);
  assert.match(pkg.scripts['verify:supply-chain'], /scripts\/supply-chain-risk-audit\.mjs/);
  assert.match(pkg.scripts['verify:free-tier'], /scripts\/free-tier-readiness\.mjs/);
  assert.match(pkg.scripts['verify:publish-decision'], /scripts\/publish-decision\.mjs/);
  assert.match(pkg.scripts['test:repo'], /--batch-size 8/);
  assert.match(pkg.scripts['test:repo:smoke'], /--only local-quality-contract/);
  assert.match(readme, /verify:local:source/);
  assert.match(readme, /verify:publish-decision/);
  assert.match(offlineDocs, /okForPublicSourceSnapshot/);
  assert.match(localDocs, /No paid-plan assumption/);
});

test('toolbox doctor emits a structured local tooling report', () => {
  const result = runNode(['scripts/toolbox-doctor.mjs', '--json']);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.toolboxDoctor.v1');
  assert.equal(report.githubPaidPlanRequired, false);
  assert.ok(Array.isArray(report.commands));
  assert.ok(report.commands.some((cmd) => cmd.command === 'npm run verify:local:source'));
});

test('local quality suite can render all local profiles without executing them', () => {
  for (const profile of ['smoke', 'source', 'full', 'release']) {
    const result = runNode(['scripts/local-quality-suite.mjs', '--profile', profile, '--json', '--plan-only']);
    assert.equal(result.status, 0, `${profile}: ${result.stderr || result.stdout}`);
    const report = JSON.parse(result.stdout);
    assert.equal(report.schema, 'mcpace.localQualitySuite.v1');
    assert.equal(report.profile, profile);
    assert.equal(report.status, 'planned');
    assert.ok(report.steps.length > 0);
  }
});
