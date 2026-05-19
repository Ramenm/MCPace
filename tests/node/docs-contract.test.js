const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

const activeDocs = [
  'README.md',
  'CONTRIBUTING.md',
  'docs/README.md',
  'docs/host-setup.md',
  'docs/test-strategy.md',
  'docs/eval-plan.md',
  'docs/verification-matrix.md',
  'docs/mcp-spec-alignment.md',
  'docs/client-metadata-routing.md',
  'docs/client-surface-matrix.md',
  'docs/server-segmentation-and-auto-discovery.md',
  'docs/technology-decision.md',
  'docs/technology-evaluation.md',
  'docs/toolchain-policy.md',
  'docs/rust-rewrite-architecture.md',
  'docs/rust-migration-plan.md',
  'docs/rewrite-cutover-plan.md',
  'docs/legacy-powershell-removal.md',
  'docs/recovery-runbook.md',
  'docs/runtime-profiles.md',
  'docs/project-registry-v1.md',
  'docs/runtime-lab.md',
  'docs/runtime-performance.md',
  'docs/performance-decision-log-20260430.md',
  'docs/performance-verification.md',
  'TODO.md',
  'STATE.md',
  'DECISIONS.md'
];

test('README reflects the Rust-only repo contract and current client/server grouped read paths', () => {
  const readme = read('README.md');
  const normalized = readme.replace(/\s+/g, ' ');
  assert.match(normalized, /Rust-first local MCP hub/i);
  assert.match(normalized, /client plan/i);
  assert.match(normalized, /lab report/i);
  assert.match(normalized, /release build/i);
  assert.match(normalized, /does not publish/i);
  assert.match(normalized, /single local MCP hub for many clients/i);
  assert.match(normalized, /not implemented yet/i);
  assert.match(normalized, /2025-11-25/);
  assert.doesNotMatch(normalized, /release command remains planned/i);
  assert.doesNotMatch(normalized, /release still fail/i);
  assert.doesNotMatch(readme, /pwsh\s+\.\//i);
  assert.doesNotMatch(readme, /\.\/manager\.ps1/i);
});

test('active docs do not instruct removed shell entrypoints', () => {
  const combined = activeDocs.map((file) => read(file)).join('\n');
  assert.doesNotMatch(combined, /pwsh\s+\.\//i);
  assert.doesNotMatch(combined, /\.\/manager\.ps1/i);
  assert.doesNotMatch(combined, /\.\/manager\.sh/i);
  assert.doesNotMatch(combined, /manager\.cmd\s/i);
  assert.doesNotMatch(combined, /Invoke-Pester/i);
});

test('spec alignment doc still names the checked MCP baseline, transport subset, and session-aware rules', () => {
  const specDoc = read('docs/mcp-spec-alignment.md');
  assert.match(specDoc, /2025-11-25/);
  assert.match(specDoc, /stdio/i);
  assert.match(specDoc, /Streamable HTTP/i);
  assert.match(specDoc, /stateful/i);
  assert.match(specDoc, /cancellation/i);
});

test('routing and arbitration docs describe single-entry-point planning and server serialization', () => {
  const routingDoc = read('docs/client-metadata-routing.md');
  const arbitrationDoc = read('docs/server-segmentation-and-auto-discovery.md');
  assert.match(routingDoc, /one future entry point/i);
  assert.match(routingDoc, /MCPACE_CLIENT_METADATA_JSON/);
  assert.match(arbitrationDoc, /single-session/i);
  assert.match(arbitrationDoc, /should own the child process/i);
});

test('client surface matrix keeps local/cloud/API connector divergence explicit', () => {
  const surfaceDoc = read('docs/client-surface-matrix.md');
  assert.match(surfaceDoc, /surface/i);
  assert.match(surfaceDoc, /cloud/i);
  assert.match(surfaceDoc, /tools-only/i);
  assert.match(surfaceDoc, /public-http-only/i);
});


test('runtime performance doc names the new resource controls and bounded parallelism contract', () => {
  const readme = read('README.md');
  const perfDoc = read('docs/runtime-performance.md');
  const combined = `${readme}\n${perfDoc}`;

  assert.match(combined, /--max-connections/);
  assert.match(combined, /--io-timeout-ms/);
  assert.match(combined, /--max-body-bytes/);
  assert.match(combined, /--overview-cache-ms/);
  assert.match(perfDoc, /available_parallelism/);
  assert.match(perfDoc, /bounded/i);
  assert.match(perfDoc, /upstream session pool/i);
  assert.match(perfDoc, /tools\/list.*coalesc/i);
  assert.match(perfDoc, /header count/i);
  assert.match(perfDoc, /fixed worker pool/i);
  assert.match(perfDoc, /zero-buffer/i);
  assert.match(perfDoc, /128.*entries/i);
  assert.match(perfDoc, /health.*cache/i);
  assert.match(perfDoc, /cache\.stale/i);
  assert.match(perfDoc, /runtime\.http/i);
  assert.match(perfDoc, /session-pool shards/i);
  assert.match(perfDoc, /runtime\.upstreamSessionPool/i);

  const decisionLog = read('docs/performance-decision-log-20260430.md');
  assert.match(decisionLog, /refresh=1/);
  assert.match(decisionLog, /singleflight/i);
  assert.match(decisionLog, /head-of-line/i);
  assert.match(decisionLog, /rendezvous/i);
  assert.match(decisionLog, /manual dashboard refresh/i);
  assert.match(decisionLog, /health checks/i);
  assert.match(decisionLog, /runtime counters/i);
  assert.match(decisionLog, /pool[\s\S]*shards/i);
  assert.match(decisionLog, /stale snapshot/i);
});
