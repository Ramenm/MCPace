const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { cleanChildEnv, repoRoot } = require('./helpers');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('dashboard avoids stale refreshes and background-tab polling churn', () => {
  const html = read('src/dashboard/index.html');

  assert.match(html, /id="refresh-mode"/);
  assert.match(html, /document\.visibilityState/);
  assert.match(html, /visibilitychange/);
  assert.match(html, /AbortController/);
  assert.match(html, /refreshSeq/);
  assert.match(html, /refreshId !== state\.refreshSeq/);
  assert.match(html, /Promise\.allSettled/);
  assert.match(html, /Logs refresh failed/);
  assert.match(html, /scheduleAutoRefresh/);
  assert.doesNotMatch(html, /setInterval\s*\(/);
  assert.match(html, /__mcpaceDashboard/);
});

test('dashboard chaos smoke is wired into package scripts and release evidence', () => {
  const packageJson = JSON.parse(read('package.json'));
  const releaseManifest = JSON.parse(read('release-manifest.json'));
  const docs = read('docs/dashboard-chaos-verification.md');
  const matrix = read('reports/dashboard-chaos-scenario-matrix-20260516.md');

  assert.equal(
    packageJson.scripts['verify:dashboard-chaos'],
    'node scripts/dashboard-chaos-smoke.mjs --json --write reports/dashboard-chaos-smoke-latest.json --markdown reports/dashboard-chaos-smoke-latest.md'
  );
  assert.equal(
    packageJson.scripts['benchmark:dashboard-chaos'],
    'node scripts/dashboard-chaos-smoke.mjs --json --no-write --tabs 10 --events 160 --servers 250 --clients 40'
  );
  assert.match(packageJson.scripts['verify:experience'], /verify:performance/);
  assert.match(packageJson.scripts['verify:experience'], /verify:dashboard-chaos/);

  assert.ok(releaseManifest.includePaths.includes('reports/dashboard-chaos-smoke-latest.json'));
  assert.ok(releaseManifest.includePaths.includes('reports/dashboard-chaos-smoke-latest.md'));
  assert.ok(releaseManifest.includePaths.includes('reports/dashboard-chaos-scenario-matrix-20260516.md'));
  assert.match(docs, /visible-tab\s+resume/i);
  assert.match(matrix, /Many tabs open/);
});

test('dashboard chaos smoke exercises random tabs without a browser dependency', () => {
  const result = spawnSync(
    process.execPath,
    [
      'scripts/dashboard-chaos-smoke.mjs',
      '--json',
      '--no-write',
      '--tabs', '3',
      '--events', '30',
      '--servers', '60',
      '--clients', '10',
      '--max-elapsed-ms', '6000',
      '--max-operation-ms', '250',
    ],
    {
      cwd: repoRoot,
      encoding: 'utf8',
      env: cleanChildEnv(),
      timeout: 15000,
      windowsHide: true,
    }
  );

  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.dashboardChaosSmoke.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.scenario.tabs, 3);
  assert.equal(report.summary.totalOperations, 90);
  assert.ok(report.summary.maxOperationMs <= 250);
  assert.ok(report.summary.maxRenderMs <= 90);
  assert.ok(report.summary.abortedFetches > 0);
  assert.ok(report.summary.partialFailures > 0);
  assert.equal(report.checks.every((check) => check.ok), true);
});
