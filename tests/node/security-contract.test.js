const test = require('node:test');
const assert = require('node:assert/strict');
const { read } = require('./helpers');

test('upstream stderr diagnostics are bounded and redact likely secrets before surfacing errors', () => {
  const upstream = read('src/upstream.rs');

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
