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

test('compact MCP overhead benchmark measures classification, registry, and scheduler cost', () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-overhead-benchmark-'));
  const jsonPath = path.join(temp, 'mcp-overhead-benchmark.json');
  const mdPath = path.join(temp, 'mcp-overhead-benchmark.md');
  const result = spawnSync(process.execPath, [
    'scripts/mcp-overhead-benchmark.mjs',
    '--json',
    '--strict',
    '--packages', '50',
    '--servers', '50',
    '--tools-per-server', '20',
    '--operations', '5000',
    '--max-classifier-ms', '250',
    '--max-scheduler-ms', '750',
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
  assert.equal(report.schema, 'mcpace.mcpOverheadBenchmark.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.inputs.packages, 50);
  assert.equal(report.inputs.servers, 50);
  assert.equal(report.inputs.tools, 1000);
  assert.equal(report.inputs.operations, 5000);
  assert.ok(report.measurements.classification.perPackageUs > 0);
  assert.ok(report.measurements.registry.perToolUs > 0);
  assert.ok(report.measurements.scheduling.perDecisionUs > 0);
  assert.equal(report.measurements.scheduling.randomServerStarts, 0);
  assert.equal(report.measurements.scheduling.activeLocksAfterDrain, 0);
  assert.ok(report.checks.every((check) => check.ok), JSON.stringify(report.checks, null, 2));
  assert.match(fs.readFileSync(mdPath, 'utf8'), /MCP overhead benchmark/);
});

test('compact MCP overhead benchmark is wired into orchestration and release evidence', () => {
  const packageJson = JSON.parse(read('package.json'));
  const releaseManifest = JSON.parse(read('release-manifest.json'));
  assert.match(packageJson.scripts['verify:mcp-overhead-benchmark'], /mcp-overhead-benchmark\.mjs/);
  assert.match(packageJson.scripts['verify:overhead:quick'], /verify:mcp-overhead-benchmark/);
  assert.match(packageJson.scripts['verify:orchestration'], /verify:overhead:quick/);
  assert.match(packageJson.scripts['benchmark:mcp-overhead'], /--servers 1000/);
  assert.ok(releaseManifest.includePaths.includes('reports/mcp-overhead-benchmark-latest.json'));
  assert.ok(releaseManifest.includePaths.includes('reports/mcp-overhead-benchmark-latest.md'));
  assert.match(read('docs/mcp-overhead-and-optimization.md'), /verify:mcp-overhead-benchmark/);
});
