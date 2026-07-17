import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import test from "node:test";
import { sameStringRecord } from "../../scripts/lib/proof-records.mjs";
import { generatedReportFreshness } from "../../scripts/lib/report-freshness.mjs";
import {
	createVerifiedArtifactCopy,
	provenanceGeneratorSha256,
	rustBuildProvenance,
	sha256File,
	verifyRustProofBinding,
} from "../../scripts/lib/rust-build-provenance.mjs";

const repoRoot = path.resolve(import.meta.dirname, "..", "..");

function read(relativePath) {
	return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function parseJson(source, label) {
	try {
		return JSON.parse(source);
	} catch (error) {
		assert.fail(`${label} is not valid JSON: ${error?.message || error}`);
	}
}

function runAssuranceJson() {
	const result = spawnSync(
		process.execPath,
		["scripts/project-assurance.mjs", "--json"],
		{
			cwd: repoRoot,
			encoding: "utf8",
		},
	);
	assert.equal(result.status, 0, result.stderr || result.stdout);
	return parseJson(result.stdout, "project assurance output");
}

test("proof record validation rejects omitted and substituted inputs", () => {
	const expected = { "reports/rust-live-proof.json": "a", "src/main.rs": "b" };
	assert.equal(sameStringRecord({ ...expected }, expected), true);
	assert.equal(sameStringRecord({ "src/main.rs": "b" }, expected), false);
	assert.equal(
		sameStringRecord(
			{ "reports/other.json": "a", "src/main.rs": "b" },
			expected,
		),
		false,
	);
	assert.equal(
		sameStringRecord({ ...expected, "scripts/unrelated.mjs": "c" }, expected),
		false,
	);
});

test("generated report freshness ignores time but rejects substituted content", () => {
	const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-report-freshness-"));
	try {
		fs.mkdirSync(path.join(tmp, "reports"));
		const expected = {
			schema: "example.v1",
			generatedAt: "2026-07-18T00:00:00.000Z",
			status: "pass",
		};
		fs.writeFileSync(
			path.join(tmp, "reports", "example.json"),
			JSON.stringify({ ...expected, generatedAt: "2026-07-17T00:00:00.000Z" }),
		);
		fs.writeFileSync(path.join(tmp, "reports", "example.md"), "current\n");
		const options = {
			repoRoot: tmp,
			jsonPath: "reports/example.json",
			expectedReport: expected,
			markdownPath: "reports/example.md",
			expectedMarkdown: "current\n",
		};
		assert.deepEqual(generatedReportFreshness(options), []);
		fs.writeFileSync(
			path.join(tmp, "reports", "example.json"),
			JSON.stringify({ ...expected, status: "stale" }),
		);
		assert.match(generatedReportFreshness(options).join("\n"), /is stale/);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});

test("aggregate endgame enforcement does not rewrite source-bound proof evidence", () => {
	const endgame = read("scripts/endgame-readiness.mjs");
	assert.match(endgame, /args\.enforce \? \["--enforce"\] : \[\]/);
	assert.doesNotMatch(endgame, /args\.enforce \? \["--write"\] : \[\]/);
});

test("project assurance model checks user-visible truth, safety, and unverified gates", () => {
	const report = runAssuranceJson();
	assert.equal(report.schema, "mcpace.projectAssurance.v1");
	assert.equal(report.summary.fail, 0);
	assert.ok(
		report.summary.pass >= 8,
		"expected most assurance claims to be statically proven",
	);
	assert.equal(
		report.overall,
		report.summary.warn > 0 ? "needs-live-rust-proof" : "pass",
	);

	const claimIds = new Set(report.claims.map((claim) => claim.id));
	for (const id of [
		"safe-empty-default",
		"user-readiness",
		"operator-plan",
		"frontend-backend-contract",
		"server-launch-visible",
		"add-server-preflight",
		"http-boundary",
		"human-in-loop-tools",
		"release-reproducibility",
		"rust-runtime-unverified-here",
		"live-e2e-unverified-here",
	]) {
		assert.ok(claimIds.has(id), `missing assurance claim ${id}`);
	}

	assert.ok(
		report.correctVerificationFlow.some((step) => step.includes("check:rust")),
		"assurance flow must not hide the Rust-host gate",
	);
	assert.ok(
		report.reviewModel.some((item) => /hidden by default/i.test(item.question)),
		"assurance model must state what the user should not see",
	);
});

test("assurance artifacts are part of the release bundle contract", () => {
	const manifest = parseJson(
		read("release-manifest.json"),
		"release-manifest.json",
	);
	for (const required of [
		"scripts/project-assurance.mjs",
		"reports/assurance.md",
		"reports/assurance.json",
		"reports/bundle-manifest.json",
		"reports/frontend-qa.json",
		"reports/live-mcp-e2e-proof.json",
		"scripts/live-mcp-e2e-proof.mjs",
		"scripts/lib/proof-records.mjs",
		"scripts/lib/report-freshness.mjs",
		"scripts/lib/rust-build-provenance.mjs",
		"reports/rust-live-proof.json",
		"reports/supply-chain-evidence.json",
	]) {
		assert.ok(
			manifest.includePaths.includes(required),
			`release manifest missing ${required}`,
		);
	}

	const packageJson = parseJson(read("package.json"), "package.json");
	assert.match(packageJson.scripts.assurance, /project-assurance\.mjs --write/);
	assert.match(
		packageJson.scripts["check:assurance"],
		/project-assurance\.mjs --check/,
	);
	assert.match(packageJson.scripts.check, /check:assurance/);
	assert.match(packageJson.scripts["check:inventory"], /project-inventory\.mjs --check/);
	assert.match(packageJson.scripts.check, /check:inventory/);
	for (const script of [
		"scripts/project-assurance.mjs",
		"scripts/platform-proof.mjs",
		"scripts/project-inventory.mjs",
	]) {
		assert.match(read(script), /generatedReportFreshness/);
	}
	assert.match(
		packageJson.scripts["proof:live-mcp-e2e"],
		/live-mcp-e2e-proof\.mjs/,
	);
	assert.match(read("scripts/check-ci.mjs"), /proof:live-mcp-e2e/);
});

test("live MCP proof is source-bound and covers the full harmless user path", () => {
	const proof = parseJson(
		read("reports/live-mcp-e2e-proof.json"),
		"reports/live-mcp-e2e-proof.json",
	);
	assert.equal(proof.schema, "mcpace.liveMcpE2eProof.v1");
	assert.equal(proof.status, "pass");
	assert.equal(proof.readOnlyResult, "fixture:read-only-proof");
	for (const id of [
		"dashboard-add-disabled",
		"dashboard-enable",
		"dashboard-test",
		"client-initialize",
		"client-tools-list",
		"client-read-only-upstream-call",
		"runtime-resource-row",
		"process-tree-cleanup",
	]) {
		assert.ok(
			proof.steps.some((step) => step.id === id && step.status === "pass"),
			`live proof missing ${id}`,
		);
	}
	const provenance = rustBuildProvenance(repoRoot);
	assert.equal(proof.sourceFingerprint, provenance.fingerprint);
	assert.equal(proof.sourceFileCount, provenance.fileCount);
	assert.equal(Object.keys(proof.sourceFiles).length, provenance.fileCount);
	assert.equal(
		proof.proofGeneratorSha256,
		sha256File(path.join(repoRoot, "scripts/live-mcp-e2e-proof.mjs")),
	);
	for (const relativePath of [
		"package.json",
		"reports/rust-live-proof.json",
		"scripts/lib/rust-build-provenance.mjs",
		"scripts/live-mcp-e2e-proof.mjs",
	]) {
		assert.equal(
			proof.proofInputs?.[relativePath],
			sha256File(path.join(repoRoot, relativePath)),
			`live proof input is stale for ${relativePath}`,
		);
	}
	assert.equal(
		proof.rustBuildBinding?.proofGeneratorSha256,
		sha256File(path.join(repoRoot, "scripts/rust-live-proof.mjs")),
	);
	assert.equal(
		proof.rustBuildBinding?.provenanceGeneratorSha256,
		provenanceGeneratorSha256(),
	);
	assert.equal(
		proof.rustBuildBinding?.releaseArtifact?.sourceFingerprint,
		provenance.fingerprint,
	);
	assert.equal(
		proof.rustBuildBinding?.releaseArtifact?.sha256,
		proof.binarySha256,
	);
	assert.deepEqual(proof.proofInputSnapshots?.before, proof.proofInputs);
	assert.deepEqual(proof.proofInputSnapshots?.after, proof.proofInputs);
	assert.deepEqual(proof.binaryStability, {
		selectedBeforeSha256: proof.binarySha256,
		privateCopyBeforeSha256: proof.binarySha256,
		privateCopyAfterSha256: proof.binarySha256,
		selectedAfterSha256: proof.binarySha256,
		strategy: "private-hash-verified-copy",
		stable: true,
	});
	assert.equal(proof.processTreeCleanup?.verified, true);
	assert.ok(proof.processTreeCleanup?.ownedPids?.length >= 1);
	for (const [relativePath, expectedHash] of Object.entries(
		proof.sourceFiles,
	)) {
		const actualHash = crypto
			.createHash("sha256")
			.update(fs.readFileSync(path.join(repoRoot, relativePath)))
			.digest("hex");
		assert.equal(
			actualHash,
			expectedHash,
			`live proof is stale for ${relativePath}`,
		);
	}
});

test("Rust proof binding rejects changed sources and foreign binaries", () => {
	const root = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-proof-binding-"));
	try {
		fs.mkdirSync(path.join(root, "src"), { recursive: true });
		fs.writeFileSync(
			path.join(root, "Cargo.toml"),
			"[package]\nname='fixture'\nversion='0.1.0'\n",
		);
		fs.writeFileSync(path.join(root, "Cargo.lock"), "# fixture\n");
		fs.writeFileSync(
			path.join(root, "rust-toolchain.toml"),
			"[toolchain]\nchannel='1.95.0'\n",
		);
		fs.writeFileSync(path.join(root, "src", "main.rs"), "fn main() {}\n");
		const binary = path.join(root, "mcpace-fixture");
		const foreign = path.join(root, "mcpace-foreign");
		const generator = path.join(root, "rust-live-proof.mjs");
		fs.writeFileSync(binary, "bound binary\n");
		fs.writeFileSync(foreign, "foreign binary\n");
		fs.writeFileSync(generator, "// proof generator\n");
		const provenance = rustBuildProvenance(root);
		const proofSnapshot = {
			sourceFingerprint: provenance.fingerprint,
			sourceFileCount: provenance.fileCount,
			proofGeneratorSha256: sha256File(generator),
			provenanceGeneratorSha256: provenanceGeneratorSha256(),
		};
		const binarySha256 = sha256File(binary);
		const report = {
			schema: "mcpace.rustLiveProof.v1",
			status: "pass",
			blockers: 0,
			releaseBuildExecuted: true,
			proofGeneratorSha256: proofSnapshot.proofGeneratorSha256,
			provenanceGeneratorSha256: proofSnapshot.provenanceGeneratorSha256,
			proofInputSnapshots: {
				before: proofSnapshot,
				after: proofSnapshot,
			},
			rustBuildInputs: {
				fingerprint: provenance.fingerprint,
				fileCount: provenance.fileCount,
			},
			releaseArtifact: {
				sha256: binarySha256,
				sourceFingerprint: provenance.fingerprint,
			},
			releaseArtifactStability: {
				beforeSha256: binarySha256,
				afterSha256: binarySha256,
				stable: true,
			},
		};
		assert.equal(
			verifyRustProofBinding({
				repoRoot: root,
				binaryPath: binary,
				report,
				proofGeneratorPath: generator,
			}).binarySha256,
			report.releaseArtifact.sha256,
		);
		const unstableGenerator = structuredClone(report);
		unstableGenerator.proofInputSnapshots.after.proofGeneratorSha256 =
			"0".repeat(64);
		assert.throws(
			() =>
				verifyRustProofBinding({
					repoRoot: root,
					binaryPath: binary,
					report: unstableGenerator,
					proofGeneratorPath: generator,
				}),
			/stabilize sources and proof generators/,
		);
		const unstableArtifact = structuredClone(report);
		unstableArtifact.releaseArtifactStability.afterSha256 = "0".repeat(64);
		assert.throws(
			() =>
				verifyRustProofBinding({
					repoRoot: root,
					binaryPath: binary,
					report: unstableArtifact,
					proofGeneratorPath: generator,
				}),
			/not bound to the current Rust proof/,
		);
		const privateCopy = path.join(root, "private-execution-copy");
		createVerifiedArtifactCopy(binary, privateCopy, binarySha256);
		fs.writeFileSync(binary, "replacement bytes\n");
		assert.equal(sha256File(privateCopy), binarySha256);
		fs.writeFileSync(binary, "bound binary\n");
		assert.throws(
			() =>
				createVerifiedArtifactCopy(
					foreign,
					path.join(root, "rejected-private-copy"),
					binarySha256,
				),
			/private execution artifact does not match/,
		);
		fs.writeFileSync(
			path.join(root, "src", "main.rs"),
			'fn main() { println!("changed"); }\n',
		);
		assert.throws(
			() =>
				verifyRustProofBinding({
					repoRoot: root,
					binaryPath: binary,
					report,
					proofGeneratorPath: generator,
				}),
			/stabilize sources and proof generators|current Rust build inputs/,
		);
		fs.writeFileSync(path.join(root, "src", "main.rs"), "fn main() {}\n");
		assert.throws(
			() =>
				verifyRustProofBinding({
					repoRoot: root,
					binaryPath: foreign,
					report,
					proofGeneratorPath: generator,
				}),
			/not bound to the current Rust proof/,
		);
	} finally {
		fs.rmSync(root, { recursive: true, force: true });
	}
});

test("live proof cleanup kills a detached descendant after its leader exits", async () => {
	const { stopProcessTree } = await import(
		"../../scripts/live-mcp-e2e-proof.mjs"
	);
	const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-proof-cleanup-"));
	const pidFile = path.join(tmp, "descendant.pid");
	const descendantSource =
		'process.on("SIGTERM", () => {}); setInterval(() => {}, 1000);';
	const leaderSource = `
const fs = require("node:fs");
const { spawn } = require("node:child_process");
const descendant = spawn(process.execPath, ["-e", ${JSON.stringify(descendantSource)}], { stdio: "ignore" });
fs.writeFileSync(process.argv[1], String(descendant.pid));
setTimeout(() => process.exit(0), 50);
`;
	const leader = spawn(process.execPath, ["-e", leaderSource, pidFile], {
		detached: true,
		stdio: "ignore",
	});
	let descendantPid;
	try {
		for (let attempt = 0; attempt < 100; attempt += 1) {
			if (fs.existsSync(pidFile)) {
				descendantPid = Number(fs.readFileSync(pidFile, "utf8"));
				break;
			}
			await new Promise((resolve) => setTimeout(resolve, 20));
		}
		assert.ok(descendantPid, "leader did not publish the descendant pid");
		if (leader.exitCode === null) {
			await new Promise((resolve) => leader.once("exit", resolve));
		}
		assert.notEqual(leader.exitCode, null, "leader must exit before cleanup");
		const cleanup = await stopProcessTree(leader, [descendantPid]);
		assert.equal(cleanup.verified, true);
		assert.deepEqual(cleanup.ownedPids, [descendantPid]);
		await new Promise((resolve) => setTimeout(resolve, 100));
		assert.throws(() => process.kill(descendantPid, 0), /ESRCH/);
	} finally {
		if (descendantPid) {
			try {
				process.kill(descendantPid, "SIGKILL");
			} catch {
				// Already cleaned up.
			}
		}
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});

test("generated proof reports use environment-neutral root metadata", () => {
	const commands = [
		["scripts/project-assurance.mjs", "--json", "mcpace.projectAssurance.v1"],
		["scripts/platform-proof.mjs", "--json", "mcpace.platformProof.v1"],
		["scripts/project-inventory.mjs", "--json", "mcpace.projectInventory.v1"],
	];
	for (const [script, flag, schema] of commands) {
		const result = spawnSync(process.execPath, [script, flag], {
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
		});
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const report = parseJson(result.stdout, `${script} output`);
		assert.equal(report.schema, schema);
		assert.equal(
			report.root,
			".",
			`${script} must not leak a machine-local absolute root`,
		);
		assert.equal(report.rootName, path.basename(repoRoot));
	}
});
