import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const readJson = (...parts) => JSON.parse(read(...parts));

test('random 500 npm MCP sweep records broad held-out coverage and evidence gaps', () => {
  const sweep = readJson('eval', 'random-500-npm-sweep.json');
  assert.equal(sweep.schema, 'mcpace.random500NpmSweep.v1');
  assert.equal(sweep.sampleCount, 500);
  assert.equal(sweep.metadataFetched, 500);
  assert.ok(sweep.packDryRunManifestsFetched >= 100);
  assert.ok(sweep.downloadedTarballs >= 40);
  assert.equal(sweep.executedForeignCode, false);
  assert.equal(sweep.fetchedMetadataAndTarballManifestsOnly, true);
  assert.equal(sweep.packageTarballsExcludedFromRelease, true);
  assert.equal(sweep.fetchFailedCount, 0);
  assert.equal(sweep.mismatchCount, 0);
  assert.equal(sweep.conservativeMissCount, 0);
  assert.ok(sweep.unknownCount > 0);
  assert.ok(sweep.needsMoreSourcesCount > 0);

  for (const group of [
    'credential-provider',
    'browser',
    'database-connection',
    'dangerous-process',
    'external-read',
    'memory-context',
    'project-analysis',
    'project-filesystem',
    'transport-gateway',
    'unknown-source',
  ]) {
    assert.ok(sweep.routingGroups[group] > 0, `missing routing group ${group}`);
  }

  const weak = sweep.records.filter((record) => record.needsMoreSources);
  assert.ok(weak.some((record) => record.recommendedAdditionalEvidence?.includes('safe tools/list probe')));
});

test('runtime evidence source ledger explains how new random servers are classified', () => {
  const ledger = readJson('eval', 'runtime-evidence-sources.json');
  assert.match(ledger.schema, /^mcpace\.runtimeEvidenceSources\.v[123]$/);
  const ids = new Set(ledger.layers.map((layer) => layer.id));
  for (const id of [
    'client-config',
    'local-approved-catalog',
    'mcp-registry-server-json',
    'package-registry-metadata',
    'package-artifact-manifest',
    'safe-initialize-probe',
    'safe-tools-list-probe',
    'resources-prompts-probe',
    'runtime-observations',
  ]) {
    assert.ok(ids.has(id), `missing evidence layer ${id}`);
  }
  assert.match(ledger.policyRule, /Never relax unknown\/random servers/);
});
