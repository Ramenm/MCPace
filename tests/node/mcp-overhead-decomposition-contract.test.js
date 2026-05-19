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
const report = readJson('reports/mcp-overhead-decomposition-latest.json');
const markdown = read('reports/mcp-overhead-decomposition-latest.md');
const script = read('scripts/mcp-overhead-decomposition.mjs');
const runtimeBenchmark = read('scripts/benchmark-runtime.mjs');
const performanceDocs = read('docs/performance-verification.md');
const adapterDiscovery = read('src/adapter/discovery.rs');

test('MCP overhead decomposition gate is wired into experience checks', () => {
  assert.equal(
    packageJson.scripts['verify:mcp-overhead-decomposition'],
    'node scripts/mcp-overhead-decomposition.mjs --json --write reports/mcp-overhead-decomposition-latest.json --markdown reports/mcp-overhead-decomposition-latest.md'
  );
  assert.match(packageJson.scripts['benchmark:mcp-overhead-decomposition'], /--servers 250/);
  assert.match(packageJson.scripts['verify:overhead:deep'], /verify:mcp-overhead-decomposition/);
  assert.ok(releaseManifest.includePaths.includes('reports/mcp-overhead-decomposition-latest.json'));
  assert.ok(releaseManifest.includePaths.includes('reports/mcp-overhead-decomposition-latest.md'));
});

test('MCP overhead report proves route, projection, scheduler, and JSON overhead boundaries', () => {
  assert.equal(report.schema, 'mcpace.mcpOverheadDecomposition.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.safety.startsMcpServers, false);
  assert.equal(report.safety.callsMcpTools, false);
  assert.equal(report.safety.executesThirdPartyPackages, false);
  assert.ok(report.scenario.servers >= 100);
  assert.ok(report.scenario.toolCount >= 5000);
  assert.ok(report.summary.routeIndexSpeedup >= 5);
  assert.ok(report.summary.projectionCacheHitSpeedup >= 20);
  assert.ok(report.summary.schedulerPerOperationMs < 0.01);
  assert.ok(report.summary.smallJsonRpcPerOperationMs < 0.02);
  assert.ok(report.summary.metadataClassifierPerOperationMs < 0.25);
  assert.deepEqual(report.blockers, []);
  assert.ok(report.checks.every((check) => check.ok), 'all overhead checks must pass');
  assert.match(markdown, /Route index speedup/);
  assert.match(markdown, /Visibility cache-hit speedup/);
});

test('overhead script locks the important optimizations without executing random servers', () => {
  assert.match(script, /buildRouteIndex/);
  assert.match(script, /byQualifiedName/);
  assert.match(script, /visibility-projection-cache-hit/);
  assert.match(script, /scheduler-lock-cycle/);
  assert.match(script, /does-not-start-random-mcp-servers/);
  assert.doesNotMatch(script, /tools\/call/);
});

test('runtime HTTP benchmark defaults to keep-alive and can still measure connection churn explicitly', () => {
  assert.match(runtimeBenchmark, /keepAlive: true/);
  assert.match(runtimeBenchmark, /--no-keep-alive/);
  assert.match(runtimeBenchmark, /Connection: keepAlive \? 'keep-alive' : 'close'/);
  assert.match(runtimeBenchmark, /new Agent\(\{/);
  assert.match(performanceDocs, /keep-alive/i);
});

test('Rust adapter search keeps bounded top-k insertion instead of full resort per match', () => {
  assert.match(adapterDiscovery, /fn insert_scored_tool_bounded/);
  const topKFunction = adapterDiscovery.slice(adapterDiscovery.indexOf('fn insert_scored_tool_bounded'), adapterDiscovery.indexOf('#[derive(Clone, Debug)]'));
  assert.match(topKFunction, /scored\.insert\(position, item\)/);
  assert.match(topKFunction, /if position >= keep_limit/);
  assert.doesNotMatch(topKFunction, /sort_by_key/);
  assert.doesNotMatch(topKFunction, /key\.clone\(\)/);
});
