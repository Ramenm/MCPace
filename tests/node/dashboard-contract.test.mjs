import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';

const repoRoot = path.resolve(import.meta.dirname, '..', '..');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
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

function frontendServerActions(html) {
  const actions = new Set();
  for (const match of html.matchAll(/(?:runServerAction|postServerAction)\(\s*"(server-[a-z-]+)"/g)) {
    actions.add(match[1]);
  }
  for (const match of html.matchAll(/"(server-(?:enable|disable|policy|autotune|test|install-command))"/g)) {
    actions.add(match[1]);
  }
  return sorted([...actions]);
}

test('dashboard frontend references only backend routes that dashboard.rs handles', () => {
  const html = read('src/dashboard/index.html');
  const routes = backendRoutes();
  const endpoints = frontendGetEndpoints(html);

  const missing = [];
  for (const endpoint of endpoints) {
    if (endpoint === '/api/actions/${endpoint}') continue;
    const method = endpoint.includes('/api/actions/') ? 'POST' : 'GET';
    const route = `${method} ${stripQuery(endpoint)}`;
    if (!routes.has(route)) missing.push(route);
  }

  for (const action of frontendServerActions(html)) {
    const route = `POST /api/actions/${action}`;
    if (!routes.has(route)) missing.push(route);
  }

  assert.deepEqual(sorted(missing), []);
});

test('dashboard element registry only points at markup ids that exist', () => {
  const html = read('src/dashboard/index.html');
  const ids = new Set([...html.matchAll(/\bid="([^"]+)"/g)].map((match) => match[1]));
  const registered = [...html.matchAll(/\$\("([^"]+)"\)/g)].map((match) => match[1]);
  const missing = registered.filter((id) => !ids.has(id));
  assert.deepEqual(sorted(missing), []);
});

test('dashboard action payload contract is aligned with backend parser keys', () => {
  const html = read('src/dashboard/index.html');
  const backend = read('src/dashboard.rs');

  for (const key of ['server', 'name', 'mode', 'maxWorkers', 'maxInFlightPerWorker', 'timeoutMs', 'changes', 'commandLine', 'force', 'disabled', 'dryRun']) {
    assert.match(`${html}\n${backend}`, new RegExp(key), `${key} should be visible in the dashboard contract`);
  }

  assert.match(backend, /server_policy_command_args/);
  assert.match(backend, /action_server_name/);
  assert.match(html, /actionPayloadForPolicy/);
  assert.match(html, /normalizeProbeEvidence/);
});


test('dashboard exposes server launch metadata and command install workflow', () => {
  const html = read('src/dashboard/index.html');
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
});

test('dashboard embedded script parses as JavaScript', () => {
  const html = read('src/dashboard/index.html');
  const match = html.match(/<script>([\s\S]*?)<\/script>/);
  assert.ok(match, 'dashboard should include one inline script');
  assert.doesNotThrow(() => new Function(match[1]));
});

test('dashboard overview exposes backend operator plan and UI consumes it', () => {
  const html = read('src/dashboard/index.html');
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

test('dashboard exposes user-readiness decision layer for the normal user view', () => {
  const html = read('src/dashboard/index.html');
  const overview = read('src/dashboard/overview.rs');

  assert.match(overview, /"userReadiness"/);
  assert.match(overview, /mcpace\.userReadiness\.v1/);
  assert.match(overview, /build_user_readiness_json/);
  assert.match(overview, /shouldSee/);
  assert.match(overview, /shouldHide/);
  assert.match(overview, /environment variable values/);
  assert.match(html, /id="user-readiness-title"/);
  assert.match(html, /renderUserReadiness\(overview\.userReadiness/);
  assert.match(html, /normalizeUserReadiness/);
});

test('dashboard Test button dispatches one probe per click', () => {
  const html = read('src/dashboard/index.html');
  const calls = [...html.matchAll(/runServerAction\("server-test"/g)];
  assert.equal(calls.length, 1, 'one Test click should not double-probe the same server');
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
  const html = read('src/dashboard/index.html');
  const overview = read('src/dashboard/overview.rs');
  const resources = read('src/resources.rs');
  const sessionPool = read('src/upstream/session_pool.rs');

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
