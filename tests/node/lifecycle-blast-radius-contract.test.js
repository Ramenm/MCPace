const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { cleanChildEnv, repoRoot } = require('./helpers');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('lifecycle blast-radius smoke is wired into package scripts and release evidence', () => {
  const packageJson = JSON.parse(read('package.json'));
  const releaseManifest = JSON.parse(read('release-manifest.json'));
  const script = read('scripts/lifecycle-blast-radius-smoke.mjs');
  const docs = read('docs/mcp-lifecycle-blast-radius.md');

  assert.equal(
    packageJson.scripts['verify:lifecycle-blast-radius'],
    'node scripts/lifecycle-blast-radius-smoke.mjs --json --write reports/lifecycle-blast-radius-latest.json --markdown reports/lifecycle-blast-radius-latest.md'
  );
  assert.equal(
    packageJson.scripts['benchmark:lifecycle-blast-radius'],
    'node scripts/lifecycle-blast-radius-smoke.mjs --json --no-write'
  );
  assert.match(packageJson.scripts['verify:hardening'], /verify:lifecycle-blast-radius/);
  assert.match(packageJson.scripts['verify:hardening'], /verify:tool-exposure-safety/);
  assert.ok(releaseManifest.includePaths.includes('reports/lifecycle-blast-radius-latest.json'));
  assert.ok(releaseManifest.includePaths.includes('reports/lifecycle-blast-radius-latest.md'));
  assert.match(script, /paid-server-registers-disabled-without-output-secret-leak/);
  assert.match(script, /server-remove-dry-run-does-not-delete/);
  assert.match(script, /source-corrupt-settings-fragment-isolated/);
  assert.match(script, /supply-chain-package-launchers-are-documented-risk/);
  assert.match(docs, /registered but disabled/i);
  assert.match(docs, /Owned by MCPace/);
  assert.match(docs, /Not owned by MCPace/);
});

test('source hardening prevents corrupt fragments and normalized duplicate force-replace drift', () => {
  const writeSource = read('src/mcp_sources/write.rs');
  const registrySource = read('src/mcp_sources.rs');

  assert.match(writeSource, /let existing_key = servers/);
  assert.match(writeSource, /servers\.remove\(&existing_key\)/);
  assert.match(writeSource, /servers\.insert\(display_name, entry\)/);
  assert.match(registrySource, /failed to read MCP settings source/);
  assert.match(registrySource, /skipping/);
  assert.match(registrySource, /continue;/);
});

test('lifecycle blast-radius smoke can execute against the vendored binary', () => {
  const result = spawnSync(
    process.execPath,
    ['scripts/lifecycle-blast-radius-smoke.mjs', '--json', '--no-write'],
    {
      cwd: repoRoot,
      encoding: 'utf8',
      env: cleanChildEnv(),
      timeout: 30000,
      windowsHide: true,
    }
  );

  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.schema, 'mcpace.lifecycleBlastRadiusSmoke.v1');
  assert.equal(report.status, 'pass');
  const ids = report.checks.map((check) => check.id);
  assert.ok(ids.includes('paid-server-registers-disabled-without-output-secret-leak'));
  assert.ok(ids.includes('server-enable-is-explicit-state-transition'));
  assert.ok(ids.includes('server-disable-is-explicit-state-transition'));
  assert.ok(ids.includes('server-remove-dry-run-does-not-delete'));
  assert.ok(ids.includes('server-remove-deletes-only-target-entry'));
  assert.ok(ids.includes('source-force-replace-removes-normalized-duplicate-key'));
  assert.ok(report.commands.every((command) => !JSON.stringify(command).includes('sk_live_must_not_appear')));
});
