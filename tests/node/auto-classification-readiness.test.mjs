import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const readJson = (...parts) => JSON.parse(read(...parts));

test('auto classification readiness records what is done and what is still missing', () => {
  const readiness = readJson('eval', 'auto-classification-readiness.json');
  assert.equal(readiness.schema, 'mcpace.autoClassificationReadiness.v2');
  assert.equal(readiness.status, 'conservative-auto-ready-live-probe-added');
  assert.ok(readiness.implementedEvidenceSources.some((source) => source.id === 'lab-corpus'));

  const missing = new Map(readiness.requiredForFullAutomation.map((item) => [item.id, item]));
  for (const id of ['live-safe-probe', 'evidence-score-runtime-field', 'tool-schema-effect-parser']) {
    assert.equal(missing.get(id)?.missing, false, `${id} should be implemented now`);
  }
  for (const id of ['permission-manifest-inference', 'rug-pull-and-drift-detection', 'multi-registry-corpus']) {
    assert.equal(missing.get(id)?.missing, true, `${id} should remain tracked as a remaining gap`);
  }

  assert.ok(readiness.completionDefinition.usableNow.length > 0);
  assert.equal(readiness.finalAutoPipeline, 'eval/final-auto-pipeline.json');
  assert.ok(readiness.completionDefinition.notFinishedUntil.some((item) => /permission manifest/.test(item)));
  assert.ok(readiness.completionDefinition.usableNow.some((item) => /safe conservative defaults/.test(item)));
});
