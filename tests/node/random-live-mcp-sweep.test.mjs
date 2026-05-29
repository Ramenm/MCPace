import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const readJson = (...parts) => JSON.parse(read(...parts));

test('random live npm MCP sweep proves metadata hints matter and weak packages remain conservative', () => {
  const sweep = readJson('eval', 'random-live-npm-sweep.json');
  assert.equal(sweep.schema, 'mcpace.randomLiveNpmSweep.v1');
  assert.equal(sweep.sampleCount, 20);
  assert.equal(sweep.executedForeignCode, false);
  assert.equal(sweep.fetchedMetadataOnly, true);
  assert.match(sweep.selectedFrom, /npm search --json "mcp" --searchlimit=250/);
  assert.ok(sweep.commandOnlyUnknown > sweep.withProfileHintsUnknown);
  assert.ok(sweep.needsProfileHints >= 5);
  assert.ok(sweep.withProfileHintsUnknown <= 2);

  const byName = new Map(sweep.records.map((record) => [record.name, record]));
  for (const required of [
    '@benborla29/mcp-server-mysql',
    'mcp-atlassian',
    '@taazkareem/clickup-mcp-server',
    '@midscene/android-mcp',
    '@piotr-agier/google-drive-mcp',
    'supergateway',
    'kai-mcp',
    '@z_ai/mcp-server',
    '@extentos/mcp-server',
  ]) {
    assert.ok(byName.has(required), `missing random live package ${required}`);
  }

  assert.equal(byName.get('@benborla29/mcp-server-mysql').withProfileHints.routingGroup, 'database-connection');
  assert.equal(byName.get('@midscene/android-mcp').withProfileHints.runtimeType, 'interactive');
  assert.equal(byName.get('@piotr-agier/google-drive-mcp').withProfileHints.routingGroup, 'credential-provider');
  assert.equal(byName.get('supergateway').withProfileHints.routingGroup, 'transport-gateway');
  assert.equal(byName.get('kai-mcp').withProfileHints.routingGroup, 'project-analysis');
  assert.equal(byName.get('@z_ai/mcp-server').withProfileHints.stateClass, 'unknown-conservative');
  assert.equal(byName.get('@extentos/mcp-server').withProfileHints.stateClass, 'unknown-conservative');
});

test('auto install writes profile hints and loader consumes them for future random servers', () => {
  const discover = read('src', 'server', 'discover.rs');
  const autoinstall = read('src', 'mcp_autoinstall.rs');
  const write = read('src', 'mcp_sources', 'write_helpers.rs');
  const loader = read('src', 'server', 'loader.rs');
  const docs = read('docs', 'lab-harness.md');

  assert.match(discover, /profile_hints_from_candidate/);
  assert.match(discover, /"profileHints"/);
  assert.match(autoinstall, /profile_hints\.extend\(profile_hints_for_plan\(&plan\)\)/);
  assert.match(write, /"mcpaceProfileHints"/);
  assert.match(loader, /mcpaceProfileHints/);
  assert.match(loader, /source_signal_args/);
  assert.match(loader, /remote_file_api/);
  assert.match(loader, /transport-gateway/);
  assert.doesNotMatch(loader, /"desktop",\n\s*"screenshot"/);
  assert.match(docs, /Random held-out npm sweep/);
  assert.match(docs, /dependency names are not treated as trusted semantic evidence/);
});
