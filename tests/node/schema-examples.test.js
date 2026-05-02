const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

function validateSubset(schema, value) {
  assert.equal(typeof value, 'object');
  assert.equal(typeof value.runtime, 'object');
  assert.equal(typeof value.clients, 'object');
  assert.ok(Array.isArray(value.clients.supported));
  assert.ok(Array.isArray(value.servers));
  assert.equal(schema.type, 'object');
  assert.ok(schema.required.includes('runtime'));
  assert.ok(schema.required.includes('clients'));
  assert.ok(schema.required.includes('servers'));
  assert.ok(schema.properties.runtime.properties.ingress);
}

test('hub schema and examples parse and satisfy the repo subset contract', () => {
  const schema = readJson('schemas/mcpace-hub.schema.json');
  const minimal = readJson('examples/mcpace-hub.minimal.json');
  const workstation = readJson('examples/mcpace-hub.workstation.json');
  validateSubset(schema, minimal);
  validateSubset(schema, workstation);
});


test('project config schema covers current serve and MCP settings registry fields', () => {
  const schema = readJson('schemas/mcpace-config.schema.json');
  const config = readJson('mcpace.config.json');
  assert.equal(schema.type, 'object');
  for (const field of ['name', 'version', 'ports', 'serve', 'mcpSettings', 'servers']) {
    assert.ok(schema.required.includes(field), `schema should require ${field}`);
    assert.ok(Object.hasOwn(config, field), `config should include ${field}`);
  }
  assert.ok(schema.properties.serve.required.includes('mcpPath'));
  assert.ok(schema.properties.serve.required.includes('publicUrl'));
  assert.ok(schema.properties.mcpSettings.required.includes('includePaths'));
  assert.ok(schema.properties.mcpSettings.required.includes('includeDirs'));
  assert.equal(config.serve.mcpPath, '/mcp');
  assert.deepEqual(config.mcpSettings.includeDirs, ['mcp_settings.d']);
});
