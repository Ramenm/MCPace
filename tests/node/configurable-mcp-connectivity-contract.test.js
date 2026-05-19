const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { test } = require('node:test');

const runtimepaths = readFileSync('src/runtimepaths.rs', 'utf8');
const mcpSources = [
  readFileSync('src/mcp_sources.rs', 'utf8'),
  readFileSync('src/mcp_sources/paths.rs', 'utf8'),
  readFileSync('src/mcp_sources/import.rs', 'utf8'),
  readFileSync('src/mcp_sources/write.rs', 'utf8'),
  readFileSync('src/mcp_sources/write_helpers.rs', 'utf8'),
].join('\n');
const upstream = [
  readFileSync('src/upstream.rs', 'utf8'),
  readFileSync('src/upstream/diagnostics.rs', 'utf8'),
  readFileSync('src/upstream/policy_suggestions.rs', 'utf8'),
  readFileSync('src/upstream/policy_audit.rs', 'utf8'),
  readFileSync('src/upstream/inventory.rs', 'utf8'),
  readFileSync('src/upstream/process_config.rs', 'utf8'),
  readFileSync('src/upstream/projection.rs', 'utf8'),
  readFileSync('src/upstream/source_type.rs', 'utf8'),
  readFileSync('src/upstream/server_config.rs', 'utf8'),
  readFileSync('src/upstream/stdio_runtime.rs', 'utf8'),
  readFileSync('src/upstream/tests.rs', 'utf8'),
].join('\n');
const loader = readFileSync('src/server/loader.rs', 'utf8');
const serverArgs = readFileSync('src/server/args.rs', 'utf8');
const serverCommand = readFileSync('src/server.rs', 'utf8');
const serverSources = readFileSync('src/server/sources.rs', 'utf8');
const serverAdd = readFileSync('src/server/add.rs', 'utf8');
const serverImport = readFileSync('src/server/import.rs', 'utf8');
const serverRemove = readFileSync('src/server/remove.rs', 'utf8');
const serverToggle = readFileSync('src/server/toggle.rs', 'utf8');
const serverTest = readFileSync('src/server/test.rs', 'utf8');
const serverRender = readFileSync('src/server/render.rs', 'utf8');
const app = readFileSync('src/app.rs', 'utf8');
const clientActions = readFileSync('src/client/actions.rs', 'utf8');
const connect = [
  readFileSync('src/connect.rs', 'utf8'),
  readFileSync('src/connect/args.rs', 'utf8'),
  readFileSync('src/connect/model.rs', 'utf8'),
  readFileSync('src/connect/render.rs', 'utf8'),
].join('\n');
const catalog = readFileSync('src/catalog.rs', 'utf8');
const dashboard = [
  readFileSync('src/dashboard.rs', 'utf8'),
  readFileSync('src/dashboard/http_boundary.rs', 'utf8'),
  readFileSync('src/dashboard/http_headers.rs', 'utf8'),
  readFileSync('src/dashboard/http_session.rs', 'utf8'),
  readFileSync('src/dashboard/http_tools.rs', 'utf8'),
  readFileSync('src/dashboard/mcp_http.rs', 'utf8'),
  readFileSync('src/dashboard/tool_runtime.rs', 'utf8'),
  readFileSync('src/dashboard/tests.rs', 'utf8'),
  readFileSync('src/dashboard/index.html', 'utf8'),
].join('\n');
const setup = readFileSync('src/setup.rs', 'utf8');
const doctor = readFileSync('src/doctor.rs', 'utf8');
const config = JSON.parse(readFileSync('mcpace.config.json', 'utf8'));
const releaseManifest = JSON.parse(readFileSync('release-manifest.json', 'utf8'));

test('serve endpoint advertised to clients is configurable instead of a fixed localhost literal', () => {
  assert.match(runtimepaths, /MCPACE_PUBLIC_MCP_URL/);
  assert.match(runtimepaths, /MCPACE_SERVE_HOST/);
  assert.match(runtimepaths, /MCPACE_SERVE_PORT/);
  assert.match(runtimepaths, /serve", "publicUrl/);
  assert.match(clientActions, /configured_mcp_url\(root_path\)/);
  assert.match(clientActions, /public_mcp_url_or_placeholder/);
  assert.match(setup, /resolve_serve_endpoint\(Some\(&root_path\)\)/);
  assert.equal(config.serve.host, '127.0.0.1');
  assert.equal(config.serve.port, 39022);
  assert.equal(config.serve.mcpPath, '/mcp');
});

test('upstream MCP server sources are extensible without recompiling built-in server names', () => {
  assert.match(mcpSources, /MCPACE_MCP_SETTINGS/);
  assert.match(mcpSources, /MCPACE_MCP_SETTINGS_DIRS/);
  assert.match(mcpSources, /mcpSettings", "includePaths/);
  assert.match(mcpSources, /mcpSettings", "includeDirs/);
  assert.match(mcpSources, /mcp_settings\.d/);
  assert.match(mcpSources, /duplicate MCP server/);
  assert.match(upstream, /load_mcp_server_registry\(root_path\)/);
  assert.match(loader, /load_mcp_server_registry\(root_path\)/);
  assert.ok(Array.isArray(config.mcpSettings.includePaths));
  assert.deepEqual(config.mcpSettings.includeDirs, ['mcp_settings.d']);
  assert.ok(releaseManifest.includePaths.includes('mcp_settings.d'));
});

test('HTTP session affinity keeps different clients chats and projects separable', () => {
  assert.match(dashboard, /Mcp-Session-Id/);
  assert.match(dashboard, /generated_mcp_http_session_id/);
  assert.match(dashboard, /x-mcpace-conversation-id/);
  assert.match(dashboard, /x-mcpace-chat-id/);
  assert.match(dashboard, /x-mcpace-project-root/);
  assert.match(dashboard, /upstream_session_pools/);
});

test('configured MCP route is actually accepted by the HTTP router and setup probe', () => {
  assert.match(dashboard, /configured_http_paths\(config\)/);
  assert.match(dashboard, /matches_configured_path\(\s*&request\.path,\s*&mcp_path/s);
  assert.match(dashboard, /handle_mcp_http_route\(stream, request, config\)/);
  assert.match(setup, /http_mcp_request\(\s*&probe_host,\s*port,\s*&mcp_path/s);
  assert.match(setup, /Accept: application\/json, text\/event-stream/);
});

test('doctor runtime prerequisites use the same multi-source MCP server registry as runtime routing', () => {
  assert.match(doctor, /use crate::mcp_sources/);
  assert.match(doctor, /load_mcp_server_registry\(root_path\)/);
  assert.doesNotMatch(doctor, /root_path\.join\("mcp_settings\.json"\)/);
});

test('server sources command gives users a native MCP source inventory', () => {
  assert.match(serverArgs, /sources/);
  assert.match(app, /server sources \[--json\] \[--root <path>\]/);
  assert.match(serverSources, /load_mcp_source_report\(&root_path\)/);
  assert.match(serverRender, /render_sources/);
  assert.match(serverRender, /MCP settings sources/);
  assert.match(mcpSources, /load_mcp_source_report/);
});

test('server add provides a first-class BYO MCP onboarding path without manual JSON editing', () => {
  assert.match(serverArgs, /server add <name> --command <cmd>/);
  assert.match(serverArgs, /--settings <path>/);
  assert.match(serverArgs, /--dry-run/);
  assert.match(serverArgs, /--force/);
  assert.match(serverAdd, /write_mcp_server_entry/);
  assert.match(serverRender, /render_add_result/);
  assert.match(mcpSources, /default_mcp_server_fragment_path/);
  assert.match(mcpSources, /mcp_settings\.d/);
  assert.match(mcpSources, /parse_key_value_pairs/);
  assert.match(mcpSources, /validate_env_name/);
  assert.match(mcpSources, /validate_http_header_name/);
  assert.match(mcpSources, /validate_remote_mcp_url/);
});


test('server import provides native migration from existing MCP client configs', () => {
  assert.match(serverArgs, /import/);
  assert.match(serverArgs, /--from <mcp-settings\.json>/);
  assert.match(app, /server import --from <mcp-settings\.json>/);
  assert.match(serverCommand, /mod import/);
  assert.match(serverCommand, /import::run/);
  assert.match(serverImport, /import_mcp_server_entries/);
  assert.match(serverRender, /render_import_result/);
  assert.match(mcpSources, /McpServerImportOptions/);
  assert.match(mcpSources, /import_mcp_server_entries/);
  assert.match(mcpSources, /target_file_count/);
  assert.match(mcpSources, /mcpServers object/);
});

test('server remove provides native cleanup for BYO MCP fragments without manual JSON editing', () => {
  assert.match(serverArgs, /remove/);
  assert.match(app, /server remove <name> \[--settings <path>\] \[--dry-run\] \[--json\]/);
  assert.match(serverCommand, /mod remove/);
  assert.match(serverCommand, /remove::run/);
  assert.match(serverRemove, /remove_mcp_server_entry/);
  assert.match(serverRender, /render_remove_result/);
  assert.match(mcpSources, /McpServerRemoveOptions/);
  assert.match(mcpSources, /find_source_path_for_server/);
});

test('upstream loader normalizes HTTP transport aliases before runtime diagnostics', () => {
  assert.match(upstream, /fn infer_source_type/);
  assert.match(upstream, /streamable-http/);
  assert.match(upstream, /=> "http"\.to_string\(\)/);
  assert.match(upstream, /blocked-http-upstream/);
});



test('server enable and disable provide native pause/resume without deleting BYO MCP entries', () => {
  assert.match(serverArgs, /enable/);
  assert.match(serverArgs, /disable/);
  assert.match(app, /server enable\|disable <name>/);
  assert.match(serverCommand, /mod toggle/);
  assert.match(serverCommand, /toggle::run/);
  assert.match(serverToggle, /set_mcp_server_enabled/);
  assert.match(serverRender, /render_toggle_result/);
  assert.match(mcpSources, /McpServerToggleOptions/);
  assert.match(mcpSources, /set_mcp_server_enabled/);
  assert.match(mcpSources, /previousEnabled/);
});

test('server test provides native live stdio smoke before wiring a client', () => {
  assert.match(serverArgs, /test/);
  assert.match(serverArgs, /--timeout-ms <ms>/);
  assert.match(app, /server test \[<name>\|--name <server>\] \[--timeout-ms <ms>\]/);
  assert.match(serverCommand, /mod test/);
  assert.match(serverCommand, /test::run/);
  assert.match(serverTest, /upstream::probe_servers/);
  assert.match(serverRender, /render_test_result/);
});

test('client export guidance uses resolved endpoint URL instead of hardcoded local port messages', () => {
  assert.match(clientActions, /configured_mcp_url\(root_path\)/);
  assert.doesNotMatch(clientActions, /DEFAULT_LOCAL_MCP_PORT/);
  assert.match(clientActions, /public_mcp_url_or_placeholder/);
  assert.match(runtimepaths, /PUBLIC_MCP_RELAY_PLACEHOLDER_URL/);
});

test('source audit treats extracted Rust test modules as tests instead of production code', () => {
  const sourceAudit = readFileSync('scripts/audit-source.mjs', 'utf8');
  assert.match(sourceAudit, /src\\\/.+\\\/tests\\\.rs/);
  assert.match(sourceAudit, /splitProductionAndTestRust\(lines, relative\)/);
});


test('connect command gives users one client-first wiring report', () => {
  assert.match(app, /"connect" => connect::run/);
  assert.match(catalog, /name: "connect"/);
  assert.match(catalog, /aliases: &\["guide", "next", "onboard"\]/);
  assert.match(connect, /mcpace\.connectReport\.v1/);
  assert.match(connect, /resolve_serve_endpoint\(Some\(root_path\)\)/);
  assert.match(connect, /load_mcp_source_report\(root_path\)/);
  assert.match(connect, /load_server_records\(root_path\)/);
  assert.match(connect, /client_catalog::load_registry\(Some\(root_path\)\)/);
  assert.match(connect, /verify::collect_readiness\(root_path\)/);
  assert.match(connect, /mcpace server install <npm-package\|npm:package\|pypi:package>/);
  assert.doesNotMatch(connect, /mcpace server presets/);
  assert.doesNotMatch(connect, /mcpace server starter/);
  assert.match(connect, /mcpace server import/);
  assert.match(connect, /mcpace server test/);
  assert.match(connect, /mcpace client export/);
  assert.match(connect, /mcpace client install/);
});

test('connect command stays read-only and does not mutate MCP settings or client configs', () => {
  assert.doesNotMatch(connect, /write_mcp_server_entry/);
  assert.doesNotMatch(connect, /remove_mcp_server_entry/);
  assert.doesNotMatch(connect, /set_mcp_server_enabled/);
  assert.doesNotMatch(connect, /fs::write/);
  assert.doesNotMatch(connect, /create_dir_all/);
});

test('useful MCP installs are automatic package/url/command specs instead of packaged static catalogs', () => {
  const autoInstall = readFileSync('src/mcp_autoinstall.rs', 'utf8');
  const serverInstall = readFileSync('src/server/install.rs', 'utf8');
  const serverArgsNow = readFileSync('src/server/args.rs', 'utf8');
  const serverRenderNow = readFileSync('src/server/render.rs', 'utf8');
  const serverRoot = readFileSync('src/server.rs', 'utf8');
  const lib = readFileSync('src/lib.rs', 'utf8');
  const releaseManifestNow = JSON.parse(readFileSync('release-manifest.json', 'utf8'));
  const configNow = JSON.parse(readFileSync('mcpace.config.json', 'utf8'));
  const schemaNow = JSON.parse(readFileSync('schemas/mcpace-config.schema.json', 'utf8'));

  assert.match(lib, /mod mcp_autoinstall/);
  assert.match(serverRoot, /mod install/);
  assert.match(serverRoot, /install::run/);
  assert.doesNotMatch(serverRoot, /starter/);
  assert.match(serverArgsNow, /server install <npm-package\|npm:package\|pypi:package\|oci:image\|url>/);
  assert.doesNotMatch(serverArgsNow, /server presets/);
  assert.doesNotMatch(serverArgsNow, /server starter/);
  assert.match(serverInstall, /install_auto/);
  assert.match(serverRenderNow, /render_install_result/);

  assert.match(autoInstall, /npx/);
  assert.match(autoInstall, /uvx/);
  assert.match(autoInstall, /docker/);
  assert.match(autoInstall, /Streamable HTTP is treated as session-bound/);
  assert.match(autoInstall, /statefulness is inferred later from source hints and live MCP probes/);
  assert.ok(!releaseManifestNow.includePaths.includes('presets'));
  assert.ok(configNow.autoProfile);
  assert.equal(configNow.mcpPresets, undefined);
  assert.ok(schemaNow.properties.autoProfile);
  assert.equal(schemaNow.properties.mcpPresets, undefined);
});
