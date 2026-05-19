const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { test } = require('node:test');

const packageJson = JSON.parse(readFileSync('package.json', 'utf8'));
const releaseManifest = JSON.parse(readFileSync('release-manifest.json', 'utf8'));
const scenarioScript = readFileSync('scripts/mcp-install-scenario-smoke.mjs', 'utf8');
const docs = readFileSync('docs/mcp-server-install-scenarios.md', 'utf8');
const matrix = readFileSync('reports/mcp-install-scenario-matrix-20260516.md', 'utf8');
const latestReport = JSON.parse(readFileSync('reports/mcp-install-scenario-smoke-latest.json', 'utf8'));

test('MCP install scenario smoke is a first-class verification command', () => {
  assert.equal(
    packageJson.scripts['verify:mcp-install-scenarios'],
    'node scripts/mcp-install-scenario-smoke.mjs --json --write reports/mcp-install-scenario-smoke-latest.json --markdown reports/mcp-install-scenario-smoke-latest.md'
  );
  assert.equal(
    packageJson.scripts['benchmark:mcp-install-scenarios'],
    'node scripts/mcp-install-scenario-smoke.mjs --json --no-write --servers 100'
  );
  assert.match(scenarioScript, /auto-install-dry-run-is-config-only/);
  assert.match(scenarioScript, /reinstall-without-force-is-blocked/);
  assert.match(scenarioScript, /hundred-server-config-scale/);
  assert.match(scenarioScript, /paid-server-can-be-registered-disabled/);
});

test('MCP install scenario evidence covers domain ownership, cost, transport, and scale boundaries', () => {
  assert.match(docs, /Registration does not download an npm\/PyPI package, start a process, call a remote endpoint, or invoke a tool/);
  assert.match(docs, /The MCPace endpoint domain/);
  assert.match(docs, /The upstream MCP server domain/);
  assert.match(docs, /Cost and billing boundaries/);
  assert.match(docs, /Configure 100 servers/);
  assert.match(matrix, /Remote domains/);
  assert.match(matrix, /Package manager/);
  assert.match(matrix, /Ownership/);
});

test('latest MCP install scenario report is included in release evidence allowlist', () => {
  assert.equal(latestReport.schema, 'mcpace.mcpInstallScenarioSmoke.v1');
  assert.equal(latestReport.status, 'pass');
  const ids = latestReport.checks.map((check) => check.id);
  assert.ok(ids.includes('auto-install-dry-run-is-config-only'));
  assert.ok(ids.includes('reinstall-without-force-is-blocked'));
  assert.ok(ids.includes('remote-http-server-add'));
  assert.ok(ids.includes('paid-server-can-be-registered-disabled'));
  assert.ok(ids.includes('hundred-server-config-scale'));
  assert.ok(latestReport.observations.some((item) => item.includes('does not download packages')));
  assert.ok(releaseManifest.includePaths.includes('reports/mcp-install-scenario-smoke-latest.json'));
  assert.ok(releaseManifest.includePaths.includes('reports/mcp-install-scenario-smoke-latest.md'));
  assert.ok(releaseManifest.includePaths.includes('reports/mcp-install-scenario-matrix-20260516.md'));
  assert.ok(releaseManifest.includePaths.includes('reports/install-readiness-latest.json'));
});
