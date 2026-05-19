const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const test = require('node:test');
const { cleanChildEnv, repoRoot } = require('./helpers');

function read(rel) {
  return fs.readFileSync(path.join(repoRoot, rel), 'utf8');
}

test('fixture overhead gate measures actual MCP stdio cold and warm lifecycle without third-party execution', () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-fixture-overhead-'));
  const jsonPath = path.join(temp, 'fixture-overhead.json');
  const mdPath = path.join(temp, 'fixture-overhead.md');
  const run = spawnSync(process.execPath, [
    'scripts/mcp-fixture-overhead.mjs',
    '--json',
    '--cold-runs', '2',
    '--warm-lists', '4',
    '--write', jsonPath,
    '--markdown', mdPath,
  ], { cwd: repoRoot, encoding: 'utf8', env: cleanChildEnv(), timeout: 45000 });
  assert.equal(run.status, 0, run.stderr || run.stdout);
  const report = JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
  assert.equal(report.schema, 'mcpace.mcpFixtureOverhead.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.safety.startsThirdPartyMcpServers, false);
  assert.equal(report.safety.callsThirdPartyTools, false);
  assert.equal(report.cold.failures, 0);
  assert.equal(report.warm.failures, 0);
  assert.ok(report.cold.stats.totalMs.p95 >= 0);
  assert.ok(report.warm.stats.toolsListMs.p95 >= 0);
  assert.ok(report.checks.some((check) => check.id === 'warm-tools-list-budget' && check.ok));
  assert.match(fs.readFileSync(mdPath, 'utf8'), /MCP fixture overhead/);
});

test('fixture overhead gate is wired into performance scripts and documentation', () => {
  const pkg = JSON.parse(read('package.json'));
  assert.match(pkg.scripts['verify:mcp-fixture-overhead'], /mcp-fixture-overhead\.mjs/);
  assert.match(pkg.scripts['benchmark:mcp-fixture-overhead'], /--cold-runs 15/);
  assert.match(pkg.scripts['verify:overhead:quick'], /verify:mcp-fixture-overhead/);
  assert.doesNotMatch(pkg.scripts['verify:performance'], /verify:mcp-fixture-overhead/);
  const script = read('scripts/mcp-fixture-overhead.mjs');
  assert.doesNotMatch(script, /npm install/);
  assert.doesNotMatch(script, /method:\s*['"]tools\/call/);
  const doc = read('docs/mcp-overhead-and-optimization.md');
  assert.match(doc, /verify:mcp-fixture-overhead/);
  assert.match(doc, /cold process spawn \+ `initialize`/);
  const manifest = JSON.parse(read('release-manifest.json'));
  assert.ok(manifest.includePaths.includes('reports/mcp-fixture-overhead-latest.json'));
  assert.ok(manifest.includePaths.includes('reports/mcp-fixture-overhead-latest.md'));
});
