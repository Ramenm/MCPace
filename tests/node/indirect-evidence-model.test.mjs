import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const readJson = (...parts) => JSON.parse(read(...parts));

test('indirect evidence model makes package names weak and non-policy-bearing', () => {
  const model = readJson('eval', 'indirect-evidence-model.json');
  assert.equal(model.schema, 'mcpace.indirectEvidenceModel.v1');
  assert.equal(model.nameEvidencePolicy.packageName, 'weak-index-only');
  assert.equal(model.nameEvidencePolicy.installSpec, 'launcher-only-not-policy');
  assert.equal(model.nameEvidencePolicy.neverRelaxFromNameOnly, true);
  assert.match(model.principle, /identity, not semantic truth/);

  const ranked = model.indirectEvidenceRanked.map((entry) => entry.id);
  for (const required of [
    'observed-initialize',
    'observed-tools-list',
    'entrypoint-and-package-shape',
    'transport-and-auth-shape',
    'dependency-families',
    'resource-prompt-surface',
    'runtime-observations',
  ]) {
    assert.ok(ranked.includes(required), `missing indirect evidence channel ${required}`);
  }

  assert.ok(model.decisionRules.some((rule) => /name-only/.test(rule)));
  assert.ok(model.minimalAutomaticPipeline.some((step) => /initialize \+ tools\/list/.test(step)));
});

test('dynamic discovery does not persist package identity as semantic profile hints', () => {
  const discover = read('src', 'server', 'discover.rs');
  const start = discover.indexOf('fn profile_hints_from_candidate');
  const end = discover.indexOf('\nfn candidate_json', start);
  assert.notEqual(start, -1);
  assert.notEqual(end, -1);
  const body = discover.slice(start, end);

  assert.match(body, /Keep identity fields out of semantic profile hints/);
  assert.doesNotMatch(body, /candidate\.name\.as_str\(\)/);
  assert.doesNotMatch(body, /candidate\.install_spec\.as_str\(\)/);
  assert.doesNotMatch(body, /candidate\.package\.as_str\(\)/);
  assert.match(body, /candidate\.description\.as_str\(\)/);
  assert.match(body, /registry-type:/);
  assert.match(body, /transport:/);
});

test('runtime evidence ledger points to the indirect evidence model', () => {
  const ledger = readJson('eval', 'runtime-evidence-sources.json');
  assert.equal(ledger.schema, 'mcpace.runtimeEvidenceSources.v3');
  assert.equal(ledger.indirectEvidenceModel, 'eval/indirect-evidence-model.json');
  assert.equal(ledger.nameEvidencePolicy.neverRelaxFromNameOnly, true);
  assert.match(ledger.policyRule, /package\/server name alone/);

  const docs = read('docs', 'lab-harness.md');
  assert.match(docs, /Name-free \/ indirect evidence policy/);
  assert.match(docs, /package names or server display names as trusted semantic evidence/);
  assert.match(docs, /name-only server must become `needs-safe-probe`/);
});


test('loader excludes identity names and raw package specs from semantic runtime signals', () => {
  const loader = read('src', 'server', 'loader.rs');

  const sourceArgsStart = loader.indexOf('fn source_signal_args(');
  const sourceArgsEnd = loader.indexOf('\nfn infer_generic_source_policy(', sourceArgsStart);
  assert.notEqual(sourceArgsStart, -1);
  assert.notEqual(sourceArgsEnd, -1);
  const sourceArgsBody = loader.slice(sourceArgsStart, sourceArgsEnd);
  assert.match(sourceArgsBody, /raw_arg_is_semantic_signal/);
  assert.doesNotMatch(sourceArgsBody, /let\s+mut\s+signal_args\s*=\s*args\.to_vec\(\)/);
  assert.match(sourceArgsBody, /profile_hints/);

  const sourceSignalsStart = loader.indexOf('fn source_signals(');
  const sourceSignalsEnd = loader.indexOf('\nfn command_semantic_signal(', sourceSignalsStart);
  assert.notEqual(sourceSignalsStart, -1);
  assert.notEqual(sourceSignalsEnd, -1);
  const sourceSignalsBody = loader.slice(sourceSignalsStart, sourceSignalsEnd);
  assert.match(sourceSignalsBody, /let\s+_identity\s*=\s*\(normalized_name,\s*display_name\)/);
  assert.match(sourceSignalsBody, /command_semantic_signal\(command\)/);
  assert.doesNotMatch(sourceSignalsBody, /format!\(\s*"\{\} \{\} \{\} \{\} \{\}"/);

  const commandSignalStart = loader.indexOf('fn command_semantic_signal(');
  assert.notEqual(commandSignalStart, -1);
  const commandSignalBody = loader.slice(commandSignalStart, loader.indexOf('\nfn signal_tokens(', commandSignalStart));
  assert.match(commandSignalBody, /"bash"/);
  assert.match(commandSignalBody, /"ssh"/);
  assert.doesNotMatch(commandSignalBody, /"npx"/);
  assert.doesNotMatch(commandSignalBody, /"uvx"/);
});
