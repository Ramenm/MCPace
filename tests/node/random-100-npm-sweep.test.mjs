import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const readJson = (...parts) => JSON.parse(read(...parts));

test('random 100 npm MCP sweep validates automatic classification coverage and safe unknowns', () => {
  const sweep = readJson('eval', 'random-100-npm-sweep.json');
  assert.equal(sweep.schema, 'mcpace.random100NpmSweep.v1');
  assert.equal(sweep.sampleCount, 100);
  assert.equal(sweep.downloadedTarballs, 100);
  assert.equal(sweep.executedForeignCode, false);
  assert.equal(sweep.fetchedMetadataAndTarballManifestsOnly, true);
  assert.equal(sweep.packageTarballsExcludedFromRelease, true);
  assert.ok(sweep.counts.match >= 95);
  assert.equal(sweep.mismatchCount, 0);
  assert.equal(sweep.conservativeMissCount, 0);
  assert.ok(sweep.unknownCount <= 5);

  const byName = new Map(sweep.records.map((record) => [record.name, record]));
  const expected = new Map([
    ['@contentful/mcp-server', 'credential-provider'],
    ['@browserstack/mcp-server', 'remote-browser'],
    ['@apify/actors-mcp-server', 'credential-provider'],
    ['@fangjunjie/ssh-mcp-server', 'dangerous-process'],
    ['@modelcontextprotocol/inspector-client', 'sdk-or-example'],
    ['@modelcontextprotocol/server-cohort-heatmap', 'sdk-or-example'],
    ['targetprocess-mcp-server', 'credential-provider'],
    ['@netlify/mcp', 'credential-provider'],
    ['shadcn-ui-mcp-server', 'external-read'],
  ]);
  for (const [name, routingGroup] of expected) {
    assert.ok(byName.has(name), `missing random package ${name}`);
    assert.equal(byName.get(name).actualByScript.routingGroup, routingGroup);
  }

  const unknownNames = sweep.records
    .filter((record) => record.actualByScript.stateClass === 'unknown-conservative')
    .map((record) => record.name)
    .sort();
  assert.deepEqual(unknownNames, [
    '@agentick/mcp',
    '@milaboratories/pl-mcp-server',
    '@yjzf/mcp-server-yjzf',
    'terry-mcp',
    '@vibeframe/mcp-server',
  ].sort());
});

test('random 100 audit guardrails are represented in classifier and schemas', () => {
  const loader = read('src', 'server', 'loader.rs');
  const configSchema = readJson('schemas', 'mcpace-config.schema.json');
  const profileSchema = readJson('schemas', 'mcpace-server-profile.schema.json');

  assert.match(loader, /sdk-or-example/);
  assert.match(loader, /not-runnable/);
  assert.match(loader, /plan-only/);
  assert.match(loader, /contentful/);
  assert.match(loader, /browserstack/);
  assert.match(loader, /targetprocess/);
  assert.doesNotMatch(loader, /"process",\n\s*"command-runner"/);

  const serverPolicy = configSchema.$defs.serverPolicy.properties;
  assert.ok(serverPolicy.concurrencyPolicy.enum.includes('plan-only'));
  assert.ok(serverPolicy.runtimeType.enum.includes('package-artifact'));
  assert.ok(serverPolicy.stateClass.enum.includes('not-a-server'));
  assert.ok(serverPolicy.effectClass.enum.includes('not-runnable'));
  assert.ok(profileSchema.properties.runtimeType.enum.includes('package-artifact'));
});
