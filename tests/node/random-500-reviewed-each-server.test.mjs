import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const readJson = (...parts) => JSON.parse(read(...parts));

test('random 500 second-pass review inspects every server and records automatic action', () => {
  const review = readJson('eval', 'random-500-reviewed-each-server.json');
  assert.equal(review.schema, 'mcpace.random500ReviewedEachServer.v1');
  assert.equal(review.sampleCount, 500);
  assert.equal(review.reviewedEveryRecord, true);
  assert.equal(review.executedForeignCode, false);
  assert.equal(review.foreignPackageArtifactsExcludedFromRelease, true);
  assert.equal(review.records.length, 500);
  assert.ok(review.changedClassificationCount > 100, 'second pass should catch broad-signal mistakes');
  assert.ok(review.reviewedUnknownCount < review.previousUnknownCount, 'second pass should reduce metadata-only unknowns');

  for (const action of ['static-safe-policy', 'needs-safe-probe', 'plan-only', 'blocked-high-risk']) {
    assert.ok(review.automaticActions[action] > 0, `missing automatic action ${action}`);
  }

  for (const record of review.records) {
    assert.ok(record.name, 'record missing name');
    assert.ok(record.previousClassifier?.routingGroup, `${record.name} missing previous classifier`);
    assert.ok(record.reviewedClassifier?.routingGroup, `${record.name} missing reviewed classifier`);
    assert.ok(record.reviewedClassifier?.automaticAction, `${record.name} missing automatic action`);
  }
});

test('random 500 second-pass review fixes known broad signal errors', () => {
  const review = readJson('eval', 'random-500-reviewed-each-server.json');
  const byName = new Map(review.records.map((record) => [record.name, record]));
  const expected = new Map([
    ['@contentful/mcp-server', 'credential-provider'],
    ['@agent-infra/mcp-shared', 'sdk-or-example'],
    ['mcp-google-docs', 'external-read'],
    ['@mcp-ui/client', 'sdk-or-example'],
    ['deploysapp-mcp', 'credential-provider'],
  ]);

  for (const [name, routingGroup] of expected) {
    assert.ok(byName.has(name), `missing reviewed package ${name}`);
    assert.equal(byName.get(name).reviewedClassifier.routingGroup, routingGroup);
  }

  const contentful = byName.get('@contentful/mcp-server');
  assert.equal(contentful.previousClassifier.routingGroup, 'browser');
  assert.equal(contentful.reviewedClassifier.routingGroup, 'credential-provider');
  assert.equal(contentful.reviewedClassifier.automaticAction, 'static-safe-policy');
});

test('runtime evidence ledger documents simplified automatic actions', () => {
  const ledger = readJson('eval', 'runtime-evidence-sources.json');
  assert.equal(ledger.schema, 'mcpace.runtimeEvidenceSources.v3');
  for (const action of ['static-safe-policy', 'needs-safe-probe', 'plan-only', 'blocked-high-risk']) {
    assert.ok(ledger.fastPathPolicy[action], `missing fast path policy for ${action}`);
  }
  assert.equal(ledger.random500SecondPassFinding.source, 'eval/random-500-reviewed-each-server.json');
  assert.match(ledger.recommendedAutomaticPipeline.join('\n'), /safe initialize \+ tools\/list/);
});
