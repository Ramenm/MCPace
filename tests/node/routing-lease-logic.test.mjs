import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const sliceFn = (source, name, nextName) => {
  const start = source.indexOf(`fn ${name}(`);
  assert.notEqual(start, -1, `missing function ${name}`);
  const end = nextName ? source.indexOf(`\nfn ${nextName}(`, start) : source.length;
  assert.notEqual(end, -1, `missing end marker ${nextName}`);
  return source.slice(start, end);
};

test('client plan counters and hub-owned stdio only count routable active routes', () => {
  const plan = read('src', 'client', 'plan.rs');
  const buildPlan = sliceFn(plan, 'build_plan', 'build_server_plan');

  assert.match(buildPlan, /let route_is_routable = server_plan_is_routable\(&plan\);/);
  assert.match(buildPlan, /if route_is_routable && plan\.upstream_transport == "stdio"/);
  assert.match(buildPlan, /if route_is_routable \{\s*if plan\.request_strategy == "parallel-safe"/s);
  assert.match(buildPlan, /server_record_is_routable\(record\)\s*&& \(record\.scope_class == "project-local"/s);
  assert.match(buildPlan, /server_record_is_routable\(record\) && record\.concurrency_policy == "single-session"/);

  const planRoutable = sliceFn(plan, 'server_plan_is_routable', 'server_is_not_routable');
  assert.match(planRoutable, /plan\.parallelism_limit > 0/);
  assert.match(planRoutable, /plan\.scheduler_lane != "disabled"/);
  assert.match(planRoutable, /plan\.scheduler_lane != "legacy-disabled"/);
  assert.match(planRoutable, /plan\.request_strategy != "disabled-no-route"/);
  assert.match(planRoutable, /plan\.request_strategy != "legacy-compat-disabled"/);
});

test('admission state exposes policy/platform-disabled routes before lease acquisition', () => {
  const plan = read('src', 'client', 'plan.rs');
  const admission = sliceFn(plan, 'admission_state', 'server_record_is_routable');

  assert.match(admission, /if !record\.platform_supported/);
  assert.match(admission, /"unsupported-platform"\.to_string\(\)/);
  assert.match(admission, /record\.source_enabled && record\.effective_enabled/);
  assert.match(admission, /"configured-source"\.to_string\(\)/);
  assert.match(admission, /else if record\.source_enabled/);
  assert.match(admission, /"disabled-by-policy"\.to_string\(\)/);
});

test('lease acquisition blocks disabled, legacy, zero-capacity, and malformed route states', () => {
  const leases = read('src', 'hub', 'leases.rs');
  const blockers = sliceFn(leases, 'route_blockers', 'find_conflict');

  assert.match(blockers, /route\.admission_state != "configured-source"/);
  assert.match(blockers, /route\.request_strategy == "disabled-no-route"/);
  assert.match(blockers, /route\.request_strategy == "legacy-compat-disabled"/);
  assert.match(blockers, /route\.scheduler_lane == "disabled"/);
  assert.match(blockers, /route\.scheduler_lane == "legacy-disabled"/);
  assert.match(blockers, /route\.startup_strategy == "disabled"/);
  assert.match(blockers, /route\.parallelism_limit == 0/);
  assert.match(blockers, /parallelismLimit=\{\}/);

  const cleanName = sliceFn(leases, 'clean_required_server_name', 'non_empty_token');
  assert.match(cleanName, /server_name\.chars\(\)\.any\(\|ch\| ch\.is_control\(\)\)/);
});


test('destructive tool policy overrides host-readonly/browser observation inference', () => {
  const loader = read('src', 'server', 'loader.rs');
  const runtime = sliceFn(loader, 'infer_runtime_classification', 'policy_mentions_destructive');
  const runtimeReadonlyIndex = runtime.indexOf('effect_class: "read-only"');
  const runtimeMutatingIndex = runtime.indexOf('effect_class: "host-mutating"');
  assert.ok(runtimeReadonlyIndex > -1, 'missing browser observation read-only branch');
  assert.ok(runtimeMutatingIndex > runtimeReadonlyIndex, 'missing conservative host-mutating fallback');
  assert.match(runtime, /\(signals\.contains\("browser-observation"\) \|\| state_binding == "host-readonly"\)\s*&& !destructive_tools/s);

  const parallel = sliceFn(loader, 'infer_parallel_safety_class', 'policy_is_explicit_stateless');
  assert.match(parallel, /\(signals\.contains\("browser-observation"\) \|\| state_binding == "host-readonly"\)\s*&& !destructive_tools/s);
  const hostReadonlyIndex = parallel.indexOf('P1_host_readonly_candidate');
  const destructiveIndex = parallel.indexOf('P0_mutating_requires_serialization');
  assert.ok(hostReadonlyIndex > -1, 'missing host-readonly parallel candidate');
  assert.ok(destructiveIndex > hostReadonlyIndex, 'host-readonly destructive fallback should reach the mutating guard');
});

test('declared policy enum tokens are canonicalized before routing decisions', () => {
  const loader = read('src', 'server', 'loader.rs');
  const normalize = sliceFn(loader, 'normalize_server_record', 'policy_token');

  for (const key of [
    'scopeClass',
    'concurrencyPolicy',
    'stateBinding',
    'credentialBinding',
    'projectRootMode',
    'worktreeBinding',
    'stateProfileMode',
    'hostLock',
    'startupStrategy',
    'routingGroup',
    'runtimeType',
    'stateClass',
    'effectClass',
  ]) {
    assert.match(normalize, new RegExp(`policy_token\\(\\s*policy,\\s*"${key}"`), `${key} should use canonical token parsing`);
  }

  assert.match(normalize, /let conflict_domain = policy_string\(policy, "conflictDomain"/);
  const tokenFn = sliceFn(loader, 'policy_token', 'policy_string');
  assert.match(tokenFn, /text_utils::normalize_flag\(&policy_string\(policy, key, fallback\)\)/);
});
