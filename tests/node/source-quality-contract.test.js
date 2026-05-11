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
  assert.ok(Number.isInteger(report.summary.productionUnwraps));
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
  const dashboard = [
    read('src/dashboard.rs'),
    read('src/dashboard/http_boundary.rs'),
    read('src/dashboard/http_headers.rs'),
    read('src/dashboard/http_session.rs'),
    read('src/dashboard/http_tools.rs'),
    read('src/dashboard/mcp_http.rs'),
    read('src/dashboard/tool_runtime.rs'),
    read('src/dashboard/response.rs'),
    read('src/dashboard/tests.rs'),
    read('src/dashboard/index.html'),
  ].join('\n');

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
  assert.equal(packageJson.scripts['lint:npm'], 'node scripts/check-node-syntax.mjs --json');
  assert.equal(packageJson.scripts['lint:node'], 'node scripts/check-node-syntax.mjs --json');
  assert.match(testStrategy, /audit:source/);
  assert.match(architecture, /HTTP adapter/i);
  assert.match(architecture, /protocol primitives stay transport and command agnostic/i);
  assert.match(sourceQuality, /critical/i);
  assert.match(sourceQuality, /large module/i);
});



test('node syntax lint uses discovery instead of a hardcoded package.json file list', () => {
  const packageJson = JSON.parse(read('package.json'));
  const syntaxScript = read('scripts/check-node-syntax.mjs');

  assert.equal(packageJson.scripts['lint:npm'], 'node scripts/check-node-syntax.mjs --json');
  assert.ok(packageJson.scripts['lint:npm'].length < 80, 'lint:npm should stay a short dispatcher');
  assert.match(syntaxScript, /discoverNodeSourceFiles/);
  assert.match(syntaxScript, /SOURCE_ROOTS/);
  assert.match(syntaxScript, /node --check/);
  assert.doesNotMatch(packageJson.scripts['lint:npm'], /&&/);
  assert.doesNotMatch(packageJson.scripts['lint:npm'], /tests\/node\/.*\.test\.js/);
});

test('dashboard HTTP boundary remains split into focused modules instead of a single oversized route file', () => {
  const dashboard = read('src/dashboard.rs');
  const mcpHttp = read('src/dashboard/mcp_http.rs');
  const boundary = read('src/dashboard/http_boundary.rs');
  const session = read('src/dashboard/http_session.rs');
  const tools = read('src/dashboard/http_tools.rs');
  const runtime = read('src/dashboard/tool_runtime.rs');

  assert.match(dashboard, /mod mcp_http/);
  assert.match(dashboard, /mod http_boundary/);
  assert.match(dashboard, /mod http_session/);
  assert.match(mcpHttp, /pub\(super\) fn handle_mcp_http_route/);
  assert.match(mcpHttp, /fn handle_mcp_http_request/);
  assert.match(boundary, /pub\(super\) fn accepts_streamable_http_post/);
  assert.match(session, /pub\(super\) fn normalize_mcp_http_session_id/);
  assert.match(tools, /pub\(super\) fn http_tool_definitions/);
  assert.match(runtime, /pub\(super\) fn run_http_tool/);

  const dashboardLines = dashboard.split(/\r?\n/).length;
  assert.ok(dashboardLines < 1200, `src/dashboard.rs should stay below the audit warning threshold after the split; got ${dashboardLines}`);
});


test('adapter route projection and proxy helpers stay in focused child modules', () => {
  const adapter = read('src/adapter.rs');
  const discovery = read('src/adapter/discovery.rs');
  const profile = read('src/adapter/profile.rs');
  const proxyUri = read('src/adapter/proxy_uri.rs');

  assert.match(adapter, /mod profile/);
  assert.match(adapter, /mod proxy_uri/);
  assert.match(adapter, /pub use self::profile::adapter_profile/);
  assert.match(adapter, /use self::discovery::\{[\s\S]*shape_tool_for_client/);
  assert.match(adapter, /use self::proxy_uri::\{[\s\S]*encode_resource_uri/);

  assert.match(profile, /pub fn adapter_profile/);
  assert.match(profile, /fn client_profile_from_initialize/);
  assert.doesNotMatch(adapter, /fn client_profile_from_initialize/);
  assert.match(proxyUri, /pub\(super\) fn encode_resource_uri/);
  assert.match(proxyUri, /pub\(super\) fn decode_resource_uri/);
  assert.match(proxyUri, /pub\(super\) fn maybe_meta_errors/);
  assert.match(proxyUri, /pub\(super\) fn is_unsupported_method_error/);

  for (const helper of [
    'shape_tool_for_client',
    'paginated_tool_list',
    'tool_names',
    'projected_tool_definition',
    'take_tools_with_budget',
    'estimate_json_tokens',
    'tool_projection_rank',
  ]) {
    assert.match(discovery, new RegExp(`pub\\(super\\) fn ${helper}\\(`), `${helper} should be visible to the adapter root after extraction`);
  }

  const adapterLines = adapter.trimEnd().split(/\r?\n/).length;
  assert.ok(adapterLines < 1200, `src/adapter.rs should stay below the focused split target; got ${adapterLines}`);
});

test('runtime roots stay below the large-module audit threshold after modularization', () => {
  const reportResult = spawnSync(
    process.execPath,
    ['scripts/audit-source.mjs', '--json', '--include', 'src'],
    CHILD_OPTIONS,
  );

  assert.equal(reportResult.status, 0, reportResult.stderr || reportResult.stdout);
  const report = JSON.parse(reportResult.stdout);
  assert.equal(report.summary.largeModules, 0, 'production Rust modules should stay below the source-audit large-module threshold');

  for (const relativePath of [
    'src/adapter.rs',
    'src/client/actions.rs',
    'src/dashboard.rs',
    'src/upstream.rs',
  ]) {
    const lines = read(relativePath).trimEnd().split(/\r?\n/).length;
    assert.ok(lines < 1500, `${relativePath} should stay below 1500 lines; got ${lines}`);
  }

  for (const relativePath of [
    'src/adapter/discovery.rs',
    'src/client/actions/render_models.rs',
    'src/dashboard/index.html',
    'src/upstream/server_config.rs',
    'src/upstream/stdio_runtime.rs',
  ]) {
    assert.equal(fs.existsSync(path.join(repoRoot, relativePath)), true, `${relativePath} should exist as an extracted boundary`);
  }
});

test('client catalog built-in defaults stay isolated from registry loading behavior', () => {
  const catalog = read('src/client_catalog.rs');
  const builtin = read('src/client_catalog/builtin.rs');
  const catalogScript = read('scripts/lib/client-catalog.mjs');

  assert.match(catalog, /mod builtin/);
  assert.match(catalog, /pub use self::builtin::CLIENT_TARGETS/);
  assert.doesNotMatch(catalog, /id: "codex"/);
  assert.match(builtin, /pub const CLIENT_TARGETS/);
  assert.match(builtin, /id: "codex"/);
  assert.match(builtin, /id: "public-http-connector"/);
  assert.match(catalogScript, /src\/client_catalog\/builtin\.rs/);

  const rootLines = catalog.trimEnd().split(/\r?\n/).length;
  const builtinLines = builtin.trimEnd().split(/\r?\n/).length;
  assert.ok(rootLines < 1000, `src/client_catalog.rs should stay under focused boundary target; got ${rootLines}`);
  assert.ok(builtinLines < 500, `src/client_catalog/builtin.rs should stay a compact defaults file; got ${builtinLines}`);
});



test('server preset rendering stays outside the generic server list/capability renderer', () => {
  const server = read('src/server.rs');
  const presets = read('src/server/presets.rs');
  const render = read('src/server/render.rs');
  const presetRender = read('src/server/preset_render.rs');

  assert.match(server, /mod preset_render/);
  assert.match(presets, /use super::preset_render/);
  assert.doesNotMatch(render, /render_preset_catalog/);
  assert.doesNotMatch(render, /Useful MCP presets/);
  assert.match(presetRender, /pub\(super\) fn render_preset_catalog/);
  assert.match(presetRender, /pub\(super\) fn render_preset_install_result/);
  assert.match(presetRender, /pub\(super\) fn render_starter_result/);
  assert.match(presetRender, /repository-flag/);

  const renderLines = render.trimEnd().split(/\r?\n/).length;
  const presetRenderLines = presetRender.trimEnd().split(/\r?\n/).length;
  assert.ok(renderLines < 450, `src/server/render.rs should stay a generic server renderer; got ${renderLines}`);
  assert.ok(presetRenderLines < 180, `src/server/preset_render.rs should stay a focused preset renderer; got ${presetRenderLines}`);
});

test('client list rendering stays split from install and export mutation paths', () => {
  const actions = read('src/client/actions.rs');
  const list = read('src/client/actions/list.rs');

  assert.match(actions, /mod list/);
  assert.match(actions, /pub\(super\) use self::list::run_list/);
  assert.doesNotMatch(actions, /Known client targets:/);
  assert.match(list, /pub\((?:super|in crate::client)\) fn run_list/);
  assert.match(list, /Known client targets:/);
  assert.match(list, /count_static/);

  const actionsLines = actions.trimEnd().split(/\r?\n/).length;
  const listLines = list.trimEnd().split(/\r?\n/).length;
  assert.ok(actionsLines < 1400, `src/client/actions.rs should stay below the post-list-split target; got ${actionsLines}`);
  assert.ok(listLines < 250, `src/client/actions/list.rs should remain a focused read-only command; got ${listLines}`);
});


test('client install backup and restore helpers stay outside the client action dispatcher', () => {
  const actions = read('src/client/actions.rs');
  const backup = read('src/client/actions/backup.rs');

  assert.match(actions, /mod backup/);
  assert.match(actions, /restore_client_install_backup/);
  assert.doesNotMatch(actions, /fn resolve_backup_path/);
  assert.doesNotMatch(actions, /fn latest_backup_path/);
  assert.doesNotMatch(actions, /fn required_manifest_string/);
  assert.match(backup, /pub\(super\) struct ClientInstallBackup/);
  assert.match(backup, /pub\(super\) fn restore_client_install_backup/);
  assert.match(backup, /fn resolve_backup_path/);
  assert.match(backup, /fn latest_backup_path/);
  assert.match(backup, /configPathHash/);

  const actionsLines = actions.trimEnd().split(/\r?\n/).length;
  const backupLines = backup.trimEnd().split(/\r?\n/).length;
  assert.ok(actionsLines < 1250, `src/client/actions.rs should stay under the post-backup-split target; got ${actionsLines}`);
  assert.ok(backupLines < 260, `src/client/actions/backup.rs should stay a focused backup module; got ${backupLines}`);
});

test('mcp stdio server argument parsing stays split from JSON-RPC serving loop', () => {
  const server = read('src/mcp_server.rs');
  const args = read('src/mcp_server/args.rs');

  assert.match(server, /mod args/);
  assert.match(server, /use self::args::\{parse_args, write_help\}/);
  assert.doesNotMatch(server, /fn parse_args/);
  assert.doesNotMatch(server, /fn write_help/);
  assert.match(args, /pub\(super\) struct ParsedArgs/);
  assert.match(args, /pub\(super\) error: Option<String>/);
  assert.match(args, /pub\(super\) root_override: Option<PathBuf>/);
  assert.match(args, /pub\(super\) fn parse_args/);
  assert.match(args, /pub\(super\) fn write_help/);

  const serverLines = server.trimEnd().split(/\r?\n/).length;
  const argsLines = args.trimEnd().split(/\r?\n/).length;
  assert.ok(serverLines < 1250, `src/mcp_server.rs should remain below the focused boundary target; got ${serverLines}`);
  assert.ok(argsLines < 150, `src/mcp_server/args.rs should stay a small parser module; got ${argsLines}`);
});
