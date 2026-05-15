import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';

const repoRoot = path.resolve(new URL('../..', import.meta.url).pathname);
const read = (relative) => fs.readFileSync(path.join(repoRoot, relative), 'utf8');
const exists = (relative) => fs.existsSync(path.join(repoRoot, relative));

test('mixed upstream topology hardening is documented and wired as a gate', () => {
  assert.equal(exists('docs/mixed-upstream-topologies.md'), true);
  const doc = read('docs/mixed-upstream-topologies.md');
  for (const term of [
    /local stdio command/i,
    /local\/plain Streamable HTTP/i,
    /HTTPS remote Streamable HTTP/i,
    /legacy HTTP\+SSE/i,
    /blocked-https-upstream/,
    /blocked-legacy-sse-upstream/,
    /One server cannot poison the topology/i,
    /Tool names are server-scoped/i,
    /npm run verify:mixed-upstreams/,
  ]) {
    assert.match(doc, term);
  }
  assert.match(read('docs/README.md'), /mixed-upstream-topologies\.md/);
  assert.match(read('docs/dynamic-adapter.md'), /mixed-upstream-topologies\.md/);

  const pkg = JSON.parse(read('package.json'));
  assert.match(pkg.scripts['verify:mixed-upstreams'], /simulate-mixed-upstreams\.mjs/);
  assert.match(pkg.scripts['benchmark:mixed-upstreams'], /--tools 500000/);
});

test('source type normalization keeps legacy SSE distinct from Streamable HTTP', () => {
  const sourceType = read('src/upstream/source_type.rs');
  assert.match(sourceType, /"remote-sse" \| "sse"/);
  assert.match(sourceType, /"legacy-sse"\.to_string\(\)/);
  assert.doesNotMatch(sourceType, /remote-sse"[^\n]*=> "http"\.to_string/);

  const upstream = read('src/upstream.rs');
  assert.match(upstream, /blocked-https-upstream/);
  assert.match(upstream, /blocked-legacy-sse-upstream/);
  assert.match(upstream, /blocked-unsupported-transport/);
  assert.match(upstream, /deprecated HTTP\+SSE transport/);
});

test('mixed topology simulator covers success, blocked, and failing server lanes', () => {
  const result = spawnSync(process.execPath, [
    'scripts/simulate-mixed-upstreams.mjs',
    '--servers', '50',
    '--tools', '100000',
    '--search-limit', '25',
    '--projection-budget', '64',
    '--page-size', '128',
    '--memory-limit-mib', '512',
    '--json',
  ], {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: 30_000,
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, 'pass');
  assert.equal(report.scenario.servers, 50);
  assert.equal(report.scenario.tools, 100000);
  assert.equal(report.transportMatrix.stdioDirect, true);
  assert.equal(report.transportMatrix.plainStreamableHttpDirect, true);
  assert.equal(report.transportMatrix.httpsDirect, false);
  assert.equal(report.transportMatrix.legacyHttpSseDirect, false);
  assert.ok(report.results.callableServerCount > 0);
  assert.ok(report.results.blockedServerCount > 0);
  assert.ok(report.results.failedServerCount > 0);
  assert.ok(report.results.retainedSearchCandidates <= 25);
  assert.ok(report.results.projectedToolCount <= 64);
  assert.ok(report.results.firstPageCount <= 128);
  assert.ok(report.results.duplicateToolNameCollisions > 0);
  assert.equal(report.results.qualifiedNameCollisions, 0);
  assert.equal(report.budgets.failureIsolation, true);
  assert.equal(report.budgets.collisionSafe, true);
  assert.ok(report.counts.statuses['callable-stdio'] > 0);
  assert.ok(report.counts.statuses['callable-http'] > 0);
  assert.ok(report.counts.statuses['blocked-https-upstream'] > 0);
  assert.ok(report.counts.statuses['blocked-legacy-sse-upstream'] > 0);
  assert.ok(report.counts.statuses['blocked-unsupported-transport'] > 0);
  assert.ok(report.counts.statuses['catalog-failed'] > 0);
});

test('install and local quality gates include mixed upstream topology proof', () => {
  assert.match(read('scripts/install-readiness-harness.mjs'), /runMixedUpstreamSimulation/);
  assert.match(read('scripts/install-readiness-harness.mjs'), /mixed-upstream-topology/);
  assert.match(read('scripts/local-quality-suite.mjs'), /mixed-upstream-topology/);
});
