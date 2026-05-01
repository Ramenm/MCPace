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

test('windows desktop MCP is not bundled or enabled by default', () => {
  const settings = readJson('mcp_settings.json').mcpServers;
  const config = readJson('mcpace.config.json');
  const profiles = config.profiles.runtime.profiles;
  const manager = readJson('manager.settings.json');

  assert.equal(settings['windows-mcp'], undefined);
  assert.equal(config.servers['windows-mcp'], undefined);
  assert.equal(profiles.desktop, undefined);
  assert.equal(manager.runtimeProfile.active, 'manual');
});

test('stateful reference MCP policies are user-supplied rather than bundled', () => {
  const config = readJson('mcpace.config.json');

  for (const name of ['filesystem', 'memory', 'sequential-thinking', 'lean-ctx', 'serena']) {
    assert.equal(config.servers[name], undefined, `${name} should not be bundled in the default distribution`);
  }
});

test('default distribution does not enable or bundle upstream MCP servers', () => {
  const config = readJson('mcpace.config.json');
  const settings = readJson('mcp_settings.json').mcpServers;
  const profiles = config.profiles.runtime.profiles;

  assert.equal(config.profiles.runtime.default, 'manual');
  assert.deepEqual(settings, {});
  assert.deepEqual(config.servers, {});
  assert.deepEqual(profiles.manual.serverOverrides, {});
  assert.deepEqual(profiles.labs.serverOverrides, {});
});
