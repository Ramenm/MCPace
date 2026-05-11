const test = require('node:test');
const assert = require('node:assert/strict');
const { cleanChildEnv, read } = require('./helpers');

test('upstream stderr diagnostics are bounded and redact likely secrets before surfacing errors', () => {
  const upstream = [
    read('src/upstream.rs'),
    read('src/upstream/diagnostics.rs'),
    read('src/upstream/policy_suggestions.rs'),
    read('src/upstream/policy_audit.rs'),
    read('src/upstream/inventory.rs'),
    read('src/upstream/process_config.rs'),
    read('src/upstream/projection.rs'),
    read('src/upstream/source_type.rs'),
    read('src/upstream/stdio_runtime.rs'),
    read('src/upstream/tests.rs'),
  ].join('\n');

  assert.match(upstream, /fn stderr_suffix\(/);
  assert.match(upstream, /sanitize_stderr_diagnostic/);
  assert.match(upstream, /redact_sensitive_assignments/);
  assert.match(upstream, /redact_bearer_tokens/);
  assert.match(upstream, /STDERR_DIAGNOSTIC_MAX_LINES/);
  assert.match(upstream, /STDERR_DIAGNOSTIC_MAX_CHARS_PER_LINE/);
  assert.match(upstream, /stderr_suffix_redacts_sensitive_diagnostics_without_removing_context/);
  assert.match(upstream, /stderr_suffix_bounds_diagnostic_line_count_and_length/);
  assert.doesNotMatch(upstream, /lines\.push\(trimmed\.to_string\(\)\)/);
});

test('security posture documentation covers MCP stderr redaction and explicit env allowlisting', () => {
  const summary = read('reports/summary.md');
  const memory = read('memory-bank/systemPatterns.md');

  assert.match(summary, /stderr/i);
  assert.match(summary, /redact/i);
  assert.match(summary, /env/i);
  assert.match(memory, /environment is cleared/i);
});

test('HTTP MCP route validates standard header/body agreement when clients send MCP headers', () => {
  const dashboard = [
    read('src/dashboard.rs'),
    read('src/dashboard/http_boundary.rs'),
    read('src/dashboard/http_headers.rs'),
    read('src/dashboard/http_session.rs'),
    read('src/dashboard/http_tools.rs'),
    read('src/dashboard/mcp_http.rs'),
    read('src/dashboard/tool_runtime.rs'),
    read('src/dashboard/tests.rs'),
    read('src/dashboard/index.html'),
  ].join('\n');
  const spec = read('docs/mcp-http-api-spec.md');

  assert.match(dashboard, /fn validate_mcp_standard_headers\(/);
  assert.match(dashboard, /fn mcp_standard_header_name/);
  assert.match(dashboard, /request_header_string\(Some\(request\), "mcp-method"\)/);
  assert.match(dashboard, /request_header_string\(Some\(request\), "mcp-name"\)/);
  assert.match(dashboard, /Mcp-Method header/);
  assert.match(dashboard, /Mcp-Name header/);
  assert.match(read('src/mcp_protocol.rs'), /ERROR_HEADER_MISMATCH:\s*i64\s*=\s*-32001/);
  assert.match(dashboard, /mismatched Mcp-Method response/);
  assert.match(dashboard, /mismatched Mcp-Name response/);
  assert.match(spec, /Mcp-Method/);
  assert.match(spec, /Mcp-Name/);
  assert.match(spec, /header\/body request smuggling/i);
});


test('child process test helpers do not pass registry credentials or sandbox secrets by default', () => {
  const env = cleanChildEnv();

  assert.equal(env.NPM_CONFIG_REGISTRY, undefined);
  assert.equal(env.PIP_INDEX_URL, undefined);
  assert.equal(env.UV_INDEX_URL, undefined);
  assert.equal(env.CAAS_ARTIFACTORY_READER_PASSWORD, undefined);
  assert.equal(env.NODE_TEST_CONTEXT, undefined);
  assert.equal(env.CI, undefined);
  assert.equal(env.PATH || env.Path, process.env.PATH || process.env.Path);
});


test('source proof child-process runners use sanitized environment helpers', () => {
  // Arrange
  const helper = read('scripts/lib/safe-child-env.mjs');
  const files = [
    'scripts/archive-release.mjs',
    'scripts/proof-report.mjs',
    'scripts/run-rust-tests.mjs',
    'scripts/verify-rust-quality.mjs',
    'scripts/verify-npm-pack.mjs',
    'scripts/verify-platform-packages.mjs',
    'scripts/publish-npm-artifacts.mjs'
  ];

  // Act / Assert
  assert.match(helper, /SAFE_CHILD_ENV_KEYS/);
  assert.doesNotMatch(helper, /NPM_CONFIG_REGISTRY/);
  for (const file of files) {
    const source = read(file);
    assert.match(source, /safe-child-env\.mjs/, `${file} should import the shared safe child env helper`);
    assert.doesNotMatch(source, /env:\s*\{\s*\.\.\.process\.env\s*\}/, `${file} should not pass the full parent environment to child processes`);
  }
});



test('HTTP MCP session ids are visible-ASCII bounded, generated from OS randomness, and backed by a stateful store', () => {
  const dashboard = [
    read('src/dashboard.rs'),
    read('src/dashboard/http_boundary.rs'),
    read('src/dashboard/http_headers.rs'),
    read('src/dashboard/http_session.rs'),
    read('src/dashboard/http_tools.rs'),
    read('src/dashboard/mcp_http.rs'),
    read('src/dashboard/tool_runtime.rs'),
    read('src/dashboard/tests.rs'),
    read('src/dashboard/index.html'),
  ].join('\n');

  assert.match(dashboard, /fn normalize_mcp_http_session_id\(/);
  assert.match(dashboard, /0x21\.\.=0x7e/);
  assert.match(dashboard, /resources::MAX_HTTP_HEADER_LINE_BYTES/);
  assert.match(dashboard, /fn os_random_hex\(/);
  assert.match(dashboard, /getrandom::getrandom/);
  assert.match(dashboard, /mcpace-fallback-/);
  assert.match(dashboard, /struct McpHttpSessionStore/);
  assert.match(dashboard, /create_or_replace\(/);
  assert.match(dashboard, /touch_from_request\(/);
  assert.match(dashboard, /close_from_request\(/);
  assert.match(dashboard, /McpHttpSessionErrorKind::Unknown \| McpHttpSessionErrorKind::Expired => "404 Not Found"/);
  assert.match(dashboard, /missing required Mcp-Session-Id header after initialize/);
  assert.match(read('docs/mcp-http-api-spec.md'), /unknown, expired, or already-closed `Mcp-Session-Id`/);
});
