import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const repoRoot = path.resolve(import.meta.dirname, "..", "..");

function read(relativePath) {
	return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function parseJson(source, label) {
	try {
		return JSON.parse(source);
	} catch (error) {
		assert.fail(
			`${label} is not valid JSON: ${error instanceof Error ? error.message : String(error)}`,
		);
	}
}

function readJson(relativePath) {
	return parseJson(read(relativePath), relativePath);
}

function runPlatformProofJson() {
	const result = spawnSync(
		process.execPath,
		["scripts/platform-proof.mjs", "--json"],
		{
			cwd: repoRoot,
			encoding: "utf8",
		},
	);
	assert.equal(result.status, 0, result.stderr || result.stdout);
	return parseJson(result.stdout, "platform proof output");
}

test("platform proof covers Linux macOS and Windows with native smoke gates", () => {
	const report = runPlatformProofJson();
	assert.equal(report.schema, "mcpace.platformProof.v1");
	assert.equal(report.overall, "pass");
	assert.equal(report.evidenceKind, "static-plan-contract");
	assert.equal(report.executionEvidence, false);
	assert.match(report.scope, /does not claim.*executed/i);
	assert.deepEqual(report.platforms.published, ["darwin", "linux", "win32"]);
	assert.deepEqual(report.platforms.workflow, ["darwin", "linux", "win32"]);
	assert.ok(report.summary.publishedTargetCount >= 6);
	assert.equal(report.summary.publicCommandCount, 10);
	assert.ok(report.summary.smokeCommandCount >= 15);
	assert.equal(
		report.smokeCommands.find((item) => item.command === "status --json")
			?.expects,
		"jsonOrStatusOne",
	);

	const smokeCommands = new Set(
		report.smokeCommands.map((item) => item.command),
	);
	for (const command of [
		"advanced doctor --json",
		"advanced server list --json",
		"advanced server capabilities --json",
		"advanced client list --json",
		"advanced dev lab report --json",
		"advanced autostart --help",
		"uninstall --help",
	]) {
		assert.ok(smokeCommands.has(command), `missing smoke command ${command}`);
	}

	assert.match(report.uiDecision.decision, /Tauri/i);
	assert.match(report.uiDecision.nextTuiGate, /Ratatui/i);
});

test("platform proof Markdown discloses static-only evidence and scope", () => {
	const markdown = read("reports/platform-proof.md");
	assert.match(markdown, /Static contract status:/);
	assert.match(markdown, /Evidence kind: \*\*static-plan-contract\*\*/);
	assert.match(markdown, /Remote execution evidence: \*\*false\*\*/);
	assert.match(markdown, /does not claim that the remote OS matrix executed/i);
	assert.match(markdown, /does not prove.*matrix executed successfully/i);
	assert.doesNotMatch(markdown, /- Overall: \*\*pass\*\*/);
});

test("platform proof workflow is manual and runs Node Rust and binary smoke on all desktop OS families", () => {
	const workflow = read(".github/workflows/platform-proof.yml");
	assert.match(workflow, /workflow_dispatch/);
	assert.match(workflow, /ubuntu-latest/);
	assert.match(workflow, /macos-latest/);
	assert.match(workflow, /macos-15-intel/);
	assert.match(workflow, /windows-latest/);
	assert.match(workflow, /npm run check:platform/);
	assert.match(workflow, /npm run check/);
	assert.match(workflow, /npm run check:rust/);
	assert.match(workflow, /cargo build --release/);
	assert.match(workflow, /npm run platform:binary-smoke/);
	assert.match(workflow, /Smoke isolated runtime lifecycle/);
	assert.match(workflow, /npm run check:installer-runtime-smoke -- --binary/);
	assert.match(workflow, /npm run proof:autostart/);
	assert.match(workflow, /MCPACE_DISPOSABLE_AUTOSTART_PROOF:\s*["']?1/);
	assert.match(workflow, /--confirm-disposable-user/);
});

test("destructive autostart proof is double-gated to disposable users", () => {
	const script = read("scripts/autostart-lifecycle-proof.mjs");
	const releaseWorkflow = read(".github/workflows/release.yml");
	for (const source of [script, releaseWorkflow]) {
		assert.match(source, /MCPACE_DISPOSABLE_AUTOSTART_PROOF/);
		assert.match(source, /--confirm-disposable-user/);
	}
	assert.match(script, /refusing to modify the current user's login startup/);
	assert.match(script, /supervisorVerified/);
	assert.match(script, /evidence\.recoveryOwnership/);
});

test("platform docs assign source and launcher dry-runs to the correct commands", () => {
	const docs = read("docs/platform-testing.md");
	assert.match(
		docs,
		/`release:dry-run` validates only the tracked source-archive input/,
	);
	assert.match(
		docs,
		/`pack:npm:dry-run` separately validates launcher packaging/,
	);
	assert.doesNotMatch(docs, /source-archive and launcher packaging only/);
});

test("native binary smoke and static platform proof share one canonical command matrix", () => {
	const binarySmoke = read("scripts/platform-binary-smoke.mjs");
	const platformProof = read("scripts/platform-proof.mjs");
	for (const source of [binarySmoke, platformProof]) {
		assert.match(source, /platformSmokeCommands/);
		assert.match(source, /lib\/platform-smoke-commands\.mjs/);
	}
	assert.doesNotMatch(binarySmoke, /args:\s*\[\s*["']doctor["']/);
	assert.doesNotMatch(binarySmoke, /jsonOrNonzero/);
	assert.match(binarySmoke, /jsonOrStatusOne/);
});

test("platform proof scripts and reports are part of package checks and release bundle", () => {
	const packageJson = readJson("package.json");
	assert.match(packageJson.scripts.platform, /platform-proof\.mjs --write/);
	assert.match(
		packageJson.scripts["check:platform"],
		/platform-proof\.mjs --check/,
	);
	assert.match(
		packageJson.scripts["platform:binary-smoke"],
		/platform-binary-smoke\.mjs/,
	);
	assert.match(packageJson.scripts.check, /check:platform/);

	const manifest = readJson("release-manifest.json");
	for (const required of [
		"scripts/platform-proof.mjs",
		"scripts/platform-binary-smoke.mjs",
		"scripts/lib/platform-smoke-commands.mjs",
		"reports/platform-proof.md",
		"reports/platform-proof.json",
	]) {
		assert.ok(
			manifest.includePaths.includes(required),
			`release manifest missing ${required}`,
		);
	}
});
