const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

test('rust command coverage reflects the currently implemented launch and install surfaces', () => {
  const coverage = readJson('reports/rust-command-coverage.json');
  const commands = new Set(coverage.nativeRustCommands);

  for (const command of [
    'dashboard',
    'serve',
    'serve start',
    'serve status',
    'serve stop',
    'mcp-server',
    'client install'
  ]) {
    assert.ok(commands.has(command), `missing native command coverage for ${command}`);
  }

  assert.ok(!coverage.plannedCommandGroups.includes('client install'));
  assert.match(coverage.implementedReadOnlyNotes.client, /config-writing install/i);
  assert.doesNotMatch(coverage.implementedReadOnlyNotes.client, /install\/config-writing export not implemented yet/i);
});

test('project-control docs no longer underclaim client install as undone work', () => {
  const todo = read('TODO.md');
  const state = read('STATE.md');
  const tombstones = read('reports/tombstones.md');

  assert.doesNotMatch(todo, /Implement `client install` \/ `client export`/i);
  assert.doesNotMatch(state, /Only then finish `client install`/i);
  assert.doesNotMatch(tombstones, /client install\/export/i);
});
