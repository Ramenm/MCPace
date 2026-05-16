const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { test } = require('node:test');

function read(path) {
  return readFileSync(path, 'utf8');
}

function readJson(path) {
  return JSON.parse(read(path));
}

const packageJson = readJson('package.json');
const releaseManifest = readJson('release-manifest.json');
const playwrightReport = readJson('reports/playwright-dashboard-e2e-latest.json');
const externalReport = readJson('reports/external-tool-internet-latest.json');
const liveExternalReport = readJson('reports/external-tool-internet-live-latest.json');
const docs = read('docs/browser-e2e-and-external-tooling.md');
const matrix = read('reports/external-tool-scenario-matrix-20260516.md');
const playwrightWrapper = read('scripts/playwright-dashboard-e2e.mjs');
const externalWrapper = read('scripts/external-tool-internet-smoke.mjs');
const playwrightSpec = read('tests/e2e/dashboard.playwright.spec.mjs');
const playwrightParallelSpec = read('tests/e2e/dashboard.parallel.playwright.spec.mjs');
const playwrightConfig = read('tests/e2e/playwright.config.mjs');
const overheadReport = readJson('reports/overhead-audit-latest.json');
const overheadScript = read('scripts/overhead-audit.mjs');

test('Playwright dashboard E2E lane is wired and backed by a real run report', () => {
  assert.equal(
    packageJson.scripts['verify:playwright-e2e'],
    'node scripts/playwright-dashboard-e2e.mjs --json --write reports/playwright-dashboard-e2e-latest.json --markdown reports/playwright-dashboard-e2e-latest.md'
  );
  assert.equal(playwrightReport.schema, 'mcpace.playwrightDashboardE2E.v2');
  assert.equal(playwrightReport.status, 'pass');
  assert.match(playwrightReport.tool.package, /@playwright\/test@/);
  assert.match(playwrightReport.summary.stdoutTail, /passed/);
  assert.ok(playwrightReport.checks.every((check) => check.ok));
  assert.match(playwrightSpec, /browser\.newContext/);
  assert.match(playwrightSpec, /Array\.from\(\{ length: 5 \}/);
  assert.match(playwrightSpec, /synthetic logs outage/);
  assert.match(playwrightSpec, /message\.type\(\) === 'error'/);
  assert.match(playwrightWrapper, /temporary npm install/);
  assert.match(playwrightParallelSpec, /test\.describe\.configure\(\{ mode: 'parallel' \}\)/);
  assert.match(playwrightParallelSpec, /__mcpaceClientSession/);
  assert.match(playwrightConfig, /fullyParallel: true/);
  assert.match(playwrightConfig, /MCPACE_PLAYWRIGHT_WORKERS/);
  assert.ok(playwrightReport.summary.parallelState.clientCount >= 4);
  assert.ok(playwrightReport.summary.parallelState.workerCount >= 2);
  assert.deepEqual(playwrightReport.summary.parallelState.conflicts, []);
});

test('external tool and internet scenarios cover local, package, remote, and API tools', () => {
  assert.equal(
    packageJson.scripts['verify:external-tool-internet'],
    'node scripts/external-tool-internet-smoke.mjs --json --write reports/external-tool-internet-latest.json --markdown reports/external-tool-internet-latest.md'
  );
  assert.equal(
    packageJson.scripts['verify:external-tool-internet:live'],
    'node scripts/external-tool-internet-smoke.mjs --json --live-internet --write reports/external-tool-internet-live-latest.json --markdown reports/external-tool-internet-live-latest.md'
  );
  assert.equal(externalReport.schema, 'mcpace.externalToolInternetSmoke.v1');
  assert.equal(externalReport.status, 'pass');
  const categories = new Set(externalReport.scenarios.map((scenario) => scenario.category));
  for (const expected of ['local-only', 'package-manager', 'container-runtime', 'external-api', 'external-web', 'remote-mcp']) {
    assert.ok(categories.has(expected), `missing ${expected}`);
  }
  assert.match(externalWrapper, /does not execute third-party MCP packages/);
  assert.match(matrix, /npx-launched stdio/);
  assert.match(matrix, /Remote Streamable HTTP/);
});

test('live internet report is explicit when host policy blocks direct outbound checks', () => {
  assert.equal(liveExternalReport.schema, 'mcpace.externalToolInternetSmoke.v1');
  assert.equal(liveExternalReport.mode, 'live-internet');
  assert.ok(['pass', 'blocked'].includes(liveExternalReport.status));
  assert.ok(Array.isArray(liveExternalReport.liveResults));
  assert.ok(liveExternalReport.liveResults.length > 0);
  if (liveExternalReport.status === 'blocked') {
    assert.ok(liveExternalReport.notes.some((note) => /blocked by the current host\/network policy/i.test(note)));
  }
});


test('overhead audit keeps test tooling out of runtime and measures launcher cost', () => {
  assert.equal(
    packageJson.scripts['verify:overhead-audit'],
    'node scripts/overhead-audit.mjs --json --write reports/overhead-audit-latest.json --markdown reports/overhead-audit-latest.md'
  );
  assert.equal(overheadReport.schema, 'mcpace.overheadAudit.v1');
  assert.equal(overheadReport.status, 'pass');
  assert.ok(overheadReport.packageFootprint.rootDependencyCount === 0);
  assert.ok(overheadReport.packageFootprint.cliRuntimeDependencyCount === 0);
  assert.ok(overheadReport.fileSizes['src/dashboard/index.html'] < 100000);
  assert.match(overheadScript, /measureLauncherOverhead/);
  assert.match(overheadScript, /temporary prefix|Playwright is not a runtime dependency|PLAYWRIGHT/);
});

test('docs and release manifest include the browser/live tooling evidence', () => {
  assert.match(docs, /verify:playwright-e2e/);
  assert.match(docs, /Puppeteer/);
  assert.match(docs, /Cypress/);
  assert.match(docs, /verify:external-tool-internet:live/);
  assert.match(docs, /Parallel client\/session isolation lane/);
  assert.match(docs, /verify:overhead-audit/);
  for (const required of [
    'reports/playwright-dashboard-e2e-latest.json',
    'reports/playwright-dashboard-e2e-latest.md',
    'reports/external-tool-internet-latest.json',
    'reports/external-tool-internet-latest.md',
    'reports/external-tool-internet-live-latest.json',
    'reports/external-tool-internet-live-latest.md',
    'reports/external-tool-scenario-matrix-20260516.md',
    'reports/playwright-parallel-session-matrix-20260516.md',
    'reports/overhead-audit-latest.json',
    'reports/overhead-audit-latest.md'
  ]) {
    assert.ok(releaseManifest.includePaths.includes(required), `missing ${required}`);
  }
});
