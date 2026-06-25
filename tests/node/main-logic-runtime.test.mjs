import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const repoRoot = path.resolve(import.meta.dirname, '..', '..');
const read = (relativePath) => fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
const indexOfOrThrow = (source, needle, label = needle) => {
  const index = source.indexOf(needle);
  assert.notEqual(index, -1, `${label} is missing`);
  return index;
};

const indexOfRegexOrThrow = (source, pattern, label = String(pattern)) => {
  const match = source.match(pattern);
  assert.ok(match?.index !== undefined, `${label} is missing`);
  return match.index;
};

test('public command routing matches the documented primary runtime path', () => {
  const app = read('src/app.rs');
  const catalog = read('src/catalog.rs');
  const help = read('src/app.rs');
  assert.match(catalog, /name: "setup"[\s\S]*aliases: &\["up", "quickstart", "bootstrap", "one-click"\]/);
  assert.match(app, /"setup" => setup::run/);
  assert.match(app, /"serve" => serve::run/);
  assert.match(app, /"server" => server::run/);
  assert.match(app, /"client" => client::run/);
  assert.match(help, /mcpace up/);
  assert.match(help, /does not add a default upstream server/);
});

test('main read-modify-write flows take locks before reading current state', () => {
  const clientActions = read('src/client/actions.rs');
  const clientLock = indexOfOrThrow(clientActions, 'acquire_exclusive_file_lock(\n                &self.config_path,\n                "client config install",');
  const clientRead = indexOfOrThrow(clientActions, 'fs::read_to_string(&self.config_path)');
  const clientWrite = indexOfOrThrow(clientActions, 'runtimepaths::write_text_atomic(&self.config_path, &update.contents)');
  assert.ok(clientLock < clientRead, 'client config install lock must be acquired before reading config');
  assert.ok(clientRead < clientWrite, 'client config install must write atomically after computing update');

  const policy = read('src/server/policy.rs');
  const policyLock = indexOfOrThrow(policy, 'acquire_exclusive_file_lock(&config_path, "server policy update")');
  const policyRead = indexOfOrThrow(policy, 'fs::read_to_string(&config_path)');
  const policyWrite = indexOfOrThrow(policy, 'runtimepaths::write_text_atomic(\n            &config_path,');
  assert.ok(policyLock < policyRead, 'server policy lock must be acquired before reading mcpace.config.json');
  assert.ok(policyRead < policyWrite, 'server policy update must write atomically after parsing current config');

  const projects = read('src/projects.rs');
  const upsertProject = projects.slice(indexOfOrThrow(projects, 'fn upsert_project(path: &Path, summary: &ProjectSummary)'));
  const projectLock = indexOfRegexOrThrow(
    upsertProject,
    /acquire_exclusive_file_lock\(\s*path,\s*"project registry update"\s*\)/,
    'project registry update lock call',
  );
  const projectRead = indexOfOrThrow(upsertProject, 'json_helpers::read_json_file(path)?');
  const projectWrite = indexOfOrThrow(upsertProject, 'runtimepaths::write_text_atomic(path, &root.to_pretty_string())');
  assert.ok(projectLock < projectRead, 'project registry lock must be acquired before reading registry');
  assert.ok(projectRead < projectWrite, 'project registry update must write atomically after merge');
});

test('setup home import participates in the same MCP settings namespace contract', () => {
  const setup = read('src/setup.rs');
  const namespaceLock = indexOfOrThrow(setup, 'mcp_sources::acquire_mcp_settings_namespace_lock(root_path)');
  const targetLock = indexOfRegexOrThrow(
    setup,
    /runtimepaths::acquire_exclusive_file_lock\(\s*&target_path,\s*"home MCP import"\s*\)/,
    'home MCP import target-file lock call',
  );
  const collect = indexOfOrThrow(setup, 'let sources = collect_existing_home_mcp_sources(root_path, warnings);');
  const write = indexOfOrThrow(setup, 'runtimepaths::write_private_text_atomic(&target_path, &serialized)');
  assert.ok(namespaceLock < targetLock, 'home import must acquire namespace lock before target-file lock');
  assert.ok(targetLock < collect, 'home import should collect and decide while the MCP namespace is locked');
  assert.ok(collect < write, 'home import writes the computed import file atomically');
});

test('serve/dashboard boundary remains opt-in and request-gated', () => {
  const dashboard = read('src/dashboard.rs');
  const boundary = read('src/dashboard/http_boundary.rs');
  const mcpHttp = read('src/dashboard/mcp_http.rs');
  assert.match(dashboard, /std::env::var\("MCPACE_TOOL_LIST_WARMUP"\)/);
  assert.match(dashboard, /value\.trim\(\)\.to_ascii_lowercase\(\)\.as_str\(\)/);
  assert.match(dashboard, /"1" \| "true" \| "yes" \| "on" \| "enabled"/);
  assert.match(boundary, /multiple Origin headers are not allowed/);
  assert.match(boundary, /origin_allowed_for_bind/);
  assert.match(mcpHttp, /track_request_id/);
  assert.match(mcpHttp, /mark_initialized/);
});

test('release lane still publishes native packages before the launcher package', () => {
  const workflow = read('.github/workflows/publish-npm.yml');
  const contract = read('scripts/verify-npm-publish-contract.mjs');
  const builder = read('scripts/build-native-npm-package.mjs');
  const nativeJob = indexOfOrThrow(workflow, 'native-packages:');
  const publishJob = indexOfOrThrow(workflow, 'publish:');
  const nativePublish = indexOfOrThrow(workflow, 'Publish native npm packages');
  const launcherPublish = indexOfOrThrow(workflow, 'Publish main npm launcher');
  assert.ok(nativeJob < publishJob, 'workflow must build native packages before publish job');
  assert.ok(nativePublish < launcherPublish, 'native packages must publish before launcher');
  assert.match(contract, /verifyNativePackageTarball/);
  assert.match(contract, /package\/package\.json/);
  assert.match(builder, /function writePackageJson/);
  assert.match(builder, /unknown release target/);
});
