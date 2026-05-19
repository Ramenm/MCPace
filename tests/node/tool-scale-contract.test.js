const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const repoRoot = path.resolve(__dirname, '..', '..');
const read = (relativePath) => fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));

test('large tool scale contract is documented and wired as an explicit gate', () => {
  assert.equal(exists('docs/tool-scale-and-reuse-hardening.md'), true);
  const doc = read('docs/tool-scale-and-reuse-hardening.md');
  for (const term of [
    /50 configured callable upstream servers/i,
    /100,000 to 200,000 aggregate upstream tools/i,
    /Broker-first startup/i,
    /Search before projection/i,
    /MCPACE_CATALOG_TOOL_LIMIT/,
    /MCPACE_PROJECTION_CANDIDATE_LIMIT/,
    /npm run verify:tool-scale/,
  ]) {
    assert.match(doc, term);
  }
  assert.match(read('docs/README.md'), /tool-scale-and-reuse-hardening\.md/);
  assert.match(read('docs/dynamic-adapter.md'), /tool-scale-and-reuse-hardening\.md/);

  const pkg = JSON.parse(read('package.json'));
  assert.match(pkg.scripts['verify:tool-scale'], /simulate-tool-scale\.mjs/);
  assert.match(pkg.scripts['benchmark:tool-scale'], /--tools 200000/);
});

test('upstream search avoids all-tools flatten and keeps bounded top-k candidates', () => {
  const discovery = read('src/adapter/discovery.rs');
  const searchBody = discovery.slice(
    discovery.indexOf('pub fn upstream_search'),
    discovery.indexOf('type ScoredSearchTool'),
  );
  assert.doesNotMatch(searchBody, /upstream::catalog_tools\(/);
  assert.match(searchBody, /upstream::callable_tools_raw_catalog/);
  assert.match(searchBody, /scan_search_listing/);
  assert.match(discovery, /fn insert_scored_tool_bounded/);
  assert.match(discovery, /scored\.insert\(position, item\)/);
  assert.match(discovery, /scored\.pop\(\)/);
});

test('all-server catalog and projection diagnostics have bounded response surfaces', () => {
  const inventory = read('src/upstream/inventory.rs');
  assert.match(inventory, /fn catalog_response_tool_limit/);
  assert.match(inventory, /MCPACE_CATALOG_TOOL_LIMIT/);
  assert.match(inventory, /MCPACE_CATALOG_SERVER_TOOL_SAMPLE_LIMIT/);
  assert.match(inventory, /fn flatten_catalog_tools_with_limit/);
  assert.match(inventory, /toolsTruncated/);
  assert.match(inventory, /returnedToolCount/);

  const adapter = read('src/adapter.rs');
  assert.match(adapter, /DEFAULT_PROJECTION_CANDIDATE_MULTIPLIER/);
  assert.match(adapter, /MCPACE_PROJECTION_CANDIDATE_LIMIT/);
  assert.match(adapter, /projectionCandidatesTruncated/);
  assert.match(adapter, /projectableCandidateLimit/);
  assert.match(adapter, /MCPACE_PROJECTION_BROKER_SAMPLE_LIMIT/);
});

test('tool scale simulator validates 50-server six-figure tool scenarios', () => {
  const result = spawnSync(process.execPath, [
    'scripts/simulate-tool-scale.mjs',
    '--servers', '50',
    '--tools', '100000',
    '--search-limit', '25',
    '--projection-budget', '64',
    '--page-size', '128',
    '--memory-limit-mib', '512',
    '--json',
  ], {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: 30_000,
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'pass');
  assert.equal(report.scenario.servers, 50);
  assert.equal(report.scenario.tools, 100000);
  assert.equal(report.results.searchSpaceToolCount, 100000);
  assert.ok(report.results.retainedSearchCandidates <= 25);
  assert.ok(report.results.projectedToolCount <= 64);
  assert.ok(report.results.firstPageCount <= 128);
  assert.equal(report.algorithm.boundedTopK, true);
  assert.equal(report.algorithm.lazyCompactToolMaterialization, true);
  assert.equal(report.algorithm.materializesFullCatalog, false);
  assert.match(read('scripts/simulate-tool-scale.mjs'), /\.\/lib\/bounded-top-k\.mjs/);
});
