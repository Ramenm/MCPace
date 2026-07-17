#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { repoRoot as defaultRepoRoot } from "./lib/project-metadata.mjs";
import { listWorkingTreeFiles } from "./lib/repo-files.mjs";

const SKIP_DIRS = new Set([
	".git",
	"node_modules",
	"target",
	"dist",
	".artifacts",
	".pi-subagents",
	"reports",
]);
const LEGACY_PATTERN =
	/\blegacy\b|legacy[-_][A-Za-z0-9_-]+|[A-Za-z0-9_-]+[-_]legacy|stdio[-_]shim|\bdeprecated\b/gi;

const ALLOWLIST = [
	{
		prefix: "scripts/legacy-subsystem-map.mjs",
		reason: "legacy inventory tool",
	},
	{
		prefix: "scripts/architecture-debt-inventory.mjs",
		reason: "architecture debt inventory tool",
	},
	{
		prefix: "scripts/architecture-boundary-guard.mjs",
		reason: "architecture boundary regression guard",
	},
	{
		prefix: "scripts/check-ci.mjs",
		reason: "CI invokes legacy inventory guard",
	},
	{
		prefix: "scripts/mcp-transport-contract.mjs",
		reason: "stdio-shim compatibility contract",
	},
	{
		prefix: "tests/node/architecture-debt-inventory.test.mjs",
		reason: "architecture debt inventory tests",
	},
	{
		prefix: "tests/node/architecture-boundary-guard.test.mjs",
		reason: "architecture boundary guard tests",
	},
	{
		prefix: "tests/node/internal-logic-guardrails.test.mjs",
		reason: "legacy routing guard tests",
	},
	{
		prefix: "src/catalog.rs",
		reason: "documented stdio-shim command alias metadata",
	},
	{
		prefix: "src/app/tests.rs",
		reason: "hidden compatibility entrypoint routing tests",
	},
	{
		prefix: "src/client_catalog/builtin.rs",
		reason: "client catalog marks deprecated SSE support",
	},
	{ prefix: "src/lib.rs", reason: "internal stdio_shim module declaration" },
	{
		prefix: "src/mcp_sources/import.rs",
		reason: "client import accepts existing stdio-shim configs",
	},
	{
		prefix: "src/reporoot.rs",
		reason: "root recovery reads retired Run entry during cleanup",
	},
	{
		prefix: "eval/runtime-capabilities.json",
		reason: "checked-in evaluation model vocabulary",
	},
	{
		prefix: "scripts/legacy-boundary-guard.mjs",
		reason: "legacy inventory tool",
	},
	{
		prefix: "tests/node/legacy-subsystem-map.test.mjs",
		reason: "legacy inventory tests",
	},
	{
		prefix: "tests/node/project-hygiene.test.mjs",
		reason: "retired-surface and compatibility guard tests",
	},
	{
		prefix: "tests/node/source-archive-policy.test.mjs",
		reason: "fixture asserts old generated artifacts stay excluded",
	},
	{
		prefix: "tests/node/routing-lease-logic.test.mjs",
		reason: "quarantined legacy transport routing tests",
	},
	{
		prefix: "tests/node/dashboard-contract.test.mjs",
		reason: "dashboard keeps legacy panels folded",
	},
	{
		prefix: "tests/node/docs-and-package.test.mjs",
		reason: "retired bridge regression tests",
	},
	{
		prefix: "tests/node/modernization-budget.test.mjs",
		reason: "modernization budget tests",
	},
	{
		prefix: "tests/node/runtime-state-classification.test.mjs",
		reason: "classification schema tests",
	},
	{
		prefix: "src/service.rs",
		reason:
			"autostart coordinator references quarantined legacy cleanup module",
	},
	{
		prefix: "src/service/legacy.rs",
		reason: "quarantined Windows Run-entry cleanup for retired MCPace launcher",
	},
	{ prefix: "src/service/tests.rs", reason: "Windows Run-entry cleanup tests" },
	{
		prefix: "src/source_type.rs",
		reason: "single source-type alias quarantine",
	},
	{
		prefix: "src/server/loader.rs",
		reason: "legacy SSE classification quarantine",
	},
	{
		prefix: "src/server/discover.rs",
		reason:
			"official Registry deprecated-status entries are blocked from install",
	},
	{
		prefix: "src/server/discover/tests.rs",
		reason: "official Registry deprecated-status blocking tests",
	},
	{ prefix: "src/upstream.rs", reason: "legacy SSE forwarding block" },
	{ prefix: "src/upstream/tests.rs", reason: "legacy SSE forwarding tests" },
	{
		prefix: "src/client/plan.rs",
		reason: "legacy transport route disabled policy",
	},
	{
		prefix: "src/hub/leases.rs",
		reason: "legacy transport route disabled guard",
	},
	{ prefix: "src/stdio_shim.rs", reason: "documented compatibility alias" },
	{ prefix: "src/app.rs", reason: "documented compatibility alias routing" },
	{
		prefix: "src/setup.rs",
		reason: "client import compatibility alias handling",
	},
	{
		prefix: "src/setup/tests.rs",
		reason: "client import compatibility alias tests",
	},
	{
		prefix: "src/upstream/inventory.rs",
		reason: "legacy adapter diagnostic copy",
	},
	{
		prefix: "src/dashboard/diagnostics.rs",
		reason: "legacy adapter diagnostic copy",
	},
	{
		prefix: "src/dashboard/frontend/app.render.js",
		reason: "dashboard legacy quarantine rendering",
	},
	{ prefix: "docs/", reason: "migration and architecture documentation" },
	{
		prefix: "schemas/",
		reason: "versioned config/profile schema compatibility values",
	},
	{ prefix: "CHANGELOG.md", reason: "historical changelog" },
	{ prefix: "SECURITY.md", reason: "policy vocabulary" },
	{
		prefix: "release-manifest.json",
		reason: "source manifest includes inventory scripts",
	},
	{
		prefix: "metadata/merge/PROJECT_PAYLOAD_MANIFEST.json",
		reason: "merged overlay integrity manifest records exact project paths",
	},
	{ prefix: "package.json", reason: "npm script names" },
];

function parseArgs(argv) {
	const args = { json: false, enforce: false, repoRoot: defaultRepoRoot };
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (arg === "--json") args.json = true;
		else if (arg === "--enforce") args.enforce = true;
		else if (arg === "--repo") args.repoRoot = path.resolve(argv[++index]);
		else if (arg === "--help" || arg === "-h") {
			console.log(
				"Usage: node scripts/legacy-boundary-guard.mjs [--json] [--enforce] [--repo DIR]",
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

function readText(file) {
	try {
		return fs.readFileSync(file, "utf8");
	} catch {
		return "";
	}
}

function allowed(relative) {
	return ALLOWLIST.find(
		(item) => relative === item.prefix || relative.startsWith(item.prefix),
	);
}

function matchingLines(source) {
	const lines = source.replace(/\r\n/g, "\n").split("\n");
	const hits = [];
	for (let index = 0; index < lines.length; index += 1) {
		const line = lines[index];
		LEGACY_PATTERN.lastIndex = 0;
		if (LEGACY_PATTERN.test(line))
			hits.push({ line: index + 1, text: line.trim().slice(0, 220) });
	}
	return hits;
}

function run() {
	const args = parseArgs(process.argv.slice(2));
	const files = listWorkingTreeFiles(args.repoRoot).filter((file) => {
		const relative = normalize(path.relative(args.repoRoot, file));
		return !relative.split("/").some((part) => SKIP_DIRS.has(part));
	});
	const findings = [];
	for (const file of files) {
		const relative = normalize(path.relative(args.repoRoot, file));
		if (relative === "package-lock.json" || relative.startsWith("eval/random-"))
			continue;
		const hits = matchingLines(readText(file));
		if (hits.length === 0) continue;
		const allow = allowed(relative);
		findings.push({
			file: relative,
			status: allow ? "allowed" : "unexpected",
			reason: allow?.reason ?? "not in the legacy/compat allowlist",
			hits: hits.slice(0, 12),
			hitCount: hits.length,
		});
	}
	const unexpected = findings.filter((item) => item.status === "unexpected");
	const report = {
		schema: "mcpace.legacyBoundaryGuard.v1",
		generatedAt: new Date().toISOString(),
		status: unexpected.length === 0 ? "pass" : "fail",
		allowedFiles: findings.length - unexpected.length,
		unexpectedFiles: unexpected.length,
		findings,
		policy: {
			intent:
				"Legacy code and compatibility shims must be quarantined to documented files; new legacy markers outside this allowlist require an explicit retirement note.",
			allowlistEntries: ALLOWLIST.length,
		},
	};
	if (args.json) console.log(JSON.stringify(report, null, 2));
	else {
		console.log(
			`${report.status}: ${report.unexpectedFiles} unexpected legacy/compat marker files, ${report.allowedFiles} allowed files`,
		);
		for (const item of unexpected)
			console.log(`- unexpected ${item.file}: ${item.reason}`);
	}
	if (args.enforce && unexpected.length > 0) process.exitCode = 1;
}

try {
	run();
} catch (error) {
	console.error(error?.stack ?? String(error));
	process.exitCode = 1;
}
