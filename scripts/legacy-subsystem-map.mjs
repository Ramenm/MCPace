#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { repoRoot as defaultRepoRoot } from "./lib/project-metadata.mjs";
import { cargoLockRefreshFindings } from "./lib/cargo-policy.mjs";

const SKIP_DIRS = new Set([
	".git",
	"node_modules",
	"target",
	"dist",
	".artifacts",
]);

function parseArgs(argv) {
	const args = { json: false, repoRoot: defaultRepoRoot };
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (arg === "--json") args.json = true;
		else if (arg === "--repo") args.repoRoot = path.resolve(argv[++index]);
		else if (arg === "--help" || arg === "-h") {
			console.log(
				"Usage: node scripts/legacy-subsystem-map.mjs [--json] [--repo DIR]",
			);
			process.exit(0);
		} else {
			throw new Error(`unknown argument: ${arg}`);
		}
	}
	return args;
}

function normalize(value) {
	return value.split(path.sep).join("/");
}

function walkFiles(root, predicate = () => true) {
	const files = [];
	const stack = [root];
	while (stack.length > 0) {
		const current = stack.pop();
		if (!fs.existsSync(current)) continue;
		for (const entry of fs
			.readdirSync(current, { withFileTypes: true })
			.sort((left, right) => left.name.localeCompare(right.name))) {
			const full = path.join(current, entry.name);
			const relative = normalize(path.relative(root, full));
			if (entry.isDirectory()) {
				if (
					!SKIP_DIRS.has(entry.name) &&
					!relative.split("/").some((part) => SKIP_DIRS.has(part))
				)
					stack.push(full);
			} else if (entry.isFile() && predicate(full)) {
				files.push(full);
			}
		}
	}
	return files.sort();
}

function readText(file) {
	return fs.existsSync(file) ? fs.readFileSync(file, "utf8") : "";
}

function productionRustSource(source) {
	return source.replace(/#\[cfg\(test\)\]\s*mod\s+tests\s*\{[\s\S]*$/m, "");
}

function isStandaloneRustTestFile(relative) {
	return (
		relative.endsWith("/tests.rs") ||
		relative.endsWith("_test.rs") ||
		relative.endsWith("_tests.rs")
	);
}

function rel(repoRoot, files) {
	return files.map((file) => normalize(path.relative(repoRoot, file))).sort();
}

function grep(repoRoot, files, pattern) {
	return rel(
		repoRoot,
		files.filter((file) => pattern.test(readText(file))),
	);
}

function grepBy(repoRoot, files, predicate) {
	return rel(
		repoRoot,
		files.filter((file) =>
			predicate(file, readText(file), normalize(path.relative(repoRoot, file))),
		),
	);
}

function rawHttpTcpFiles(repoRoot, files) {
	return grepBy(repoRoot, files, (_file, source, relative) => {
		if (isStandaloneRustTestFile(relative)) return false;
		if (relative === "src/http_probe.rs") return true;
		return /\bTcpListener\b|\bTcpStream\b|\bconnect_timeout\s*\(|\bto_socket_addrs\s*\(|\bread_to_end\s*\(/.test(
			productionRustSource(source),
		);
	});
}

function adHocDiagnosticFiles(repoRoot, files) {
	return grepBy(repoRoot, files, (_file, source, relative) => {
		if (relative === "src/diagnostics.rs" || isStandaloneRustTestFile(relative))
			return false;
		return /\beprintln!\s*\(|\bprintln!\s*\(|\bwriteln!\s*\(\s*stderr\b/.test(
			source,
		);
	});
}

function lineCount(file) {
	const content = readText(file).replace(/\r\n/g, "\n").replace(/\n$/, "");
	return content.length === 0 ? 0 : content.split("\n").length;
}

function dashboardFrontendChunks(repoRoot) {
	return walkFiles(path.join(repoRoot, "src/dashboard/frontend"), (file) => {
		const name = path.basename(file);
		return (
			name.startsWith("app") &&
			name.endsWith(".js") &&
			!name.endsWith(".min.js")
		);
	}).map((file) => ({
		file: normalize(path.relative(repoRoot, file)),
		lines: lineCount(file),
	}));
}

function finding({
	id,
	subsystem,
	status,
	title,
	evidence = [],
	replacement,
	next,
}) {
	return {
		id,
		subsystem,
		status,
		title,
		evidence: evidence.slice(0, 40),
		evidenceCount: evidence.length,
		truncated: evidence.length > 40,
		replacement,
		next,
	};
}

function run() {
	const args = parseArgs(process.argv.slice(2));
	const repoRoot = args.repoRoot;
	const srcRoot = path.join(repoRoot, "src");
	const scriptsRoot = path.join(repoRoot, "scripts");
	const rustFiles = walkFiles(srcRoot, (file) => file.endsWith(".rs"));
	const jsFiles = walkFiles(scriptsRoot, (file) => file.endsWith(".mjs"));
	const evalPartials = walkFiles(path.join(repoRoot, "eval"), (file) =>
		file.endsWith(".partial.jsonl"),
	);
	const cargoToml = readText(path.join(repoRoot, "Cargo.toml"));
	const compatDeps = [
		...cargoToml.matchAll(
			/^\s*([A-Za-z0-9_-]+)\s*=\s*\{\s*path\s*=\s*"crates\/compat\//gm,
		),
	].map((match) => match[1]);
	const cargoLockIssues = cargoLockRefreshFindings(repoRoot);

	const manualCli = grep(
		repoRoot,
		rustFiles,
		/parse_args|while\s+index\s*<\s*args\.len\(\)/,
	);
	const configPatchers = grep(
		repoRoot,
		rustFiles,
		/upsert_toml|find_toml|parse_yaml|upsert_yaml/,
	);
	const rawHttp = rawHttpTcpFiles(repoRoot, rustFiles);
	const adHocLogging = adHocDiagnosticFiles(repoRoot, rustFiles);
	const zipWriter = fs.existsSync(
		path.join(repoRoot, "scripts/lib/zip-writer.mjs"),
	)
		? ["scripts/lib/zip-writer.mjs"]
		: [];
	const frontendChunks = dashboardFrontendChunks(repoRoot);
	const maxFrontendChunkLines = frontendChunks.reduce(
		(max, chunk) => Math.max(max, chunk.lines),
		0,
	);
	const stdioShim = readText(path.join(repoRoot, "src/stdio_shim.rs"));

	const findings = [
		finding({
			id: "dependencies.compat-crates",
			subsystem: "dependencies",
			status: compatDeps.length === 0 ? "done" : "blocked",
			title: "Local compatibility crates no longer shadow standard crates",
			evidence: compatDeps,
			replacement: "upstream crates.io dependencies",
			next:
				compatDeps.length === 0
					? "Keep dependency-policy guard enabled."
					: "Replace path dependencies under crates/compat with upstream crates.",
		}),
		finding({
			id: "dependencies.cargo-lock-refresh",
			subsystem: "dependencies",
			status: cargoLockIssues.length === 0 ? "done" : "blocked",
			title: "Cargo.lock must be refreshed after upstream dependency migration",
			evidence: cargoLockIssues.map(
				(item) =>
					`${item.crate}: locked=${item.lock ?? "<missing>"}, required=${item.dependency}`,
			),
			replacement:
				"reviewed package-specific lockfile update on the pinned Rust toolchain",
			next:
				cargoLockIssues.length === 0
					? "Keep strict release gate enabled."
					: "Update only the intended package, review Cargo.lock, then run every locked check.",
		}),
		finding({
			id: "cli.manual-argv",
			subsystem: "cli",
			status: manualCli.length === 0 ? "done" : "open",
			title: "Command parsing still has handwritten argv scanners",
			evidence: manualCli,
			replacement: "clap derive",
			next: "Keep all public command parsing on clap derive; serve now keeps a typed passthrough contract for dashboard-safe flags.",
		}),
		finding({
			id: "config.lossless-editing",
			subsystem: "client-config",
			status: configPatchers.length === 0 ? "done" : "open",
			title:
				configPatchers.length === 0
					? "Client config editing is centralized behind a typed edit boundary"
					: "Client config editing still has scattered hand-written TOML/YAML upserts",
			evidence: configPatchers,
			replacement:
				"toml_edit for TOML; narrow maintained YAML handling only where required",
			next:
				configPatchers.length === 0
					? "Keep src/config_edit.rs as the single mutation boundary; replace TOML internals with toml_edit after Cargo.lock refresh is possible."
					: "Start with TOML targets because lossless comment/order preservation is well supported.",
		}),
		finding({
			id: "mcp.stdio-preview",
			subsystem: "mcp-runtime",
			status: /Live MCP stdio message forwarding is not implemented yet/.test(
				stdioShim,
			)
				? "open"
				: "done",
			title: /Live MCP stdio message forwarding is not implemented yet/.test(
				stdioShim,
			)
				? "Public mcpace stdio command exists but live forwarding remains preview"
				: "Public mcpace stdio command delegates to the live JSON-RPC server",
			evidence: ["src/stdio_shim.rs"],
			replacement: "real mcpace stdio launcher/proxy, then rmcp spike",
			next: /Live MCP stdio message forwarding is not implemented yet/.test(
				stdioShim,
			)
				? "Implement stdout-only JSON-RPC forwarding and keep diagnostic logs on stderr/file."
				: "Keep stdio-shim as a compatibility alias and route new client exports through mcpace stdio.",
		}),
		finding({
			id: "http.raw-tcp",
			subsystem: "networking",
			status: rawHttp.length === 0 ? "done" : "open",
			title: "Some HTTP/TCP handling is still implemented directly",
			evidence: rawHttp,
			replacement:
				"ureq/reqwest for outbound HTTP; later axum+tower-http under security tests",
			next: "Replace upstream outbound HTTP before dashboard server migration.",
		}),
		finding({
			id: "observability.ad-hoc-logging",
			subsystem: "observability",
			status: adHocLogging.length === 0 ? "done" : "open",
			title:
				"Runtime diagnostics still have direct stdout/stderr macro call sites",
			evidence: adHocLogging,
			replacement:
				"tracing + tracing-subscriber; protocol-safe stderr helper until tracing lands",
			next: "Continue replacing direct stderr diagnostics in agent/serve/dashboard; stdio now uses a protocol-safe stderr helper.",
		}),
		finding({
			id: "frontend.large-module",
			subsystem: "dashboard",
			status: maxFrontendChunkLines > 2000 ? "open" : "done",
			title:
				"Dashboard frontend JavaScript is split into bounded source chunks",
			evidence: frontendChunks.map(
				(chunk) => `${chunk.file}:${chunk.lines} lines`,
			),
			replacement:
				"small plain JS chunks now; Vite + TypeScript modules, framework only if needed later",
			next:
				maxFrontendChunkLines > 2000
					? "Keep splitting API/client/state/render chunks before considering a UI framework."
					: "Keep dashboard JS chunks below the 2000-line budget while preserving route and DOM contract tests.",
		}),
		finding({
			id: "release.zip-writer",
			subsystem: "release-engineering",
			status: zipWriter.length === 0 ? "done" : "open",
			title: "Release tooling still owns a ZIP writer implementation",
			evidence: zipWriter,
			replacement:
				"zip crate or checked npm ZIP library; cargo-packager/cargo-dist spike for installer graph",
			next: "Keep current tests as contract, then replace ZIP serialization with a maintained library.",
		}),
		finding({
			id: "source.generated-partials",
			subsystem: "source-hygiene",
			status: evalPartials.length === 0 ? "done" : "blocked",
			title:
				"Checked-in eval partial streams are removed from the clean source tree",
			evidence: rel(repoRoot, evalPartials),
			replacement:
				"final JSON/CSV fixtures only; partial streams are runtime artifacts",
			next:
				evalPartials.length === 0
					? "Keep source-archive policy pattern enabled."
					: "Delete *.partial.jsonl or move to ignored runtime output.",
		}),
	];

	const report = {
		schema: "mcpace.legacySubsystemMap.v1",
		generatedAt: new Date().toISOString(),
		repoRoot: ".",
		rustFiles: rustFiles.length,
		scriptFiles: jsFiles.length,
		summary: {
			total: findings.length,
			done: findings.filter((item) => item.status === "done").length,
			open: findings.filter((item) => item.status === "open").length,
			blocked: findings.filter((item) => item.status === "blocked").length,
		},
		findings,
	};

	if (args.json) console.log(JSON.stringify(report, null, 2));
	else {
		console.log(
			`${report.summary.done}/${report.summary.total} subsystem modernization items done; ${report.summary.blocked} blocked`,
		);
		for (const item of findings)
			console.log(`- ${item.status}: ${item.id} — ${item.title}`);
	}
}

try {
	run();
} catch (error) {
	console.error(error?.stack ?? String(error));
	process.exitCode = 1;
}
