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

function run(script, args = [], options = {}) {
	return spawnSync(process.execPath, [script, "--json", ...args], {
		cwd: repoRoot,
		encoding: "utf8",
		windowsHide: true,
		maxBuffer: 8 * 1024 * 1024,
		...options,
	});
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

test("supply-chain evidence emits the final dependency/provenance source report", () => {
	const result = run("scripts/supply-chain-evidence.mjs");
	assert.equal(result.status, 0, result.stderr || result.stdout);
	const report = parseJson(result.stdout, "supply-chain evidence");
	assert.equal(report.schema, "mcpace.supplyChainEvidence.v1");
	assert.ok(["pass", "warn", "blocked"].includes(report.status));
	for (const id of [
		"npm-lock-integrity",
		"npm-install-scripts-disabled",
		"cargo-dependency-evidence",
		"workflow-supply-chain-shape",
		"release-manifest-hygiene",
	]) {
		assert.ok(
			report.findings.some((item) => item.id === id),
			`${id} should be reported`,
		);
	}
	assert.match(report.fileHashes.packageLock, /^[a-f0-9]{64}$/);
});

test("supply-chain evidence rejects lockfile-v3 packages with install scripts", () => {
	const dir = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-supply-chain-install-script-"),
	);
	try {
		fs.writeFileSync(
			path.join(dir, "package.json"),
			JSON.stringify({ scripts: {} }),
		);
		fs.writeFileSync(
			path.join(dir, "package-lock.json"),
			JSON.stringify({
				lockfileVersion: 3,
				packages: {
					"": {},
					"node_modules/example": {
						version: "1.0.0",
						resolved: "https://registry.npmjs.org/example/-/example-1.0.0.tgz",
						integrity: "sha512-test",
						hasInstallScript: true,
					},
				},
			}),
		);

		const result = run("scripts/supply-chain-evidence.mjs", ["--repo", dir]);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const report = parseJson(result.stdout, "fixture supply-chain evidence");
		const lockFinding = report.findings.find(
			(item) => item.id === "npm-lock-integrity",
		);
		assert.equal(lockFinding.status, "blocker");
		assert.deepEqual(lockFinding.lifecycleScripts, ["node_modules/example"]);
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
});

test("supply-chain workflow evidence follows multiline publish and check:ci gate ownership", () => {
	const dir = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-supply-chain-workflow-"),
	);
	try {
		fs.mkdirSync(path.join(dir, ".github", "workflows"), { recursive: true });
		fs.mkdirSync(path.join(dir, "scripts"), { recursive: true });
		fs.writeFileSync(
			path.join(dir, "package.json"),
			JSON.stringify({ scripts: {} }),
		);
		fs.writeFileSync(
			path.join(dir, "package-lock.json"),
			JSON.stringify({ lockfileVersion: 3, packages: { "": {} } }),
		);
		fs.writeFileSync(
			path.join(dir, ".github", "workflows", "publish-npm.yml"),
			"permissions:\n  id-token: write\njobs:\n  publish:\n    steps:\n      - run: |\n          npm publish \\\n            --access public \\\n            --provenance\n",
		);
		fs.writeFileSync(
			path.join(dir, ".github", "workflows", "release.yml"),
			"jobs:\n  release:\n    steps:\n      - run: npm run check:ci\n      - uses: actions/attest@0123456789012345678901234567890123456789\n",
		);
		fs.writeFileSync(
			path.join(dir, ".github", "workflows", "security.yml"),
			"uses: github/codeql-action/init@0123456789012345678901234567890123456789\nuses: ossf/scorecard-action@0123456789012345678901234567890123456789\n",
		);
		fs.writeFileSync(
			path.join(dir, "scripts", "check-ci.mjs"),
			"release-readiness.mjs --json --enforce\nendgame-readiness.mjs --json --enforce\n",
		);
		fs.writeFileSync(
			path.join(dir, "scripts", "endgame-readiness.mjs"),
			"rust-live-proof.mjs\n",
		);

		const result = run("scripts/supply-chain-evidence.mjs", ["--repo", dir]);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const report = parseJson(result.stdout, "workflow supply-chain fixture");
		const workflowFinding = report.findings.find(
			(item) => item.id === "workflow-supply-chain-shape",
		);
		assert.equal(workflowFinding.status, "pass");
		assert.ok(Object.values(workflowFinding.checks).every(Boolean));
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
});

test("endgame readiness aggregates final gates while preserving exact blockers", () => {
	const emptyPath = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-endgame-empty-path-"),
	);
	try {
		const env = { ...process.env };
		const pathKey =
			Object.keys(env).find((key) => key.toLowerCase() === "path") || "PATH";
		env[pathKey] = emptyPath;
		const result = run("scripts/endgame-readiness.mjs", [], { env });
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const report = parseJson(result.stdout, "endgame readiness");
		assert.equal(report.schema, "mcpace.endgameReadiness.v1");
		assert.ok(["pass", "warn", "blocked"].includes(report.status));
		for (const id of [
			"mcp-transport-contract",
			"rust-boundary-contract",
			"supply-chain-evidence",
			"release-readiness",
			"rust-live-proof",
			"modernization-budget",
			"source-archive-policy",
		]) {
			assert.ok(
				report.findings.some((item) => item.id === id),
				`${id} should be part of endgame readiness`,
			);
		}
		const rustFinding = report.findings.find(
			(item) => item.id === "rust-live-proof",
		);
		assert.equal(rustFinding?.status, "blocker");
		assert.ok(
			rustFinding?.diagnostics?.blockingFindings.some((item) =>
				item.id.startsWith("tool-"),
			),
			JSON.stringify(rustFinding),
		);
		assert.equal("stdoutTail" in rustFinding.diagnostics, false);
		assert.equal("stderrTail" in rustFinding.diagnostics, false);
		for (const finding of report.findings.filter(
			(item) => item.status === "blocker" && item.id !== "rust-live-proof",
		)) {
			for (const diagnostic of finding.diagnostics.blockingFindings) {
				assert.deepEqual(Object.keys(diagnostic).sort(), ["id", "status"]);
			}
		}
		assert.ok(
			report.endgameDefinition.some((item) =>
				item.includes("cargo check/test/fmt/clippy/build"),
			),
		);
		assert.ok(
			report.endgameDefinition.some((item) =>
				item.includes("Rust typed-boundary"),
			),
		);
	} finally {
		fs.rmSync(emptyPath, { recursive: true, force: true });
	}
});

test("endgame readiness does not publish unparsed child output", () => {
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-endgame-redaction-"));
	const scriptDir = path.join(dir, "scripts");
	const secret = "SUPER_SECRET_PARSE_PAYLOAD";
	try {
		fs.mkdirSync(scriptDir, { recursive: true });
		for (const script of [
			"mcp-transport-contract.mjs",
			"rust-boundary-contract.mjs",
			"supply-chain-evidence.mjs",
			"release-readiness.mjs",
			"modernization-inventory.mjs",
			"verify-modernization-budget.mjs",
			"verify-clean-archive.mjs",
		]) {
			fs.writeFileSync(
				path.join(scriptDir, script),
				`console.log(${JSON.stringify(JSON.stringify({ status: "pass", findings: [] }))});\n`,
			);
		}
		fs.writeFileSync(
			path.join(scriptDir, "rust-live-proof.mjs"),
			`process.stdout.write(${JSON.stringify(secret)});\n`,
		);

		const result = run("scripts/endgame-readiness.mjs", ["--repo", dir]);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		assert.doesNotMatch(result.stdout, new RegExp(secret));
		const report = parseJson(result.stdout, "redacted endgame readiness");
		const rustFinding = report.findings.find(
			(item) => item.id === "rust-live-proof",
		);
		assert.equal(rustFinding.status, "blocker");
		assert.equal(rustFinding.detail, "script failed (INVALID_JSON)");
		assert.deepEqual(rustFinding.diagnostics.blockingFindings, []);

		fs.writeFileSync(
			path.join(scriptDir, "rust-live-proof.mjs"),
			`console.log(JSON.stringify({ schema: "untrusted", status: "blocked", findings: [{ id: "cargo-test-locked", status: "blocker", detail: ${JSON.stringify(secret)}, stdoutTail: ${JSON.stringify(secret)} }] }));\n`,
		);
		const untrustedResult = run("scripts/endgame-readiness.mjs", [
			"--repo",
			dir,
		]);
		assert.equal(
			untrustedResult.status,
			0,
			untrustedResult.stderr || untrustedResult.stdout,
		);
		assert.doesNotMatch(untrustedResult.stdout, new RegExp(secret));
		const untrustedReport = parseJson(
			untrustedResult.stdout,
			"untrusted-schema endgame readiness",
		);
		const untrustedRustFinding = untrustedReport.findings.find(
			(item) => item.id === "rust-live-proof",
		);
		assert.deepEqual(untrustedRustFinding.diagnostics.blockingFindings, [
			{ id: "cargo-test-locked", status: "blocker" },
		]);
		assert.equal(untrustedRustFinding.command.startsWith("node "), true);
		assert.deepEqual(untrustedReport.releaseBlockingFindings, [
			{
				gate: "rust-live-proof",
				id: "cargo-test-locked",
				detail: "blocked",
			},
		]);
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
});

test("endgame and supply-chain scripts are exposed in package metadata and CI list", () => {
	const packageJson = readRootPackageJson();
	assert.equal(
		packageJson.scripts["check:supply-chain-evidence"],
		"node scripts/supply-chain-evidence.mjs --json",
	);
	assert.equal(
		packageJson.scripts["check:rust-boundaries"],
		"node scripts/rust-boundary-contract.mjs --json",
	);
	assert.equal(
		packageJson.scripts["check:endgame"],
		"node scripts/endgame-readiness.mjs --json",
	);
	assert.equal(
		packageJson.scripts["check:endgame:enforce"],
		"node scripts/endgame-readiness.mjs --json --enforce",
	);
	const ciList = run("scripts/check-ci.mjs", ["--list"]);
	assert.equal(ciList.status, 0, ciList.stderr || ciList.stdout);
	const listed = parseJson(ciList.stdout, "check:ci list");
	assert.ok(
		listed.steps.some((step) => step.label === "check:rust-boundaries"),
	);
	assert.ok(
		listed.steps.some((step) => step.label === "check:supply-chain-evidence"),
	);
	assert.ok(listed.steps.some((step) => step.label === "check:endgame"));
});
