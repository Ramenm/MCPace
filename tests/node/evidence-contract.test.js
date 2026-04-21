const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

function exists(relativePath) {
  return fs.existsSync(path.join(repoRoot, relativePath));
}

test('runtime capability evidence paths exist', () => {
  const capabilities = readJson(path.join('eval', 'runtime-capabilities.json'));
  const missing = [];

  for (const feature of capabilities.features) {
    assert.ok(Array.isArray(feature.evidence), `${feature.id} should expose evidence`);
    assert.ok(feature.evidence.length >= 1, `${feature.id} should include at least one evidence path`);
    for (const evidencePath of feature.evidence) {
      if (!exists(evidencePath)) {
        missing.push(`${feature.id}: ${evidencePath}`);
      }
    }
  }

  assert.deepEqual(missing, []);
});

test('seed prompt eval evidence paths exist and stay grounded', () => {
  const fixtureDir = path.join(repoRoot, 'eval', 'fixtures', 'seed');
  const missing = [];

  for (const name of fs.readdirSync(fixtureDir).filter((value) => value.endsWith('.json')).sort()) {
    const value = readJson(path.join('eval', 'fixtures', 'seed', name));
    assert.equal(value.track, 'seed-prompt');
    assert.equal(typeof value.grounding, 'object');
    assert.ok(Array.isArray(value.grounding.evidence), `${name} should include evidence`);
    assert.ok(value.grounding.evidence.length >= 1, `${name} should include at least one evidence path`);
    for (const evidencePath of value.grounding.evidence) {
      if (!exists(evidencePath)) {
        missing.push(`${name}: ${evidencePath}`);
      }
    }
  }

  assert.deepEqual(missing, []);
});
