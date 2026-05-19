const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const repoRoot = path.resolve(__dirname, '..', '..');
const read = (relativePath) => fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));

test('system lifecycle hardening document covers install through publish', () => {
  assert.equal(exists('docs/system-lifecycle-hardening.md'), true);
  const doc = read('docs/system-lifecycle-hardening.md');
  for (const term of [
    /Critical node map/,
    /Install/,
    /First start/,
    /Runtime/,
    /Restart/,
    /Crash recovery/,
    /Upgrade and reinstall/,
    /Uninstall/,
    /Diagnostics and support bundles/,
    /Release and publish/,
    /Durable user config/,
    /Disposable cache/,
    /Ephemeral runtime facts/,
  ]) {
    assert.match(doc, term);
  }
  assert.match(read('docs/runtime-state-cache-lifecycle.md'), /system-lifecycle-hardening\.md/);
  assert.match(read('docs/architecture-boundaries.md'), /system-lifecycle-hardening\.md/);
  assert.match(read('docs/README.md'), /system-lifecycle-hardening\.md/);
});

test('system lifecycle audit is wired into package and install readiness', () => {
  const pkg = JSON.parse(read('package.json'));
  assert.match(pkg.scripts['verify:system-lifecycle'], /system-lifecycle-audit\.mjs/);
  assert.match(pkg.scripts['verify:lifecycle'], /verify:system-lifecycle/);
  assert.match(pkg.scripts['verify:lifecycle'], /system-lifecycle-contract\.test\.js/);

  assert.match(read('scripts/local-quality-suite.mjs'), /system-lifecycle-audit/);
  assert.match(read('scripts/install-readiness-harness.mjs'), /collectSystemLifecycleAudit/);
  assert.match(read('scripts/install-readiness-harness.mjs'), /system-lifecycle-audit/);
});

test('system lifecycle audit passes on the current source tree', () => {
  const result = spawnSync(process.execPath, ['scripts/system-lifecycle-audit.mjs', '--json', '--strict'], {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: 30_000,
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'pass');
  assert.equal(report.summary.blockers, 0);
  assert.equal(report.summary.warnings, 0);
});

test('critical production mutation paths use atomic writes', () => {
  const expectations = [
    ['src/init.rs', /runtimepaths::write_text_atomic\(path, &value\.to_pretty_string\(\)\)/],
    ['src/mcp_sources/import.rs', /runtimepaths::write_text_atomic\(target_path, &serialized\)/],
    ['src/service.rs', /runtimepaths::write_text_atomic\(&script_path, &script\)/],
    ['src/serve.rs', /runtimepaths::write_text_atomic\(path, &contents\)/],
    ['src/hub/runtime.rs', /runtimepaths::write_text_atomic\(path, &contents\)/],
  ];
  for (const [file, pattern] of expectations) {
    assert.match(read(file), pattern, file);
  }
  assert.doesNotMatch(read('src/service.rs'), /max_connections: Option<usize>,\s*max_connections: Option<usize>/);
});


test('cleanup command is wired as a safe lifecycle surface', () => {
  assert.match(read('src/cleanup.rs'), /cleanup_report/);
  assert.match(read('src/cleanup.rs'), /status\|cache\|runtime\|logs\|all-safe/);
  assert.match(read('src/cleanup.rs'), /preserves durable user config|durable config/i);
  assert.match(read('src/app.rs'), /"cleanup" => cleanup::run/);
  assert.match(read('src/catalog.rs'), /name: "cleanup"/);
  assert.match(read('docs/system-lifecycle-hardening.md'), /mcpace cleanup/);
});
