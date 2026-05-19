const assert = require('node:assert/strict');
const fs = require('node:fs');
const { spawnSync } = require('node:child_process');
const test = require('node:test');

const root = process.cwd();

function read(rel) { return fs.readFileSync(rel, 'utf8'); }

test('mass MCP package survey is a safe fixture-replay gate', () => {
  const run = spawnSync(process.execPath, ['scripts/mcp-mass-package-survey.mjs', '--json', '--no-write'], { cwd: root, encoding: 'utf8', timeout: 30000 });
  assert.equal(run.status, 0, run.stderr || run.stdout);
  const report = JSON.parse(run.stdout);
  assert.equal(report.schema, 'mcpace.mcpMassPackageSurvey.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.mode, 'fixture-replay');
  assert.ok(report.summary.packageCount >= 100);
  assert.equal(report.safety.executesThirdPartyPackages, false);
  assert.equal(report.safety.startsMcpServers, false);
  assert.equal(report.safety.callsMcpTools, false);
  assert.equal(report.safety.packageInstallScriptsAllowed, false);
  assert.equal(report.safety.defaultServerEnablement, false);
  assert.ok(report.packages.every((pkg) => pkg.classification.executeDefault === false));
  assert.ok(report.packages.every((pkg) => Array.isArray(pkg.classification.locks) && pkg.classification.locks.length > 0));
});

test('mass survey live mode and install-lock benchmark are wired without random MCP execution', () => {
  const pkg = JSON.parse(read('package.json'));
  assert.match(pkg.scripts['verify:mcp-mass-package-survey'], /mcp-mass-package-survey\.mjs/);
  assert.match(pkg.scripts['verify:mcp-mass-package-survey:live100'], /--live --limit 100/);
  assert.match(pkg.scripts['verify:mcp-mass-package-survey:live100'], /--download-tarballs 10/);
  assert.match(pkg.scripts['verify:mcp-mass-package-survey'], /mcp-mass-package-survey-fixture-latest/);
  assert.match(pkg.scripts['benchmark:mcp-mass-package-install-lock'], /--resolve-install-lock/);
  assert.match(pkg.scripts['benchmark:mcp-mass-package-install-lock:chunked'], /--resolve-install-lock-chunks 10/);
  assert.match(pkg.scripts['benchmark:mcp-mass-package-install-lock:chunked-smoke'], /--resolve-install-lock-max-chunks 2/);
  const script = read('scripts/mcp-mass-package-survey.mjs');
  assert.match(script, /npm search/);
  assert.match(script, /npm install --package-lock-only --ignore-scripts/);
  assert.match(script, /npm pack --json --ignore-scripts/);
  assert.match(script, /resolveInstallLockChunks/);
  assert.match(script, /partial/);
  assert.doesNotMatch(script, /method:\s*['\"]tools\/call/);
  assert.doesNotMatch(script, /spawnSync\([^\n]+node_modules\/\.bin/);
  const report = JSON.parse(read('reports/mcp-mass-package-survey-latest.json'));
  assert.equal(report.status, 'pass');
  assert.equal(report.mode, 'live-npm-search-metadata');
  assert.equal(report.summary.packageCount, 100);
  assert.ok(report.summary.downloadedTarballs >= 10);
  assert.ok(report.summary.highRiskCount > 0);
  const installAttempt = JSON.parse(read('reports/mcp-mass-package-install-lock-attempt-100.json'));
  assert.equal(installAttempt.summary.packageCount, 100);
  assert.equal(installAttempt.safety.packageInstallScriptsAllowed, false);
  assert.ok(['blocked', 'pass'].includes(installAttempt.status));
  const chunkedSmoke = JSON.parse(read('reports/mcp-mass-package-install-lock-chunked-smoke-100.json'));
  assert.equal(chunkedSmoke.summary.packageCount, 100);
  assert.equal(chunkedSmoke.safety.packageInstallScriptsAllowed, false);
  assert.ok(chunkedSmoke.installLock.partial);
  assert.ok(chunkedSmoke.installLock.attemptedPackages > 0);
  assert.ok(chunkedSmoke.installLock.remainingPackages > 0);
});

test('mass survey docs explain metadata-only pressure testing', () => {
  const doc = read('docs/mcp-mass-package-survey-and-race-audit.md');
  assert.match(doc, /registry-pressure test/i);
  assert.match(doc, /does \*\*not\*\* start package bins/i);
  assert.match(doc, /does \*\*not\*\* call `tools\/call`/i);
  assert.match(doc, /disabled until explicit operator review/i);
});
