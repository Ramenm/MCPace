const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('local HTTP serving uses a fixed rendezvous-backed worker pool with runtime telemetry', () => {
  const dashboard = [
    read('src/dashboard.rs'),
    read('src/dashboard/overview.rs'),
    read('src/dashboard/tool_runtime.rs'),
  ].join('\n');
  const resources = read('src/resources.rs');

  assert.match(dashboard, /mpsc::sync_channel::<TcpStream>\(0\)/);
  assert.match(dashboard, /HttpRuntimeMetrics/);
  assert.match(dashboard, /HttpRuntimeMetricsSnapshot/);
  assert.match(dashboard, /active_connections/);
  assert.match(dashboard, /max_active_connections/);
  assert.match(dashboard, /"GET", "\/api\/resources"/);
  assert.match(dashboard, /runtime_resources_response/);
  assert.match(resources, /default_http_connection_limit/);
  assert.match(resources, /MAX_HTTP_HEADER_COUNT/);
});

test('HTTP upstream session pooling is sharded and capacity remains bounded', () => {
  const dashboard = [
    read('src/dashboard.rs'),
    read('src/dashboard/overview.rs'),
    read('src/dashboard/tool_runtime.rs'),
  ].join('\n');
  const upstream = [
    read('src/upstream.rs'),
    read('src/upstream/lease_runtime.rs'),
    read('src/upstream/session_pool.rs'),
    read('src/upstream/tool_cache.rs'),
  ].join('\n');
  const resources = read('src/resources.rs');

  assert.match(dashboard, /upstream_session_pools: Vec<Mutex<upstream::UpstreamSessionPool>>/);
  assert.match(dashboard, /fn new_upstream_session_pools/);
  assert.match(dashboard, /fn upstream_pool_for_context/);
  assert.match(dashboard, /DefaultHasher/);
  assert.match(dashboard, /runtime\.upstreamSessionPool|"upstreamSessionPool"/);
  assert.match(resources, /default_upstream_session_pool_shard_count/);
  assert.match(upstream, /pub fn with_max_sessions\(max_sessions: usize\)/);
  assert.match(upstream, /max_sessions: usize/);
  assert.match(upstream, /self\.max_session_count\(\)/);
});

test('runtime benchmark helper is wired into package scripts and documents operator usage', () => {
  const packageJson = JSON.parse(read('package.json'));
  const benchmarkScript = read('scripts/benchmark-runtime.mjs');
  const perfDoc = read('docs/runtime-performance.md');
  const readme = read('README.md');

  assert.equal(packageJson.scripts['benchmark:runtime'], 'node scripts/benchmark-runtime.mjs');
  assert.equal(packageJson.scripts['lint:npm'], 'node scripts/check-node-syntax.mjs --json');
  assert.equal(fs.existsSync(path.join(repoRoot, 'scripts/benchmark-runtime.mjs')), true);
  assert.match(benchmarkScript, /--url/);
  assert.match(benchmarkScript, /--concurrency/);
  assert.match(benchmarkScript, /p95/);
  assert.match(perfDoc, /benchmark:runtime/);
  assert.match(readme, /benchmark:runtime/);
});
