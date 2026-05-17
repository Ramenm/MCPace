import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';

const repoRoot = path.resolve(new URL('../..', import.meta.url).pathname);
const read = (relative) => fs.readFileSync(path.join(repoRoot, relative), 'utf8');
const exists = (relative) => fs.existsSync(path.join(repoRoot, relative));

test('tool exposure defaults are broker-first and safe-projection-first', () => {
  const adapter = read('src/adapter.rs');
  assert.match(adapter, /fn default_tool_exposure_mode\(\) -> ToolExposureMode \{\n\s*ToolExposureMode::Broker\n\}/);
  assert.match(adapter, /const DEFAULT_PROJECTED_TOOL_SAFETY:\s*ProjectionSafety\s*=\s*ProjectionSafety::Safe/);
  assert.match(adapter, /allowUnknownTool/);
  assert.match(adapter, /allowUnknownUpstreamTool/);

  const dynamicAdapter = read('docs/dynamic-adapter.md');
  const universalAdapter = read('docs/universal-dynamic-adapter.md');
  assert.match(dynamicAdapter, /projection safety default is `safe`/i);
  assert.match(universalAdapter, /projection safety default is `safe`/i);
});

test('brokered calls fail closed unless requested tool is currently advertised', () => {
  const source = read('src/upstream/lease_runtime.rs');
  assert.match(source, /fn validate_upstream_tool_known\(/);
  assert.match(source, /fn validate_upstream_batch_tools_known\(/);
  assert.match(source, /fn validate_upstream_tool_known_with_pool\(/);
  assert.match(source, /fn validate_upstream_batch_tools_known_with_pool\(/);
  assert.match(source, /cached_tools_list\(root_path, server, timeout, false\)/);
  assert.match(source, /not present in .*current tools\/list/);
  assert.match(source, /MCPACE_ALLOW_UNKNOWN_UPSTREAM_TOOLS/);
  assert.match(source, /allowUnknownTool/);
  assert.match(source, /allowUnknownUpstreamTool/);
  assert.ok((source.match(/validate_upstream_tool_known\(/g) || []).length >= 2);
  assert.ok((source.match(/validate_upstream_tool_known_with_pool\(/g) || []).length >= 2);
  assert.ok((source.match(/validate_upstream_batch_tools_known\(/g) || []).length >= 2);
  assert.ok((source.match(/validate_upstream_batch_tools_known_with_pool\(/g) || []).length >= 2);
  assert.ok(source.indexOf('validate_upstream_tool_policy') < source.indexOf('validate_upstream_tool_known'));
});

test('tool metadata injection is a first-class advisory risk signal', () => {
  const policyAudit = read('src/upstream/policy_audit.rs');
  assert.match(policyAudit, /add_metadata_based_advisory_signals/);
  assert.match(policyAudit, /metadata-injection/);
  assert.match(policyAudit, /ignore previous/);
  assert.match(policyAudit, /system prompt/);
  assert.match(policyAudit, /exfiltrate/);
  assert.match(policyAudit, /private key/);
  assert.match(policyAudit, /risk_class_recommends_policy/);
});

test('user-facing tool surfaces expose unknown-tool override only as an explicit opt-in', () => {
  for (const file of ['src/mcp_server/tool_surface.rs', 'src/dashboard/http_tools.rs']) {
    const source = read(file);
    assert.match(source, /allowUnknownTool/);
    assert.match(source, /allowUnknownTool.*boolean|boolean.*allowUnknownTool/s);
  }

  const safetyDoc = read('docs/tool-exposure-and-call-safety.md');
  assert.match(safetyDoc, /broker-first/i);
  assert.match(safetyDoc, /Known-tool call guard/i);
  assert.match(safetyDoc, /allowUnknownTool=true/);
  assert.match(safetyDoc, /metadata-injection/i);
  assert.match(read('docs/README.md'), /tool-exposure-and-call-safety\.md/);
});

test('tool exposure safety audit is wired as a repeatable gate', () => {
  assert.equal(exists('scripts/tool-exposure-safety-audit.mjs'), true);
  const pkg = JSON.parse(read('package.json'));
  assert.match(pkg.scripts['verify:tool-exposure-safety'], /tool-exposure-safety-audit\.mjs/);
  assert.match(read('scripts/install-readiness-harness.mjs'), /tool-exposure-safety/);
  assert.match(read('scripts/local-quality-suite.mjs'), /tool-exposure-safety/);

  const result = spawnSync(process.execPath, [
    'scripts/tool-exposure-safety-audit.mjs',
    '--json',
    '--strict',
  ], {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: 30_000,
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'pass');
  assert.equal(report.summary.blockers, 0);
});
