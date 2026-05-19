const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const test = require('node:test');
const { cleanChildEnv, repoRoot } = require('./helpers');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('MCP overhead stress measures 100-server style hub overhead without random execution', () => {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-overhead-stress-'));
  const jsonPath = path.join(tempDir, 'stress.json');
  const mdPath = path.join(tempDir, 'stress.md');
  const result = spawnSync(process.execPath, [
    'scripts/mcp-overhead-stress.mjs',
    '--json',
    '--servers', '30',
    '--tools', '30000',
    '--operations', '6000',
    '--package-profiles', '300',
    '--memory-limit-mib', '128',
    '--write', jsonPath,
    '--markdown', mdPath,
  ], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: cleanChildEnv(),
    timeout: 120000,
    windowsHide: true,
  });

  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
  assert.equal(report.schema, 'mcpace.mcpOverheadStress.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.safety.startsMcpServers, false);
  assert.equal(report.safety.callsMcpTools, false);
  assert.equal(report.safety.executesThirdPartyPackages, false);
  assert.equal(report.toolCatalog.searchSpaceToolCount, 30000);
  assert.ok(report.toolCatalog.retainedSearchCandidates <= report.scenario.searchLimit);
  assert.ok(report.toolCatalog.projectedToolCount <= report.scenario.projectionBudget);
  assert.equal(report.lookup.unknownForwarded, 0);
  assert.equal(report.scheduler.violations.length, 0);
  assert.equal(report.scheduler.waitingAtEnd, 0);
  assert.ok(report.scheduler.blockedDisabled > 0);
  assert.ok(report.scheduler.blockedUnknownTool > 0);
  assert.ok(report.scheduler.blockedReviewGate > 0);
  assert.equal(report.profileClassification.executeDefault, 0);
  assert.match(fs.readFileSync(mdPath, 'utf8'), /synthetic server\/tool profiles only/i);
});

test('overhead gates share bounded top-k and signal policy instead of forked classifiers', () => {
  const pkg = JSON.parse(read('package.json'));
  assert.match(pkg.scripts['verify:mcp-overhead-stress'], /mcp-overhead-stress\.mjs/);
  assert.match(pkg.scripts['benchmark:mcp-overhead-stress'], /--tools 500000/);
  assert.match(pkg.scripts['verify:overhead:quick'], /verify:mcp-overhead-stress/);
  assert.match(pkg.scripts['verify:orchestration'], /verify:overhead:quick/);

  assert.match(read('scripts/simulate-tool-scale.mjs'), /\.\/lib\/bounded-top-k\.mjs/);
  assert.match(read('scripts/simulate-mixed-upstreams.mjs'), /\.\/lib\/bounded-top-k\.mjs/);
  assert.match(read('scripts/mcp-overhead-benchmark.mjs'), /\.\/lib\/mcp-signal-policy\.mjs/);
  assert.match(read('scripts/mcp-overhead-stress.mjs'), /\.\/lib\/mcp-signal-policy\.mjs/);
  assert.doesNotMatch(read('scripts/mcp-overhead-benchmark.mjs'), /const SIGNAL_RULES/);
  assert.doesNotMatch(read('scripts/mcp-overhead-stress.mjs'), /const PROFILE_PATTERNS/);

  const latest = JSON.parse(read('reports/mcp-overhead-stress-latest.json'));
  assert.equal(latest.status, 'pass');
  assert.equal(latest.safety.defaultServerEnablement, false);
});
