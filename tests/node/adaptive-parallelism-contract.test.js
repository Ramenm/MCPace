const test = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..', '..');

function run(args) {
  return spawnSync(process.execPath, args, {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, NO_COLOR: '1' },
  });
}

test('adaptive parallelism audit passes and emits evidence-backed profiles', () => {
  const result = run(['scripts/adaptive-parallelism-audit.mjs', '--json', '--no-write']);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'pass');
  assert.ok(report.summary.profileCount >= 0);
  assert.ok(report.summary.edgeCaseCount >= 13);
  assert.equal(report.summary.staticCatalogPresent, false);
  assert.ok(report.summary.statefulCount >= 1);
  assert.ok(report.summary.statelessCount >= 1);
  assert.ok(report.checks.every((check) => check.ok), JSON.stringify(report.checks, null, 2));
  assert.ok(report.checks.some((check) => check.id === 'no-packaged-upstream-catalog'));
  const filesystem = report.edgeCases.find((edgeCase) => edgeCase.id === 'project-filesystem-write')?.actual;
  assert.ok(filesystem, 'filesystem edge case should be classified automatically');
  assert.equal(filesystem.parallelSafetyClass, 'P3_project_safe');
  assert.equal(filesystem.defaultPoolModel, 'project-pool');
  assert.equal(filesystem.maxInFlightPerWorker, 1);
  assert.ok(filesystem.lockDomains.includes('file'));
});

test('legacy transports are recognized as legacy and not folded into stable Streamable HTTP', () => {
  const loader = fs.readFileSync(path.join(root, 'src/server/loader.rs'), 'utf8');
  assert.match(loader, /sse-legacy/);
  assert.match(loader, /legacy-compat/);
  assert.doesNotMatch(loader, /remote-sse"\s*\|[\s\S]{0,80}=>\s*"http"\.to_string\(\)/);
});

test('client routing exposes adaptive pool identity and conservative probe-gated fallback', () => {
  const model = fs.readFileSync(path.join(root, 'src/client/model.rs'), 'utf8');
  const plan = fs.readFileSync(path.join(root, 'src/client/plan.rs'), 'utf8');
  const render = fs.readFileSync(path.join(root, 'src/client/render.rs'), 'utf8');
  assert.match(model, /parallel_safety_class/);
  assert.match(model, /worker_pool_key/);
  assert.match(plan, /bounded-worker-pool-pending-probe/);
  assert.match(plan, /maxInFlightPerWorker=1/);
  assert.match(render, /parallelSafetyClass/);
  assert.match(render, /workerPoolKey/);
});


test('adaptive edge-case matrix covers scheduler failure modes', () => {
  const result = run(['scripts/adaptive-parallelism-audit.mjs', '--json', '--no-write']);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  const byId = new Map(report.edgeCases.map((edgeCase) => [edgeCase.id, edgeCase]));
  for (const id of [
    'unknown-stdio-npx',
    'legacy-sse',
    'remote-streamable-http',
    'stateless-remote-http',
    'credential-scoped-api',
    'project-filesystem-write',
    'repo-git-write',
    'browser-automation',
    'shared-exclusive-desktop',
    'readonly-stdio-candidate',
    'stateful-memory',
    'local-database',
    'oci-unknown',
  ]) {
    assert.ok(byId.has(id), `${id} should be in the adaptive edge-case matrix`);
    assert.equal(byId.get(id).ok, true, JSON.stringify(byId.get(id), null, 2));
  }
  assert.equal(byId.get('unknown-stdio-npx').actual.maxInFlightPerWorker, 1);
  assert.equal(byId.get('legacy-sse').actual.defaultPoolModel, 'legacy-disabled');
  assert.equal(byId.get('remote-streamable-http').actual.defaultPoolModel, 'remote-http-session-pool');
  assert.equal(byId.get('remote-streamable-http').actual.parallelSafetyClass, 'P2_session_safe');
  assert.equal(byId.get('stateless-remote-http').actual.parallelSafetyClass, 'P4_stateless_remote_candidate');
  assert.equal(byId.get('stateful-memory').actual.defaultPoolModel, 'singleton');
  assert.equal(byId.get('stateful-memory').actual.parallelSafetyClass, 'P2_session_safe');
  assert.equal(byId.get('local-database').actual.defaultPoolModel, 'project-pool');
  assert.equal(byId.get('browser-automation').actual.parallelSafetyClass, 'PX_forbidden_browser_until_context_isolated');
});


test('adaptive worker plan materializes scheduler decisions with locks and degradation', () => {
  const result = run(['scripts/adaptive-worker-plan.mjs', '--json']);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'pass');
  assert.ok(report.summary.runtimePlanCount >= 0);
  assert.ok(report.summary.edgePlanCount >= 10);
  assert.ok(report.checks.every((check) => check.ok), JSON.stringify(report.checks, null, 2));
  const byId = new Map(report.plans.map((plan) => [plan.serverId, plan]));
  assert.equal(byId.get('unknown-stdio-npx').maxInFlightPerWorker, 1);
  assert.equal(byId.get('legacy-sse').poolModel, 'legacy-disabled');
  assert.equal(byId.get('legacy-sse').maxWorkers, 0);
  assert.ok(byId.get('remote-streamable-http').affinityKeys.includes('transportSessionId'));
  assert.ok(byId.get('credential-scoped-api').affinityKeys.includes('credentialProfile'));
  assert.ok(byId.get('project-filesystem-write').locks.some((lock) => lock.domain === 'file'));
  assert.ok(byId.get('browser-automation').affinityKeys.includes('browserContextId'));
  assert.ok(byId.get('browser-automation').requiresConsent);
  for (const plan of report.plans) {
    assert.ok(plan.degradationPolicy.onConflict, `${plan.serverId} should have conflict degradation`);
    assert.ok(plan.degradationPolicy.onAuthMixup, `${plan.serverId} should have auth-mixup degradation`);
  }
});


test('adaptive schemas and architecture docs are present', () => {
  for (const rel of [
    'schemas/mcpace-server-profile.schema.json',
    'schemas/mcpace-worker-plan.schema.json',
    'docs/adaptive-mcp-orchestration.md',
    'docs/adaptive-edge-case-coverage.md',
  ]) {
    assert.ok(fs.existsSync(path.join(root, rel)), `${rel} should exist`);
  }
  const doc = fs.readFileSync(path.join(root, 'docs/adaptive-mcp-orchestration.md'), 'utf8');
  assert.match(doc, /Legacy SSE/);
  assert.match(doc, /maxInFlightPerWorker=1/);
  assert.match(doc, /Safe probes must not/);
  assert.doesNotMatch(doc, /mcp-server-taxonomy\.json/);
  const edgeDoc = fs.readFileSync(path.join(root, 'docs/adaptive-edge-case-coverage.md'), 'utf8');
  assert.match(edgeDoc, /Unknown stdio package/);
  assert.match(edgeDoc, /Browser automation/);
  assert.match(edgeDoc, /Explicit stateless Streamable HTTP remote/);
});
