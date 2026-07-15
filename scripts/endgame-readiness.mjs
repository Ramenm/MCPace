#!/usr/bin/env node
import process from "node:process";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { repoRoot as defaultRepoRoot } from "./lib/project-metadata.mjs";
import { childEnvForCommand } from "./lib/safe-child-env.mjs";

const SHORT_TIMEOUT_MS = 3 * 60 * 1000;
const LONG_TIMEOUT_MS = 45 * 60 * 1000;

function parseArgs(argv) {
	const args = { json: false, enforce: false, repoRoot: defaultRepoRoot };
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (arg === "--json") args.json = true;
		else if (arg === "--enforce") args.enforce = true;
		else if (arg === "--repo") args.repoRoot = path.resolve(argv[++index]);
		else if (arg === "--help" || arg === "-h") {
			console.log(
				"Usage: node scripts/endgame-readiness.mjs [--json] [--enforce] [--repo DIR]",
			);
			process.exit(0);
		} else {
			throw new Error(`unknown argument: ${arg}`);
		}
	}
	return args;
}

function runJson(
	repoRoot,
	label,
	script,
	args = [],
	timeoutMs = SHORT_TIMEOUT_MS,
) {
	const startedAt = Date.now();
	const result = spawnSync(process.execPath, [script, "--json", ...args], {
		cwd: repoRoot,
		encoding: "utf8",
		env: childEnvForCommand(process.execPath),
		maxBuffer: 16 * 1024 * 1024,
		shell: false,
		timeout: timeoutMs,
		windowsHide: true,
	});
	let parsed = null;
	let parseError = null;
	try {
		parsed = JSON.parse(result.stdout || "{}");
	} catch (error) {
		parseError = error?.message || String(error);
	}
	const ok = !result.error && result.status === 0 && parsed && !parseError;
	return {
		label,
		script,
		command: [process.execPath, script, "--json", ...args].join(" "),
		ok,
		exitCode: result.status,
		signal: result.signal,
		error: result.error?.message || parseError || null,
		stdoutTail: String(result.stdout || "")
			.split(/\r?\n/)
			.slice(-30)
			.join("\n")
			.trim(),
		stderrTail: String(result.stderr || "")
			.split(/\r?\n/)
			.slice(-30)
			.join("\n")
			.trim(),
		durationMs: Date.now() - startedAt,
		report: parsed,
	};
}

function statusFromReport(step) {
	if (!step.ok) return "blocker";
	const reportStatus = step.report?.status || "unknown";
	if (reportStatus === "pass") return "pass";
	if (reportStatus === "warn") return "warn";
	if (
		reportStatus === "blocked" ||
		reportStatus === "failed" ||
		reportStatus === "fail"
	)
		return "blocker";
	return "warn";
}

function detailFromReport(step) {
	if (!step.ok) return step.error || "script did not return valid JSON";
	const report = step.report;
	const pieces = [`status=${report.status || "unknown"}`];
	if (Number.isFinite(report.blockers))
		pieces.push(`blockers=${report.blockers}`);
	if (Number.isFinite(report.failures))
		pieces.push(`failures=${report.failures}`);
	if (Number.isFinite(report.warnings))
		pieces.push(`warnings=${report.warnings}`);
	return pieces.join(", ");
}

function run(args) {
	const repoRoot = args.repoRoot;
	const steps = [
		runJson(
			repoRoot,
			"mcp-transport-contract",
			"scripts/mcp-transport-contract.mjs",
		),
		runJson(
			repoRoot,
			"rust-boundary-contract",
			"scripts/rust-boundary-contract.mjs",
		),
		runJson(
			repoRoot,
			"supply-chain-evidence",
			"scripts/supply-chain-evidence.mjs",
		),
		runJson(repoRoot, "release-readiness", "scripts/release-readiness.mjs"),
		runJson(
			repoRoot,
			"rust-live-proof",
			"scripts/rust-live-proof.mjs",
			args.enforce ? ["--write"] : [],
			LONG_TIMEOUT_MS,
		),
		runJson(
			repoRoot,
			"modernization-inventory",
			"scripts/modernization-inventory.mjs",
		),
		runJson(
			repoRoot,
			"modernization-budget",
			"scripts/verify-modernization-budget.mjs",
		),
		runJson(
			repoRoot,
			"source-archive-policy",
			"scripts/verify-clean-archive.mjs",
			["--source-tree"],
		),
	];
	const findings = steps.map((step) => ({
		id: step.label,
		status: statusFromReport(step),
		detail: detailFromReport(step),
		command: step.command,
		durationMs: step.durationMs,
	}));

	const blockers = findings.filter((item) => item.status === "blocker");
	const warnings = findings.filter((item) => item.status === "warn");
	let status = "pass";
	if (blockers.length > 0) status = "blocked";
	else if (warnings.length > 0) status = "warn";
	const releaseBlockingFindings = [];
	for (const step of steps) {
		const report = step.report;
		if (!report) continue;
		for (const item of report.findings || []) {
			if (
				item.status === "blocker" ||
				item.status === "failed" ||
				item.status === "fail"
			) {
				releaseBlockingFindings.push({
					gate: step.label,
					id: item.id,
					detail: item.detail || item.title || item.reason || "blocked",
				});
			}
		}
	}

	return {
		schema: "mcpace.endgameReadiness.v1",
		generatedAt: new Date().toISOString(),
		repoRoot: ".",
		status,
		enforce: args.enforce,
		blockers: blockers.length,
		warnings: warnings.length,
		summary: {
			gates: steps.length,
			pass: findings.filter((item) => item.status === "pass").length,
			warn: warnings.length,
			blocked: blockers.length,
		},
		findings,
		releaseBlockingFindings,
		endgameDefinition: [
			"MCP stdio and Streamable HTTP source contracts pass",
			"Rust typed-boundary and raw HTTP/TCP allowlist contracts pass",
			"Node/tooling/package/source archive gates pass from a clean tree",
			"Supply-chain evidence has no blockers",
			"Pinned Rust tools are available",
			"Cargo.lock is synchronized with Cargo.toml",
			"cargo check/test/fmt/clippy/build pass with --locked",
			"release-ready enforce gate exits 0 on the release host",
		],
	};
}

try {
	const args = parseArgs(process.argv.slice(2));
	const report = run(args);
	if (args.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
	else {
		console.log(
			`${report.status}: ${report.summary.gates} endgame gates, ${report.blockers} blockers, ${report.warnings} warnings`,
		);
		for (const item of report.findings)
			console.log(`- ${item.status}: ${item.id} — ${item.detail}`);
	}
	process.exitCode = args.enforce && report.blockers > 0 ? 1 : 0;
} catch (error) {
	console.error(error?.stack || error);
	process.exitCode = 1;
}
