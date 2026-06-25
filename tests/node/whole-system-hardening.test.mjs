import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('MCP Streamable HTTP session touch, replay tracking, and readiness classification are one critical section', () => {
  const mcpHttp = read('src/dashboard/mcp_http.rs');
  assert.match(mcpHttp, /fn prepare_mcp_session_for_request\(/);
  assert.match(mcpHttp, /let mut session_store = config\s*\.http_session_store\s*\.lock\(\)/s);
  assert.match(mcpHttp, /touch_from_request\(request, now_ms\(\)\)/);
  assert.match(mcpHttp, /track_request_id\(&session_id, key\)/);
  assert.match(mcpHttp, /require_initialized && !session\.initialized/);

  const body = mcpHttp.slice(
    mcpHttp.indexOf('fn prepare_mcp_session_for_request('),
    mcpHttp.indexOf('fn mark_mcp_session_initialized('),
  );
  assert.ok(body.indexOf('touch_from_request') < body.indexOf('track_request_id'), 'session touch must precede replay tracking');
  assert.ok(body.indexOf('track_request_id') < body.indexOf('require_initialized && !session.initialized'), 'replay tracking and readiness must be ordered inside one helper');
  assert.doesNotMatch(body, /config\s*\.http_session_store\s*\.lock\(\)[\s\S]*config\s*\.http_session_store\s*\.lock\(\)/, 'helper should not split the decision across two mutex windows');
});

test('non-loopback serve bind has a matching Host/Origin request-boundary mode instead of being self-contradictory', () => {
  const dashboard = read('src/dashboard.rs');
  const boundary = read('src/dashboard/http_boundary.rs');

  assert.match(dashboard, /allow_nonlocal_host: bool/);
  assert.match(dashboard, /allow_nonlocal_host: non_loopback_bind/);
  assert.match(dashboard, /validate_origin_for_bind\(\s*request,\s*config\.allow_nonlocal_host,?\s*\)/s);

  assert.match(boundary, /pub\(super\) fn validate_origin_for_bind\(/);
  assert.match(boundary, /host_allowed_for_bind\(host, allow_nonlocal_host\)/);
  assert.match(boundary, /origin_allowed_for_bind\(origin, host, allow_nonlocal_host\)/);
  assert.match(boundary, /normalized_authority\(origin_authority\) == normalized_authority\(host_header\)/);
  assert.match(boundary, /multiple Origin headers are not allowed/);
  assert.match(boundary, /is_valid_http_authority\(host_header\)/);
  assert.match(boundary, /is_allowed_local_authority\(host_header\)/);
});

test('MCP settings import source follows the same regular-file policy as write targets', () => {
  const importer = read('src/mcp_sources/import.rs');
  assert.match(importer, /let source_value = read_import_source\(&source_path\)\?/);
  assert.match(importer, /fn read_import_source\(path: &Path\) -> Result<JsonValue, String>/);
  assert.match(importer, /fs::symlink_metadata\(path\)/);
  assert.match(importer, /metadata\.file_type\(\)\.is_symlink\(\)/);
  assert.match(importer, /must be a regular file, not a symlink/);
  assert.doesNotMatch(importer, /if !source_path\.is_file\(\)/);
});

test('holistic runtime model documents the cross-layer behavior, not just isolated hardening primitives', () => {
  const doc = read('docs/holistic-runtime-model.md');
  assert.match(doc, /Session touch, request-id replay tracking, and initialized-state classification/);
  assert.match(doc, /Non-loopback serve mode is no longer internally self-contradictory/);
  assert.match(doc, /Importing MCP settings rejects symlink and non-regular source files/);
  assert.match(doc, /Local\/default serve mode remains stricter/);
});
