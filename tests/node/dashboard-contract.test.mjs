import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';

const repoRoot = path.resolve(import.meta.dirname, '..', '..');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function dashboardHtml() {
  return read('src/dashboard/index.html');
}

function dashboardCss() {
  return read('src/dashboard/frontend/styles.css');
}

function dashboardJs() {
  return read('src/dashboard/frontend/app.js');
}

function dashboardBundle() {
  return `${dashboardHtml()}\n${dashboardCss()}\n${dashboardJs()}`;
}

function stripQuery(value) {
  return value.split('?')[0];
}

function sorted(values) {
  return [...new Set(values)].sort((a, b) => a.localeCompare(b));
}

function backendRoutes() {
  const source = read('src/dashboard.rs');
  const routes = [];
  for (const match of source.matchAll(/\("(GET|POST|DELETE)",\s*"([^"]+)"\)/g)) {
    routes.push(`${match[1]} ${match[2]}`);
  }
  // These paths are configured dynamically but still handled by dashboard.rs before the match arms.
  routes.push('GET /healthz');
  routes.push('GET /mcp');
  routes.push('POST /mcp');
  routes.push('DELETE /mcp');
  return new Set(routes);
}

function frontendGetEndpoints(html) {
  const endpoints = [];
  for (const match of html.matchAll(/(?:timedFetchJson|fetchJson|runAction)\(\s*"([^"]+)"/g)) {
    endpoints.push(match[1]);
  }
  return sorted(endpoints.filter((endpoint) => endpoint.startsWith('/')));
}

function frontendDashboardActions(html) {
  const actions = new Set();
  for (const match of html.matchAll(/(?:runServerAction|postServerAction)\(\s*"((?:server|client)-[a-z-]+)"/g)) {
    actions.add(match[1]);
  }
  for (const match of html.matchAll(/"((?:server-(?:enable|disable|policy|autotune|test|install-command|import-config)|client-(?:install|restore)))"/g)) {
    actions.add(match[1]);
  }
  return sorted([...actions]);
}

test('dashboard frontend references only backend routes that dashboard.rs handles', () => {
  const html = dashboardBundle();
  const routes = backendRoutes();
  const endpoints = frontendGetEndpoints(html);

  const missing = [];
  for (const endpoint of endpoints) {
    if (endpoint === '/api/actions/${endpoint}') continue;
    const method = endpoint.includes('/api/actions/') ? 'POST' : 'GET';
    const route = `${method} ${stripQuery(endpoint)}`;
    if (!routes.has(route)) missing.push(route);
  }

  for (const action of frontendDashboardActions(html)) {
    const route = `POST /api/actions/${action}`;
    if (!routes.has(route)) missing.push(route);
  }

  assert.deepEqual(sorted(missing), []);
});

test('dashboard element registry only points at markup ids that exist', () => {
  const html = dashboardHtml();
  const js = dashboardJs();
  const ids = new Set([...html.matchAll(/\bid="([^"]+)"/g)].map((match) => match[1]));
  const registered = [...js.matchAll(/\$\("([^"]+)"\)/g)].map((match) => match[1]);
  const missing = registered.filter((id) => !ids.has(id));
  assert.deepEqual(sorted(missing), []);
});

test('dashboard action payload contract is aligned with backend parser keys', () => {
  const html = dashboardBundle();
  const backend = read('src/dashboard.rs');

  for (const key of ['server', 'name', 'mode', 'maxWorkers', 'maxInFlightPerWorker', 'timeoutMs', 'changes', 'commandLine', 'sourcePath', 'settingsPath', 'force', 'disabled', 'dryRun', 'allowReviewInstall', 'clientId', 'backup', 'diff']) {
    assert.match(`${html}\n${backend}`, new RegExp(key), `${key} should be visible in the dashboard contract`);
  }

  assert.match(backend, /server_policy_command_args/);
  assert.match(backend, /action_server_name/);
  assert.match(html, /actionPayloadForPolicy/);
  assert.match(html, /normalizeProbeEvidence/);
});


test('dashboard action boundary rejects ambiguous CLI-shaped inputs', () => {
  const backend = read('src/dashboard.rs');

  assert.match(backend, /fn validate_action_name_field/);
  assert.match(backend, /cannot start with '-'/);
  assert.match(backend, /server discovery mode must be preview, install, apply, auto-install, auto, or auto-mode/);
  assert.match(backend, /fn push_reuse_policy_arg/);
  assert.match(backend, /reusePolicy' must be one of sticky, ttl, or never/);
  assert.match(backend, /'affinity' accepts at most 8 entries/);
  assert.match(backend, /validate_action_token_field\("affinity"/);
});


test('dashboard exposes server launch metadata and command install workflow', () => {
  const html = dashboardBundle();
  const backend = read('src/dashboard.rs');
  const model = read('src/server/model.rs');

  for (const key of ['sourcePath', 'sourceCommand', 'sourceArgs', 'sourceEnvNames', 'sourceHeaderNames']) {
    assert.match(model, new RegExp(key), `${key} should be exported in server JSON`);
    assert.match(html, new RegExp(key), `${key} should be visible to dashboard logic`);
  }

  assert.match(html, /id="server-install-form"/);
  assert.match(html, /postServerAction\("server-install-command"/);
  assert.match(backend, /write_server_install_command_action/);
  assert.match(backend, /server install commandLine cannot contain control characters or newlines/);
  assert.match(html, /id="server-import-form"/);
  assert.match(html, /id="server-import-disabled"/);
  assert.match(html, /disabled: els\.serverImportDisabled/);
  assert.match(html, /postServerAction\("server-import-config"/);
  assert.match(backend, /write_server_import_config_action/);
  assert.match(backend, /server import requires a non-empty sourcePath field/);
  assert.match(backend, /args\.push\("--disabled"\.to_string\(\)\)/);
  assert.match(html, /id="client-setup-panel"/);
  assert.match(html, /id="client-apply-all"/);
  assert.match(html, /clientApplyAll: \$\("client-apply-all"\)/);
  assert.match(html, /runClientSetupAction\("client-install"/);
  assert.match(html, /runClientSetupAction\("client-restore"/);
  assert.match(html, /renderClientSetup\(clients, overview\.clients \|\| \{\}\)/);
  assert.match(backend, /write_client_install_action/);
  assert.match(backend, /write_client_restore_action/);
  assert.match(backend, /client action requires a non-empty clientId field/);
  assert.match(backend, /validate_action_token_field/);
});


test('dashboard import preview explains a concrete config diff', () => {
  const html = dashboardBundle();

  assert.match(html, /class="import-diff-grid"/);
  assert.match(html, /Will add/);
  assert.match(html, /Will replace/);
  assert.match(html, /Will skip/);
  assert.match(html, /Saved state/);
});

test('dashboard keeps foundation first and protocol details folded', () => {
  const html = dashboardBundle();

  assert.doesNotMatch(html, /id="connection-map"/);
  assert.doesNotMatch(html, /id="setup-queue"/);
  assert.match(html, /id="protocol-compat-panel"/);
  assert.match(html, /protocolCompatPanel: \$\("protocol-compat-panel"\)/);
  assert.match(html, /renderProtocolCompatibility\(overview, servers, clients, instances\)/);
  assert.match(html, /Client ingress/);
  assert.match(html, /Tool evidence/);
  assert.doesNotMatch(html, /<a href="#connection-map">/);
  assert.doesNotMatch(html, /<a href="#setup-queue">/);
  const renderers = [...html.matchAll(/function renderProtocolCompatibility/g)];
  assert.equal(renderers.length, 1, 'protocol compatibility renderer should not drift into duplicate implementations');
});

test('dashboard frontend shell links parseable external assets', () => {
  const html = dashboardHtml();
  const css = dashboardCss();
  const js = dashboardJs();
  assert.match(html, /<link rel="stylesheet" href="\/dashboard\.css">/);
  assert.match(html, /<script src="\/dashboard\.js" defer><\/script>/);
  assert.doesNotMatch(html, /<script>([\s\S]*?)<\/script>/);
  assert.match(css, /MCPace dashboard styles/);
  assert.doesNotThrow(() => new Function(js));
});

test('dashboard static markup ids are unique', () => {
  const html = dashboardHtml();
  const ids = [...html.matchAll(/\bid="([^"]+)"/g)]
    .map((match) => match[1])
    .filter((id) => !id.includes('${'));
  const counts = new Map();
  for (const id of ids) counts.set(id, (counts.get(id) || 0) + 1);
  const duplicates = [...counts.entries()]
    .filter(([, count]) => count > 1)
    .map(([id]) => id)
    .sort();
  assert.deepEqual(duplicates, []);
});


test('dashboard starts from a plain five-step base setup foundation', () => {
  const html = dashboardBundle();
  const base = html.indexOf('id="base-setup"');
  const signals = html.indexOf('class="signal-strip"');
  const nextAction = html.indexOf('id="next-action-board"');
  const servers = html.indexOf('id="servers-title"');

  assert.ok(base > 0, 'basic setup panel should exist');
  assert.ok(signals > base, 'basic setup should appear before live summary metrics');
  assert.ok(nextAction === -1 || nextAction > signals, 'legacy next-action layer should not be first');
  assert.ok(servers > base, 'servers should follow the base setup foundation');
  assert.match(html, /Make the simple path work first/);
  assert.match(html, /id="base-step-grid"/);
  assert.match(html, /id="base-rules"/);
  assert.match(html, /id="base-progress-label" role="status" aria-live="polite"/);
  assert.match(html, /id="base-safety"[^>]+role="status"[^>]+aria-live="polite"/);
  assert.match(html, /\.base-step\.active/);
  assert.match(html, /aria-current="step"/);
  assert.match(html, /function renderBaseSetup/);
  assert.match(html, /function buildFoundationModelFromOverview/);
  assert.match(html, /function setupFoundationModel/);
  assert.match(html, /overview\.dashboardFoundation/);
  assert.match(html, /A target catalog is not the same as a wired client/);
  assert.match(html, /action === "client" \|\| action === "clients"/);
  assert.match(html, /data-global-action="clients"/);
  assert.match(html, /return items\.slice\(0, 1\)/);
  assert.match(html, /function baseStepCard\(index, step = \{\}, currentKey = ""\)/);
  assert.match(html, /baseStepCard\(index \+ 1, step, currentBaseStepKey\)/);
  assert.match(html, /labelForBaseStepAction\(action, key = ""\)/);
  assert.match(html, /actionLabel/);
  assert.doesNotMatch(html, /arguments\.length > 5/);
  assert.match(html, /of 5 basics complete/);
  assert.match(html, /Next: \$\{text\(model\.nextStep\?\.title, model\.title\)\}/);
  assert.match(html, /els\.baseSetup\?\.addEventListener\("click"/);
  assert.doesNotMatch(html, /href="#connection-map"/);
  assert.doesNotMatch(html, /href="#setup-queue"/);
});

test('dashboard overview exposes a backend-owned foundation model', () => {
  const html = dashboardBundle();
  const overview = read('src/dashboard/overview.rs');

  assert.match(overview, /"dashboardFoundation"/);
  assert.match(overview, /mcpace\.dashboardFoundation\.v1/);
  assert.match(overview, /build_dashboard_foundation_json/);
  assert.match(overview, /"nextStep"/);
  assert.match(overview, /"stateKey"/);
  assert.match(overview, /"nextStepKey"/);
  assert.match(overview, /"actionLabel"/);
  assert.match(overview, /"safety"/);
  assert.match(overview, /mcpace\.dashboardSafety\.v1/);
  assert.match(overview, /enabledWithoutEvidence/);
  assert.match(overview, /remoteSources/);
  assert.match(overview, /secretBearingSources/);
  assert.match(overview, /server_source_is_remote/);
  assert.match(overview, /server_has_secret_boundary/);
  assert.match(html, /function normalizeFoundationSafety/);
  assert.match(html, /function renderBaseSafety/);
  assert.match(html, /renderBaseSafety\(model\.safety \|\| \{\}\)/);
  assert.match(html, /foundation\.nextStep \? normalizeFoundationStep\(foundation\.nextStep/);
  assert.match(html, /dataset\.foundationState/);
  for (const key of ['backend', 'client', 'source', 'tools', 'routing']) {
    assert.match(overview, new RegExp(key));
  }
  assert.match(overview, /preview, save disabled, review, enable, then test/);
  assert.match(overview, /let routing_ready = runtime_ready\s*&&\s*enabled_servers > 0\s*&&\s*cached_ok > 0/s);
  assert.match(overview, /"routingReady"/);
  assert.match(overview, /foundation_actions_json\(primary_action\)/);
  assert.match(html, /buildFoundationModelFromOverview\(overview\.dashboardFoundation\)/);
  assert.match(html, /const routingSafe = Boolean\(runtimeReady && enabled > 0 && usable/);
  assert.match(html, /const routingIssue = !runtimeReady/);
  assert.match(overview, /let next_step_key = json_string\(&next_step, "key", "ready"\);/);
  assert.match(overview, /let primary_action_label = json_string\(&next_step, "actionLabel", "Refresh"\);/);
  assert.doesNotMatch(overview, /let primary_action = if !runtime_ready/);
  assert.match(html, /const primaryAction = \{ label: text\(nextStep\.actionLabel, "Open"\), action: text\(nextStep\.action, "refresh"\) \};/);
  assert.doesNotMatch(html, /!runtimeReady \? \{ label: "Repair", action: "repair" \} : !clientReady/);
});

test('dashboard foundation contract is documented as a small schema', () => {
  const schema = JSON.parse(read('schemas/mcpace-dashboard-foundation.schema.json'));
  const docs = read('docs/dashboard-base.md');

  assert.equal(schema.properties.schema.const, 'mcpace.dashboardFoundation.v1');
  assert.deepEqual(schema.properties.total, { type: 'integer', const: 5 });
  assert.ok(schema.required.includes('nextStep'));
  assert.ok(schema.required.includes('stateKey'));
  assert.ok(schema.required.includes('nextStepKey'));
  assert.equal(schema.properties.steps.minItems, 5);
  assert.equal(schema.properties.steps.maxItems, 5);
  assert.ok(schema.$defs.step.properties.key.enum.includes('routing'));
  assert.ok(schema.$defs.step.required.includes('actionLabel'));
  assert.equal(schema.properties.safety.$ref, '#/$defs/safety');
  assert.equal(schema.$defs.safety.properties.schema.const, 'mcpace.dashboardSafety.v1');
  assert.ok(schema.$defs.safety.properties.counts.$ref.includes('safetyCounts'));
  assert.match(docs, /mcpace-dashboard-foundation\.schema\.json/);
  assert.match(docs, /action-label discipline/);
  assert.match(docs, /dashboardFoundation\.safety/);
});

test('dashboard validates base setup forms next to the relevant field', () => {
  const html = dashboardBundle();

  for (const id of ['server-import-error', 'server-discover-error', 'server-install-error']) {
    assert.match(html, new RegExp(`id="${id}"`));
  }
  assert.match(html, /function setFieldError/);
  assert.match(html, /aria-invalid/);
  assert.ok(html.includes('serverImportError: $("server-import-error")'));
  assert.ok(html.includes('serverDiscoverError: $("server-discover-error")'));
  assert.ok(html.includes('serverInstallError: $("server-install-error")'));
  assert.match(html, /novalidate/);
  assert.match(html, /Preview → Save disabled → Review → Enable → Test/);
});

test('dashboard keeps setup actions progressively disclosed', () => {
  const html = dashboardBundle();
  const serverList = html.indexOf('<div class="list" id="server-list"');
  const setupTools = html.indexOf('<details class="clean setup-tools" id="setup-tools"');
  const importPanel = html.indexOf('<section class="discovery-panel import-panel" id="server-import-panel"');
  const clientSetupPanel = html.indexOf('<section class="client-setup-panel" id="client-setup-panel"');
  const automationPanel = html.indexOf('<section class="automation-panel" id="automation-panel"');
  const setupQueue = html.indexOf('<section class="setup-queue" id="setup-queue"');
  const discoveryPanel = html.indexOf('<section class="discovery-panel" id="server-discovery-panel"');

  assert.ok(serverList > 0, 'server list should exist');
  assert.equal(setupQueue, -1, 'legacy setup queue should not compete with the base path');
  assert.match(html, /return items\.slice\(0, 1\)/);
  assert.ok(setupTools > serverList, 'setup tools should follow the routine server list');
  assert.ok(importPanel > setupTools, 'import UI should live inside the setup drawer');
  assert.ok(discoveryPanel > importPanel, 'discovery should follow import inside setup tools');
  assert.ok(clientSetupPanel > discoveryPanel, 'client setup should follow import and discovery');
  assert.ok(automationPanel > clientSetupPanel, 'automation UI should follow client setup inside the setup drawer');
  assert.match(html, /function updateSetupToolsState/);
  assert.match(html, /function revealElementById/);
  assert.match(html, /MCP lifecycle/);
});

test('dashboard overview exposes backend operator plan and UI consumes it', () => {
  const html = dashboardBundle();
  const overview = read('src/dashboard/overview.rs');
  const backend = read('src/dashboard.rs');

  assert.match(overview, /"operatorPlan"/);
  assert.match(overview, /mcpace\.operatorPlan\.v1/);
  assert.match(overview, /build_operator_plan_json/);
  assert.match(overview, /operator_commands/);
  assert.match(overview, /blockers/);
  assert.match(overview, /safeguards/);
  assert.match(html, /id="operator-plan-panel"/);
  assert.match(html, /renderOperatorPlan\(overview\.operatorPlan/);
  assert.match(html, /renderServerRunbook/);
  assert.match(html, /installCommandIntent/);
  assert.match(html, /commandLineLooksComposed/);
  assert.match(backend, /command_line_uses_shell_composition/);
  assert.match(backend, /remove shell chaining, pipes, redirects, backticks, or command substitutions/);
});

test('dashboard keeps user-readiness data available without adding a first-screen layer', () => {
  const html = dashboardBundle();
  const overview = read('src/dashboard/overview.rs');

  assert.match(overview, /"userReadiness"/);
  assert.match(overview, /mcpace\.userReadiness\.v1/);
  assert.match(overview, /build_user_readiness_json/);
  assert.match(overview, /shouldSee/);
  assert.match(overview, /shouldHide/);
  assert.match(overview, /environment variable values/);
  assert.doesNotMatch(html, /id="user-readiness-title"/);
  assert.match(html, /renderUserReadiness\(overview\.userReadiness/);
  assert.match(html, /normalizeUserReadiness/);
});


test('dashboard base model keeps backend reachability separate from runtime readiness', () => {
  const html = dashboardBundle();
  const overview = read('src/dashboard/overview.rs');
  const docs = read('docs/dashboard-base.md');

  assert.match(overview, /let backend_status = "good";/);
  assert.match(overview, /Runtime prerequisites are a separate concern/);
  assert.match(overview, /"backendReachable", JsonValue::bool\(true\)/);
  assert.match(overview, /"runtimeReady", JsonValue::bool\(runtime_ready\)/);
  assert.match(overview, /Backend online only means \/api\/overview responded/);
  assert.match(html, /backend online is not the same as runtime ready/);
  assert.match(html, /\/api\/overview responded\. Runtime is checked before use\./);
  assert.match(html, /Runtime prerequisites are a use-boundary problem/);
  assert.match(overview, /Runtime prerequisites are checked at the routing\/use boundary/);
  assert.doesNotMatch(html, /<a href="#connection-map">Connection<\/a>/);
  assert.match(docs, /Do not conflate layers/);
});

test('dashboard Test button dispatches one direct probe per click and enable flow reuses it', () => {
  const html = dashboardBundle();
  const branchStart = html.indexOf('if (action === "test") {');
  const branchEnd = html.indexOf('if (action === "workers-dec"', branchStart);
  assert.ok(branchStart > 0 && branchEnd > branchStart, 'explicit Test branch should exist');
  const directTestBranch = html.slice(branchStart, branchEnd);
  assert.match(directTestBranch, /await runServerTest\(name, control\)/);
  const directCalls = [...html.matchAll(/runServerAction\("server-test"/g)];
  assert.equal(directCalls.length, 1, 'server-test dispatch should live in one helper only');
  assert.match(html, /function runServerTest\(serverName, control, options = \{\}\)/);
  assert.match(html, /function enableAndTestServer\(serverName, control\)/);
  assert.match(html, /data-server-action="enable-test"/);
  assert.match(html, /Enable &amp; test/);
  assert.match(html, /await runServerAction\("server-enable", \{ server: serverName \}, control, "Enabling…", \{ sync: false \}\)/);
  assert.match(html, /return runServerTest\(serverName, control\)/);
  assert.match(html, /This can launch the upstream command or call a remote endpoint/);
  assert.match(html, /typeof window\.confirm === "function"/);
});

test('overview command fanout has unique result keys', () => {
  const overview = read('src/dashboard/overview.rs');
  const block = overview.match(/run_json_commands_parallel\([\s\S]*?vec!\[([\s\S]*?)\]\s*,\s*\)\?/);
  assert.ok(block, 'overview should call run_json_commands_parallel with a literal fanout');
  const keys = [...block[1].matchAll(/\("([^"]+)",\s*vec!\[/g)].map((match) => match[1]);
  assert.deepEqual(keys, sorted(keys), 'overview result keys should be declared in stable sorted order');
  assert.equal(keys.length, new Set(keys).size, 'overview result keys should be unique');
  for (const key of keys) {
    assert.match(overview, new RegExp(`take_parallel_result\\(&mut results, "${key}"\\)`));
  }
});


test('dashboard exposes runtime control and per-server resource monitoring contracts', () => {
  const html = dashboardBundle();
  const overview = read('src/dashboard/overview.rs');
  const resources = read('src/resources.rs');
  const sessionPool = read('src/upstream/session_pool.rs');

  assert.match(overview, /"automation"/);
  assert.match(overview, /mcpace\.dashboardAutomation\.v1/);
  assert.match(overview, /"discoveryControl"/);
  assert.match(overview, /mcpace\.discoveryControl\.v1/);
  assert.match(html, /id="automation-panel"/);
  assert.match(html, /renderAutomation\(overview, servers, instances\)/);
  assert.match(html, /Import existing/);
  assert.match(overview, /"runtimeControlPlane"/);
  assert.match(overview, /mcpace\.runtimeControlPlane\.v1/);
  assert.match(overview, /build_runtime_control_plane_json/);
  assert.match(overview, /toolRisk/);
  assert.match(overview, /parallelism/);
  assert.match(overview, /isolation/);
  assert.match(overview, /resourceBudget/);
  assert.match(overview, /mcpace\.serverResourceMonitoring\.v1/);
  assert.match(html, /runtimeControlForServer/);
  assert.match(html, /renderRuntimeControl/);
  assert.match(html, /Runtime control plane/);
  assert.match(html, /Server resources/);
  assert.match(resources, /process_resource_snapshot_json/);
  assert.match(sessionPool, /session_snapshots/);
});

test('dashboard exposes backend-owned access review boundary', () => {
  const html = dashboardBundle();
  const overview = read('src/dashboard/overview.rs');
  const docs = read('docs/dashboard-base.md');

  assert.match(overview, /"accessReview"/);
  assert.match(overview, /mcpace\.dashboardAccessReview\.v1/);
  assert.match(overview, /build_dashboard_access_review_json/);
  for (const key of [
    'approvalRequired',
    'hiddenSecretNames',
    'remoteHttp',
    'enabledWithoutEvidence',
    'sensitiveWithoutEvidence'
  ]) {
    assert.match(overview, new RegExp(key));
  }
  assert.match(overview, /Treat tool annotations as hints/);
  assert.match(html, /id="access-review"/);
  assert.match(html, /accessReviewList: \$\("access-review-list"\)/);
  assert.match(html, /function renderAccessReview/);
  assert.match(html, /renderAccessReview\(overview\.accessReview, servers\)/);
  assert.match(html, /Access review/);
  assert.match(docs, /Access review boundary/);
  assert.match(docs, /not a sixth base step/);
});

test('dashboard access review contract is documented as a small schema', () => {
  const schema = JSON.parse(read('schemas/mcpace-dashboard-access-review.schema.json'));
  const docs = read('docs/dashboard-base.md');

  assert.equal(schema.properties.schema.const, 'mcpace.dashboardAccessReview.v1');
  assert.ok(schema.required.includes('items'));
  assert.ok(schema.required.includes('counts'));
  for (const key of ['approvalRequired', 'hiddenSecretNames', 'remoteHttp', 'enabledWithoutEvidence', 'sensitiveWithoutEvidence']) {
    assert.ok(schema.properties.counts.properties[key], `${key} should stay in the access review counts contract`);
  }
  assert.ok(schema.$defs.item.properties.status.enum.includes('bad'));
  assert.match(docs, /mcpace-dashboard-access-review\.schema\.json/);
});

test('dashboard frontend assets are split but still embedded by Rust routes', () => {
  const html = dashboardHtml();
  const css = dashboardCss();
  const js = dashboardJs();
  const dashboard = read('src/dashboard.rs');
  const response = read('src/dashboard/response.rs');
  const docs = read('docs/frontend.md');
  const manifest = JSON.parse(read('release-manifest.json'));

  assert.ok(html.length < 60000, 'HTML shell should stay readable after frontend split');
  assert.ok(css.length > 1000, 'CSS asset should hold dashboard visual rules');
  assert.ok(js.length > 1000, 'JS asset should hold dashboard behavior');
  assert.match(html, /<link rel="stylesheet" href="\/dashboard\.css">/);
  assert.match(html, /<script src="\/dashboard\.js" defer><\/script>/);
  assert.doesNotMatch(html, /<style>[\s\S]*?<\/style>/);
  assert.doesNotMatch(html, /<script>[\s\S]*?<\/script>/);
  assert.match(dashboard, /const DASHBOARD_CSS: &str = include_str!\("dashboard\/frontend\/styles\.css"\)/);
  assert.match(dashboard, /const DASHBOARD_JS: &str = include_str!\("dashboard\/frontend\/app\.js"\)/);
  assert.match(dashboard, /"GET", "\/dashboard\.css"/);
  assert.match(dashboard, /"GET", "\/dashboard\.js"/);
  assert.match(response, /style-src 'self' 'unsafe-inline'/);
  assert.match(response, /script-src 'self'/);
  assert.match(docs, /Dashboard frontend architecture/);
  assert.ok(manifest.includePaths.includes('docs/frontend.md'));
});

test('dashboard keeps derived server policy work behind an advanced drawer', () => {
  const html = dashboardHtml();
  const bundle = dashboardBundle();
  const serverList = html.indexOf('id="server-list"');
  const setupTools = html.indexOf('id="setup-tools"');
  const addServer = html.indexOf('class="server-install-panel"');
  const advanced = html.indexOf('id="server-advanced"');
  const autoPanel = html.indexOf('id="server-auto-panel"');
  const operatorPlan = html.indexOf('id="operator-plan-panel"');

  assert.ok(serverList > 0, 'server rows should exist');
  assert.ok(setupTools > serverList, 'setup tools stay after routine server rows');
  assert.ok(addServer > setupTools, 'manual add remains a setup task after the setup drawer');
  assert.ok(advanced > addServer, 'bulk policy and derived plans should not precede add/manual setup');
  assert.ok(autoPanel > advanced, 'policy panel should live inside the advanced drawer');
  assert.ok(operatorPlan > autoPanel, 'operator/backend plan should live inside the advanced drawer');
  assert.match(html, /<details class="clean server-advanced" id="server-advanced">/);
  assert.match(html, /Folded by default/);
  assert.match(bundle, /v14 frontend foundation/);
});
