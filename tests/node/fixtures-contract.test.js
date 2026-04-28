const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

function listJson(relativeDir) {
  return fs.readdirSync(path.join(repoRoot, relativeDir))
    .filter((name) => name.endsWith('.json'))
    .sort();
}

test('runtime capability inventory parses and separates implementation status from public claim status', () => {
  const value = readJson('eval/runtime-capabilities.json');
  const rootVersion = readJson('package.json').version;
  assert.equal(value.version, rootVersion);
  assert.ok(value.claimStatusLegend);
  assert.ok(Array.isArray(value.features));
  assert.ok(value.features.length >= 10);

  const statuses = new Set();
  const claimStatuses = new Set();
  const featuresById = new Map();
  for (const feature of value.features) {
    assert.match(feature.id, /^[a-z0-9][a-z0-9-]*$/);
    assert.equal(typeof feature.area, 'string');
    assert.equal(typeof feature.title, 'string');
    assert.ok(['implemented', 'planned', 'missing'].includes(feature.status));
    assert.ok([
      'supported',
      'supported-local-only',
      'control-plane-only',
      'bootstrap-only',
      'connectable-preview',
      'requires-host-proof',
      'planned'
    ].includes(feature.claimStatus));
    assert.ok(['p0', 'p1', 'p2'].includes(feature.priority));
    assert.ok(Array.isArray(feature.evidence));
    featuresById.set(feature.id, feature);
    statuses.add(feature.status);
    claimStatuses.add(feature.claimStatus);
  }

  assert.ok(statuses.has('implemented'));
  assert.ok(statuses.has('planned'));
  assert.ok(claimStatuses.has('control-plane-only'));
  assert.ok(claimStatuses.has('bootstrap-only'));
  assert.ok(claimStatuses.has('connectable-preview'));
  assert.equal(featuresById.get('adapter-config-merge-safety')?.status, 'implemented');
  assert.match(
    featuresById.get('adapter-config-merge-safety')?.summary ?? '',
    /real-looking JSON\/YAML merge regressions/
  );
});

test('runtime lab fixtures parse and separate typical, edge, adversarial, and held-out cases', () => {
  const files = listJson('eval/fixtures/runtime');
  assert.ok(files.length >= 8);

  const categories = new Set();
  const proofLayers = new Set();
  let heldOutCount = 0;
  for (const file of files) {
    const value = readJson(path.join('eval', 'fixtures', 'runtime', file));
    assert.equal(typeof value.id, 'string');
    assert.equal(typeof value.suite, 'string');
    assert.equal(typeof value.category, 'string');
    assert.ok(['typical', 'edge', 'adversarial', 'held-out'].includes(value.category));
    assert.equal(typeof value.proofLayer, 'string');
    assert.ok(['planner', 'runtime', 'adapter', 'compat', 'release'].includes(value.proofLayer));
    assert.equal(typeof value.heldOut, 'boolean');
    assert.equal(typeof value.traffic, 'object');
    assert.ok(Array.isArray(value.traffic.serverPolicies));
    assert.ok(Array.isArray(value.checks));
    assert.ok(value.checks.length >= 1);
    assert.ok(Array.isArray(value.requires));
    assert.ok(value.requires.length >= 1);
    categories.add(value.category);
    proofLayers.add(value.proofLayer);
    if (value.heldOut) heldOutCount += 1;
  }

  assert.ok(categories.has('typical'));
  assert.ok(categories.has('edge'));
  assert.ok(categories.has('adversarial'));
  assert.ok(categories.has('held-out'));
  assert.ok(proofLayers.has('planner'));
  assert.ok(proofLayers.has('runtime'));
  assert.ok(heldOutCount >= 1);
});


test('runtime fixtures keep cloud and tools-only client surfaces explicit', () => {
  const files = listJson('eval/fixtures/runtime');
  const clientArchetypes = new Set();
  for (const file of files) {
    const value = readJson(path.join('eval', 'fixtures', 'runtime', file));
    clientArchetypes.add(value.traffic?.clientArchetype);
  }

  assert.ok(clientArchetypes.has('claude-api-connector'));
  assert.ok(clientArchetypes.has('github-copilot-cloud-agent'));
  assert.ok(clientArchetypes.has('cursor-cloud-agents'));
  assert.ok(clientArchetypes.has('windsurf'));
});

test('windows desktop MCP is stdio-shaped and gated by explicit desktop profile', () => {
  const settings = readJson('mcp_settings.json').mcpServers['windows-mcp'];
  const config = readJson('mcpace.config.json');
  const policy = config.servers['windows-mcp'];
  const profiles = config.profiles.runtime.profiles;
  const manager = readJson('manager.settings.json');

  assert.equal(settings.enabled, true);
  assert.equal(settings.type, 'stdio');
  assert.equal(settings.command, 'uvx');
  assert.deepEqual(settings.args, ['windows-mcp']);
  assert.equal(policy.autoStart, false);
  assert.equal(policy.transportPreference, 'stdio-http-bridge');
  assert.deepEqual(policy.platforms, ['windows']);
  assert.equal(policy.policy.hostLock, 'desktop-session');
  assert.equal(policy.policy.concurrencyPolicy, 'single-session');
  assert.ok(Array.isArray(policy.toolPolicies));
  assert.deepEqual(
    policy.toolPolicies.map((entry) => entry.riskClass).sort(),
    ['desktop-control', 'desktop-observation', 'system-control'],
  );
  assert.deepEqual(
    policy.toolPolicies.map((entry) => entry.allowArgument).sort(),
    ['allowDesktopControl', 'allowDesktopObservation', 'allowSystemControl'],
  );
  assert.equal(profiles.safe.serverOverrides['windows-mcp'], undefined);
  assert.equal(profiles.desktop.serverOverrides['windows-mcp'].enabled, true);
  assert.equal(manager.runtimeProfile.active, 'desktop');
  assert.ok(!JSON.stringify(settings).includes('host.docker.internal'));
});

test('stateful reference MCP servers use conservative declarative policies', () => {
  const config = readJson('mcpace.config.json');

  const filesystem = config.servers.filesystem;
  assert.equal(filesystem.policy.concurrencyPolicy, 'single-writer');
  assert.equal(filesystem.policy.parallelismLimit, 1);
  assert.equal(filesystem.policy.stateBinding, 'workspace-roots');
  assert.deepEqual(
    filesystem.toolPolicies.map((entry) => entry.riskClass),
    ['filesystem-mutation'],
  );
  assert.ok(filesystem.toolPolicies[0].tools.includes('write_file'));
  assert.ok(filesystem.toolPolicies[0].tools.includes('edit_file'));

  const memory = config.servers.memory;
  assert.equal(memory.policy.concurrencyPolicy, 'single-writer');
  assert.equal(memory.policy.parallelismLimit, 1);
  assert.equal(memory.policy.stateBinding, 'runtime-memory');
  assert.deepEqual(
    memory.toolPolicies.map((entry) => entry.riskClass),
    ['memory-mutation'],
  );
  assert.ok(memory.toolPolicies[0].tools.includes('add_observations'));
  assert.ok(memory.toolPolicies[0].tools.includes('delete_entities'));

  const sequentialThinking = config.servers['sequential-thinking'];
  assert.equal(sequentialThinking.policy.concurrencyPolicy, 'single-session');
  assert.equal(sequentialThinking.policy.parallelismLimit, 1);
  assert.equal(sequentialThinking.policy.stateBinding, 'chat-session');
  assert.equal(sequentialThinking.policy.routingGroup, 'session');

  assert.deepEqual(
    config.servers['lean-ctx'].toolPolicies.map((entry) => entry.riskClass).sort(),
    ['lean-mutation', 'lean-shell'],
  );
  assert.ok(config.servers['lean-ctx'].toolPolicies[0].tools.includes('ctx_edit'));
  assert.ok(config.servers['lean-ctx'].toolPolicies[1].tools.includes('ctx_shell'));

  assert.deepEqual(
    config.servers.serena.toolPolicies.map((entry) => entry.riskClass).sort(),
    ['code-mutation', 'serena-memory-mutation'],
  );
  assert.ok(config.servers.serena.toolPolicies[0].tools.includes('replace_symbol_body'));
  assert.ok(config.servers.serena.toolPolicies[1].tools.includes('write_memory'));
});

test('canary MCP integrations stay profile gated with mutation policies', () => {
  const config = readJson('mcpace.config.json');
  const settings = readJson('mcp_settings.json').mcpServers;
  const labs = config.profiles.runtime.profiles.labs.serverOverrides;

  assert.deepEqual(
    config.servers.browser.toolPolicies.map((entry) => entry.riskClass),
    ['browser-control'],
  );
  assert.equal(config.servers.browser.toolPolicies[0].allowArgument, 'allowBrowserControl');
  assert.ok(config.servers.browser.toolPolicies[0].tools.includes('browser_navigate'));
  assert.ok(config.servers.browser.toolPolicies[0].tools.includes('browser_javascript'));

  assert.equal(settings.time.enabled, true);
  assert.equal(config.servers.time.defaultEnabled, true);
  assert.equal(config.servers.time.policy.concurrencyPolicy, 'multi-reader');

  for (const server of ['git', 'everything', 'sqlite', 'playwright']) {
    assert.equal(settings[server].enabled, true);
    assert.equal(config.servers[server].defaultEnabled, undefined);
    assert.equal(labs[server].enabled, true);
  }

  assert.deepEqual(
    config.servers.git.toolPolicies.map((entry) => entry.riskClass),
    ['git-mutation'],
  );
  assert.ok(config.servers.git.toolPolicies[0].tools.includes('git_commit'));
  assert.ok(config.servers.git.toolPolicies[0].tools.includes('git_checkout'));

  assert.deepEqual(
    config.servers.sqlite.toolPolicies.map((entry) => entry.riskClass),
    ['sqlite-mutation'],
  );
  assert.ok(config.servers.sqlite.toolPolicies[0].tools.includes('write_query'));
  assert.ok(config.servers.sqlite.toolPolicies[0].tools.includes('create_table'));

  assert.deepEqual(
    config.servers.playwright.toolPolicies.map((entry) => entry.riskClass),
    ['browser-control'],
  );
  assert.ok(config.servers.playwright.toolPolicies[0].tools.includes('browser_click'));
  assert.ok(config.servers.playwright.toolPolicies[0].tools.includes('browser_run_code'));
});
