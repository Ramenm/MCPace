const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { read, readJson, repoRoot, cleanChildEnv } = require('./helpers.js');

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
  const upstream = [
    read('src/upstream.rs'),
    read('src/upstream/diagnostics.rs'),
    read('src/upstream/policy_suggestions.rs'),
    read('src/upstream/policy_audit.rs'),
    read('src/upstream/inventory.rs'),
    read('src/upstream/process_config.rs'),
    read('src/upstream/projection.rs'),
    read('src/upstream/source_type.rs'),
    read('src/upstream/server_config.rs'),
    read('src/upstream/stdio_runtime.rs'),
    read('src/upstream/tool_cache.rs'),
    read('src/upstream/lease_runtime.rs'),
    read('src/upstream/tests.rs'),
  ].join('\n');

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
  const upstream = [
    read('src/upstream.rs'),
    read('src/upstream/inventory.rs'),
    read('src/upstream/policy_audit.rs'),
    read('src/upstream/policy_suggestions.rs'),
  ].join('\n');

  assert.match(readme, /Bring Your Own MCP servers \(BYO MCP\)/);
  assert.match(readme, /server-candidates\.json` stay empty/);
  assert.match(dynamicAdapter, /BYO MCP configuration model/);
  assert.match(codexGuide, /Packaged defaults remain empty/);
  assert.match(upstream, /"bring-your-own-mcp-servers"/);
  assert.match(upstream, /"mcp_settings\.json\.mcpServers"/);
  assert.match(upstream, /"requiresRecompileForNewServers"/);
  assert.match(upstream, /"installsUpstreamPackages"/);
});

test('full doctor treats Serena context separately from project root', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'mcpace-doctor-serena-'));
  try {
    const appData = path.join(root, 'AppData', 'Roaming');
    const localAppData = path.join(root, 'AppData', 'Local');
    fs.mkdirSync(appData, { recursive: true });
    fs.mkdirSync(localAppData, { recursive: true });
    const config = path.join(root, 'mcp_settings.json');
    fs.writeFileSync(
      config,
      JSON.stringify({
        mcpServers: {
          'serena-test': {
            enabled: true,
            type: 'stdio',
            command: 'uvx',
            initTimeout: 120000,
            options: { timeout: 120000 },
            args: [
              'serena',
              'start-mcp-server',
              '--context',
              'ide',
              '--project',
              '${MCPACE_PRIMARY_WORKSPACE}'
            ],
            env_vars: ['GITHUB_TOKEN', 'GITHUB_PERSONAL_ACCESS_TOKEN']
          }
        }
      }, null, 2)
    );

    const result = spawnSync(
      process.execPath,
      ['scripts/mcpace-full-doctor.mjs', '--root', repoRoot, '--config', config, '--json'],
      {
        cwd: repoRoot,
        encoding: 'utf8',
        env: cleanChildEnv({
          HOME: root,
          USERPROFILE: root,
          APPDATA: appData,
          LOCALAPPDATA: localAppData,
          MCPACE_HOME: root,
        })
      }
    );
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const report = JSON.parse(result.stdout);
    const serena = report.checks.find((check) => check.id === 'server.serena-project:serena-test');
    assert.equal(serena.status, 'pass');
    assert.equal(serena.meta.projectRoot, '${MCPACE_PRIMARY_WORKSPACE}');
    assert.doesNotMatch(serena.detail, /^ide$/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
