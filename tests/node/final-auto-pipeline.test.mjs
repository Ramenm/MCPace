import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const readJson = (...parts) => JSON.parse(read(...parts));

test('final auto pipeline records conservative readiness and remaining target-machine checks', () => {
  const pipeline = readJson('eval', 'final-auto-pipeline.json');
  assert.equal(pipeline.schema, 'mcpace.finalAutoPipeline.v1');
  assert.equal(pipeline.status, 'production-grade-conservative-auto-ready');
  assert.equal(pipeline.random500Evidence.sampleCount, 500);
  assert.equal(pipeline.random500Evidence.executedForeignCode, false);
  assert.equal(pipeline.random500Evidence.foreignPackageArtifactsExcludedFromRelease, true);
  assert.ok(pipeline.readinessScore.safeConservativeRuntime >= 80);
  assert.ok(pipeline.readinessScore.fullAutomaticPolicyWidening < pipeline.readinessScore.productionConfidenceAfterPassingTargetProbe);
  assert.ok(pipeline.completionState.done.some((item) => /mcpace lab probe/.test(item)));
  assert.ok(pipeline.completionState.requiresTargetMachineEvidence.some((item) => /Rust cargo/.test(item)));
});

test('lab probe is a one-step safe live probe and does not call tools', () => {
  const lab = read('src', 'lab.rs');
  const args = read('src', 'lab', 'args.rs');
  const render = read('src', 'lab', 'render.rs');

  assert.match(args, /report\|list\|matrix\|coverage\|gaps\|show\|probe/);
  assert.match(args, /--timeout-ms/);
  assert.match(args, /--refresh/);
  assert.match(lab, /upstream::probe_servers/);
  assert.match(render, /initialize \+ notifications\/initialized \+ tools\/list only/);
  assert.match(render, /tools\/call: not executed/);
});

test('server profile evidence exposes score, level, automatic action and next step', () => {
  const loader = read('src', 'server', 'loader.rs');
  assert.match(loader, /struct EvidenceDecision/);
  assert.match(loader, /evidenceScore/);
  assert.match(loader, /evidenceLevel/);
  assert.match(loader, /automaticAction/);
  assert.match(loader, /needs-safe-probe/);
  assert.match(loader, /blocked-high-risk/);
  assert.match(loader, /static-safe-policy/);
});

test('tool policy audit uses schema-based indirect evidence, not only names', () => {
  const audit = read('src', 'upstream', 'policy_audit.rs');
  assert.match(audit, /add_schema_based_advisory_signals/);
  assert.match(audit, /inputSchema/);
  assert.match(audit, /outputSchema/);
  assert.match(audit, /schema-token:\{\}/);
  assert.match(audit, /"path"/);
  assert.match(audit, /"sql"/);
  assert.match(audit, /"token"/);
  assert.match(audit, /"command"/);
});
