const test = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { cleanChildEnv, repoRoot, read } = require('./helpers.js');

const CHILD_OPTIONS = {
  cwd: repoRoot,
  encoding: 'utf8',
  env: cleanChildEnv(),
  timeout: 30_000,
  maxBuffer: 4 * 1024 * 1024,
};

test('source audit reports architectural risk signals and fails only on critical production debt', () => {
  const result = spawnSync(
    process.execPath,
    ['scripts/audit-source.mjs', '--json', '--fail-on-critical'],
    CHILD_OPTIONS,
  );

  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.ok, true);
  assert.equal(report.critical.length, 0);
  assert.ok(report.summary.rustFiles > 0);
  assert.ok(report.summary.nodeFiles > 0);
  assert.ok(report.summary.productionRustLines > report.summary.testRustLines);
  assert.ok(report.summary.largeModules >= 0);
  assert.ok(report.summary.directThreadSpawns >= 1);
  assert.ok(report.summary.commandSpawns >= 1);
  assert.ok(report.summary.productionUnwraps >= 1);
  assert.ok(report.summary.unsafeOperations >= 1);
  assert.ok(report.summary.foreignFunctionBlocks >= 1);
  assert.match(JSON.stringify(report.policy), /panic/);
  assert.match(JSON.stringify(report.policy), /modules over 1500/i);
  assert.match(JSON.stringify(report.policy), /Unsafe Rust and FFI/i);
});

test('source audit checks explicit architecture boundaries', () => {
  const result = spawnSync(
    process.execPath,
    ['scripts/audit-source.mjs', '--json', '--include', 'src/mcp_protocol.rs,src/resources.rs'],
    CHILD_OPTIONS,
  );

  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.ok, true);
  assert.ok(report.architecture.boundaries.length >= 2);
  assert.deepEqual(report.architecture.boundaries.map((boundary) => boundary.ok), [true, true]);
  assert.match(JSON.stringify(report.architecture.boundaries), /protocol primitives/i);
  assert.match(JSON.stringify(report.architecture.boundaries), /resource defaults/i);
});

test('source audit can write a durable JSON report artifact', () => {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-source-audit-'));
  const outputPath = path.join(tempDir, 'source-audit.json');
  const result = spawnSync(
    process.execPath,
    ['scripts/audit-source.mjs', '--json', '--write', outputPath, '--fail-on-critical'],
    CHILD_OPTIONS,
  );

  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.equal(fs.existsSync(outputPath), true);
  const stdoutReport = JSON.parse(result.stdout);
  const writtenReport = JSON.parse(fs.readFileSync(outputPath, 'utf8'));
  assert.equal(writtenReport.ok, stdoutReport.ok);
  assert.equal(writtenReport.critical.length, 0);
  assert.equal(writtenReport.summary.files, stdoutReport.summary.files);
  assert.match(writtenReport.generatedAt, /^\d{4}-\d{2}-\d{2}T/);
});


test('unsafe and FFI process detach code stays centralized', () => {
  const reportResult = spawnSync(
    process.execPath,
    ['scripts/audit-source.mjs', '--json', '--include', 'src'],
    CHILD_OPTIONS,
  );

  assert.equal(reportResult.status, 0, reportResult.stderr || reportResult.stdout);
  const report = JSON.parse(reportResult.stdout);
  assert.equal(report.ok, true);
  assert.equal(report.critical.length, 0);
  assert.ok(report.summary.unsafeOperations >= 1);
  assert.ok(report.summary.foreignFunctionBlocks >= 1);

  const lib = read('src/lib.rs');
  const processDetach = read('src/process_detach.rs');
  const hubLauncher = read('src/hub/launcher.rs');
  const serve = read('src/serve.rs');

  assert.match(lib, /mod process_detach/);
  assert.match(processDetach, /configure_unix_new_session/);
  assert.match(processDetach, /SAFETY:/);
  assert.doesNotMatch(hubLauncher, /unsafe\s*\{/);
  assert.doesNotMatch(serve, /fn setsid|pre_exec\(/);
  assert.match(hubLauncher, /configure_unix_new_session/);
  assert.match(serve, /configure_unix_new_session/);
});

test('local HTTP routes convert internal command failures into structured JSON errors', () => {
  const dashboard = read('src/dashboard.rs');

  assert.match(dashboard, /fn handle_http_request/);
  assert.match(dashboard, /fn write_json_error_response/);
  assert.match(dashboard, /500 Internal Server Error/);
  assert.match(dashboard, /internal_error/);
  assert.match(dashboard, /dashboard_returns_json_500_for_internal_route_errors/);
});

test('source audit script remains documented and wired into package scripts', () => {
  const packageJson = JSON.parse(read('package.json'));
  const testStrategy = read('docs/test-strategy.md');
  const architecture = read('docs/architecture-boundaries.md');
  const sourceQuality = read('docs/source-quality.md');

  assert.equal(packageJson.scripts['audit:source'], 'node scripts/audit-source.mjs --fail-on-critical');
  assert.match(packageJson.scripts.test, /audit:source/);
  assert.match(packageJson.scripts['lint:npm'], /scripts\/audit-source\.mjs/);
  assert.match(packageJson.scripts['lint:npm'], /source-quality-contract\.test\.js/);
  assert.match(testStrategy, /audit:source/);
  assert.match(architecture, /HTTP adapter/i);
  assert.match(architecture, /protocol primitives stay transport and command agnostic/i);
  assert.match(sourceQuality, /critical/i);
  assert.match(sourceQuality, /large module/i);
});
