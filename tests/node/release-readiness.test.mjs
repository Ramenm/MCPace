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

function runReleaseReadiness(args = [], options = {}) {
	return spawnSync(
		process.execPath,
		["scripts/release-readiness.mjs", "--json", ...args],
		{
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
			maxBuffer: 4 * 1024 * 1024,
			...options,
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

test("release readiness reports Rust/live-release blockers without failing the fast local report mode", () => {
	const result = runReleaseReadiness();
	assert.equal(result.status, 0, result.stderr || result.stdout);
	const report = parseJson(result.stdout, "release-readiness report");
	assert.equal(report.schema, "mcpace.releaseReadiness.v1");
	assert.ok(["pass", "warn", "blocked"].includes(report.status));
	assert.ok(Array.isArray(report.requiredCommandPlan));
	assert.ok(
		report.requiredCommandPlan.some((command) =>
			command.startsWith("npm run check:ci"),
		),
	);
	for (const duplicateOwner of [
		"cargo check --locked",
		"npm run proof:rust-live:enforce",
		"npm run check:release-ready:enforce",
		"npm run check:endgame:enforce",
	]) {
		assert.equal(report.requiredCommandPlan.includes(duplicateOwner), false);
	}
	assert.ok(
		report.findings.some((item) => item.id === "cargo-lock-synchronized"),
	);
	for (const id of [
		"publish-workflow-requests-provenance",
		"release-workflow-enforces-release-ready",
		"release-workflow-enforces-rust-live-proof",
	]) {
		assert.ok(
			report.findings.some((item) => item.id === id && item.status === "pass"),
			`${id} should pass`,
		);
	}
});

test("release readiness enforce mode fails closed on an intentionally stale minimal release tree", () => {
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-release-ready-"));
	try {
		fs.mkdirSync(path.join(dir, ".github/workflows"), { recursive: true });
		fs.writeFileSync(
			path.join(dir, "rust-toolchain.toml"),
			'[toolchain]\nchannel = "99.99.99"\n',
		);
		fs.writeFileSync(
			path.join(dir, "Cargo.toml"),
			'[package]\nname = "demo"\nversion = "0.1.0"\n\n[dependencies]\nclap = "4"\nserde = "1"\ngetrandom = "0.3"\n',
		);
		fs.writeFileSync(
			path.join(dir, "Cargo.lock"),
			'[[package]]\nname = "getrandom"\nversion = "0.2.17"\n',
		);
		fs.writeFileSync(
			path.join(dir, "package.json"),
			JSON.stringify({ scripts: { "check:ci": "node scripts/check-ci.mjs" } }),
		);
		fs.writeFileSync(
			path.join(dir, ".github/workflows/publish-npm.yml"),
			"permissions:\n  id-token: write\njobs:\n  publish:\n    steps:\n      - run: npm publish --provenance\n",
		);
		fs.writeFileSync(
			path.join(dir, ".github/workflows/release.yml"),
			"permissions:\n  id-token: write\n  attestations: write\njobs:\n  release:\n    steps:\n      - uses: actions/attest@v3\n",
		);

		const result = runReleaseReadiness(["--enforce", "--repo", dir]);
		assert.notEqual(result.status, 0, result.stdout);
		const report = parseJson(result.stdout, "stale release-readiness fixture");
		assert.equal(report.status, "blocked");
		assert.ok(
			report.findings.some(
				(item) => item.id === "tool-rustc" && item.status === "blocker",
			),
		);
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

test("release readiness gate is exposed as report and enforce scripts", () => {
	const packageJson = readRootPackageJson();
	assert.equal(
		packageJson.scripts["check:rust-boundaries"],
		"node scripts/rust-boundary-contract.mjs --json",
	);
	assert.equal(
		packageJson.scripts["check:release-ready"],
		"node scripts/release-readiness.mjs --json",
	);
	assert.equal(
		packageJson.scripts["check:release-ready:enforce"],
		"node scripts/release-readiness.mjs --json --enforce",
	);
	assert.equal(
		packageJson.scripts["proof:rust-live:enforce"],
		"node scripts/rust-live-proof.mjs --json --enforce",
	);
	assert.equal(
		packageJson.scripts["check:endgame:enforce"],
		"node scripts/endgame-readiness.mjs --json --enforce",
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
	assert.ok(
		listed.steps.some((step) => step.label === "check:rust-boundaries"),
	);
	assert.ok(listed.steps.some((step) => step.label === "check:release-ready"));
	assert.equal(
		listed.steps.some((step) => step.label === "proof:rust-live"),
		false,
	);
	assert.ok(
		listed.steps.some(
			(step) =>
				step.label === "check:endgame" && step.timeoutMs >= 20 * 60 * 1000,
		),
	);
});
