#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { repoRoot as defaultRepoRoot } from "./lib/project-metadata.mjs";
import {
	cargoLockRefreshFindings,
	cargoLockRefreshMessage,
} from "./lib/cargo-policy.mjs";

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
				"Usage: node scripts/modernization-inventory.mjs [--json] [--repo DIR]",
			);
			process.exit(0);
		} else {
			throw new Error(`unknown argument: ${arg}`);
		}
	}
	return args;
}

function normalize(file) {
	return file.split(path.sep).join("/");
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
			if (entry.isDirectory()) {
				if (!SKIP_DIRS.has(entry.name)) stack.push(full);
			} else if (entry.isFile() && predicate(full)) {
				files.push(full);
			}
		}
	}
	return files.sort();
}

function finding(
	id,
	severity,
	title,
	files,
	recommendation,
	replacement = null,
) {
	return {
		id,
		severity,
		title,
		count: files.length,
		files: files.slice(0, 50),
		truncated: files.length > 50,
		replacement,
		recommendation,
	};
}

function text(file) {
	return fs.readFileSync(file, "utf8");
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

function grep(files, pattern) {
	return files
		.filter((file) => pattern.test(text(file)))
		.map((file) => normalize(path.relative(currentRepoRoot, file)));
}

function grepBy(files, predicate) {
	return files
		.filter((file) =>
			predicate(
				file,
				text(file),
				normalize(path.relative(currentRepoRoot, file)),
			),
		)
		.map((file) => normalize(path.relative(currentRepoRoot, file)));
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
			if (ch === "\n") {
				// Multiline signatures are allowed; keep parsing across the declaration.
			}
			cursor += 1;
		}
		if (depth !== 0 || commaAt === -1) continue;
		const errorType = text.slice(commaAt + 1, cursor).trim();
		if (errorType === "String") spans.push({ start, end: cursor + 1 });
		index = cursor + 1;
	}
	return spans;
}

function hasStringlyErrorResult(source) {
	return resultStringErrorSpans(source).length > 0;
}

function rawHttpTcpFiles(files) {
	return grepBy(files, (file, source, relative) => {
		if (isStandaloneRustTestFile(relative)) return false;
		if (relative === "src/http_probe.rs") return true;
		return /\bTcpListener\b|\bTcpStream\b|\bconnect_timeout\s*\(|\bto_socket_addrs\s*\(|\bread_to_end\s*\(/.test(
			productionRustSource(source),
		);
	});
}

function adHocDiagnosticFiles(files) {
	return grepBy(files, (_file, source, relative) => {
		if (relative === "src/diagnostics.rs" || isStandaloneRustTestFile(relative))
			return false;
		return /\beprintln!\s*\(|\bprintln!\s*\(|\bwriteln!\s*\(\s*stderr\b/.test(
			source,
		);
	});
}

function lineCount(file) {
	const content = text(file).replace(/\r\n/g, "\n").replace(/\n$/, "");
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

let currentRepoRoot = defaultRepoRoot;

function run() {
	const args = parseArgs(process.argv.slice(2));
	currentRepoRoot = args.repoRoot;
	const cargoToml = path.join(args.repoRoot, "Cargo.toml");
	const rustFiles = walkFiles(path.join(args.repoRoot, "src"), (file) =>
		file.endsWith(".rs"),
	);
	const jsFiles = walkFiles(path.join(args.repoRoot, "scripts"), (file) =>
		file.endsWith(".mjs"),
	);
	const testFiles = walkFiles(path.join(args.repoRoot, "tests"), (file) =>
		file.endsWith(".mjs"),
	);
	const allTextFiles = [
		...rustFiles,
		...jsFiles,
		...testFiles,
		cargoToml,
	].filter((file) => fs.existsSync(file));
	const findings = [];

	const cargo = fs.existsSync(cargoToml) ? text(cargoToml) : "";
	const pathDependencies = [
		...cargo.matchAll(/^([A-Za-z0-9_-]+)\s*=\s*\{\s*path\s*=\s*"([^"]+)"/gm),
	].map((match) => ({ crate: match[1], path: match[2] }));
	if (pathDependencies.length > 0) {
		findings.push({
			id: "cargo-path-compat-dependencies",
			severity: "high",
			title:
				"Cargo.toml still uses local compatibility crates that shadow standard crates",
			count: pathDependencies.length,
			crates: pathDependencies,
			replacement: "use upstream crates.io dependencies where possible",
			recommendation:
				"Replace fake compat crates in small compile-verified PRs; keep a temporary facade only at MCPace module boundaries.",
		});
	}

	const cargoLockIssues = cargoLockRefreshFindings(args.repoRoot);
	if (cargoLockIssues.length > 0) {
		findings.push({
			id: "cargo-lock-needs-refresh",
			severity: "high",
			title:
				"Cargo.lock is not refreshed after replacing compat crates with upstream crates",
			count: cargoLockIssues.length,
			crates: cargoLockIssues,
			replacement:
				"reviewed package-specific lockfile update with the pinned Rust toolchain",
			recommendation: `${cargoLockRefreshMessage(cargoLockIssues)}. Update only the intended package, review Cargo.lock, then run every locked check before treating dependency modernization as complete.`,
		});
	}

	findings.push(
		finding(
			"manual-cli-parsing",
			"medium",
			"Commands manually parse argv instead of a typed CLI definition",
			grep(rustFiles, /parse_args|while\s+index\s*<\s*args\.len\(\)/),
			"Move command definitions to clap derive in phases: new commands first, then setup/serve/client/server.",
			"clap derive",
		),
	);

	findings.push(
		finding(
			"stringly-errors",
			"medium",
			"Rust modules return String errors across subsystem boundaries",
			grepBy(rustFiles, (_file, source) => hasStringlyErrorResult(source)),
			"Introduce thiserror for domain errors and anyhow at CLI boundaries.",
			"thiserror + anyhow",
		),
	);

	findings.push(
		finding(
			"raw-http-tcp",
			"medium",
			"HTTP/TCP handling is implemented directly in subsystem code",
			rawHttpTcpFiles(rustFiles),
			"Replace outbound HTTP first with ureq/reqwest; move dashboard server only under security contract tests.",
			"ureq/reqwest; later axum+tower-http",
		),
	);

	findings.push(
		finding(
			"manual-config-patching",
			"medium",
			"Client config patching is scattered outside the typed config_edit boundary",
			grep(rustFiles, /upsert_toml|find_toml|parse_yaml|upsert_yaml/),
			"Keep TOML/YAML config mutation centralized behind src/config_edit.rs; swap the TOML internals to toml_edit after Cargo.lock can be refreshed on a Rust host.",
			"typed config_edit boundary now; toml_edit internals after Cargo.lock refresh",
		),
	);

	findings.push(
		finding(
			"stdout-stderr-ad-hoc-logging",
			"low",
			"Runtime modules use ad-hoc stdout/stderr diagnostic macros",
			adHocDiagnosticFiles(rustFiles),
			"Route runtime diagnostics through tracing or protocol-safe stderr helpers while preserving CLI stdout contracts.",
			"tracing",
		),
	);

	const frontendChunks = dashboardFrontendChunks(args.repoRoot);
	const oversizedFrontendChunks = frontendChunks.filter(
		(chunk) => chunk.lines > 2000,
	);
	if (oversizedFrontendChunks.length > 0) {
		findings.push({
			id: "large-dashboard-frontend-module",
			severity: "medium",
			title: "Dashboard frontend has oversized JavaScript source chunks",
			count: Math.max(...oversizedFrontendChunks.map((chunk) => chunk.lines)),
			files: oversizedFrontendChunks.map(
				(chunk) => `${chunk.file}:${chunk.lines} lines`,
			),
			replacement:
				"small plain JS chunks now; Vite + TypeScript modules when the module graph needs imports",
			recommendation:
				"Keep each dashboard JS chunk under 2000 lines before adopting any larger UI framework.",
		});
	}

	const actionable = findings.filter((item) => item.count > 0);
	const report = {
		schema: "mcpace.modernizationInventory.v1",
		status: "pass",
		generatedAt: new Date().toISOString(),
		repoRoot: ".",
		findings: actionable,
		summary: {
			findings: actionable.length,
			high: actionable.filter((item) => item.severity === "high").length,
			medium: actionable.filter((item) => item.severity === "medium").length,
			low: actionable.filter((item) => item.severity === "low").length,
		},
	};

	if (args.json) console.log(JSON.stringify(report, null, 2));
	else {
		console.log(
			`${report.summary.findings} modernization findings (${report.summary.high} high, ${report.summary.medium} medium, ${report.summary.low} low)`,
		);
		for (const item of report.findings)
			console.log(
				`- ${item.severity}: ${item.id} — ${item.title} (${item.count})`,
			);
	}
}

try {
	run();
} catch (error) {
	console.error(error?.stack ?? String(error));
	process.exitCode = 1;
}
