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

test('eval governance files expose scenario map, rubric, metrics, and regression loop', () => {
  const rootVersion = readJson('package.json').version;
  const matrix = readJson(path.join('eval', 'scenario-matrix.json'));
  const rubric = readJson(path.join('eval', 'scoring-rubric.json'));
  const datasetPlan = readJson(path.join('eval', 'dataset-plan.json'));

  assert.equal(matrix.version, rootVersion);
  assert.equal(rubric.version, rootVersion);
  assert.equal(datasetPlan.version, rootVersion);

  assert.ok(Array.isArray(matrix.families));
  assert.ok(matrix.families.length >= 6);
  for (const family of matrix.families) {
    assert.match(family.id, /^[a-z0-9][a-z0-9-]*$/);
    assert.ok(['high', 'medium', 'low'].includes(family.prevalence));
    assert.equal(typeof family.whyItMatters, 'string');
    for (const key of ['seedTypical', 'seedEdge', 'seedAdversarial', 'seedHeldOut', 'runtimeFixtures']) {
      assert.ok(Array.isArray(family[key]), `${family.id}.${key} should be an array`);
    }
  }

  const rubricDimensions = new Set(rubric.dimensions.map((value) => value.id));
  for (const required of ['task-success', 'factual-support', 'honesty-and-uncertainty']) {
    assert.ok(rubricDimensions.has(required), required);
  }
  const rubricMetrics = new Set(rubric.metrics.map((value) => value.id));
  for (const required of ['task-success-rate', 'unsupported-claim-rate', 'uncertainty-rate']) {
    assert.ok(rubricMetrics.has(required), required);
  }
  assert.equal(rubric.judgementPolicy.preferAbstentionOverGuessing, true);
  assert.equal(rubric.judgementPolicy.criticalUnsupportedClaimFailsCase, true);
  assert.equal(rubric.judgementPolicy.humanCalibration.required, true);

  const trackIds = new Set(datasetPlan.tracks.map((value) => value.id));
  assert.ok(trackIds.has('seed-prompt'));
  assert.ok(trackIds.has('runtime-lab'));
  const methods = datasetPlan.tracks.flatMap((track) => track.evaluationMethods);
  for (const required of ['binary-checks', 'rubric-scoring', 'pairwise-on-close-calls', 'human-calibration', 'schema-validation']) {
    assert.ok(methods.includes(required), required);
  }
  const regressionText = JSON.stringify(datasetPlan.regressionPolicy);
  assert.match(regressionText, /held-out/i);
  assert.match(regressionText, /human/i);
});

test('scenario matrix references real fixture ids and keeps held-out lanes separate', () => {
  const matrix = readJson(path.join('eval', 'scenario-matrix.json'));
  const seedIds = new Set(listJson(path.join('eval', 'fixtures', 'seed')).map((name) => name.replace(/\.json$/, '')));
  const runtimeIds = new Set(listJson(path.join('eval', 'fixtures', 'runtime')).map((name) => name.replace(/\.json$/, '')));
  let heldOutReferences = 0;

  for (const family of matrix.families) {
    for (const id of family.seedTypical) assert.ok(seedIds.has(id), id);
    for (const id of family.seedEdge) assert.ok(seedIds.has(id), id);
    for (const id of family.seedAdversarial) assert.ok(seedIds.has(id), id);
    for (const id of family.seedHeldOut) {
      assert.ok(seedIds.has(id), id);
      heldOutReferences += 1;
    }
    for (const id of family.runtimeFixtures) assert.ok(runtimeIds.has(id), id);
  }

  assert.ok(heldOutReferences >= 2);
});

test('seed prompt fixtures stay normalized and split across typical, edge, adversarial, and held-out', () => {
  const files = listJson(path.join('eval', 'fixtures', 'seed'));
  assert.ok(files.length >= 20);

  const splitCounts = new Map();
  const sourceTypes = new Set();
  for (const file of files) {
    const value = readJson(path.join('eval', 'fixtures', 'seed', file));
    assert.equal(value.track, 'seed-prompt');
    assert.equal(typeof value.bucket, 'string');
    assert.ok(['typical', 'edge', 'adversarial', 'held-out'].includes(value.split), `${file}: ${value.split}`);
    assert.equal(value.heldOut, value.split === 'held-out');
    assert.ok(['historical-regression', 'repo-grounded-policy', 'domain-expert'].includes(value.sourceType));
    assert.equal(typeof value.prompt, 'string');
    assert.equal(typeof value.failureMode, 'string');
    assert.ok(Array.isArray(value.expected.good));
    assert.ok(Array.isArray(value.expected.bad));
    assert.ok(Array.isArray(value.expected.unacceptable));
    assert.ok(value.expected.good.length >= 1, `${file} should require some positive behavior`);
    assert.ok(value.expected.unacceptable.length >= 1, `${file} should forbid at least one unacceptable behavior`);
    assert.ok(Array.isArray(value.metrics));
    assert.ok(value.metrics.includes('task-success-rate'));
    assert.ok(value.metrics.includes('unsupported-claim-rate'));
    assert.ok(value.metrics.includes('uncertainty-rate'));
    assert.ok(Array.isArray(value.scoring.methods));
    assert.ok(value.scoring.methods.includes('binary-checks'));
    assert.ok(value.scoring.methods.includes('rubric'));
    assert.ok(Array.isArray(value.scoring.binaryChecks));
    assert.ok(value.scoring.binaryChecks.length >= 2);
    splitCounts.set(value.split, (splitCounts.get(value.split) || 0) + 1);
    sourceTypes.add(value.sourceType);
  }

  for (const split of ['typical', 'edge', 'adversarial', 'held-out']) {
    assert.ok(splitCounts.get(split) >= 1, split);
  }
  assert.ok(sourceTypes.has('historical-regression'));
  assert.ok(sourceTypes.has('repo-grounded-policy'));
});
