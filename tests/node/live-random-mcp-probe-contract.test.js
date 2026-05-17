const assert = require('node:assert/strict');
const fs = require('node:fs');
const { execFileSync } = require('node:child_process');
const test = require('node:test');

const projectRoot = process.cwd();

test('live random MCP probe script replays saved real-package evidence safely', () => {
  const output = execFileSync('node', ['scripts/live-random-mcp-probe.mjs', '--json', '--no-write'], {
    cwd: projectRoot,
    encoding: 'utf8',
  });
  const report = JSON.parse(output);
  assert.equal(report.schema, 'mcpace.liveRandomMcpProbe.v5');
  assert.equal(report.status, 'pass');
  assert.equal(report.mode, 'fixture-replay');
  assert.ok(report.summary.total >= 12);
  assert.ok(report.summary.ok >= 10);
  assert.ok(report.summary.totalTools >= 90);
  assert.deepEqual(report.summary.policyMismatches, []);
  assert.deepEqual(report.summary.unexpectedFailures, []);
  assert.equal(report.safety.executesThirdPartyPackages, false);
  assert.equal(report.safety.destructiveToolCallsAllowed, false);
  assert.equal(report.safety.userSecretsPassedToRuntime, false);
  assert.equal(report.safety.packageInstallScriptsAllowed, false);
  assert.equal(report.safety.packageManagerEnvWhitelisted, true);
  assert.equal(report.safety.packageManagerHomeIsolated, true);
  assert.equal(report.safety.packageManagerOutputRedacted, true);
  assert.equal(report.safety.packageManagerCredentialsMayBeUsedForMirrors, true);
});

test('live probe evidence covers core problematic server classes across npm and PyPI', () => {
  const report = JSON.parse(fs.readFileSync('reports/live-random-mcp-probe-latest.json', 'utf8'));
  const policies = new Set(report.results.map((item) => item.suggestedPolicy));
  for (const policy of [
    'project-filesystem-single-writer',
    'state-profile-single-session',
    'project-repo-single-writer',
    'database-path-single-writer',
    'network-fetch-review',
    'shared-exclusive-host-lock',
    'credential-scoped-review',
    'network-docs-multi-reader-review',
    'test-fixture-disabled',
  ]) {
    assert.ok(policies.has(policy), `missing policy ${policy}`);
  }
  assert.ok(report.results.some((item) => item.kind === 'npm' && item.status === 'ok'));
  assert.ok(report.results.some((item) => item.kind === 'pypi' && item.status === 'ok'));
  const chrome = report.results.find((item) => item.id === 'chrome-devtools');
  assert.ok(chrome.riskSignals.includes('browser-or-desktop'));
  const brave = report.results.find((item) => item.id === 'deprecated-brave-search');
  assert.equal(brave.status, 'startup-error');
  assert.ok(brave.suggestedPolicy.includes('credential'));
});

test('download mode exposes deterministic filters, env hardening, and explicit canary probes', () => {
  const help = execFileSync('node', ['scripts/live-random-mcp-probe.mjs', '--help'], {
    cwd: projectRoot,
    encoding: 'utf8',
  });
  assert.match(help, /--ids <list>/);
  assert.match(help, /--force-canaries/);
  assert.match(help, /--allow-heavy-installs/);
  assert.match(help, /--kinds <list>/);
  const script = fs.readFileSync('scripts/live-random-mcp-probe.mjs', 'utf8');
  assert.match(script, /defaultSkipReason/);
  assert.match(script, /respondToServerRequest/);
  assert.match(script, /roots\/list/);
  assert.match(script, /serverSideRequests/);
  assert.match(script, /allowedStatuses/);
  assert.match(script, /hardSkipReason/);
  assert.match(script, /cleanPackageManagerEnv/);
  assert.match(script, /runCommandWithTimeout/);
  assert.match(script, /redactSensitive/);
  assert.match(script, /resolveExecutable/);
  assert.match(script, /validateSelection/);
  assert.match(script, /appendCapped/);
  assert.match(script, /droppedJsonRpcMessages/);
  assert.match(script, /wrapperEnv/);
  assert.match(script, /hardTimer/);
  assert.doesNotMatch(script, /npm install[^\n]+env: process\.env/);
  assert.doesNotMatch(script, /uv.*env: process\.env/);
  assert.doesNotMatch(script, /stdoutRaw/);
  assert.match(script, /code-runner/);
  assert.match(script, /cluster-admin-credential-review/);
  assert.match(script, /cloud-admin-credential-review/);
  assert.match(script, /blockchain-wallet-review/);
});
