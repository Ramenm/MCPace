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

test('defect gates encode bug intake, triage, and regression-proof requirements', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-defect-gates-'));
  const jsonPath = path.join(tmpDir, 'defect-gates.json');
  const mdPath = path.join(tmpDir, 'defect-gates.md');
  const result = runNode(['scripts/defect-gates.mjs', '--json', '--write', jsonPath, '--markdown', mdPath]);
  const report = JSON.parse(result.stdout);

  assert.equal(report.schema, 'mcpace.defectGates.v1');
  assert.equal(report.status, 'pass', JSON.stringify(report.checks.filter((check) => check.status !== 'pass')));
  assert.ok(report.summary.total >= 10);
  assert.equal(report.summary.blockers, 0);
  assert.ok(report.checks.some((check) => check.id === 'session-fixation-guard'));
  assert.ok(report.checks.some((check) => check.id === 'nonlocal-bind-guard'));
  assert.ok(report.operatingModel.repair.includes('minimal failing test'));

  assert.equal(JSON.parse(fs.readFileSync(jsonPath, 'utf8')).schema, 'mcpace.defectGates.v1');
  assert.match(fs.readFileSync(mdPath, 'utf8'), /MCPace defect gates/);
});

test('package script exposes defect gates for CI and local maintainer runs', () => {
  const pkg = JSON.parse(fs.readFileSync(path.join(repoRoot, 'package.json'), 'utf8'));
  assert.equal(
    pkg.scripts['verify:defect-gates'],
    'node scripts/defect-gates.mjs --json --write reports/defect-gates-latest.json --markdown reports/defect-gates-latest.md'
  );
  assert.match(fs.readFileSync(path.join(repoRoot, '.github', 'workflows', 'ci.yml'), 'utf8'), /npm run verify:defect-gates/);
});
