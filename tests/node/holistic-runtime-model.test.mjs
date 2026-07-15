import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';

const repoRoot = path.resolve(import.meta.dirname, '../..');
const read = (relativePath) => fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');

test('dashboard tool-list warmup is explicit opt-in, not a serve-start side effect', () => {
  const dashboard = read('src/dashboard.rs');
  assert.match(dashboard, /fn tool_list_cache_warmup_enabled\(\) -> bool/);
  assert.match(dashboard, /"1" \| "true" \| "yes" \| "on" \| "enabled"/);
  assert.match(dashboard, /\.unwrap_or\(false\)/);
  assert.doesNotMatch(dashboard, /\.unwrap_or\(true\)/);
});

test('dashboard HTTP parser rejects invalid header values before routing', () => {
  const dashboard = read('src/dashboard.rs');
  assert.match(dashboard, /let trimmed = value\.trim\(\)\.to_string\(\);\s*if !http_boundary::is_valid_http_header_value\(&trimmed\)/s);
  assert.match(dashboard, /"Invalid HTTP header value"/);

  const boundary = read('src/dashboard/http_boundary.rs');
  assert.match(boundary, /pub\(super\) fn is_valid_http_header_value\(value: &str\) -> bool/);
  assert.match(boundary, /byte == b' ' \|\| \(0x21\.\.=0x7e\)\.contains\(&byte\)/);
});

test('dashboard worker startup fails gracefully instead of panicking', () => {
  const dashboard = read('src/dashboard.rs');
  assert.doesNotMatch(dashboard, /expect\("failed to spawn MCPace HTTP worker"\)/);
  assert.match(dashboard, /match worker \{/);
  assert.match(dashboard, /dashboard failed to spawn HTTP worker/);
  assert.match(dashboard, /join_request_workers\(handles, stderr\)/);
});

test('runtime private directories are created private on first creation', () => {
  const runtimePaths = read('src/runtimepaths.rs');
  assert.match(runtimePaths, /fn create_private_dir\(path: &Path\) -> RuntimePathResult<\(\)>/);
  assert.match(runtimePaths, /builder\.mode\(0o700\)/);
  assert.match(runtimePaths, /create\(path\)/);
  assert.match(runtimePaths, /fs::symlink_metadata\(path\)/);
  assert.match(runtimePaths, /pub enum RuntimePathError/);
  assert.match(runtimePaths, /RuntimePathResult/);
  assert.match(runtimePaths, /runtime path is not a real directory/);
});

test('release manifest ships the holistic runtime model documentation', () => {
  assert.ok(fs.existsSync(path.join(repoRoot, 'docs/holistic-runtime-model.md')));
  const manifest = JSON.parse(read('release-manifest.json'));
  assert.ok(manifest.includePaths.includes('docs/holistic-runtime-model.md'));
});
