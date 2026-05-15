const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const repoRoot = path.resolve(__dirname, '..', '..');
const read = (relativePath) => fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));

test('runtime lifecycle contract documents config/state/cache/restart/reinstall semantics', () => {
  assert.equal(exists('docs/runtime-state-cache-lifecycle.md'), true);
  const doc = read('docs/runtime-state-cache-lifecycle.md');

  for (const term of [
    /Durable config/i,
    /Recoverable state/i,
    /Disposable disk cache/i,
    /Ephemeral protocol state/i,
    /process restart/i,
    /Reinstall and upgrade behavior/i,
    /tool-list-cache/i,
    /client-install-backups/i,
    /HTTP MCP sessions/i,
    /Upstream stdio\/http session pool/i,
    /leases\.json/i,
    /write_text_atomic/i,
  ]) {
    assert.match(doc, term);
  }

  assert.match(read('docs/architecture-boundaries.md'), /runtime-state-cache-lifecycle\.md/);
});

test('durable config mutation paths use atomic write helper', () => {
  const runtimePaths = read('src/runtimepaths.rs');
  assert.match(runtimePaths, /pub fn write_text_atomic\(path: &Path, contents: &str\) -> Result<\(\), String>/);
  assert.match(runtimePaths, /fs::write\(&temp_path, contents\)/);
  assert.match(runtimePaths, /fs::rename\(&temp_path, path\)/);

  const clientActions = read('src/client/actions.rs');
  assert.match(clientActions, /runtimepaths::write_text_atomic\(&self\.config_path, &update\.contents\)/);
  assert.match(clientActions, /runtimepaths::write_text_atomic\(&content_path, existing\)/);
  assert.match(clientActions, /runtimepaths::write_text_atomic\(&manifest_path, &manifest\.to_pretty_string\(\)\)/);
  assert.doesNotMatch(clientActions, /fs::write\(&self\.config_path/);
  assert.doesNotMatch(clientActions, /fs::write\(&content_path/);
  assert.doesNotMatch(clientActions, /fs::write\(&manifest_path/);

  const clientBackup = read('src/client/actions/backup.rs');
  assert.match(clientBackup, /runtimepaths::write_text_atomic\(&config_path, &contents\)/);
  assert.doesNotMatch(clientBackup, /fs::write\(&config_path/);

  const mcpSourceWrites = read('src/mcp_sources/write.rs');
  assert.match(mcpSourceWrites, /runtimepaths::write_text_atomic\(&target_path, &serialized\)/);
  assert.doesNotMatch(mcpSourceWrites, /std::fs::write\(&target_path/);
});

test('tool-list disk cache is versioned and invalidated by MCPace/protocol version', () => {
  const toolCache = read('src/upstream/tool_cache.rs');
  const schemaMatch = toolCache.match(/const TOOL_LIST_DISK_CACHE_SCHEMA_VERSION: i64 = (\d+);/);
  assert.ok(schemaMatch, 'expected disk cache schema version constant');
  assert.ok(Number(schemaMatch[1]) >= 2, 'schema version must advance after lifecycle-key change');

  assert.match(toolCache, /"mcpaceVersion"/);
  assert.match(toolCache, /"mcpProtocolVersion"/);
  assert.match(toolCache, /env!\("CARGO_PKG_VERSION"\)/);
  assert.match(toolCache, /crate::mcp_protocol::CURRENT_PROTOCOL_VERSION/);
  assert.match(toolCache, /runtimepaths::write_text_atomic\(&path, &envelope\.to_compact_string\(\)\)/);
  assert.doesNotMatch(toolCache, /fs::write\(&path, envelope\.to_compact_string\(\)\)/);
});

test('runtime sessions and caches have explicit process-local or ttl boundaries', () => {
  const upstream = read('src/upstream.rs');
  assert.match(upstream, /TOOL_LIST_CACHE_TTL: Duration = Duration::from_secs\(30\)/);
  assert.match(upstream, /UPSTREAM_SESSION_IDLE_TTL: Duration = Duration::from_secs\(300\)/);

  const toolCache = read('src/upstream/tool_cache.rs');
  assert.match(toolCache, /TOOL_LIST_DISK_CACHE_TTL: Duration = Duration::from_secs\(24 \* 60 \* 60\)/);

  const httpSession = read('src/dashboard/http_session.rs');
  assert.match(httpSession, /DEFAULT_MCP_HTTP_SESSION_TTL_MS: u128 = 60 \* 60 \* 1000/);
  assert.match(httpSession, /sessions: HashMap<String, McpHttpSession>/);
  assert.doesNotMatch(httpSession, /std::fs|fs::/);

  const leases = read('src/hub/leases.rs');
  assert.match(leases, /DEFAULT_LEASE_TTL_MS: u128 = 120_000/);
  assert.match(leases, /MAX_LEASE_TTL_MS: u128 = 3_600_000/);
});
