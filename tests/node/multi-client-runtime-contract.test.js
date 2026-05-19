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
const report = readJson('reports/multi-client-runtime-audit-latest.json');
const releaseManifest = readJson('release-manifest.json');
const script = read('scripts/multi-client-runtime-audit.mjs');
const resources = read('src/resources.rs');
const context = read('src/client/context.rs');
const dashboard = read('src/dashboard.rs');
const toolRuntime = read('src/dashboard/tool_runtime.rs');
const leases = read('src/hub/leases.rs');
const docs = `${read('docs/multi-client-runtime.md')}\n${read('docs/universal-runtime-policy.md')}\n${read('docs/browser-e2e-and-external-tooling.md')}`;
const playwrightParallelSpec = read('tests/e2e/dashboard.parallel.playwright.spec.mjs');
const playwrightConfig = read('tests/e2e/playwright.config.mjs');

test('multi-client runtime audit is wired into package verification', () => {
  assert.equal(
    packageJson.scripts['verify:multi-client-runtime'],
    'node scripts/multi-client-runtime-audit.mjs --json --write reports/multi-client-runtime-audit-latest.json --markdown reports/multi-client-runtime-audit-latest.md'
  );
  assert.match(packageJson.scripts['verify:experience'], /verify:multi-client-runtime/);
  assert.match(packageJson.scripts['verify:browser-experience'], /verify:multi-client-runtime/);
  assert.match(script, /mcpace\.multiClientRuntimeAudit\.v1/);
});

test('multi-client runtime audit report passes with explicit accepted limits', () => {
  assert.equal(report.schema, 'mcpace.multiClientRuntimeAudit.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.summary.failed, 0);
  assert.ok(report.summary.poolMax >= 4);
  assert.ok(report.summary.shardMax >= 2);
  assert.ok(report.checks.every((check) => check.ok));
  assert.ok(report.acceptedLimits.some((item) => item.id === 'stdio-clients-without-any-session-signal'));
});

test('default upstream pool no longer uses a single global shard on multicore hosts', () => {
  assert.match(resources, /const AUTO_UPSTREAM_SESSION_POOL_MAX: usize = [4-9]/);
  assert.match(resources, /const AUTO_UPSTREAM_SESSION_SHARD_MAX: usize = [2-9]/);
  assert.match(dashboard, /context\.client_id\.hash/);
  assert.match(dashboard, /context\.session_id\.hash/);
  assert.match(dashboard, /context\.project_root\.hash/);
  assert.match(dashboard, /context\.transport\.hash/);
});

test('stdio multi-client isolation limitation is visible instead of silent', () => {
  assert.match(context, /multiple live instances of the same client/i);
  assert.match(context, /MCPACE_SESSION_ID/);
  assert.match(context, /MCPACE_CLIENT_INSTANCE_ID/);
  assert.match(docs, /What is not fully automatic/);
  assert.match(docs, /cannot invent a strictly unique/i);
});

test('HTTP sessions, hub leases, and Playwright contexts cover separate client paths', () => {
  assert.match(toolRuntime, /mcp-session-id/);
  assert.match(toolRuntime, /x-mcpace-session-id/);
  assert.match(toolRuntime, /x-mcp-client-id/);
  assert.match(leases, /requestMutexKey/);
  assert.match(leases, /parallelismLimit/);
  assert.match(leases, /takeover_allowed/);
  assert.match(playwrightParallelSpec, /browser\.newContext/);
  assert.match(playwrightParallelSpec, /test\.describe\.configure\(\{ mode: 'parallel' \}\)/);
  assert.match(playwrightConfig, /fullyParallel: true/);
  assert.match(playwrightConfig, /MCPACE_PLAYWRIGHT_WORKERS/);
});

test('release manifest includes multi-client audit evidence', () => {
  assert.ok(releaseManifest.includePaths.includes('reports/multi-client-runtime-audit-latest.json'));
  assert.ok(releaseManifest.includePaths.includes('reports/multi-client-runtime-audit-latest.md'));
});
