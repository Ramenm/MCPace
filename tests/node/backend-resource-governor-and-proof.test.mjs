import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function scripts() {
  return JSON.parse(read('package.json')).scripts || {};
}

test('backend exposes a global resource governor before route admission work', () => {
  const dashboard = read('src/dashboard.rs');
  const governor = read('src/dashboard/governor.rs');
  const overview = read('src/dashboard/overview.rs');
  const resources = read('src/resources.rs');

  assert.match(dashboard, /mod governor;/);
  assert.match(dashboard, /resource_governor:\s*GlobalResourceGovernor/);
  assert.match(dashboard, /try_enter_request\(\)/);
  assert.match(dashboard, /http\.resource_governor_rejected/);
  assert.match(dashboard, /503 Service Unavailable/);
  assert.match(governor, /mcpace\.globalResourceGovernor\.v1/);
  assert.match(governor, /activeRequestLimit/);
  assert.match(governor, /rssSoftBytes/);
  assert.match(governor, /fdSoftLimit/);
  assert.match(governor, /threadSoftLimit/);
  assert.match(governor, /http\.server\.active_requests/);
  assert.match(resources, /ENV_GLOBAL_ACTIVE_REQUEST_LIMIT/);
  assert.match(resources, /ENV_PROCESS_RSS_SOFT_BYTES/);
  assert.match(resources, /ENV_PROCESS_FD_SOFT_LIMIT/);
  assert.match(resources, /ENV_PROCESS_THREAD_SOFT_LIMIT/);
  assert.match(overview, /"processResource", process_resource_snapshot\.clone\(\)/);
  assert.match(overview, /"resourceGovernor", resource_governor_snapshot/);
});

test('HTTP latency snapshots expose OpenTelemetry-compatible aliases without renaming MCPace fields', () => {
  const latency = read('src/dashboard/latency.rs');
  const governor = read('src/dashboard/governor.rs');
  assert.match(latency, /otelAliases/);
  assert.match(latency, /http\.server\.request\.duration/);
  assert.match(latency, /http\.server\.request\.body\.size/);
  assert.match(latency, /http\.request\.header\.size/);
  assert.match(governor, /http\.server\.active_requests/);
});

test('dashboard browser lifecycle proof prevents tab wake-up refresh storms', () => {
  const html = read('src/dashboard/index.html');
  const proof = read('scripts/browser-lifecycle-proof.mjs');
  const packageScripts = scripts();

  assert.match(html, /LIFECYCLE_RESUME_MIN_INTERVAL_MS/);
  assert.match(html, /document\.wasDiscarded/);
  assert.match(html, /document\.addEventListener\("freeze"/);
  assert.match(html, /document\.addEventListener\("resume"/);
  assert.match(html, /window\.addEventListener\("pageshow"/);
  assert.match(html, /state\.lifecycle\.frozen/);
  assert.match(html, /refreshDashboard\(\{ reason: "resume" \}\)/);
  assert.doesNotMatch(html, /reason: "resume"[^\n]+force: true/);
  assert.match(proof, /mcpace\.browserLifecycleProof\.v1/);
  assert.equal(packageScripts['proof:browser-lifecycle'], 'node scripts/browser-lifecycle-proof.mjs');
});

test('service verify records applied-state probes for user supervisors', () => {
  const service = read('src/service.rs');
  assert.match(service, /"appliedState", service_applied_state_json\(config\)/);
  assert.match(service, /mcpace\.serviceAppliedState\.v1/);
  assert.match(service, /systemctl --user show/);
  assert.match(service, /MemoryAccounting/);
  assert.match(service, /TasksMax/);
  assert.match(service, /MainPID/);
  assert.match(service, /ExecStart/);
});

test('static Rust guard and trusted-publish preflight are wired into CI scripts and release manifest', () => {
  const packageScripts = scripts();
  const manifest = read('release-manifest.json');
  const publishWorkflow = read('.github/workflows/publish-npm.yml');
  const trustPreflight = read('scripts/publish-trust-preflight.mjs');
  const rustGuard = read('scripts/rust-static-guard.mjs');

  assert.equal(packageScripts['lint:rust-static'], 'node scripts/rust-static-guard.mjs --json');
  assert.match(packageScripts['lint:npm'], /lint:rust-static/);
  assert.equal(packageScripts['check:publish-trust'], 'node scripts/publish-trust-preflight.mjs');
  assert.match(packageScripts['check:ci'], /check:publish-trust/);
  assert.match(packageScripts['check:ci'], /proof:browser-lifecycle/);
  assert.match(manifest, /scripts\/rust-static-guard\.mjs/);
  assert.match(manifest, /scripts\/browser-lifecycle-proof\.mjs/);
  assert.match(manifest, /scripts\/publish-trust-preflight\.mjs/);
  assert.match(publishWorkflow, /id-token:\s*write/);
  assert.match(publishWorkflow, /--provenance/);
  assert.doesNotMatch(publishWorkflow, /NODE_AUTH_TOKEN|NPM_TOKEN|NPM_CONFIG_.*TOKEN/i);
  assert.match(trustPreflight, /mcpace\.publishTrustPreflight\.v1/);
  assert.match(rustGuard, /mcpace\.rustStaticGuard\.v1/);
});
