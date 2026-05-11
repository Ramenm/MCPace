const test = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { readJson, read } = require('./helpers');

test('bug sweep automation is wired into package scripts, CI, docs, and PR hygiene', () => {
  const pkg = readJson('package.json');
  const ci = read('.github/workflows/ci.yml');
  const prTemplate = read('.github/pull_request_template.md');
  const playbook = read('docs/bug-hunting-and-fix-playbook.md');
  const taxonomy = read('docs/defect-taxonomy-and-labels.md');
  const debugging = read('docs/maintainer-debugging-guide.md');

  assert.match(pkg.scripts['verify:bug-sweep'], /scripts\/bug-sweep\.mjs/);
  assert.match(pkg.scripts.test, /verify:bug-sweep/);
  assert.match(ci, /Verify bug-fix hygiene sweep/);
  assert.match(ci, /npm run verify:bug-sweep/);
  assert.match(prTemplate, /Root cause/);
  assert.match(prTemplate, /Regression test|Regression test or proof artifact/);
  assert.match(prTemplate, /Not-tested/);
  assert.match(playbook, /Reproduce before changing code/);
  assert.match(playbook, /State the root cause/);
  assert.match(playbook, /Add the regression test/);
  assert.match(taxonomy, /type:regression/);
  assert.match(taxonomy, /area:mcp-http/);
  assert.match(debugging, /Runtime HTTP checklist/);
  assert.match(debugging, /Flaky test checklist/);
});

test('bug sweep report passes current source invariants', () => {
  const result = spawnSync(process.execPath, ['scripts/bug-sweep.mjs', '--json'], {
    cwd: process.cwd(),
    encoding: 'utf8',
    maxBuffer: 1024 * 1024 * 8,
    windowsHide: true
  });

  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.bugSweep.v1');
  assert.match(report.status, /^pass/);
  assert.equal(report.summary.blocked, 0);
  assert.ok(report.checks.some((check) => check.id === 'runtime:http-origin-host-boundary' && check.status === 'pass'));
  assert.ok(report.checks.some((check) => check.id === 'runtime:server-minted-session-id' && check.status === 'pass'));
});
