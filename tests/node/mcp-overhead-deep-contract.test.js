const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { cleanChildEnv, repoRoot } = require('./helpers');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('deep MCP overhead audit measures 100-server path without random server execution', () => {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-overhead-deep-'));
  const jsonPath = path.join(tempDir, 'overhead-deep.json');
  const mdPath = path.join(tempDir, 'overhead-deep.md');
  const result = spawnSync(process.execPath, [
    'scripts/mcp-overhead-deep-audit.mjs',
    '--json',
    '--strict',
    '--servers', '100',
    '--tools', '10000',
    '--operations', '2500',
    '--runs', '3',
    '--profile-refreshes', '4',
    '--max-cached-profile-per-server-us', '300',
    '--write', jsonPath,
    '--markdown', mdPath,
  ], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: cleanChildEnv(),
    timeout: 120000,
  });

  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
  assert.equal(report.schema, 'mcpace.mcpOverheadDeepAudit.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.scenario.servers, 100);
  assert.equal(report.safety.startsMcpServers, false);
  assert.equal(report.safety.callsMcpTools, false);
  assert.equal(report.safety.executesThirdPartyPackages, false);
  assert.equal(report.benchmarks.toolIndex.last.lookupHits, report.benchmarks.toolIndex.last.lookupCount);
  assert.ok(report.summary.profileCachedPerServerUs < report.summary.profileColdPerServerUs);
  assert.ok(report.summary.schedulerPerOperationUs > 0);
  assert.ok(report.checks.some((check) => check.id === 'mass-survey-safety-proof-present' && check.ok));
  assert.deepEqual(report.blockers, []);
  assert.match(fs.readFileSync(mdPath, 'utf8'), /MCP overhead deep audit/);
});

test('deep MCP overhead gate is wired and reuses shared evidence/profile libraries', () => {
  const packageJson = JSON.parse(read('package.json'));
  const manifest = JSON.parse(read('release-manifest.json'));
  const script = read('scripts/mcp-overhead-deep-audit.mjs');
  const doc = read('docs/mcp-overhead-and-optimization.md');

  assert.equal(packageJson.scripts['verify:mcp-overhead-deep'], 'node scripts/mcp-overhead-deep-audit.mjs --json --strict --write reports/mcp-overhead-deep-latest.json --markdown reports/mcp-overhead-deep-latest.md');
  assert.match(packageJson.scripts['benchmark:mcp-overhead-deep'], /--servers 250/);
  assert.match(packageJson.scripts['verify:overhead:deep'], /verify:overhead-pressure/);
  assert.match(packageJson.scripts['verify:overhead:deep'], /verify:mcp-overhead-deep/);
  assert.match(packageJson.scripts['verify:experience'], /verify:overhead:quick/);
  assert.match(packageJson.scripts['verify:orchestration'], /verify:overhead:quick/);
  assert.match(packageJson.scripts['verify:hardening'], /verify:overhead:deep/);
  assert.ok(manifest.includePaths.includes('reports/mcp-overhead-deep-latest.json'));
  assert.ok(manifest.includePaths.includes('reports/mcp-overhead-deep-latest.md'));
  assert.match(script, /\.\/lib\/mcp-evidence-profile\.mjs/);
  assert.match(script, /\.\/lib\/bounded-top-k\.mjs/);
  assert.doesNotMatch(script, /from 'node:child_process'/);
  assert.doesNotMatch(script, /tools\/call/);
  assert.match(doc, /Deep 100-server overhead audit/);
  assert.match(doc, /cache profile decisions by normalized source fingerprint/i);
});
