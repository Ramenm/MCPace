#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import {
	cargoLockRefreshFindings,
	cargoLockRefreshMessage,
} from "./lib/cargo-policy.mjs";
import {
	repoRoot as defaultRepoRoot,
	readRootPackageJson,
} from "./lib/project-metadata.mjs";

function parseArgs(argv) {
	const args = { json: false, enforce: false, repoRoot: defaultRepoRoot };
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (arg === "--json") args.json = true;
		else if (arg === "--enforce") args.enforce = true;
		else if (arg === "--repo") args.repoRoot = path.resolve(argv[++index]);
		else if (arg === "--help" || arg === "-h") {
			console.log(
				"Usage: node scripts/release-readiness.mjs [--json] [--enforce] [--repo DIR]",
			);
			process.exit(0);
		} else {
			throw new Error(`unknown argument: ${arg}`);
		}
	}
	return args;
}

function readText(repoRoot, relativePath) {
	return fs.existsSync(path.join(repoRoot, relativePath))
		? fs.readFileSync(path.join(repoRoot, relativePath), "utf8")
		: "";
}

function parseJson(text, label) {
	try {
		return JSON.parse(text);
	} catch (error) {
		throw new Error(`invalid JSON in ${label}: ${error?.message || error}`, {
			cause: error,
		});
	}
}

function statusFinding(id, status, detail, extra = {}) {
	return { id, status, detail, ...extra };
}

function commandVersion(command, args = ["--version"], options = {}) {
	const startedAt = Date.now();
	const result = spawnSync(command, args, {
		cwd: options.cwd,
		encoding: "utf8",
		env: process.env,
		maxBuffer: 2 * 1024 * 1024,
		shell: false,
		timeout: options.timeoutMs ?? 20_000,
		windowsHide: true,
	});
	return {
		command: [command, ...args].join(" "),
		ok: !result.error && result.status === 0,
		exitCode: result.status,
		signal: result.signal,
		error: result.error?.message || null,
		output: `${result.stdout || ""}${result.stderr || ""}`.trim(),
		durationMs: Date.now() - startedAt,
	};
}

function rustToolchain(repoRoot) {
	const text = readText(repoRoot, "rust-toolchain.toml");
	return text.match(/^channel\s*=\s*"([^"]+)"/m)?.[1] || null;
}

function toolFinding(
	id,
	command,
	args,
	requiredVersionFragment,
	missingDetail,
	repoRoot,
) {
	const result = commandVersion(command, args, { cwd: repoRoot });
	if (!result.ok) {
		return statusFinding(id, "blocker", missingDetail, {
			command: result.command,
			error: result.error || result.output || `exit ${result.exitCode}`,
		});
	}
	if (
		requiredVersionFragment &&
		!result.output.includes(requiredVersionFragment)
	) {
		return statusFinding(
			id,
			"blocker",
			`installed, but version output does not include project pin ${requiredVersionFragment}`,
			{ command: result.command, output: result.output },
		);
	}
	return statusFinding(id, "pass", `${command} is available`, {
		command: result.command,
		output: result.output.split(/\r?\n/)[0] || "ok",
	});
}

function rustfmtFinding(repoRoot) {
	const rustc = commandVersion("rustc", ["-Vv"], { cwd: repoRoot });
	const rustfmt = commandVersion("rustfmt", ["--version", "--verbose"], {
		cwd: repoRoot,
	});
	if (!rustfmt.ok) {
		return statusFinding(
			"tool-rustfmt",
			"blocker",
			"Install rustfmt from the pinned Rust toolchain.",
			{
				command: rustfmt.command,
				error: rustfmt.error || rustfmt.output || `exit ${rustfmt.exitCode}`,
			},
		);
	}
	const rustcCommit =
		rustc.output.match(/^commit-hash:\s*([0-9a-f]+)/im)?.[1] || null;
	const matchingCommit =
		rustc.ok &&
		rustcCommit &&
		rustfmt.output.includes(rustcCommit.slice(0, 10));
	return statusFinding(
		"tool-rustfmt",
		matchingCommit ? "pass" : "blocker",
		matchingCommit
			? "rustfmt toolchain commit matches rustc"
			: "rustfmt toolchain commit does not match rustc",
		{
			command: rustfmt.command,
			output: rustfmt.output.split(/\r?\n/)[0] || "ok",
			rustcCommit,
		},
	);
}

function scriptFinding(packageJson, name, pattern) {
	const value = packageJson.scripts?.[name] || "";
	return statusFinding(
		`script-${name.replace(/[^a-z0-9]+/gi, "-")}`,
		pattern.test(value) ? "pass" : "blocker",
		pattern.test(value)
			? `${name} is wired`
			: `${name} is missing or points at the wrong command`,
		{ script: name, value },
	);
}

function shellLogicalLines(text) {
	return String(text || "").replace(/\\\r?\n\s*/g, " ");
}

function workflowFinding(repoRoot, id, relativePath, pattern, detail) {
	const text = shellLogicalLines(readText(repoRoot, relativePath));
	return statusFinding(id, pattern.test(text) ? "pass" : "blocker", detail, {
		file: relativePath,
	});
}

function releaseWorkflowGateFinding(
	repoRoot,
	id,
	directPattern,
	dependencyChecks,
	detail,
) {
	const release = shellLogicalLines(
		readText(repoRoot, ".github/workflows/release.yml"),
	);
	const direct = directPattern.test(release);
	const indirect =
		/npm run check:ci/.test(release) &&
		dependencyChecks.every(({ file, pattern }) =>
			pattern.test(shellLogicalLines(readText(repoRoot, file))),
		);
	return statusFinding(id, direct || indirect ? "pass" : "blocker", detail, {
		file: ".github/workflows/release.yml",
	});
}

function releaseCommandPlan(pinnedRust) {
	return [
		`rustup toolchain install ${pinnedRust || "<pinned-rust>"}`,
		"npm ci --ignore-scripts",
		"npm audit --audit-level=moderate --json",
		"npm run check:ci  # delegates once to enforcing endgame for all locked Cargo proof/build commands",
		"npm run release:dry-run",
	];
}

function run(args) {
	const repoRoot = args.repoRoot;
	const pinnedRust = rustToolchain(repoRoot);
	const packageJson =
		repoRoot === defaultRepoRoot
			? readRootPackageJson()
			: parseJson(readText(repoRoot, "package.json"), "package.json");
	const lockIssues = cargoLockRefreshFindings(repoRoot);
	const findings = [
		statusFinding(
			"rust-toolchain-pinned",
			pinnedRust ? "pass" : "blocker",
			pinnedRust
				? `Rust toolchain is pinned to ${pinnedRust}`
				: "rust-toolchain.toml must pin the Rust toolchain",
			{ pinnedRust },
		),
		toolFinding(
			"tool-rustc",
			"rustc",
			["--version"],
			pinnedRust,
			`Install Rust ${pinnedRust || "<pinned>"} before claiming native build readiness.`,
			repoRoot,
		),
		toolFinding(
			"tool-cargo",
			"cargo",
			["--version"],
			null,
			`Install Cargo from Rust ${pinnedRust || "<pinned>"} before refreshing Cargo.lock.`,
			repoRoot,
		),
		rustfmtFinding(repoRoot),
		toolFinding(
			"tool-clippy",
			"cargo",
			["clippy", "--version"],
			null,
			`Install clippy for Rust ${pinnedRust || "<pinned>"}.`,
			repoRoot,
		),
		scriptFinding(packageJson, "check:ci", /check-ci\.mjs/),
		scriptFinding(
			packageJson,
			"check:mcp-transport",
			/mcp-transport-contract\.mjs/,
		),
		scriptFinding(
			packageJson,
			"check:rust-boundaries",
			/rust-boundary-contract\.mjs --json/,
		),
		scriptFinding(
			packageJson,
			"check:supply-chain-evidence",
			/supply-chain-evidence\.mjs --json/,
		),
		scriptFinding(
			packageJson,
			"proof:rust-live",
			/rust-live-proof\.mjs --json/,
		),
		scriptFinding(
			packageJson,
			"proof:rust-live:enforce",
			/rust-live-proof\.mjs --json --enforce/,
		),
		scriptFinding(
			packageJson,
			"check:release-ready",
			/release-readiness\.mjs --json/,
		),
		scriptFinding(
			packageJson,
			"check:release-ready:enforce",
			/release-readiness\.mjs --json --enforce/,
		),
		scriptFinding(
			packageJson,
			"check:endgame",
			/endgame-readiness\.mjs --json/,
		),
		scriptFinding(
			packageJson,
			"check:endgame:enforce",
			/endgame-readiness\.mjs --json --enforce/,
		),
		workflowFinding(
			repoRoot,
			"publish-workflow-uses-oidc",
			".github/workflows/publish-npm.yml",
			/id-token:\s*write/,
			"npm publish workflow should use OIDC trusted publishing",
		),
		workflowFinding(
			repoRoot,
			"publish-workflow-requests-provenance",
			".github/workflows/publish-npm.yml",
			/npm publish[^\n]*--provenance/,
			"npm publish workflow should request provenance for package consumers",
		),
		workflowFinding(
			repoRoot,
			"release-workflow-attests-artifacts",
			".github/workflows/release.yml",
			/actions\/attest@/,
			"GitHub release workflow should generate artifact attestations",
		),
		releaseWorkflowGateFinding(
			repoRoot,
			"release-workflow-enforces-release-ready",
			/check:release-ready:enforce/,
			[
				{
					file: "scripts/check-ci.mjs",
					pattern: /release-readiness\.mjs[\s\S]{0,240}--enforce/,
				},
			],
			"GitHub release workflow should run the fail-closed release-ready gate",
		),
		releaseWorkflowGateFinding(
			repoRoot,
			"release-workflow-enforces-rust-live-proof",
			/proof:rust-live:enforce/,
			[
				{
					file: "scripts/check-ci.mjs",
					pattern: /endgame-readiness\.mjs[\s\S]{0,240}--enforce/,
				},
				{
					file: "scripts/endgame-readiness.mjs",
					pattern: /rust-live-proof\.mjs/,
				},
			],
			"GitHub release workflow should run the fail-closed Rust live proof gate",
		),
		statusFinding(
			"cargo-lock-synchronized",
			lockIssues.length === 0 ? "pass" : "blocker",
			cargoLockRefreshMessage(lockIssues),
			{ issues: lockIssues },
		),
	];

	const blockers = findings.filter((item) => item.status === "blocker");
	const warnings = findings.filter((item) => item.status === "warn");
	let status = "pass";
	if (blockers.length > 0) status = "blocked";
	else if (warnings.length > 0) status = "warn";
	return {
		schema: "mcpace.releaseReadiness.v1",
		generatedAt: new Date().toISOString(),
		status,
		enforce: args.enforce,
		blockers: blockers.length,
		warnings: warnings.length,
		repoRoot: ".",
		pinnedRust,
		findings,
		requiredCommandPlan: releaseCommandPlan(pinnedRust),
	};
}

try {
	const args = parseArgs(process.argv.slice(2));
	const report = run(args);
	if (args.json) console.log(JSON.stringify(report, null, 2));
	else {
		console.log(
			`${report.status}: ${report.findings.length} release-readiness checks, ${report.blockers} blockers, ${report.warnings} warnings`,
		);
		for (const item of report.findings)
			console.log(`- ${item.status}: ${item.id} — ${item.detail}`);
	}
	process.exitCode = args.enforce && report.blockers > 0 ? 1 : 0;
} catch (error) {
	console.error(error?.stack || error);
	process.exitCode = 1;
}
