import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import {
	repoRoot,
	readRootPackageJson,
} from "../../scripts/lib/project-metadata.mjs";
import { sanitizeProofText } from "../../scripts/rust-live-proof.mjs";

function runRustProof(args = []) {
	return spawnSync(
		process.execPath,
		["scripts/rust-live-proof.mjs", "--json", ...args],
		{
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
			maxBuffer: 4 * 1024 * 1024,
		},
	);
}

function parseJson(text, label) {
	try {
		return JSON.parse(text);
	} catch (error) {
		assert.fail(
			`${label} did not return valid JSON: ${error?.message || error}\n${text}`,
		);
	}
}

test("Rust proof output sanitizer removes Windows and POSIX repository roots", () => {
	const windowsRoot = "C:\\Users\\Alice\\Projects\\mcpace";
	const windowsOutput = sanitizeProofText(
		`Compiling mcpace (${windowsRoot})\nerror: ${windowsRoot}\\src\\lib.rs`,
		windowsRoot,
	);
	assert.equal(windowsOutput.includes("Alice"), false);
	assert.match(windowsOutput, /<repo>/);

	const posixRoot = "/home/alice/projects/mcpace";
	const posixOutput = sanitizeProofText(
		`Compiling mcpace (${posixRoot})\nerror: ${posixRoot}/src/lib.rs`,
		posixRoot,
	);
	assert.equal(posixOutput.includes("/home/alice"), false);
	assert.match(posixOutput, /<repo>/);
});

function writeStaleRustTree(dir) {
	fs.writeFileSync(
		path.join(dir, "rust-toolchain.toml"),
		'[toolchain]\nchannel = "1.95.0"\n',
	);
	fs.writeFileSync(
		path.join(dir, "Cargo.toml"),
		'[package]\nname = "demo"\nversion = "0.1.0"\n\n[dependencies]\nclap = "4"\nserde = "1"\ngetrandom = "0.3"\n',
	);
	fs.writeFileSync(
		path.join(dir, "Cargo.lock"),
		'[[package]]\nname = "getrandom"\nversion = "0.2.17"\n',
	);
}

test("rust live proof reports native build blockers without failing report mode", () => {
	const dir = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-rust-live-report-"),
	);
	try {
		writeStaleRustTree(dir);
		const result = runRustProof(["--skip-build", "--repo", dir]);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const report = parseJson(result.stdout, "Rust live report");
		assert.equal(report.schema, "mcpace.rustLiveProof.v1");
		assert.equal(report.status, "blocked");
		assert.ok(
			report.findings.some((item) => item.id === "cargo-lock-synchronized"),
		);
		assert.ok(report.releaseHostCommandPlan.includes("cargo check --locked"));
		assert.ok(
			report.releaseHostCommandPlan.includes(
				"cargo clippy --locked --all-targets -- -D warnings",
			),
		);
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
});

test("rust live proof enforce mode fails closed on a minimal stale Rust tree", () => {
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-rust-live-proof-"));
	try {
		writeStaleRustTree(dir);
		const result = runRustProof(["--enforce", "--skip-build", "--repo", dir]);
		assert.notEqual(result.status, 0, result.stdout);
		const report = parseJson(result.stdout, "stale Rust live fixture");
		assert.equal(report.status, "blocked");
		assert.ok(
			report.findings.some(
				(item) =>
					item.id === "cargo-lock-synchronized" && item.status === "blocker",
			),
		);
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
});

test("rust live proof scripts are exposed in package metadata and CI list", () => {
	const packageJson = readRootPackageJson();
	const proofSource = fs.readFileSync(
		path.join(repoRoot, "scripts", "rust-live-proof.mjs"),
		"utf8",
	);
	assert.match(
		proofSource,
		/DEFAULT_TIMEOUT_MS\s*=\s*30\s*\*\s*60\s*\*\s*1000/,
		"cold Windows release checks need a load-tolerant per-command deadline",
	);
	const endgameSource = fs.readFileSync(
		path.join(repoRoot, "scripts", "endgame-readiness.mjs"),
		"utf8",
	);
	assert.match(
		endgameSource,
		/LONG_TIMEOUT_MS\s*=\s*130\s*\*\s*60\s*\*\s*1000/,
	);
	const ciSource = fs.readFileSync(
		path.join(repoRoot, "scripts", "check-ci.mjs"),
		"utf8",
	);
	assert.match(
		ciSource,
		/ENDGAME_TIMEOUT_MS\s*=\s*135\s*\*\s*60\s*\*\s*1000/,
	);
	assert.equal(
		packageJson.scripts["proof:rust-live"],
		"node scripts/rust-live-proof.mjs --json",
	);
	assert.equal(
		packageJson.scripts["proof:rust-live:enforce"],
		"node scripts/rust-live-proof.mjs --json --enforce",
	);
	const ciList = spawnSync(
		process.execPath,
		["scripts/check-ci.mjs", "--list", "--json"],
		{
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
		},
	);
	assert.equal(ciList.status, 0, ciList.stderr || ciList.stdout);
	const listed = parseJson(ciList.stdout, "check:ci list");
	assert.equal(
		listed.steps.some((step) => step.label === "proof:rust-live"),
		false,
	);
	const endgameStep = listed.steps.find(
		(step) => step.label === "check:endgame",
	);
	assert.ok(endgameStep);
	assert.match(endgameStep.command, /--write-rust-proof/);
	const filteredList = spawnSync(
		process.execPath,
		[
			"scripts/check-ci.mjs",
			"--list",
			"--json",
			"--skip-native-proofs",
		],
		{
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
		},
	);
	assert.equal(filteredList.status, 0, filteredList.stderr || filteredList.stdout);
	const filtered = parseJson(filteredList.stdout, "filtered check:ci list");
	assert.equal(filtered.profile, "without-native-proofs");
	const filteredLabels = new Set(filtered.steps.map((step) => step.label));
	assert.deepEqual(
		listed.steps
			.filter((step) => !filteredLabels.has(step.label))
			.map((step) => step.label),
		["check:endgame", "proof:live-mcp-e2e"],
	);
	for (const retained of [
		"check:security-policy",
		"check:package",
		"check:install-smoke",
		"proof:browser-lifecycle",
		"check:publish-trust",
		"release:dry-run",
	]) {
		assert.equal(filteredLabels.has(retained), true, `${retained} must remain`);
	}
	const workflow = fs.readFileSync(
		path.join(repoRoot, ".github", "workflows", "ci.yml"),
		"utf8",
	);
	assert.equal(
		(workflow.match(/npm run check:endgame:enforce/g) || []).length,
		1,
	);
	assert.match(workflow, /npm run check:ci -- --skip-native-proofs/);
	assert.match(
		workflow,
		/npm run check:endgame:enforce -- --write-rust-proof/,
	);
	assert.equal(
		(workflow.match(/npm run proof:live-mcp-e2e:write/g) || []).length,
		1,
	);
	assert.doesNotMatch(workflow, /repository_visibility/);
	assert.equal(/npm run proof:rust-live:enforce/.test(workflow), false);
	assert.equal(/cargo build --release --locked --bins/.test(workflow), false);
});
