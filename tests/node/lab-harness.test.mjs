import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const parseJson = (source, label) => {
  try {
    return JSON.parse(source);
  } catch (error) {
    throw new Error(`invalid JSON in ${label}: ${error.message}`, { cause: error });
  }
};
const readJson = (...parts) => parseJson(read(...parts), parts.join('/'));
const readJsonFile = (file) => parseJson(
  fs.readFileSync(file, 'utf8'),
  path.relative(repoRoot, file),
);

function fixtureFiles() {
  const dir = path.join(repoRoot, 'eval', 'fixtures', 'runtime');
  return fs.readdirSync(dir)
    .filter((name) => name.endsWith('.json'))
    .sort()
    .map((name) => path.join(dir, name));
}

test('lab command is one-step and defaults to an evidence report', () => {
  const labRun = read('src', 'lab.rs');
  const labArgs = read('src', 'lab', 'args.rs');
  const labRender = read('src', 'lab', 'render.rs');
  const app = read('src', 'app.rs');

  assert.match(labRun, /unwrap_or\("report"\)/);
  assert.match(labArgs, /Usage: mcpace advanced dev lab \[report\|list\|matrix\|coverage\|gaps\|show\|probe\]/);
  assert.match(labArgs, /Default action: report/);
  assert.match(labRender, /server -> evidence -> runtimeType\/stateClass\/effectClass -> concurrencyPolicy/);
  assert.match(labRender, /Live safe probe/);
  assert.match(labRender, /Evidence matrix sample/);
  assert.match(app, /"lab" => lab::run/);
});

test('lab golden corpus covers popular and random MCP server classes', () => {
  const corpus = readJson('eval', 'popular-server-corpus.json');
  const randomAudit = readJson('eval', 'random-server-audit.json');
  const files = fixtureFiles();
  const scenarios = files.map(readJsonFile);

  assert.ok(files.length >= 46);
  assert.ok(corpus.npmPack.length >= 10);
  assert.ok(corpus.pipDownload.length >= 4);
  assert.ok(corpus.registrySamples.length >= 3);
  assert.ok(corpus.expandedMetadataSweep.npmRecordCount >= 37);
  assert.ok(corpus.expandedMetadataSweep.pypiRecordCount >= 4);
  assert.ok(corpus.randomHeldOutAudit.recordCount >= 8);
  assert.equal(randomAudit.sampleCount, corpus.randomHeldOutAudit.recordCount);

  const ids = new Set(scenarios.map((scenario) => scenario.id));
  for (const required of [
    'popular-npm-filesystem',
    'popular-npm-memory',
    'popular-pypi-git',
    'popular-pypi-fetch',
    'popular-npm-puppeteer',
    'popular-npm-playwright',
    'popular-npm-chrome-devtools',
    'random-npm-mcpbrowser',
    'random-npm-caniuse',
    'random-npm-n8n-browser',
    'random-npm-browser-kit',
    'random-npm-browser-tabs-readonly',
    'random-npm-playwright-browser-manager',
    'random-npm-firecrawl-ambiguous',
    'random-npm-mapbox-read',
    'random-npm-kubernetes-admin',
    'random-npm-eslint-project-readonly',
    'popular-npm-github',
    'registry-random-npm-stdio',
    'registry-mcpb-nuget',
    'popular-npm-context7',
    'popular-npm-kubernetes',
    'popular-npm-phantom-wallet',
    'popular-npm-eslint',
  ]) {
    assert.ok(ids.has(required), `missing runtime scenario ${required}`);
  }

  const runtimeTypes = new Set(scenarios.map((scenario) => scenario.expected.runtimeType));
  const stateClasses = new Set(scenarios.map((scenario) => scenario.expected.stateClass));
  const effectClasses = new Set(scenarios.map((scenario) => scenario.expected.effectClass));
  const autoActions = new Set(scenarios.map((scenario) => scenario.expected.autoAction));

  for (const runtimeType of ['stateless', 'stateful', 'external', 'interactive', 'unknown']) {
    assert.ok(runtimeTypes.has(runtimeType), `missing runtimeType ${runtimeType}`);
  }
  for (const stateClass of ['stateless', 'session-stateful', 'project-stateful', 'credential-stateful', 'remote-session-stateful', 'host-stateful', 'unknown-conservative']) {
    assert.ok(stateClasses.has(stateClass), `missing stateClass ${stateClass}`);
  }
  for (const effectClass of ['read-only', 'external-read', 'ephemeral-state', 'project-mutating', 'external-mutating', 'host-mutating', 'unknown']) {
    assert.ok(effectClasses.has(effectClass), `missing effectClass ${effectClass}`);
  }
  assert.ok(autoActions.has('approved-auto-install-then-probe'));
  assert.ok(autoActions.has('plan-only'));
  assert.ok(autoActions.has('plan-only-unless-approved-probe'));
  assert.ok(stateClasses.has('remote-session-stateful'));
  assert.ok(effectClasses.has('read-only'));
  assert.ok(effectClasses.has('external-mutating'));
  assert.ok([...autoActions].some((value) => value.startsWith('plan-only')));
});

test('lab fixtures are evidence-rich and do not ship sandbox download artifacts', () => {
  const scenarios = fixtureFiles().map(readJsonFile);
  for (const scenario of scenarios) {
    assert.ok(scenario.traffic.serverArchetype, `${scenario.id} missing server archetype`);
    assert.ok(scenario.expected.runtimeType, `${scenario.id} missing runtime type`);
    assert.ok(scenario.expected.stateClass, `${scenario.id} missing state class`);
    assert.ok(scenario.expected.effectClass, `${scenario.id} missing effect class`);
    assert.ok(scenario.expected.concurrencyPolicy, `${scenario.id} missing concurrency policy`);
    assert.ok(Array.isArray(scenario.evidenceSources) && scenario.evidenceSources.length > 0, `${scenario.id} missing evidence sources`);
    assert.ok(Array.isArray(scenario.metadataLayers) && scenario.metadataLayers.length >= 3, `${scenario.id} missing metadata layers`);
    assert.ok(Array.isArray(scenario.decisionTrace) && scenario.decisionTrace.length >= 3, `${scenario.id} missing decision trace`);
    assert.ok(scenario.expected.confidence, `${scenario.id} missing evidence confidence`);
    assert.ok(scenario.expected.trustBoundary, `${scenario.id} missing trust boundary`);
    assert.ok(scenario.expected.safeProbeMode, `${scenario.id} missing safe probe mode`);
    assert.ok(Array.isArray(scenario.checks) && scenario.checks.length > 0, `${scenario.id} missing checks`);
  }

  const manifest = readJson('release-manifest.json');
  assert.ok(manifest.includePaths.includes('eval'));

  const allReleaseText = [
    read('eval', 'popular-server-corpus.json'),
    read('eval', 'package-metadata-sweep.json'),
    read('reports', 'summary.md'),
    read('docs', 'lab-harness.md'),
  ].join('\n');
  const forbiddenSandboxPattern = new RegExp([
    ['packages', '\\.', 'applied-caas'].join(''),
    ['OPEN', 'AI_', '.*', 'MIR', 'ROR'].join(''),
    ['HTTP', '_PROXY'].join(''),
    'mcp-lab-downloads',
  ].join('|'));
  assert.doesNotMatch(allReleaseText, forbiddenSandboxPattern);
  assert.equal(fs.existsSync(path.join(repoRoot, 'eval', 'npm')), false);
  assert.equal(fs.existsSync(path.join(repoRoot, 'eval', 'pypi')), false);
});

test('lab docs explain the safe random-server boundary', () => {
  const doc = read('docs', 'lab-harness.md');
  const summary = read('reports', 'summary.md');

  assert.match(doc, /Unknown servers stay `plan-only`/);
  assert.match(doc, /must not execute random server code/);
  assert.match(doc, /server -> evidence -> runtimeType\/stateClass\/effectClass -> concurrencyPolicy/);
  assert.match(summary, /npm pack/);
  assert.match(summary, /pip download --no-deps/);
  assert.match(summary, /not executing foreign MCP server code/);
  assert.match(doc, /metadata layers/);
  assert.match(doc, /browser data/);
  assert.match(doc, /browser observation/);
  assert.match(summary, /random held-out audit/);
});
