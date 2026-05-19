#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { repoRoot, readJson } from './lib/project-metadata.mjs';

const DEFAULT_JSON_OUT = path.join('reports', 'eval-fixture-check-latest.json');
const DEFAULT_MARKDOWN_OUT = path.join('reports', 'eval-fixture-check-latest.md');

function parseArgs(argv) {
  const parsed = {
    json: false,
    write: null,
    markdown: null,
    failOnWarnings: false,
    help: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; break;
      case '--markdown': parsed.markdown = argv[++index] || null; break;
      case '--fail-on-warnings': parsed.failOnWarnings = true; break;
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported eval-fixture-check argument: ${token}`);
    }
  }

  return parsed;
}

function helpText() {
  return `Usage: node scripts/eval-fixture-check.mjs [--json] [--write PATH] [--markdown PATH] [--fail-on-warnings]\n\nValidates MCPace eval governance files and offline fixtures without invoking a model provider.\n`;
}

function listJsonFiles(relativeDir) {
  const dir = path.join(repoRoot, relativeDir);
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith('.json'))
    .map((entry) => path.join(relativeDir, entry.name).split(path.sep).join('/'))
    .sort();
}

function existsRelative(relativePath) {
  return fs.existsSync(path.join(repoRoot, relativePath));
}

function isNonEmptyString(value) {
  return typeof value === 'string' && value.trim().length > 0;
}

function asArray(value) {
  return Array.isArray(value) ? value : [];
}

function pushIssue(collection, code, message, relativePath, severity = 'error') {
  collection.push({ severity, code, message, path: relativePath });
}

function collectGroundingPaths(seedFixture) {
  const grounding = seedFixture.grounding || {};
  const evidence = asArray(grounding.evidence);
  const legacy = asArray(seedFixture.groundingEvidencePaths);
  return [...new Set([...evidence, ...legacy])];
}

function validateSeedFixture(relativePath, fixture, context, issues, warnings) {
  const fileId = path.basename(relativePath, '.json');
  const pushError = (code, message) => pushIssue(issues, code, message, relativePath, 'error');
  const pushWarning = (code, message) => pushIssue(warnings, code, message, relativePath, 'warning');

  if (fixture.id !== fileId) pushError('seed-id-filename-mismatch', `fixture id must match filename (${fileId})`);
  if (fixture.track !== 'seed-prompt') pushError('seed-track-invalid', 'seed fixture track must be seed-prompt');
  if (!isNonEmptyString(fixture.bucket)) pushError('seed-bucket-missing', 'seed fixture bucket is required');
  if (!['typical', 'edge', 'adversarial', 'held-out'].includes(fixture.split)) pushError('seed-split-invalid', `invalid seed split: ${fixture.split}`);
  if (fixture.heldOut !== (fixture.split === 'held-out')) pushError('seed-heldout-mismatch', 'heldOut must equal split === held-out');
  if (!['historical-regression', 'repo-grounded-policy', 'domain-expert'].includes(fixture.sourceType)) {
    pushError('seed-source-type-invalid', `invalid sourceType: ${fixture.sourceType}`);
  }
  if (!isNonEmptyString(fixture.prompt)) pushError('seed-prompt-missing', 'prompt is required');
  if (!isNonEmptyString(fixture.failureMode)) pushError('seed-failure-mode-missing', 'failureMode is required');

  const expected = fixture.expected || {};
  for (const key of ['good', 'bad', 'unacceptable']) {
    if (!Array.isArray(expected[key]) || expected[key].length === 0) {
      pushError('seed-expected-section-missing', `expected.${key} must be a non-empty array`);
    }
  }

  const metrics = asArray(fixture.metrics);
  for (const required of ['task-success-rate', 'unsupported-claim-rate', 'uncertainty-rate']) {
    if (!metrics.includes(required)) pushError('seed-metric-missing', `missing metric ${required}`);
  }

  const scoring = fixture.scoring || {};
  const scoringMethods = asArray(scoring.methods);
  if (!scoringMethods.includes('binary-checks')) pushError('seed-binary-checks-missing', 'scoring.methods must include binary-checks');
  if (!scoringMethods.includes('rubric')) pushError('seed-rubric-method-missing', 'scoring.methods must include rubric');
  const binaryChecks = asArray(scoring.binaryChecks);
  if (binaryChecks.length < 2) pushError('seed-binary-checks-too-few', 'at least two binaryChecks are expected');
  for (const check of binaryChecks) {
    if (!isNonEmptyString(check.id)) pushError('seed-binary-check-id-missing', 'binary check id is required');
    if (!['must-include-all', 'must-exclude-all', 'must-include-any', 'must-exclude-any'].includes(check.type)) {
      pushError('seed-binary-check-type-invalid', `invalid binary check type: ${check.type}`);
    }
    if (!Array.isArray(check.items) || check.items.length === 0) pushError('seed-binary-check-items-missing', `binary check ${check.id || '<unknown>'} must have items`);
  }

  const evidencePaths = collectGroundingPaths(fixture);
  if (fixture.sourceType !== 'domain-expert' && evidencePaths.length === 0) {
    pushWarning('seed-grounding-evidence-missing', 'non-domain-expert seed fixture should cite grounding evidence paths');
  }
  for (const evidencePath of evidencePaths) {
    if (!isNonEmptyString(evidencePath)) pushError('seed-grounding-path-invalid', 'grounding evidence path must be a non-empty string');
    else if (!existsRelative(evidencePath)) pushError('seed-grounding-path-missing', `grounding evidence path does not exist: ${evidencePath}`);
  }

  const unacceptableText = JSON.stringify(expected.unacceptable || []);
  if (/exact delivery date|full runtime readiness|claim.*proven|trusted|safe/i.test(unacceptableText)) {
    context.guardrailCases += 1;
  }
}

function validateRuntimeFixture(relativePath, fixture, context, issues, warnings) {
  const fileId = path.basename(relativePath, '.json');
  const pushError = (code, message) => pushIssue(issues, code, message, relativePath, 'error');
  const pushWarning = (code, message) => pushIssue(warnings, code, message, relativePath, 'warning');

  if (fixture.id !== fileId) pushError('runtime-id-filename-mismatch', `fixture id must match filename (${fileId})`);
  if (!isNonEmptyString(fixture.suite)) pushError('runtime-suite-missing', 'runtime suite is required');
  if (!['typical', 'edge', 'adversarial', 'held-out'].includes(fixture.category)) pushError('runtime-category-invalid', `invalid runtime category: ${fixture.category}`);
  if (fixture.heldOut !== (fixture.category === 'held-out')) pushError('runtime-heldout-mismatch', 'heldOut must equal category === held-out');
  if (!isNonEmptyString(fixture.proofLayer)) pushError('runtime-proof-layer-missing', 'proofLayer is required');
  if (!isNonEmptyString(fixture.title)) pushError('runtime-title-missing', 'title is required');
  if (!isNonEmptyString(fixture.objective)) pushError('runtime-objective-missing', 'objective is required');
  if (!fixture.traffic || typeof fixture.traffic !== 'object') pushError('runtime-traffic-missing', 'traffic object is required');
  if (!Array.isArray(fixture.checks) || fixture.checks.length === 0) pushError('runtime-checks-missing', 'checks must be a non-empty array');
  if (!Array.isArray(fixture.requires) || fixture.requires.length === 0) pushWarning('runtime-requires-missing', 'requires should name capability gates');
  if (fixture.category === 'held-out') context.runtimeHeldOut += 1;
}

function validateScenarioMatrix(matrix, context, issues, warnings) {
  if (matrix.version !== context.version) pushIssue(issues, 'matrix-version-mismatch', `matrix version ${matrix.version} != package version ${context.version}`, 'eval/scenario-matrix.json');
  if (!Array.isArray(matrix.families) || matrix.families.length === 0) pushIssue(issues, 'matrix-families-missing', 'scenario matrix must contain families', 'eval/scenario-matrix.json');

  for (const family of asArray(matrix.families)) {
    if (!isNonEmptyString(family.id)) pushIssue(issues, 'matrix-family-id-missing', 'family id is required', 'eval/scenario-matrix.json');
    else context.familyIds.add(family.id);
    if (!['high', 'medium', 'low'].includes(family.prevalence)) pushIssue(issues, 'matrix-prevalence-invalid', `invalid prevalence for ${family.id}`, 'eval/scenario-matrix.json');
    if (!isNonEmptyString(family.whyItMatters)) pushIssue(issues, 'matrix-why-missing', `whyItMatters is required for ${family.id}`, 'eval/scenario-matrix.json');
    for (const key of ['seedTypical', 'seedEdge', 'seedAdversarial', 'seedHeldOut', 'runtimeFixtures']) {
      if (!Array.isArray(family[key])) pushIssue(issues, 'matrix-lane-invalid', `${family.id}.${key} must be an array`, 'eval/scenario-matrix.json');
    }
  }

  if (!asArray(matrix.families).some((family) => family.id === 'autonomous-agent-workloop')) {
    pushIssue(warnings, 'matrix-workloop-family-missing', 'autonomous-agent-workloop family is recommended for agent execution prompts', 'eval/scenario-matrix.json', 'warning');
  }
}

function validateRubric(rubric, context, issues) {
  if (rubric.version !== context.version) pushIssue(issues, 'rubric-version-mismatch', `rubric version ${rubric.version} != package version ${context.version}`, 'eval/scoring-rubric.json');
  const dimensionIds = new Set(asArray(rubric.dimensions).map((dimension) => dimension.id));
  for (const required of ['task-success', 'factual-support', 'honesty-and-uncertainty', 'scope-control', 'actionability']) {
    if (!dimensionIds.has(required)) pushIssue(issues, 'rubric-dimension-missing', `missing rubric dimension ${required}`, 'eval/scoring-rubric.json');
  }
  const metricIds = new Set(asArray(rubric.metrics).map((metric) => metric.id));
  for (const required of ['task-success-rate', 'unsupported-claim-rate', 'uncertainty-rate']) {
    if (!metricIds.has(required)) pushIssue(issues, 'rubric-metric-missing', `missing metric ${required}`, 'eval/scoring-rubric.json');
  }
  if (rubric.judgementPolicy?.preferAbstentionOverGuessing !== true) {
    pushIssue(issues, 'rubric-abstention-policy-missing', 'judgementPolicy must prefer abstention over guessing', 'eval/scoring-rubric.json');
  }
  if (rubric.judgementPolicy?.criticalUnsupportedClaimFailsCase !== true) {
    pushIssue(issues, 'rubric-unsupported-claim-policy-missing', 'critical unsupported claims must fail a case', 'eval/scoring-rubric.json');
  }
}

function validateDatasetPlan(plan, context, issues) {
  if (plan.version !== context.version) pushIssue(issues, 'dataset-version-mismatch', `dataset plan version ${plan.version} != package version ${context.version}`, 'eval/dataset-plan.json');
  const tracks = asArray(plan.tracks);
  const trackIds = new Set(tracks.map((track) => track.id));
  for (const required of ['seed-prompt', 'runtime-lab']) {
    if (!trackIds.has(required)) pushIssue(issues, 'dataset-track-missing', `missing dataset track ${required}`, 'eval/dataset-plan.json');
  }
  const regressionText = JSON.stringify(plan.regressionPolicy || {});
  for (const required of ['held-out', 'human', 'unsupported-claim']) {
    if (!regressionText.toLowerCase().includes(required)) pushIssue(issues, 'dataset-regression-policy-gap', `regressionPolicy should mention ${required}`, 'eval/dataset-plan.json');
  }
}

function referenceCoverage(matrix, seedIds, runtimeIds, issues) {
  const referencedSeedIds = new Set();
  const referencedRuntimeIds = new Set();
  for (const family of asArray(matrix.families)) {
    for (const key of ['seedTypical', 'seedEdge', 'seedAdversarial', 'seedHeldOut']) {
      for (const id of asArray(family[key])) {
        referencedSeedIds.add(id);
        if (!seedIds.has(id)) pushIssue(issues, 'matrix-seed-reference-missing', `scenario matrix references missing seed fixture ${id}`, 'eval/scenario-matrix.json');
      }
    }
    for (const id of asArray(family.runtimeFixtures)) {
      referencedRuntimeIds.add(id);
      if (!runtimeIds.has(id)) pushIssue(issues, 'matrix-runtime-reference-missing', `scenario matrix references missing runtime fixture ${id}`, 'eval/scenario-matrix.json');
    }
  }

  for (const id of seedIds) {
    if (!referencedSeedIds.has(id)) pushIssue(issues, 'seed-fixture-unreferenced', `seed fixture ${id} is not referenced by scenario matrix`, `eval/fixtures/seed/${id}.json`, 'warning');
  }
  for (const id of runtimeIds) {
    if (!referencedRuntimeIds.has(id)) pushIssue(issues, 'runtime-fixture-unreferenced', `runtime fixture ${id} is not referenced by scenario matrix`, `eval/fixtures/runtime/${id}.json`, 'warning');
  }

  return { referencedSeedIds, referencedRuntimeIds };
}

function splitCounts(seedFixtures, runtimeFixtures) {
  const seed = {};
  const runtime = {};
  for (const fixture of seedFixtures) seed[fixture.split] = (seed[fixture.split] || 0) + 1;
  for (const fixture of runtimeFixtures) runtime[fixture.category] = (runtime[fixture.category] || 0) + 1;
  return { seed, runtime };
}

function makeMarkdown(report) {
  const lines = [];
  lines.push('# Eval fixture check');
  lines.push('');
  lines.push(`Status: **${report.status}**`);
  lines.push(`Version: ${report.version}`);
  lines.push(`Generated at: ${report.generatedAt}`);
  lines.push('');
  lines.push('## Coverage');
  lines.push('');
  lines.push(`- Seed fixtures: ${report.coverage.seedFixtureCount}`);
  lines.push(`- Runtime fixtures: ${report.coverage.runtimeFixtureCount}`);
  lines.push(`- Scenario families: ${report.coverage.familyCount}`);
  lines.push(`- Held-out seed fixtures: ${report.coverage.seedSplitCounts['held-out'] || 0}`);
  lines.push(`- Held-out runtime fixtures: ${report.coverage.runtimeSplitCounts['held-out'] || 0}`);
  lines.push(`- Guardrail seed cases: ${report.coverage.guardrailCases}`);
  lines.push('');
  lines.push('## Issues');
  lines.push('');
  if (report.issues.length === 0) lines.push('- None');
  for (const issue of report.issues) lines.push(`- **${issue.code}** (${issue.path}): ${issue.message}`);
  lines.push('');
  lines.push('## Warnings');
  lines.push('');
  if (report.warnings.length === 0) lines.push('- None');
  for (const warning of report.warnings) lines.push(`- **${warning.code}** (${warning.path}): ${warning.message}`);
  lines.push('');
  lines.push('## Interpretation');
  lines.push('');
  lines.push('This is an offline structural and governance check. It does not call an LLM provider, does not score model output, and does not prove production traffic quality. Use it before provider-backed evals so schema, splits, grounding paths, and guardrails are not silently broken.');
  lines.push('');
  return `${lines.join('\n')}\n`;
}

export function runEvalFixtureCheck() {
  const pkg = readJson('package.json');
  const context = {
    version: pkg.version,
    familyIds: new Set(),
    guardrailCases: 0,
    runtimeHeldOut: 0,
  };
  const issues = [];
  const warnings = [];

  const matrix = readJson(path.join('eval', 'scenario-matrix.json'));
  const rubric = readJson(path.join('eval', 'scoring-rubric.json'));
  const datasetPlan = readJson(path.join('eval', 'dataset-plan.json'));

  validateScenarioMatrix(matrix, context, issues, warnings);
  validateRubric(rubric, context, issues);
  validateDatasetPlan(datasetPlan, context, issues);

  const seedFiles = listJsonFiles(path.join('eval', 'fixtures', 'seed'));
  const runtimeFiles = listJsonFiles(path.join('eval', 'fixtures', 'runtime'));
  const seedFixtures = [];
  const runtimeFixtures = [];

  for (const relativePath of seedFiles) {
    const fixture = readJson(relativePath);
    seedFixtures.push(fixture);
    validateSeedFixture(relativePath, fixture, context, issues, warnings);
  }
  for (const relativePath of runtimeFiles) {
    const fixture = readJson(relativePath);
    runtimeFixtures.push(fixture);
    validateRuntimeFixture(relativePath, fixture, context, issues, warnings);
  }

  const seedIds = new Set(seedFixtures.map((fixture) => fixture.id));
  const runtimeIds = new Set(runtimeFixtures.map((fixture) => fixture.id));
  referenceCoverage(matrix, seedIds, runtimeIds, warnings);

  const counts = splitCounts(seedFixtures, runtimeFixtures);
  for (const split of ['typical', 'edge', 'adversarial', 'held-out']) {
    if (!counts.seed[split]) pushIssue(issues, 'seed-split-empty', `seed split has no fixtures: ${split}`, 'eval/fixtures/seed');
    if (!counts.runtime[split]) pushIssue(issues, 'runtime-split-empty', `runtime split has no fixtures: ${split}`, 'eval/fixtures/runtime');
  }
  if (context.guardrailCases < 6) pushIssue(warnings, 'guardrail-coverage-low', 'fewer than six seed guardrail cases mention readiness/safety/unsupported-claim traps', 'eval/fixtures/seed', 'warning');

  const status = issues.length === 0 ? 'pass' : 'fail';
  return {
    schema: 'mcpace-eval-fixture-check.v1',
    generatedAt: new Date().toISOString(),
    version: context.version,
    status,
    coverage: {
      familyCount: asArray(matrix.families).length,
      seedFixtureCount: seedFixtures.length,
      runtimeFixtureCount: runtimeFixtures.length,
      seedSplitCounts: counts.seed,
      runtimeSplitCounts: counts.runtime,
      guardrailCases: context.guardrailCases,
    },
    issues,
    warnings,
  };
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    process.stdout.write(helpText());
    return;
  }

  const report = runEvalFixtureCheck();
  const jsonOut = args.write === undefined ? null : (args.write || DEFAULT_JSON_OUT);
  const markdownOut = args.markdown === undefined ? null : (args.markdown || DEFAULT_MARKDOWN_OUT);
  if (jsonOut) {
    fs.mkdirSync(path.dirname(path.join(repoRoot, jsonOut)), { recursive: true });
    fs.writeFileSync(path.join(repoRoot, jsonOut), `${JSON.stringify(report, null, 2)}\n`);
  }
  if (markdownOut) {
    fs.mkdirSync(path.dirname(path.join(repoRoot, markdownOut)), { recursive: true });
    fs.writeFileSync(path.join(repoRoot, markdownOut), makeMarkdown(report));
  }

  if (args.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  else process.stdout.write(makeMarkdown(report));

  if (report.status !== 'pass' || (args.failOnWarnings && report.warnings.length > 0)) {
    process.exitCode = 1;
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  try { main(); }
  catch (error) {
    if (process.argv.includes('--json')) {
      process.stdout.write(`${JSON.stringify({ status: 'fail', error: String(error?.message || error) }, null, 2)}\n`);
    } else {
      process.stderr.write(`${String(error?.stack || error)}\n`);
    }
    process.exitCode = 1;
  }
}
