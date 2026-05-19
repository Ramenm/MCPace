const assert = require('node:assert/strict');
const fs = require('node:fs');
const { spawnSync } = require('node:child_process');
const test = require('node:test');

const root = process.cwd();
function read(rel) { return fs.readFileSync(rel, 'utf8'); }

test('race-condition audit fuzzes multi-client/session/credential scheduler boundaries', () => {
  const run = spawnSync(process.execPath, ['scripts/mcp-race-condition-audit.mjs', '--json', '--strict', '--iterations', '1200', '--no-write'], { cwd: root, encoding: 'utf8', timeout: 30000 });
  assert.equal(run.status, 0, run.stderr || run.stdout);
  const report = JSON.parse(run.stdout);
  assert.equal(report.schema, 'mcpace.mcpRaceConditionAudit.v1');
  assert.equal(report.status, 'pass');
  assert.ok(report.summary.started > 0);
  assert.ok(report.summary.blockedDisabled > 0);
  assert.ok(report.summary.blockedUnknownTool > 0);
  assert.ok(report.summary.blockedReviewGate > 0);
  assert.deepEqual(report.simulation.violations, []);
  assert.ok(report.checks.every((check) => check.ok), JSON.stringify(report.checks, null, 2));
});

test('race audit is wired into orchestration and checks fail-closed sources', () => {
  const pkg = JSON.parse(read('package.json'));
  assert.match(pkg.scripts['verify:mcp-race-conditions'], /mcp-race-condition-audit\.mjs/);
  assert.match(pkg.scripts['verify:orchestration'], /verify:mcp-race-conditions/);
  const script = read('scripts/mcp-race-condition-audit.mjs');
  assert.match(script, /blocked-disabled/);
  assert.match(script, /blocked-unknown-tool/);
  assert.match(script, /blocked-review-gate/);
  assert.match(script, /transport-session/);
  assert.match(script, /credentialProfile/);
  assert.match(script, /browser-context/);
  const latest = JSON.parse(read('reports/mcp-race-condition-audit-latest.json'));
  assert.equal(latest.status, 'pass');
  assert.equal(latest.summary.blockers, 0);
});
