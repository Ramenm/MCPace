const assert = require('node:assert/strict');
const { test } = require('node:test');
const { read, readJson } = require('./helpers');

const catalog = readJson('presets/mcp-servers.json');
const mcpPresetsSource = read('src/mcp_presets.rs');
const serverLoader = read('src/server/loader.rs');
const dashboardHtml = read('src/dashboard/index.html');
const dashboardOverview = read('src/dashboard/overview.rs');

function preset(id) {
  const found = catalog.presets.find((item) => item.id === id);
  assert.ok(found, `missing preset ${id}`);
  return found;
}

test('starter MCP presets carry explicit reviewable policies', () => {
  for (const id of ['filesystem', 'git', 'playwright', 'context7']) {
    const item = preset(id);
    assert.equal(typeof item.policy, 'object', `${id} missing policy`);
    assert.equal(typeof item.review, 'object', `${id} missing review`);
    assert.ok(item.policy.concurrencyPolicy, `${id} missing concurrency policy`);
    assert.ok(Object.hasOwn(item.policy, 'discoveryRequiresLease'), `${id} missing discoveryRequiresLease`);
  }
});

test('stateful/problematic preset policies are conservative by default', () => {
  assert.equal(preset('filesystem').policy.scopeClass, 'project-local');
  assert.equal(preset('filesystem').policy.projectRootMode, 'required');
  assert.equal(preset('filesystem').policy.discoveryRequiresLease, true);
  assert.equal(preset('git').policy.concurrencyPolicy, 'single-writer');
  assert.equal(preset('git').policy.worktreeBinding, 'repository-root');
  assert.equal(preset('playwright').policy.scopeClass, 'shared-exclusive');
  assert.equal(preset('playwright').policy.hostLock, 'browser-profile');
  assert.equal(preset('playwright').policy.discoveryRequiresLease, true);
  assert.equal(preset('context7').policy.concurrencyPolicy, 'multi-reader');
});

test('preset install writes a policy overlay next to mcp_settings registration', () => {
  assert.match(mcpPresetsSource, /write_preset_policy_overlay/);
  assert.match(mcpPresetsSource, /mcpace\.config\.json/);
  assert.match(mcpPresetsSource, /"policy", policy/);
  assert.match(mcpPresetsSource, /"review"/);
  assert.match(mcpPresetsSource, /server '\{\}' already has a policy overlay/);
});

test('generic source inference keeps unknown servers conservative while recognizing known risky families', () => {
  assert.match(serverLoader, /infer_generic_source_policy/);
  assert.match(serverLoader, /unknown-conservative-review|settings-only/);
  assert.match(serverLoader, /browser-profile/);
  assert.match(serverLoader, /project-filesystem|project-git/);
  assert.match(serverLoader, /discoveryRequiresLease/);
});


test('dashboard exposes minimal Activity and Policy Review surfaces', () => {
  assert.match(dashboardHtml, /Policy Review/);
  assert.match(dashboardHtml, /Activity/);
  assert.match(dashboardHtml, /buildPolicyReviews/);
  assert.match(dashboardHtml, /renderActivity/);
  assert.match(dashboardHtml, /discoveryRequiresLease/);
  assert.match(dashboardOverview, /hub", "lease", "list", "--json"/);
  assert.match(dashboardOverview, /"leases"/);
});
