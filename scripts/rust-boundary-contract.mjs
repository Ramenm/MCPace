#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { repoRoot as defaultRepoRoot } from "./lib/project-metadata.mjs";

const STRINGLY_ERROR_MAX = 16;
const RAW_HTTP_ALLOWLIST = Object.freeze([
	"src/dashboard.rs",
	"src/dashboard/mcp_http.rs",
	"src/dashboard/response.rs",
	"src/http_probe.rs",
]);

const TYPED_BOUNDARY_MODULES = Object.freeze([
	{ file: "src/init.rs", errorType: "InitError" },
	{ file: "src/projects.rs", errorType: "ProjectRegistryError" },
	{ file: "src/profile.rs", errorType: "RuntimeProfileError" },
	{ file: "src/mcp_sources.rs", errorType: "McpSourceError" },
	{ file: "src/hub/runtime.rs", errorType: "HubRuntimeError" },
	{ file: "src/upstream/tool_cache.rs", errorType: "ToolListCacheError" },
	{ file: "src/upstream/stdio_runtime.rs", errorType: "StdioRuntimeError" },
	{ file: "src/server/policy.rs", errorType: "ServerPolicyError" },
	{ file: "src/upstream/inventory.rs", errorType: "UpstreamInventoryError" },
	{
		file: "src/upstream/session_pool.rs",
		errorType: "UpstreamSessionPoolError",
	},
]);

function parseArgs(argv) {
	const args = { json: false, repoRoot: defaultRepoRoot };
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (arg === "--json") args.json = true;
		else if (arg === "--repo") args.repoRoot = path.resolve(argv[++index]);
		else if (arg === "--help" || arg === "-h") {
			console.log(
				"Usage: node scripts/rust-boundary-contract.mjs [--json] [--repo DIR]",
			);
			process.exit(0);
		} else {
			throw new Error(`unknown argument: ${arg}`);
		}
	}
	return args;
}

function normalize(relativePath) {
	return relativePath.split(path.sep).join("/");
}

function read(repoRoot, relativePath) {
	return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function productionRustSource(source) {
	return source.replace(/#\[cfg\(test\)\]\s*mod\s+tests\s*\{[\s\S]*$/m, "");
}

function resultStringErrorSpans(source) {
	const spans = [];
	const text = productionRustSource(source);
	let index = 0;
	while ((index = text.indexOf("Result<", index)) !== -1) {
		const start = index;
		index += "Result<".length;
		let depth = 1;
		let cursor = index;
		let commaAt = -1;
		while (cursor < text.length && depth > 0) {
			const ch = text[cursor];
			if (ch === "<") depth += 1;
			else if (ch === ">") depth -= 1;
			else if (ch === "," && depth === 1 && commaAt === -1) commaAt = cursor;
			if (depth === 0) break;
			cursor += 1;
		}
		if (depth === 0 && commaAt !== -1) {
			const errorType = text.slice(commaAt + 1, cursor).trim();
			if (errorType === "String") spans.push({ start, end: cursor + 1 });
		}
		index = Math.max(cursor + 1, index + 1);
	}
	return spans;
}

function loadInventory(repoRoot) {
	const result = spawnSync(
		process.execPath,
		["scripts/modernization-inventory.mjs", "--json"],
		{
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
			maxBuffer: 16 * 1024 * 1024,
		},
	);
	if (result.status !== 0) {
		throw new Error(
			`modernization inventory failed: ${result.stderr || result.stdout}`,
		);
	}
	return JSON.parse(result.stdout);
}

function finding(id, status, detail, extra = {}) {
	return { id, status, detail, ...extra };
}

function hasAll(source, patterns) {
	return patterns.every((pattern) => pattern.test(source));
}

function hasRustEnum(source, errorType) {
	return (
		source.includes(`enum ${errorType}`) ||
		source.includes(`pub enum ${errorType}`) ||
		source.includes(`pub(crate) enum ${errorType}`) ||
		source.includes(`pub(super) enum ${errorType}`)
	);
}

function hasTraitImpl(source, traitName, errorType) {
	return source.includes(`impl ${traitName} for ${errorType}`);
}

function hasStringConversion(source, errorType) {
	return source.includes(`impl From<${errorType}> for String`);
}

function run(repoRoot) {
	const inventory = loadInventory(repoRoot);
	const byId = new Map(inventory.findings.map((item) => [item.id, item]));
	const findings = [];

	const stringly = byId.get("stringly-errors");
	const stringlyCount = stringly?.count ?? 0;
	findings.push(
		finding(
			"stringly-error-budget-tightened",
			stringlyCount <= STRINGLY_ERROR_MAX ? "pass" : "fail",
			`stringly-errors=${stringlyCount}, max=${STRINGLY_ERROR_MAX}`,
			{ actual: stringlyCount, max: STRINGLY_ERROR_MAX },
		),
	);

	for (const module of TYPED_BOUNDARY_MODULES) {
		const source = read(repoRoot, module.file);
		const spans = resultStringErrorSpans(source);
		const hasTypedError =
			hasRustEnum(source, module.errorType) &&
			hasTraitImpl(source, "fmt::Display", module.errorType) &&
			hasTraitImpl(source, "std::error::Error", module.errorType) &&
			hasStringConversion(source, module.errorType);
		findings.push(
			finding(
				`typed-boundary:${module.file}`,
				spans.length === 0 && hasTypedError ? "pass" : "fail",
				spans.length === 0 && hasTypedError
					? `${module.errorType} owns the module boundary`
					: `${module.file} has ${spans.length} Result<_, String> spans or missing ${module.errorType} traits`,
				{
					file: module.file,
					errorType: module.errorType,
					stringResultSpans: spans.length,
				},
			),
		);
	}

	const rawHttp = byId.get("raw-http-tcp");
	const rawHttpFiles = (rawHttp?.files ?? []).map(normalize).sort();
	const allowed = [...RAW_HTTP_ALLOWLIST].sort();
	const unexpected = rawHttpFiles.filter((file) => !allowed.includes(file));
	const missing = allowed.filter((file) => !rawHttpFiles.includes(file));
	findings.push(
		finding(
			"raw-http-tcp-allowlist",
			unexpected.length === 0 && missing.length === 0 ? "pass" : "fail",
			`raw HTTP/TCP files=${rawHttpFiles.length}, unexpected=${unexpected.length}, missing=${missing.length}`,
			{ files: rawHttpFiles, allowed, unexpected, missing },
		),
	);

	const httpProbe = read(repoRoot, "src/http_probe.rs");
	findings.push(
		finding(
			"http-probe-contract",
			hasAll(httpProbe, [
				/connect_timeout\s*\(/,
				/Content-Length/i,
				/decode_chunked_body/,
				/text\/event-stream/,
				/max_response_bytes/,
			])
				? "pass"
				: "fail",
			"shared http_probe keeps bounded TCP read, content length, chunked, and SSE handling centralized",
		),
	);

	const stdioRuntime = read(repoRoot, "src/upstream/stdio_runtime.rs");
	findings.push(
		finding(
			"stdio-jsonrpc-newline-boundary",
			/fn run_stdin_writer[\s\S]*write_all\(&request\.payload\)[\s\S]*stdin\.flush\(\)/.test(
				stdioRuntime,
			) &&
				/fn write_jsonrpc_interruption[\s\S]*deadline/.test(stdioRuntime) &&
				/to_compact_string\(\)\.into_bytes\(\)[\s\S]*payload\.push\(b'\\n'\)/.test(
					stdioRuntime,
				) &&
				/StdioRuntimeError/.test(stdioRuntime) &&
				!/Result<[^>]*,\s*String\s*>/.test(productionRustSource(stdioRuntime))
				? "pass"
				: "fail",
			"upstream stdio writer remains newline-framed JSON-RPC with a typed runtime error seam",
		),
	);

	const sourceRegistry = read(repoRoot, "src/mcp_sources.rs");
	findings.push(
		finding(
			"mcp-source-symlink-boundary",
			/symlink_metadata/.test(sourceRegistry) &&
				/McpSourceError::UnsafeSource/.test(sourceRegistry) &&
				/is a symlink/.test(sourceRegistry)
				? "pass"
				: "fail",
			"MCP source registry rejects unsafe symlink settings sources through a typed error seam",
		),
	);

	const failures = findings.filter((item) => item.status === "fail");
	return {
		schema: "mcpace.rustBoundaryContract.v1",
		generatedAt: new Date().toISOString(),
		repoRoot: ".",
		status: failures.length === 0 ? "pass" : "fail",
		failures: failures.length,
		summary: {
			findings: findings.length,
			pass: findings.filter((item) => item.status === "pass").length,
			fail: failures.length,
		},
		modernizationInventory: {
			stringlyErrors: stringlyCount,
			rawHttpTcp: rawHttp?.count ?? 0,
			cargoLockNeedsRefresh: byId.get("cargo-lock-needs-refresh")?.count ?? 0,
		},
		findings,
	};
}

try {
	const args = parseArgs(process.argv.slice(2));
	const report = run(args.repoRoot);
	if (args.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
	else {
		console.log(
			`${report.status}: ${report.summary.findings} Rust boundary contracts, ${report.failures} failures`,
		);
		for (const item of report.findings)
			console.log(`- ${item.status}: ${item.id} — ${item.detail}`);
	}
	process.exitCode = report.failures === 0 ? 0 : 1;
} catch (error) {
	console.error(error?.stack || String(error));
	process.exitCode = 1;
}
