const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const { test } = require('node:test');
const { read, readJson, packageVersion } = require('./helpers');

const packageJson = readJson('package.json');
const releaseManifest = readJson('release-manifest.json');
const registryReport = readJson('reports/registry-lab-latest.json');
const registryMarkdown = read('reports/registry-lab-latest.md');
const script = read('scripts/registry-lab.mjs');
const docs = read('docs/registry-lab-and-policy-review.md');

test('registry lab is wired as a source-only policy review lane', () => {
  assert.equal(
    packageJson.scripts['verify:registry-lab'],
    'node scripts/registry-lab.mjs --json --write reports/registry-lab-latest.json --markdown reports/registry-lab-latest.md'
  );
  assert.match(packageJson.scripts['verify:hardening'], /verify:registry-lab/);
  assert.equal(registryReport.schema, 'mcpace.registryLab.v2');
  assert.equal(registryReport.version, packageVersion());
  assert.equal(registryReport.status, 'pass');
  assert.equal(registryReport.mode, 'fixture-metadata-only');
  assert.equal(registryReport.safety.executesThirdPartyPackages, false);
  assert.equal(registryReport.safety.destructiveToolCallsAllowed, false);
  assert.match(registryReport.safety.defaultUnknownPolicy, /single-writer/);
  assert.ok(registryReport.summary.serverCount >= 8);
  assert.equal(registryReport.summary.reviewRequired, registryReport.summary.serverCount);
});

test('registry lab classifies risky server families conservatively', () => {
  const decisions = new Set(registryReport.classifications.map((item) => item.decision));
  for (const expected of [
    'project-filesystem-single-writer',
    'project-repo-single-writer',
    'shared-exclusive-host-lock',
    'state-profile-single-session',
    'database-path-single-writer',
    'disabled-dangerous-command-runner',
    'unknown-conservative-review',
    'cloud-admin-credential-review',
    'blockchain-wallet-review',
    'credential-scoped-stdio-review',
    'payments-financial-review',
    'identity-admin-credential-review',
    'secrets-manager-disabled-review',
    'messaging-external-review',
  ]) {
    assert.ok(decisions.has(expected), `missing ${expected}`);
  }
  for (const highRisk of ['payments-financial-review', 'identity-admin-credential-review', 'secrets-manager-disabled-review', 'messaging-external-review']) {
    const item = registryReport.classifications.find((entry) => entry.decision === highRisk);
    assert.equal(item.suggestedPolicy.defaultEnabled, false, `${highRisk} must not default-enable`);
    assert.equal(item.suggestedPolicy.discoveryRequiresLease, true, `${highRisk} discovery must be serialized`);
  }
  const unknown = registryReport.classifications.find((item) => item.decision === 'unknown-conservative-review');
  assert.equal(unknown.suggestedPolicy.discoveryRequiresLease, true);
  assert.equal(unknown.suggestedPolicy.parallelismLimit, 1);
  assert.equal(unknown.suggestedPolicy.defaultEnabled, false);
});

test('registry lab script does not execute package-manager launchers', () => {
  assert.match(script, /does not install, launch, or call arbitrary third-party MCP servers/);
  assert.doesNotMatch(script, /spawn\(/);
  assert.doesNotMatch(script, /execFile\(/);
  assert.doesNotMatch(script, /npx\s+-y/);
  assert.match(script, /GET|fetch/);
  const stdout = execFileSync(process.execPath, ['scripts/registry-lab.mjs', '--json', '--no-write'], {
    cwd: process.cwd(),
    encoding: 'utf8',
    timeout: 15_000,
  });
  const parsed = JSON.parse(stdout);
  assert.equal(parsed.schema, 'mcpace.registryLab.v2');
  assert.equal(parsed.safety.executesThirdPartyPackages, false);
});

test('registry lab docs and release manifest expose evidence', () => {
  assert.match(docs, /Metadata-only classification/);
  assert.match(docs, /Sandbox launch/);
  assert.match(docs, /Concurrency torture/);
  assert.match(docs, /Servers, Clients, Activity, and Policy Review/);
  assert.match(registryMarkdown, /MCP Registry Lab Report/);
  for (const required of ['reports/registry-lab-latest.json', 'reports/registry-lab-latest.md']) {
    assert.ok(releaseManifest.includePaths.includes(required), `missing ${required}`);
  }
});
