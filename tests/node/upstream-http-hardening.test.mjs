import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { repoRoot } from "../../scripts/lib/project-metadata.mjs";

function read(relativePath) {
	return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

const source = read("src/upstream/http_runtime.rs");
const sourceTests = read("src/upstream/http_runtime/tests.rs");
const httpProbe = read("src/http_probe.rs");
const setup = read("src/setup.rs");
const textUtils = read("src/text_utils.rs");

test("HTTP upstream URL parser preserves Host header authority and rejects injection primitives", () => {
	assert.match(
		source,
		/host_header:\s*String/,
		"ParsedHttpUrl must store a sanitized Host header value",
	);
	assert.match(
		source,
		/Host:\s*\{\}/,
		"HTTP request must format the Host header from ParsedHttpUrl",
	);
	assert.match(
		source,
		/target\.host_header/,
		"HTTP request must not rebuild Host from the socket host only",
	);
	assert.match(
		source,
		/url\.chars\(\)\.any\(\|ch\| ch\.is_control\(\) \|\| ch\.is_whitespace\(\)\)/,
		"URL input must reject control and whitespace characters before request formatting",
	);
	assert.match(
		source,
		/authority\.contains\('@'\)/,
		"userinfo-style authorities must be rejected instead of reinterpreted",
	);
	assert.match(
		source,
		/IPv6 HTTP upstream authorities must be bracketed/,
		"raw IPv6 authorities must be rejected",
	);
	assert.doesNotMatch(
		source,
		/and_then\([^\n]+parse::<u16>[^\n]+\)\s*\.unwrap_or\(80\)/s,
		"invalid explicit ports must not silently downgrade to port 80",
	);
});

test("all MCP HTTP probes forward only syntactically safe session headers", () => {
	assert.match(
		textUtils,
		/fn valid_http_header_value\(value: &str\) -> bool/,
		"shared outbound header value validator is missing",
	);
	assert.match(
		textUtils,
		/0x21\.\.=0x7e/,
		"header validator should allow only bounded visible ASCII without spaces or controls",
	);
	assert.match(
		source,
		/text_utils::valid_http_header_value\(session_id\)/,
		"runtime Mcp-Session-Id must be validated before forwarding",
	);
	assert.match(
		setup,
		/text_utils::valid_http_header_value\(value\)/,
		"setup probe Mcp-Session-Id must be validated before forwarding",
	);
	assert.match(
		sourceTests,
		/session\\r\\nInjected: bad/,
		"Rust regression test must cover CRLF injection in upstream session forwarding",
	);
});

test("remote Streamable HTTP uses native TLS, bounded bodies, auth headers, and one-session batches", () => {
	const cargo = read("Cargo.toml");
	const serverConfig = read("src/upstream/server_config.rs");
	const leaseRuntime = read("src/upstream/lease_runtime.rs");
	const httpClient = read("src/http_client.rs");

	assert.match(cargo, /ureq\s*=.*"rustls".*"platform-verifier"/);
	assert.match(httpClient, /RootCerts::PlatformVerifier/);
	assert.match(httpClient, /max_redirects\(0\)/);
	assert.match(source, /fn post_json_https\(/);
	assert.match(source, /validate_configured_headers\(headers\)/);
	assert.match(source, /reserved_mcp_http_header_name/);
	assert.match(source, /fn terminate_http_session\(/);
	assert.match(source, /fn run_http_tool_calls\(/);
	assert.match(serverConfig, /object_at_path\(raw, &\["headers"\]\)/);
	assert.match(
		leaseRuntime,
		/remaining_upstream_timeout\(upstream_deadline, server_name, "tools\/call batch"\)/,
	);
	assert.match(
		leaseRuntime,
		/run_http_tool_calls\(server, calls, call_timeout\)/,
	);
});

test("dashboard MCP headers reject duplicates before protocol or session routing", () => {
	const boundary = read("src/dashboard/http_boundary.rs");
	const headers = read("src/dashboard/http_headers.rs");
	const session = read("src/dashboard/http_session.rs");
	const mcpHttp = read("src/dashboard/mcp_http.rs");

	assert.match(boundary, /fn request_header_string_unique/);
	assert.match(boundary, /matches\.next\(\)\.is_some\(\)/);
	assert.match(
		boundary,
		/multiple \{\} headers are not allowed for MCP HTTP requests/,
	);

	assert.match(
		headers,
		/request_header_string_unique\(Some\(request\), "mcp-method"\)\?/,
	);
	assert.match(
		headers,
		/request_header_string_unique\(Some\(request\), "mcp-name"\)\?/,
	);
	assert.match(
		session,
		/request_header_string_unique\(Some\(request\), "mcp-session-id"\)/,
	);
	assert.match(
		session,
		/request_header_string_unique\(Some\(request\), "mcp-protocol-version"\)/,
	);
	assert.match(
		mcpHttp,
		/request_header_string_unique\(Some\(request\), "mcp-protocol-version"\)/,
	);
	assert.doesNotMatch(headers, /request_header_string\(Some\(request\), "mcp-/);
	assert.doesNotMatch(session, /request_header_string\(Some\(request\), "mcp-/);
});

test("HTTP upstream bridge reuses the shared bounded HTTP probe for JSON and SSE", () => {
	assert.match(source, /Accept: application\/json, text\/event-stream/);
	assert.match(source, /http_probe::raw_jsonrpc_response/);
	assert.match(source, /http_probe::parse_response/);
	assert.match(source, /http_probe::sse_json_rpc_body/);
	assert.doesNotMatch(source, /fn read_http_response/);
	assert.doesNotMatch(
		source,
		/TcpStream|connect_timeout|to_socket_addrs/,
		"upstream runtime should not own raw TCP transport code",
	);

	assert.match(httpProbe, /pub\(crate\) fn raw_jsonrpc_response/);
	assert.match(httpProbe, /mcp_json_rpc_response_ready\(raw, expected_id\)/);
	assert.match(httpProbe, /text\/event-stream/);
	assert.match(httpProbe, /sse_json_rpc_body/);
	assert.match(httpProbe, /json_response_id_matches/);
	assert.match(httpProbe, /decode_chunked_body/);
	assert.doesNotMatch(
		httpProbe,
		/read_to_string\(&mut raw_response\)/,
		"HTTP probe must not wait for EOF on SSE responses",
	);
	assert.match(
		httpProbe,
		/collect::<Vec<_>>\(\)/,
		"HTTP probe should try all resolved addresses, not only the first localhost address",
	);
});
