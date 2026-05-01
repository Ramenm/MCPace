const test = require('node:test');
const assert = require('node:assert/strict');
const { read, readJson } = require('./helpers.js');

test('packaged MCP defaults do not ship upstream servers or candidate recommendations', () => {
  const sourceSettings = readJson('mcp_settings.json');
  const projectConfig = readJson('mcpace.config.json');
  const candidates = readJson('server-candidates.json');
  const minimalExample = readJson('examples/mcpace-hub.minimal.json');
  const workstationExample = readJson('examples/mcpace-hub.workstation.json');

  assert.deepEqual(sourceSettings.mcpServers, {});
  assert.deepEqual(projectConfig.servers, {});
  assert.deepEqual(candidates, []);
  assert.deepEqual(minimalExample.servers, []);
  assert.deepEqual(workstationExample.servers, []);

  for (const example of [minimalExample, workstationExample]) {
    for (const profile of Object.values(example.profiles.definitions || {})) {
      assert.deepEqual(profile.serverIds, []);
    }
  }
});

test('stdio upstream launch keeps parent environment opt-in and supports Codex-shaped env_vars', () => {
  const upstream = read('src/upstream.rs');

  assert.match(upstream, /fn env_var_names_from_array/);
  assert.match(upstream, /source\s*!=\s*"local"/);
  assert.match(upstream, /fn default_child_process_environment/);
  assert.match(upstream, /command\.env_clear\(\)/);
  assert.match(upstream, /fingerprint_env_value/);
  assert.doesNotMatch(upstream, /format!\("\{\}=[^"]*", key, value\)/);
  assert.match(upstream, /spawn_stdio_server_does_not_forward_unspecified_parent_environment/);
  assert.match(upstream, /server_fingerprint_does_not_embed_secret_env_values/);
  assert.match(upstream, /env_var_names_accept_codex_local_object_entries_and_skip_remote_entries/);
});

test('BYO MCP server model is documented and surfaced in the runtime manifest', () => {
  const readme = read('README.md');
  const dynamicAdapter = read('docs/dynamic-adapter.md');
  const codexGuide = read('docs/codex-mcpace-guide.md');
  const upstream = read('src/upstream.rs');

  assert.match(readme, /Bring Your Own MCP servers \(BYO MCP\)/);
  assert.match(readme, /server-candidates\.json` stay empty/);
  assert.match(dynamicAdapter, /BYO MCP configuration model/);
  assert.match(codexGuide, /Packaged defaults remain empty/);
  assert.match(upstream, /"bring-your-own-mcp-servers"/);
  assert.match(upstream, /"mcp_settings\.json\.mcpServers"/);
  assert.match(upstream, /"requiresRecompileForNewServers"/);
  assert.match(upstream, /"installsUpstreamPackages"/);
});
