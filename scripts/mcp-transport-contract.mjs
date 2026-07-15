#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { repoRoot as defaultRepoRoot } from "./lib/project-metadata.mjs";

function parseArgs(argv) {
	const args = { json: false, repoRoot: defaultRepoRoot };
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (arg === "--json") args.json = true;
		else if (arg === "--repo") args.repoRoot = path.resolve(argv[++index]);
		else if (arg === "--help" || arg === "-h") {
			console.log(
				"Usage: node scripts/mcp-transport-contract.mjs [--json] [--repo DIR]",
			);
			process.exit(0);
		} else {
			throw new Error(`unknown argument: ${arg}`);
		}
	}
	return args;
}

function read(repoRoot, relativePath) {
	return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function sourceWithoutTests(source) {
	return source.replace(/#\[cfg\(test\)\]\s*mod\s+tests\s*\{[\s\S]*$/m, "");
}

function finding(id, status, detail, evidence = {}) {
	return { id, status, detail, ...evidence };
}

function expectMatch(id, file, source, pattern, detail) {
	return finding(id, pattern.test(source) ? "pass" : "fail", detail, { file });
}

function expectNoMatch(id, file, source, pattern, detail) {
	return finding(id, pattern.test(source) ? "fail" : "pass", detail, { file });
}

function run(repoRoot) {
	const protocol = read(repoRoot, "src/mcp_protocol.rs");
	const stdio = read(repoRoot, "src/mcp_server.rs");
	const stdioRuntime = read(repoRoot, "src/upstream/stdio_runtime.rs");
	const stdioShim = read(repoRoot, "src/stdio_shim.rs");
	const httpBoundary = read(repoRoot, "src/dashboard/http_boundary.rs");
	const httpSession = read(repoRoot, "src/dashboard/http_session.rs");
	const mcpHttp = read(repoRoot, "src/dashboard/mcp_http.rs");
	const response = read(repoRoot, "src/dashboard/response.rs");
	const dashboard = read(repoRoot, "src/dashboard.rs");

	const stdioProduction = sourceWithoutTests(stdio);
	const findings = [
		expectMatch(
			"mcp-current-protocol-version-pinned",
			"src/mcp_protocol.rs",
			protocol,
			/pub const CURRENT_PROTOCOL_VERSION:\s*&str\s*=\s*"2025-11-25";/,
			"current MCP protocol version is pinned in one protocol module",
		),
		expectMatch(
			"mcp-protocol-version-compat-window",
			"src/mcp_protocol.rs",
			protocol,
			/SUPPORTED_PROTOCOL_VERSIONS:[\s\S]*"2025-11-25"[\s\S]*"2025-06-18"[\s\S]*"2025-03-26"[\s\S]*"2024-11-05"/,
			"protocol negotiation keeps latest plus supported older versions explicit",
		),
		expectMatch(
			"stdio-shim-delegates-to-live-server",
			"src/stdio_shim.rs",
			stdioShim,
			/mcp_server::run\(&forwarded,\s*default_root,\s*stdout,\s*stderr\)/,
			"public stdio path delegates to the live MCP JSON-RPC server instead of a preview shim",
		),
		expectMatch(
			"stdio-newline-jsonrpc-framing",
			"src/mcp_server.rs",
			stdio,
			/fn write_message\(stdout:[\s\S]*to_compact_string\(\)\.as_bytes\(\)[\s\S]*write_all\(b"\\n"\)[\s\S]*stdout\.flush\(\)/,
			"stdio writes exactly compact JSON-RPC messages followed by one newline frame",
		),
		expectMatch(
			"stdio-reads-newline-delimited-input",
			"src/mcp_server.rs",
			stdio,
			/read_bounded_stdio_line\(&mut input,[\s\S]*fn read_bounded_stdio_line[\s\S]*position\(\|byte\| \*byte == b'\\n'\)[\s\S]*reader\.consume\(take_len\)/,
			"stdio reads bounded newline-delimited JSON-RPC input from stdin",
		),
		expectMatch(
			"stdio-diagnostics-stay-on-stderr",
			"src/mcp_server.rs",
			stdio,
			/diagnostics::stderr_line\(stderr/,
			"stdio diagnostics use the protocol-safe stderr helper",
		),
		expectNoMatch(
			"stdio-production-has-no-ad-hoc-stdout-logging",
			"src/mcp_server.rs",
			stdioProduction,
			/\bprintln!\s*\(|\beprintln!\s*\(|\bwriteln!\s*\(\s*stdout\s*,/,
			"stdio production path has no println/eprintln or ad-hoc stdout writes outside write_message/help boundaries",
		),
		expectMatch(
			"stdio-upstream-forwarder-newline-framing",
			"src/upstream/stdio_runtime.rs",
			stdioRuntime,
			/(?=[\s\S]*fn run_stdin_writer[\s\S]*write_all\(&request\.payload\)[\s\S]*stdin\.flush\(\))(?=[\s\S]*fn write_jsonrpc_interruption[\s\S]*deadline)(?=[\s\S]*pub\(super\) fn write_jsonrpc[\s\S]*to_compact_string\(\)\.into_bytes\(\)[\s\S]*payload\.push\(b'\\n'\))/,
			"upstream stdio bridge forwards JSON-RPC using newline frames",
		),
		expectMatch(
			"streamable-http-post-accept-contract",
			"src/dashboard/mcp_http.rs",
			mcpHttp,
			/accepts_streamable_http_post\(request\)[\s\S]*application\/json[\s\S]*text\/event-stream/,
			"Streamable HTTP POST requires Accept coverage for application/json and text/event-stream",
		),
		expectMatch(
			"streamable-http-post-content-type-contract",
			"src/dashboard/mcp_http.rs",
			mcpHttp,
			/content_type_is\(request,\s*"application\/json"\)/,
			"Streamable HTTP POST requires JSON Content-Type",
		),
		expectMatch(
			"streamable-http-session-lifecycle",
			"src/dashboard/mcp_http.rs",
			mcpHttp,
			/method != "initialize"[\s\S]*notifications\/initialized[\s\S]*prepare_mcp_session_for_request[\s\S]*create_or_replace/,
			"HTTP MCP route enforces initialize/session/initialized lifecycle before normal operations",
		),
		expectMatch(
			"http-host-origin-boundary-centralized",
			"src/dashboard/http_boundary.rs",
			httpBoundary,
			/validate_origin_for_bind[\s\S]*MissingHost[\s\S]*MultipleHost[\s\S]*OriginNotAllowed[\s\S]*is_loopback_host/,
			"local HTTP boundary validates Host, Origin, duplicate headers and loopback hosts centrally",
		),
		expectMatch(
			"dashboard-entrypoint-applies-origin-boundary-once",
			"src/dashboard.rs",
			dashboard,
			/reject_forbidden_origin\(stream, request(?:, config)?\)/,
			"dashboard entrypoint applies Origin/Host rejection before route handlers",
		),
		expectMatch(
			"http-session-ids-use-os-randomness",
			"src/dashboard/http_session.rs",
			httpSession,
			/format!\("mcpace-\{\}"[\s\S]*getrandom::fill\(&mut bytes\)/,
			"HTTP MCP session IDs are generated from OS randomness and normalized as headers",
		),
		expectMatch(
			"dashboard-security-response-headers",
			"src/dashboard/response.rs",
			response,
			/X-Content-Type-Options: nosniff[\s\S]*Referrer-Policy: no-referrer[\s\S]*X-Frame-Options: DENY[\s\S]*Content-Security-Policy:/,
			"dashboard responses carry security headers including CSP and anti-sniffing headers",
		),
	];

	const failures = findings.filter((item) => item.status === "fail");
	return {
		schema: "mcpace.mcpTransportContract.v1",
		generatedAt: new Date().toISOString(),
		status: failures.length === 0 ? "pass" : "fail",
		failures: failures.length,
		findings,
	};
}

try {
	const args = parseArgs(process.argv.slice(2));
	const report = run(args.repoRoot);
	if (args.json) console.log(JSON.stringify(report, null, 2));
	else {
		console.log(
			`${report.status}: ${report.findings.length} MCP transport contract checks, ${report.failures} failures`,
		);
		for (const item of report.findings)
			console.log(`- ${item.status}: ${item.id} — ${item.detail}`);
	}
	process.exitCode = report.failures === 0 ? 0 : 1;
} catch (error) {
	console.error(error?.stack || error);
	process.exitCode = 1;
}
