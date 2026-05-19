const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { pathToFileURL } = require('node:url');
const { cleanChildEnv, repoRoot } = require('./helpers');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('MCP overhead profile emits bounded synthetic measurements without server execution', () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-overhead-profile-'));
  const jsonPath = path.join(temp, 'mcp-overhead-profile.json');
  const mdPath = path.join(temp, 'mcp-overhead-profile.md');
  const result = spawnSync(process.execPath, [
    'scripts/mcp-overhead-profile.mjs',
    '--json',
    '--runs', '5',
    '--iterations', '5000',
    '--servers', '25',
    '--tools', '500',
    '--operations', '5000',
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
  assert.equal(report.schema, 'mcpace.mcpOverheadProfile.v1');
  assert.equal(report.status, 'pass');
  assert.equal(report.safety.startsMcpServers, false);
  assert.equal(report.safety.callsMcpTools, false);
  assert.equal(report.config.servers, 25);
  assert.equal(report.config.tools, 500);
  assert.equal(report.config.operations, 5000);
  assert.equal(report.metrics.toolIndex.build.toolCount, 500);
  assert.ok(report.metrics.route.p95Us > 0);
  assert.ok(report.metrics.packagePolicy.p95Us > 0);
  assert.ok(report.metrics.lockAdmission.stats.admitted > 0);
  assert.ok(report.checks.some((check) => check.id === 'json-rpc-routing-overhead-budget' && check.ok));
  assert.ok(report.checks.some((check) => check.id === 'tool-exact-lookup-overhead-budget' && check.ok));
  assert.ok(report.checks.some((check) => check.id === 'package-policy-overhead-budget' && check.ok));
  assert.match(fs.readFileSync(mdPath, 'utf8'), /MCP overhead profile/);
});

test('MCP signal policy is shared by mass survey, adaptive profiling, and overhead profile', async () => {
  const policy = await import(pathToFileURL(path.join(repoRoot, 'scripts/lib/mcp-signal-policy.mjs')).href);
  const filesystem = policy.classifyMcpPackageMetadata({ name: '@modelcontextprotocol/server-filesystem', description: 'filesystem read_file write_file MCP server' });
  const slack = policy.classifyMcpPackageMetadata({ name: 'slack-mcp-server', description: 'Slack OAuth token API MCP server' });
  const unknown = policy.classifyMcpPackageMetadata({ name: 'random-mcp-server', description: 'unclear side effects' });

  assert.equal(filesystem.policy, 'project-filesystem-single-writer');
  assert.equal(slack.policy, 'credential-scoped-review');
  assert.equal(unknown.executeDefault, false);
  assert.ok(unknown.locks.length > 0);

  assert.match(read('scripts/mcp-mass-package-survey.mjs'), /minimalPackageProfile/);
  assert.match(read('scripts/lib/mcp-evidence-profile.mjs'), /signalsFromServerDescriptor/);
  assert.match(read('scripts/mcp-overhead-profile.mjs'), /classifyMcpPackageMetadata/);
  assert.match(read('package.json'), /verify:mcp-overhead-profile/);
  assert.match(read('docs/mcp-overhead-and-optimization.md'), /classification drift/);
});
