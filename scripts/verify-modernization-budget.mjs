#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import process from "node:process";
import { repoRoot } from "./lib/project-metadata.mjs";

const DEFAULT_BUDGETS = Object.freeze({
	"cargo-path-compat-dependencies": {
		severity: "high",
		max: 0,
		replacement: "upstream crates.io dependencies",
	},
	"cargo-lock-needs-refresh": {
		severity: "high",
		max: 3,
		replacement:
			"reviewed package-specific lockfile update, then locked checks",
	},
	"manual-cli-parsing": {
		severity: "medium",
		max: 0,
		replacement: "clap derive",
	},
	"stringly-errors": {
		severity: "medium",
		max: 16,
		replacement: "thiserror + anyhow",
	},
	"raw-http-tcp": {
		severity: "medium",
		max: 4,
		replacement: "shared http_probe now; ureq/reqwest later for outbound HTTP",
	},
	"manual-config-patching": {
		severity: "medium",
		max: 0,
		replacement:
			"typed config_edit boundary now; toml_edit internals after Cargo.lock refresh",
	},
	"stdout-stderr-ad-hoc-logging": {
		severity: "low",
		max: 0,
		replacement: "tracing",
	},
	"large-dashboard-frontend-module": {
		severity: "medium",
		max: 0,
		replacement:
			"bounded plain JS chunks; Vite + TypeScript only after imports are needed",
	},
});

function parseArgs(argv) {
	const args = { json: false };
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (arg === "--json") args.json = true;
		else if (arg === "--help" || arg === "-h") {
			console.log(
				"Usage: node scripts/verify-modernization-budget.mjs [--json]",
			);
			process.exit(0);
		} else {
			throw new Error(`unknown argument: ${arg}`);
		}
	}
	return args;
}

function loadInventory() {
	const result = spawnSync(
		process.execPath,
		["scripts/modernization-inventory.mjs", "--json"],
		{
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
		},
	);
	if (result.status !== 0) {
		throw new Error(
			`modernization inventory failed: ${result.stderr || result.stdout}`,
		);
	}
	try {
		return JSON.parse(result.stdout);
	} catch (error) {
		throw new Error(
			`modernization inventory returned invalid JSON: ${error?.message || error}`,
			{ cause: error },
		);
	}
}

function run() {
	const args = parseArgs(process.argv.slice(2));
	const inventory = loadInventory();
	const byId = new Map(inventory.findings.map((item) => [item.id, item]));
	const findings = [];

	for (const [id, budget] of Object.entries(DEFAULT_BUDGETS)) {
		const item = byId.get(id);
		const actual = item?.count ?? 0;
		const status = actual <= budget.max ? "pass" : "fail";
		findings.push({
			id,
			status,
			severity: budget.severity,
			actual,
			max: budget.max,
			replacement: budget.replacement,
			title: item?.title ?? `No current finding for ${id}`,
		});
	}

	const failures = findings.filter((item) => item.status === "fail");
	const report = {
		schema: "mcpace.modernizationBudget.v1",
		generatedAt: new Date().toISOString(),
		status: failures.length === 0 ? "pass" : "fail",
		failures: failures.length,
		findings,
	};

	if (args.json) console.log(JSON.stringify(report, null, 2));
	else {
		console.log(
			`${report.status}: ${findings.length} modernization budgets, ${failures.length} failures`,
		);
		for (const item of findings)
			console.log(`- ${item.status}: ${item.id} ${item.actual}/${item.max}`);
	}
	process.exitCode = failures.length === 0 ? 0 : 1;
}

try {
	run();
} catch (error) {
	console.error(error?.stack ?? String(error));
	process.exitCode = 1;
}
