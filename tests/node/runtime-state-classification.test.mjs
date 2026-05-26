import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const readJson = (...parts) => JSON.parse(read(...parts));

test('server loader exposes explicit stateless/stateful runtime classification', () => {
  const model = read('src', 'server', 'model.rs');
  const loader = read('src', 'server', 'loader.rs');
  const clientModel = read('src', 'client', 'model.rs');
  const clientRender = read('src', 'client', 'render.rs');
  const instances = read('src', 'server', 'instances.rs');
  const dashboard = read('src', 'dashboard', 'index.html');

  assert.match(model, /pub runtime_type: String/);
  assert.match(model, /pub state_class: String/);
  assert.match(model, /pub effect_class: String/);
  assert.match(model, /"runtimeType"/);
  assert.match(model, /"stateClass"/);
  assert.match(model, /"effectClass"/);

  assert.match(loader, /struct RuntimeClassification/);
  assert.match(loader, /fn infer_runtime_classification/);
  assert.match(loader, /runtime_type: "stateless"/);
  assert.match(loader, /state_class: "session-stateful"/);
  assert.match(loader, /state_class: "project-stateful"/);
  assert.match(loader, /state_class: "credential-stateful"/);
  assert.match(loader, /state_class: "remote-session-stateful"/);
  assert.match(loader, /state_class: "host-stateful"/);
  assert.match(loader, /state_class: "unknown-conservative"/);
  assert.match(loader, /effect_class: "project-mutating"/);
  assert.match(loader, /effect_class: "external-read"/);
  assert.match(loader, /effect_class: "process-exec"/);
  assert.match(loader, /policy_string\(policy, \"runtimeType\"/);
  assert.match(loader, /policy_string\(policy, \"stateClass\"/);
  assert.match(loader, /policy_string\(policy, \"effectClass\"/);
  assert.match(loader, /tool_policies\.iter\(\)\.any\(policy_mentions_destructive\)/);

  assert.match(clientModel, /runtime_type: String/);
  assert.match(clientRender, /"runtimeType"/);
  assert.match(instances, /"stateClass"/);
  assert.match(dashboard, /statelessCount/);
  assert.match(dashboard, /stateful\/effectful/);
});

test('schemas document runtimeType stateClass and effectClass', () => {
  const configSchema = readJson('schemas', 'mcpace-config.schema.json');
  const profileSchema = readJson('schemas', 'mcpace-server-profile.schema.json');
  const workerSchema = readJson('schemas', 'mcpace-worker-plan.schema.json');

  const policyProps = configSchema.$defs.serverPolicy.properties;
  assert.deepEqual(policyProps.runtimeType.enum, ['stateless', 'stateful', 'external', 'interactive', 'side-effecting', 'legacy', 'package-artifact', 'unknown']);
  assert.ok(policyProps.stateClass.enum.includes('stateless'));
  assert.ok(policyProps.stateClass.enum.includes('session-stateful'));
  assert.ok(policyProps.stateClass.enum.includes('project-stateful'));
  assert.ok(policyProps.stateClass.enum.includes('credential-stateful'));
  assert.ok(policyProps.stateClass.enum.includes('unknown-conservative'));
  assert.ok(policyProps.stateClass.enum.includes('not-a-server'));
  assert.ok(policyProps.effectClass.enum.includes('read-only'));
  assert.ok(policyProps.effectClass.enum.includes('project-mutating'));
  assert.ok(policyProps.effectClass.enum.includes('external-mutating'));
  assert.ok(policyProps.effectClass.enum.includes('not-runnable'));

  for (const schema of [profileSchema, workerSchema]) {
    assert.ok(schema.required.includes('runtimeType'));
    assert.ok(schema.required.includes('stateClass'));
    assert.ok(schema.required.includes('effectClass'));
    assert.ok(schema.properties.runtimeType.enum.includes('stateless'));
    assert.ok(schema.properties.runtimeType.enum.includes('package-artifact'));
    assert.ok(schema.properties.stateClass.enum.includes('remote-session-stateful'));
    assert.ok(schema.properties.stateClass.enum.includes('not-a-server'));
    assert.ok(schema.properties.effectClass.enum.includes('process-exec'));
    assert.ok(schema.properties.effectClass.enum.includes('not-runnable'));
  }
});
