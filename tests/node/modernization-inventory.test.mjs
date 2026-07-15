import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { repoRoot } from "../../scripts/lib/project-metadata.mjs";

test("modernization inventory detects remaining self-owned infrastructure seams without reintroducing compat crates", () => {
	const result = spawnSync(
		process.execPath,
		["scripts/modernization-inventory.mjs", "--json"],
		{
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
		},
	);
	assert.equal(result.status, 0, result.stderr || result.stdout);
	let report;
	try {
		report = JSON.parse(result.stdout);
	} catch (error) {
		assert.fail(
			`inventory did not return valid JSON: ${error?.message || error}`,
		);
	}
	assert.equal(report.schema, "mcpace.modernizationInventory.v1");
	const ids = new Set(report.findings.map((item) => item.id));
	assert.equal(
		ids.has("cargo-path-compat-dependencies"),
		false,
		"upstream standard crates should not be redirected to crates/compat",
	);
	if (ids.has("cargo-lock-needs-refresh")) {
		const lockFinding = report.findings.find(
			(item) => item.id === "cargo-lock-needs-refresh",
		);
		assert.equal(lockFinding.severity, "high");
		assert.ok(lockFinding.recommendation.includes("Cargo.lock"));
	}
	assert.equal(
		ids.has("manual-cli-parsing"),
		false,
		"clap migration should keep handwritten argv parsing at zero",
	);
	assert.equal(
		ids.has("manual-config-patching"),
		false,
		"client config patching should stay centralized behind src/config_edit.rs",
	);
});

test("stringly-error inventory parses Result error type instead of guessing with a shallow regex", () => {
	const script = fs.readFileSync(
		path.join(repoRoot, "scripts/modernization-inventory.mjs"),
		"utf8",
	);
	assert.match(script, /function resultStringErrorSpans/);
	assert.match(script, /depth \+= 1/);
	assert.match(script, /errorType === ["']String["']/);
	assert.doesNotMatch(script, /Result<\[\^>\]\+,\\s\*String/);
});
