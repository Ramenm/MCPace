#!/usr/bin/env node
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { repoRoot } from "./lib/project-metadata.mjs";
import { childEnvForCommand } from "./lib/safe-child-env.mjs";
import {
	commandForPlatform,
	commandNeedsShell,
	windowsCommandShell,
} from "./lib/process.mjs";

const DEFAULT_TIMEOUT_MS = 10 * 60 * 1000;
const SHORT_TIMEOUT_MS = 3 * 60 * 1000;
const ENDGAME_TIMEOUT_MS = 45 * 60 * 1000;
const RELEASE_BINARY = path.join(
	repoRoot,
	"target",
	"release",
	process.platform === "win32" ? "mcpace.exe" : "mcpace",
);

function nodeScript(scriptPath, args = [], timeoutMs = DEFAULT_TIMEOUT_MS) {
	return {
		name: scriptPath.replace(/^scripts\//, "").replace(/\.mjs$/, ""),
		command: process.execPath,
		args: [scriptPath, ...args],
		timeoutMs,
	};
}

function binScript(relativePath, args = [], timeoutMs = SHORT_TIMEOUT_MS) {
	return {
		name: relativePath.replace(/^node_modules\/\.bin\//, ""),
		command: commandForPlatform(path.join(repoRoot, relativePath)),
		args,
		timeoutMs,
	};
}

export const CI_STEPS = Object.freeze([
	{
		label: "lint:node-syntax",
		...nodeScript("scripts/check-node-syntax.mjs", ["--json"]),
	},
	{
		label: "lint:rust-static",
		...nodeScript("scripts/rust-static-guard.mjs", ["--json"]),
	},
	{
		label: "check:rust-boundaries",
		...nodeScript(
			"scripts/rust-boundary-contract.mjs",
			["--json"],
			SHORT_TIMEOUT_MS,
		),
	},
	{
		label: "test:npm",
		...nodeScript("scripts/run-node-tests.mjs", ["--quiet"]),
	},
	{
		label: "check:platform",
		...nodeScript("scripts/platform-proof.mjs", ["--check"], SHORT_TIMEOUT_MS),
	},
	{
		label: "check:assurance",
		...nodeScript(
			"scripts/project-assurance.mjs",
			["--check"],
			SHORT_TIMEOUT_MS,
		),
	},
	{
		label: "check:clean-archive",
		...nodeScript(
			"scripts/verify-clean-archive.mjs",
			["--json"],
			SHORT_TIMEOUT_MS,
		),
	},
	{
		label: "inventory:modernization",
		...nodeScript(
			"scripts/modernization-inventory.mjs",
			["--json"],
			SHORT_TIMEOUT_MS,
		),
	},
	{
		label: "inventory:legacy",
		...nodeScript(
			"scripts/legacy-subsystem-map.mjs",
			["--json"],
			SHORT_TIMEOUT_MS,
		),
	},
	{
		label: "check:modernization-budget",
		...nodeScript(
			"scripts/verify-modernization-budget.mjs",
			["--json"],
			SHORT_TIMEOUT_MS,
		),
	},
	{
		label: "check:dependency-policy",
		...nodeScript(
			"scripts/verify-dependency-policy.mjs",
			["--json", "--enforce-cargo-lock"],
			SHORT_TIMEOUT_MS,
		),
	},
	{
		label: "check:workflow-policy",
		...nodeScript(
			"scripts/verify-workflow-policy.mjs",
			["--json"],
			SHORT_TIMEOUT_MS,
		),
	},
	{
		label: "check:security-policy",
		...nodeScript("scripts/security-policy-check.mjs", [], SHORT_TIMEOUT_MS),
	},
	{
		label: "check:mcp-transport",
		...nodeScript(
			"scripts/mcp-transport-contract.mjs",
			["--json"],
			SHORT_TIMEOUT_MS,
		),
	},
	{
		label: "check:supply-chain-evidence",
		...nodeScript(
			"scripts/supply-chain-evidence.mjs",
			["--json", "--enforce"],
			SHORT_TIMEOUT_MS,
		),
	},
	{
		label: "check:release-ready",
		...nodeScript(
			"scripts/release-readiness.mjs",
			["--json", "--enforce"],
			SHORT_TIMEOUT_MS,
		),
	},
	// Endgame owns the one live Rust proof invocation to avoid running the full Cargo suite twice.
	{
		label: "check:endgame",
		...nodeScript(
			"scripts/endgame-readiness.mjs",
			["--json", "--enforce"],
			ENDGAME_TIMEOUT_MS,
		),
	},
	{
		label: "proof:live-mcp-e2e",
		...nodeScript(
			"scripts/live-mcp-e2e-proof.mjs",
			["--json", "--write", "--binary", RELEASE_BINARY],
			SHORT_TIMEOUT_MS,
		),
	},
	{
		label: "check:package",
		...binScript("node_modules/.bin/publint", ["packages/npm/cli"]),
	},
	{
		label: "check:install-smoke",
		...nodeScript("scripts/install-smoke.mjs", [], SHORT_TIMEOUT_MS),
	},
	{
		label: "check:terminal",
		...nodeScript("scripts/terminal-contract.mjs", [], SHORT_TIMEOUT_MS),
	},
	{
		label: "proof:browser-lifecycle",
		...nodeScript("scripts/browser-lifecycle-proof.mjs", [], SHORT_TIMEOUT_MS),
	},
	{
		label: "check:publish-trust",
		...nodeScript("scripts/publish-trust-preflight.mjs", [], SHORT_TIMEOUT_MS),
	},
	{
		label: "release:dry-run",
		...nodeScript(
			"scripts/build-release-artifacts.mjs",
			["--json", "--dry-run", "--out-dir", "dist"],
			SHORT_TIMEOUT_MS,
		),
	},
]);

function parseArgs(argv) {
	return {
		json: argv.includes("--json"),
		list: argv.includes("--list"),
	};
}

function displayCommand(step) {
	return [step.command, ...step.args].join(" ");
}

function printJson(payload) {
	process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
}

function spawnStep(step) {
	if (commandNeedsShell(step.command)) {
		return spawnSync(
			windowsCommandShell(),
			["/d", "/s", "/c", step.command, ...step.args],
			{
				cwd: repoRoot,
				env: childEnvForCommand(step.command),
				stdio: "inherit",
				shell: false,
				windowsHide: true,
				timeout: step.timeoutMs,
				killSignal: "SIGKILL",
			},
		);
	}
	return spawnSync(step.command, step.args, {
		cwd: repoRoot,
		env: childEnvForCommand(step.command),
		stdio: "inherit",
		shell: false,
		windowsHide: true,
		timeout: step.timeoutMs,
		killSignal: "SIGKILL",
	});
}

function runStep(step) {
	const startedAt = Date.now();
	process.stdout.write(`\n==> ${step.label}: ${displayCommand(step)}\n`);
	const result = spawnStep(step);
	const durationMs = Date.now() - startedAt;
	return {
		label: step.label,
		command: displayCommand(step),
		status: result.status,
		signal: result.signal,
		error: result.error?.message || null,
		timedOut:
			result.error?.code === "ETIMEDOUT" ||
			result.signal === "SIGKILL" ||
			result.signal === "SIGTERM",
		timeoutMs: step.timeoutMs,
		durationMs,
	};
}

function main() {
	const args = parseArgs(process.argv.slice(2));
	if (args.list) {
		const payload = {
			schema: "mcpace.ciEntrypoint.v1",
			steps: CI_STEPS.map((step) => ({
				label: step.label,
				command: displayCommand(step),
				timeoutMs: step.timeoutMs,
			})),
		};
		if (args.json) printJson(payload);
		else
			process.stdout.write(
				`${payload.steps.map((step) => `${step.label}: ${step.command}`).join("\n")}\n`,
			);
		return 0;
	}

	const results = [];
	for (const step of CI_STEPS) {
		const result = runStep(step);
		results.push(result);
		if (result.status !== 0 || result.signal || result.error) {
			const reason = result.timedOut
				? `timed out after ${result.timeoutMs}ms`
				: result.error ||
					`exit status ${result.status ?? "<null>"}${result.signal ? ` signal ${result.signal}` : ""}`;
			process.stderr.write(`\nFAIL check:ci step ${step.label}: ${reason}\n`);
			if (args.json) {
				printJson({
					schema: "mcpace.ciEntrypoint.v1",
					status: "fail",
					failedStep: step.label,
					results,
				});
			}
			return result.status || 1;
		}
	}

	process.stdout.write("\nPASS check:ci entrypoint\n");
	if (args.json) {
		printJson({ schema: "mcpace.ciEntrypoint.v1", status: "pass", results });
	}
	return 0;
}

try {
	process.exit(main());
} catch (error) {
	process.stderr.write(`${error?.stack || error}\n`);
	process.exit(1);
}
