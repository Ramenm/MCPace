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

test('MCP overhead pressure audit measures profile, fragment, and scheduler budgets without starting servers', () => {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-overhead-pressure-'));
  const jsonPath = path.join(tempDir, 'overhead-pressure.json');
  const mdPath = path.join(tempDir, 'overhead-pressure.md');
  const result = spawnSync(process.execPath, [
    'scripts/mcp-overhead-pressure-audit.mjs',
    '--json',
    '--servers', '600',
    '--fragments', '20',
    '--operations', '2500',
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
  assert.equal(report.schema, 'mcpace.mcpOverheadPressure.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.safety.startsMcpServers, false);
  assert.equal(report.safety.callsMcpTools, false);
  assert.equal(report.safety.executesThirdPartyPackages, false);
  assert.equal(report.safety.installsPackages, false);
  assert.equal(report.safety.syntheticOnly, true);
  assert.ok(report.summary.profileAvgUs > 0);
  assert.ok(report.summary.schedulerAvgUs > 0);
  assert.ok(report.summary.blockedReviewOperations > 0);
  assert.ok(report.summary.blockedLockOperations >= 0);
  assert.ok(report.measurements.profileThroughput.value.classCounts.P0_unknown_stdio > 0);
  assert.ok(report.measurements.schedulerRouting.value.invariantViolations === 0);
  assert.ok(report.checks.some((check) => check.id === 'heap-budget' && check.ok));
  assert.match(fs.readFileSync(mdPath, 'utf8'), /Optimization plan/);
});

test('adaptive profiling and overhead pressure share one evidence-profile library', () => {
  const packageJson = JSON.parse(read('package.json'));
  const adaptive = read('scripts/adaptive-parallelism-audit.mjs');
  const pressure = read('scripts/mcp-overhead-pressure-audit.mjs');
  const lib = read('scripts/lib/mcp-evidence-profile.mjs');

  assert.equal(packageJson.scripts['verify:overhead-pressure'], 'node scripts/mcp-overhead-pressure-audit.mjs --json --write reports/mcp-overhead-pressure-latest.json --markdown reports/mcp-overhead-pressure-latest.md');
  assert.match(packageJson.scripts['verify:overhead:deep'], /verify:overhead-pressure/);
  assert.match(packageJson.scripts['verify:hardening'], /verify:overhead:deep/);
  assert.match(packageJson.scripts['benchmark:overhead-pressure'], /--servers 50000/);
  assert.match(adaptive, /\.\/lib\/mcp-evidence-profile\.mjs/);
  assert.doesNotMatch(adaptive, /function profileFrom\(/);
  assert.match(pressure, /\.\/lib\/mcp-evidence-profile\.mjs/);
  assert.match(lib, /export function profileFrom/);
  assert.match(lib, /transport-session/);
  assert.match(lib, /credential:credential-profile/);
});
