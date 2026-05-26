import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const readJson = (...parts) => JSON.parse(read(...parts));

test('metadata sweep records popular and random MCP packages without shipping downloads', () => {
  const sweep = readJson('eval', 'package-metadata-sweep.json');
  assert.equal(sweep.sandboxArtifactsExcluded, true);
  assert.ok(sweep.npmRecordCount >= 37);
  assert.ok(sweep.pypiRecordCount >= 4);
  assert.ok(sweep.classifierBucketCounts['local-browser-session'] >= 4);
  assert.ok(sweep.classifierBucketCounts['local-browser-readonly'] >= 1);
  assert.ok(sweep.classifierBucketCounts['remote-browser-session'] >= 1);
  assert.ok(sweep.classifierBucketCounts['credential-external-mutating'] >= 5);
  assert.ok(sweep.classifierBucketCounts['stateless-external-read'] >= 4);
  assert.ok(sweep.evidenceLayerOrder.includes('safe initialize/tools-list probe for approved servers'));

  const names = new Set(sweep.npm.map((record) => record.name));
  for (const required of [
    '@playwright/mcp',
    'chrome-devtools-mcp',
    'mcpbrowser',
    'sessionmcp',
    '@pipeworx/mcp-caniuse',
    '@notionhq/notion-mcp-server',
    '@supabase/mcp-server-supabase',
    '@phantom/mcp-server',
    '@n8n/mcp-browser',
    '@mcp-browser-kit/server',
    '@kazuph/mcp-browser-tabs',
    '@linatang/playwright-browser-manager-mcp',
  ]) {
    assert.ok(names.has(required), `missing metadata sweep record for ${required}`);
  }

  const releaseText = [read('eval', 'package-metadata-sweep.json'), read('eval', 'popular-server-corpus.json')].join('\n');
  const forbiddenSandboxPattern = new RegExp([`packages\\.applied-${'caas'}`, `arti${'factory'}`, `mcp-lab-${'downloads'}`, `\\.tgz`, `\\.whl`].join('|'));
  assert.doesNotMatch(releaseText, forbiddenSandboxPattern);
});

test('classifier source separates browser control, browser data, docs read, and SaaS admin', () => {
  const loader = read('src', 'server', 'loader.rs');
  assert.match(loader, /browser_data_only/);
  assert.match(loader, /browser_observation_only/);
  assert.match(loader, /browser-observation/);
  assert.match(loader, /host-readonly/);
  assert.match(loader, /remote-browser-session/);
  assert.match(loader, /documentation-lookup/);
  assert.match(loader, /external-read-api/);
  assert.match(loader, /project-analysis/);
  assert.match(loader, /network-database/);
  assert.match(loader, /db-connection/);
  assert.match(loader, /phantom/);
  assert.match(loader, /apify/);
  assert.match(loader, /state_class: "remote-session-stateful"/);
  assert.match(loader, /effect_class: "read-only"/);
});
