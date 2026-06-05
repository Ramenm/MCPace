import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

const source = read('src/upstream/http_runtime.rs');
const setup = read('src/setup.rs');
const textUtils = read('src/text_utils.rs');

test('HTTP upstream URL parser preserves Host header authority and rejects injection primitives', () => {
  assert.match(source, /host_header:\s*String/, 'ParsedHttpUrl must store a sanitized Host header value');
  assert.match(source, /Host:\s*\{\}/, 'HTTP request must format the Host header from ParsedHttpUrl');
  assert.match(source, /target\.host_header/, 'HTTP request must not rebuild Host from the socket host only');
  assert.match(source, /url\.chars\(\)\.any\(\|ch\| ch\.is_control\(\) \|\| ch\.is_whitespace\(\)\)/, 'URL input must reject control and whitespace characters before request formatting');
  assert.match(source, /authority\.contains\('@'\)/, 'userinfo-style authorities must be rejected instead of reinterpreted');
  assert.match(source, /IPv6 HTTP upstream authorities must be bracketed/, 'raw IPv6 authorities must be rejected');
  assert.doesNotMatch(source, /and_then\([^\n]+parse::<u16>[^\n]+\)\s*\.unwrap_or\(80\)/s, 'invalid explicit ports must not silently downgrade to port 80');
});

test('all MCP HTTP probes forward only syntactically safe session headers', () => {
  assert.match(textUtils, /fn valid_http_header_value\(value: &str\) -> bool/, 'shared outbound header value validator is missing');
  assert.match(textUtils, /0x21\.\.=0x7e/, 'header validator should allow only bounded visible ASCII without spaces or controls');
  assert.match(source, /text_utils::valid_http_header_value\(session_id\)/, 'runtime Mcp-Session-Id must be validated before forwarding');
  assert.match(setup, /text_utils::valid_http_header_value\(value\)/, 'setup probe Mcp-Session-Id must be validated before forwarding');
  assert.match(source, /session\\r\\nInjected: bad/, 'Rust regression test must cover CRLF injection in upstream session forwarding');
});

test('dashboard MCP headers reject duplicates before protocol or session routing', () => {
  const boundary = read('src/dashboard/http_boundary.rs');
  const headers = read('src/dashboard/http_headers.rs');
  const session = read('src/dashboard/http_session.rs');
  const mcpHttp = read('src/dashboard/mcp_http.rs');

  assert.match(boundary, /fn request_header_string_unique/);
  assert.match(boundary, /matches\.next\(\)\.is_some\(\)/);
  assert.match(boundary, /multiple \{\} headers are not allowed for MCP HTTP requests/);

  assert.match(headers, /request_header_string_unique\(Some\(request\), "mcp-method"\)\?/);
  assert.match(headers, /request_header_string_unique\(Some\(request\), "mcp-name"\)\?/);
  assert.match(session, /request_header_string_unique\(Some\(request\), "mcp-session-id"\)/);
  assert.match(session, /request_header_string_unique\(Some\(request\), "mcp-protocol-version"\)/);
  assert.match(mcpHttp, /request_header_string_unique\(Some\(request\), "mcp-protocol-version"\)/);
  assert.doesNotMatch(headers, /request_header_string\(Some\(request\), "mcp-/);
  assert.doesNotMatch(session, /request_header_string\(Some\(request\), "mcp-/);
});

test('HTTP upstream bridge handles Streamable HTTP JSON and SSE without waiting for socket EOF', () => {
  assert.match(source, /Accept: application\/json, text\/event-stream/);
  assert.match(source, /fn read_http_response/);
  assert.match(source, /http_response_ready/);
  assert.match(source, /text\/event-stream/);
  assert.match(source, /sse_json_body/);
  assert.match(source, /json_response_id_matches/);
  assert.match(source, /decode_chunked_body/);
  assert.doesNotMatch(source, /read_to_string\(&mut raw_response\)/, 'HTTP upstream must not wait for EOF on SSE responses');
  assert.match(source, /collect::<Vec<_>>\(\)/, 'HTTP upstream should try all resolved addresses, not only the first localhost address');
});
