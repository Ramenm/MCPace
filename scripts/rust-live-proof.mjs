#!/usr/bin/env node
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
	cargoLockRefreshFindings,
	cargoLockRefreshMessage,
} from "./lib/cargo-policy.mjs";
import { repoRoot as defaultRepoRoot } from "./lib/project-metadata.mjs";
import { childEnvForCommand } from "./lib/safe-child-env.mjs";
import { writeFileAtomicSync } from "./lib/atomic-fs.mjs";
import {
	provenanceGeneratorSha256,
	releaseBinaryPath,
	rustBuildProvenance,
	sha256File,
} from "./lib/rust-build-provenance.mjs";

// Windows debug linking and antivirus scanning can make a cold locked Cargo
// command exceed eight minutes even when it is still making progress.
const SCRIPT_PATH = fileURLToPath(import.meta.url);
const DEFAULT_TIMEOUT_MS = 30 * 60 * 1000;
const SHORT_TIMEOUT_MS = 20_000;

function parseArgs(argv) {
	const args = {
		json: false,
		enforce: false,
		write: false,
		repoRoot: defaultRepoRoot,
		report: "reports/rust-live-proof.json",
		skipBuild: false,
	};
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (arg === "--json") args.json = true;
		else if (arg === "--enforce") args.enforce = true;
		else if (arg === "--write") args.write = true;
		else if (arg === "--skip-build") args.skipBuild = true;
		else if (arg === "--repo") args.repoRoot = path.resolve(argv[++index]);
		else if (arg === "--report") args.report = argv[++index];
		else if (arg === "--help" || arg === "-h") {
			console.log(
				"Usage: node scripts/rust-live-proof.mjs [--json] [--enforce] [--write] [--skip-build] [--repo DIR] [--report PATH]",
			);
			process.exit(0);
		} else {
			throw new Error(`unknown argument: ${arg}`);
		}
	}
	return args;
}

function replaceLiteral(value, search, replacement, caseInsensitive) {
	if (!caseInsensitive) return value.split(search).join(replacement);
	const foldedValue = value.toLowerCase();
	const foldedSearch = search.toLowerCase();
	let result = "";
	let cursor = 0;
	while (cursor < value.length) {
		const match = foldedValue.indexOf(foldedSearch, cursor);
		if (match < 0) {
			result += value.slice(cursor);
			break;
		}
		result += value.slice(cursor, match);
		result += replacement;
		cursor = match + search.length;
	}
	return result;
}

export function sanitizeProofText(value, repoRoot) {
	let sanitized = String(value ?? "");
	const replacements = new Map();
	const addPath = (candidate, replacement) => {
		if (!candidate || String(candidate).length < 3) return;
		const raw = String(candidate);
		replacements.set(raw, replacement);
		replacements.set(raw.replaceAll("\\", "/"), replacement);
	};
	addPath(repoRoot, "<repo>");
	addPath(path.resolve(repoRoot), "<repo>");
	addPath(os.homedir(), "~");
	for (const name of ["USERPROFILE", "HOME", "APPDATA", "LOCALAPPDATA"]) {
		addPath(process.env[name], `<${name.toLowerCase()}>`);
	}
	for (const [candidate, replacement] of [...replacements.entries()].sort(
		(left, right) => right[0].length - left[0].length,
	)) {
		const caseInsensitive =
			process.platform === "win32" || /^[a-z]:[\\/]/i.test(candidate);
		sanitized = replaceLiteral(
			sanitized,
			candidate,
			replacement,
			caseInsensitive,
		);
	}
	return sanitized;
}

function readTextIfExists(repoRoot, relativePath) {
	const file = path.join(repoRoot, relativePath);
	return fs.existsSync(file) ? fs.readFileSync(file, "utf8") : "";
}

function pinnedRust(repoRoot) {
	return (
		readTextIfExists(repoRoot, "rust-toolchain.toml").match(
			/^channel\s*=\s*"([^"]+)"/m,
		)?.[1] || null
	);
}

function commandLabel(command, args = []) {
	return [command, ...args].join(" ");
}

function spawnCapture(
	repoRoot,
	command,
	args = [],
	timeoutMs = DEFAULT_TIMEOUT_MS,
) {
	const startedAt = Date.now();
	const result = spawnSync(command, args, {
		cwd: repoRoot,
		encoding: "utf8",
		env: childEnvForCommand(command),
		maxBuffer: 16 * 1024 * 1024,
		shell: false,
		timeout: timeoutMs,
		windowsHide: true,
	});
	return {
		command: commandLabel(command, args),
		status: result.status,
		signal: result.signal,
		ok: !result.error && result.status === 0,
		error: result.error?.message || null,
		timedOut: result.error?.code === "ETIMEDOUT",
		stdout: String(result.stdout || "").trim(),
		stderr: String(result.stderr || "").trim(),
		durationMs: Date.now() - startedAt,
	};
}

function versionProbe(
	repoRoot,
	id,
	command,
	args,
	expectedFragment,
	missingDetail,
) {
	const result = spawnCapture(repoRoot, command, args, SHORT_TIMEOUT_MS);
	if (!result.ok) {
		const error =
			result.error || result.stderr || result.stdout || `exit ${result.status}`;
		return {
			id,
			status: "blocker",
			detail: missingDetail,
			command: sanitizeProofText(result.command, repoRoot),
			error: sanitizeProofText(error, repoRoot),
			durationMs: result.durationMs,
		};
	}
	const output = `${result.stdout}\n${result.stderr}`.trim();
	const reportOutput = sanitizeProofText(
		output.split(/\r?\n/)[0] || "ok",
		repoRoot,
	);
	if (expectedFragment && !output.includes(expectedFragment)) {
		return {
			id,
			status: "blocker",
			detail: `installed tool does not match pinned Rust ${expectedFragment}`,
			command: sanitizeProofText(result.command, repoRoot),
			output: reportOutput,
			durationMs: result.durationMs,
		};
	}
	return {
		id,
		status: "pass",
		detail: `${command} is available`,
		command: sanitizeProofText(result.command, repoRoot),
		output: reportOutput,
		durationMs: result.durationMs,
	};
}

function rustfmtProbe(repoRoot) {
	const rustc = spawnCapture(repoRoot, "rustc", ["-Vv"], SHORT_TIMEOUT_MS);
	const rustfmt = spawnCapture(
		repoRoot,
		"rustfmt",
		["--version", "--verbose"],
		SHORT_TIMEOUT_MS,
	);
	const commit = rustc.stdout.match(/^commit-hash:\s*([0-9a-f]+)/im)?.[1];
	const output = `${rustfmt.stdout}\n${rustfmt.stderr}`.trim();
	const reportOutput = sanitizeProofText(
		output.split(/\r?\n/)[0] || "ok",
		repoRoot,
	);
	if (!rustfmt.ok) {
		const error =
			rustfmt.error ||
			rustfmt.stderr ||
			rustfmt.stdout ||
			`exit ${rustfmt.status}`;
		return {
			id: "tool-rustfmt",
			status: "blocker",
			detail: "rustfmt is unavailable",
			command: sanitizeProofText(rustfmt.command, repoRoot),
			error: sanitizeProofText(error, repoRoot),
			durationMs: rustfmt.durationMs,
		};
	}
	if (!commit || !output.includes(commit.slice(0, 10))) {
		return {
			id: "tool-rustfmt",
			status: "blocker",
			detail: "rustfmt toolchain commit does not match rustc",
			command: sanitizeProofText(rustfmt.command, repoRoot),
			output: reportOutput,
			rustcCommit: commit || null,
			durationMs: rustfmt.durationMs,
		};
	}
	return {
		id: "tool-rustfmt",
		status: "pass",
		detail: "rustfmt toolchain commit matches rustc",
		command: sanitizeProofText(rustfmt.command, repoRoot),
		output: reportOutput,
		rustcCommit: commit,
		durationMs: rustfmt.durationMs,
	};
}

function cargoStep(repoRoot, id, args, detail, timeoutMs = DEFAULT_TIMEOUT_MS) {
	const result = spawnCapture(repoRoot, "cargo", args, timeoutMs);
	return {
		id,
		status: result.ok ? "pass" : "blocker",
		detail: result.ok ? detail : `${detail} failed`,
		command: sanitizeProofText(result.command, repoRoot),
		exitCode: result.status,
		signal: result.signal,
		timedOut: result.timedOut,
		error: result.error ? sanitizeProofText(result.error, repoRoot) : null,
		stdoutTail: sanitizeProofText(
			result.stdout.split(/\r?\n/).slice(-25).join("\n"),
			repoRoot,
		),
		stderrTail: sanitizeProofText(
			result.stderr.split(/\r?\n/).slice(-25).join("\n"),
			repoRoot,
		),
		durationMs: result.durationMs,
	};
}

function writeReport(repoRoot, relativePath, report) {
	const target = path.isAbsolute(relativePath)
		? relativePath
		: path.join(repoRoot, relativePath);
	writeFileAtomicSync(target, `${JSON.stringify(report, null, 2)}\n`, {
		mode: 0o644,
	});
}

function proofInputSnapshot(repoRoot) {
	const provenance = rustBuildProvenance(repoRoot);
	return {
		provenance,
		compact: {
			sourceFingerprint: provenance.fingerprint,
			sourceFileCount: provenance.fileCount,
			proofGeneratorSha256: sha256File(SCRIPT_PATH),
			provenanceGeneratorSha256: provenanceGeneratorSha256(),
		},
	};
}

function sameProofInputSnapshot(left, right) {
	return (
		left.sourceFingerprint === right.sourceFingerprint &&
		left.sourceFileCount === right.sourceFileCount &&
		left.proofGeneratorSha256 === right.proofGeneratorSha256 &&
		left.provenanceGeneratorSha256 === right.provenanceGeneratorSha256
	);
}

function run(args) {
	const repoRoot = args.repoRoot;
	const proofBefore = proofInputSnapshot(repoRoot);
	const pinned = pinnedRust(repoRoot);
	const findings = [
		{
			id: "rust-toolchain-pinned",
			status: pinned ? "pass" : "blocker",
			detail: pinned
				? `Rust toolchain is pinned to ${pinned}`
				: "rust-toolchain.toml must pin the Rust toolchain",
			pinnedRust: pinned,
		},
		versionProbe(
			repoRoot,
			"tool-rustc",
			"rustc",
			["--version"],
			pinned,
			`Install rustc ${pinned || "<pinned>"}.`,
		),
		versionProbe(
			repoRoot,
			"tool-cargo",
			"cargo",
			["--version"],
			null,
			`Install cargo from Rust ${pinned || "<pinned>"}.`,
		),
		rustfmtProbe(repoRoot),
		versionProbe(
			repoRoot,
			"tool-clippy",
			"cargo",
			["clippy", "--version"],
			null,
			`Install clippy for Rust ${pinned || "<pinned>"}.`,
		),
	];

	const lockIssues = cargoLockRefreshFindings(repoRoot);
	findings.push({
		id: "cargo-lock-synchronized",
		status: lockIssues.length === 0 ? "pass" : "blocker",
		detail: cargoLockRefreshMessage(lockIssues),
		issues: lockIssues,
	});

	const toolBlockers = findings.filter((item) => item.status === "blocker");
	const cargoSteps = [];
	if (toolBlockers.length === 0) {
		cargoSteps.push(
			cargoStep(
				repoRoot,
				"cargo-check-locked",
				["check", "--locked"],
				"cargo check --locked passed",
			),
		);
		cargoSteps.push(
			cargoStep(
				repoRoot,
				"cargo-test-locked",
				["test", "--locked", "--", "--test-threads=1"],
				"cargo test --locked passed",
			),
		);
		cargoSteps.push(
			cargoStep(
				repoRoot,
				"cargo-fmt-check",
				["fmt", "--check"],
				"cargo fmt --check passed",
				3 * 60 * 1000,
			),
		);
		cargoSteps.push(
			cargoStep(
				repoRoot,
				"cargo-clippy-locked",
				["clippy", "--locked", "--all-targets", "--", "-D", "warnings"],
				"cargo clippy --locked passed",
			),
		);
		if (!args.skipBuild)
			cargoSteps.push(
				cargoStep(
					repoRoot,
					"cargo-build-release-locked",
					["build", "--release", "--locked", "--bins"],
					"cargo build --release --locked --bins passed",
				),
			);
		findings.push(...cargoSteps);
	} else {
		findings.push({
			id: "cargo-live-commands",
			status: "blocker",
			detail:
				"Cargo live commands were not run because required Rust tools or Cargo.lock synchronization are missing.",
			skippedCommands: [
				"cargo check --locked",
				"cargo test --locked -- --test-threads=1",
				"cargo fmt --check",
				"cargo clippy --locked --all-targets -- -D warnings",
				args.skipBuild ? null : "cargo build --release --locked --bins",
			].filter(Boolean),
		});
	}

	const releaseBinary = releaseBinaryPath(repoRoot);
	const releaseBuildPassed = cargoSteps.some(
		(step) =>
			step.id === "cargo-build-release-locked" && step.status === "pass",
	);
	const artifactBeforeSha256 =
		!args.skipBuild && releaseBuildPassed && fs.existsSync(releaseBinary)
			? sha256File(releaseBinary)
			: null;
	const proofAfter = proofInputSnapshot(repoRoot);
	const proofInputsStable = sameProofInputSnapshot(
		proofBefore.compact,
		proofAfter.compact,
	);
	const artifactAfterSha256 = artifactBeforeSha256
		? sha256File(releaseBinary)
		: null;
	const artifactStable =
		artifactBeforeSha256 !== null &&
		artifactBeforeSha256 === artifactAfterSha256;
	findings.push({
		id: "rust-build-inputs-stable",
		status: proofInputsStable ? "pass" : "blocker",
		detail: proofInputsStable
			? `Rust sources and proof generators stayed stable across the proof (${proofAfter.provenance.fileCount} files)`
			: "Rust sources or proof generators changed while Cargo proof commands were running",
		before: proofBefore.compact,
		after: proofAfter.compact,
		fileCount: proofAfter.provenance.fileCount,
	});
	const releaseArtifact =
		!args.skipBuild && releaseBuildPassed && proofInputsStable && artifactStable
			? {
					path: path.relative(repoRoot, releaseBinary).replaceAll("\\", "/"),
					sha256: artifactAfterSha256,
					sourceFingerprint: proofBefore.provenance.fingerprint,
				}
			: null;
	if (!args.skipBuild) {
		findings.push({
			id: "release-binary-source-binding",
			status: releaseArtifact ? "pass" : "blocker",
			detail: releaseArtifact
				? "fresh release binary stayed stable while being SHA-256 bound to stable Rust sources and proof generators"
				: "release binary could not be bound to stable Rust sources and proof generators",
			artifact: releaseArtifact,
			artifactBeforeSha256,
			artifactAfterSha256,
		});
	}

	const blockers = findings.filter((item) => item.status === "blocker");
	const report = {
		schema: "mcpace.rustLiveProof.v1",
		generatedAt: new Date().toISOString(),
		repoRoot: ".",
		pinnedRust: pinned,
		status: blockers.length === 0 ? "pass" : "blocked",
		enforce: args.enforce,
		blockers: blockers.length,
		releaseBuildExecuted: !args.skipBuild,
		rustBuildInputs: {
			algorithm: proofBefore.provenance.algorithm,
			fingerprint: proofBefore.provenance.fingerprint,
			fileCount: proofBefore.provenance.fileCount,
		},
		proofInputSnapshots: {
			before: proofBefore.compact,
			after: proofAfter.compact,
		},
		releaseArtifact,
		releaseArtifactStability: args.skipBuild
			? null
			: {
					beforeSha256: artifactBeforeSha256,
					afterSha256: artifactAfterSha256,
					stable: artifactStable,
				},
		proofGeneratorSha256: proofBefore.compact.proofGeneratorSha256,
		provenanceGeneratorSha256: proofBefore.compact.provenanceGeneratorSha256,
		findings,
		releaseHostCommandPlan: [
			`rustup toolchain install ${pinned || "<pinned-rust>"} --component rustfmt --component clippy`,
			"cargo check --locked",
			"cargo test --locked -- --test-threads=1",
			"cargo fmt --check",
			"cargo clippy --locked --all-targets -- -D warnings",
			args.skipBuild ? null : "cargo build --release --locked --bins",
		].filter(Boolean),
	};
	if (args.write) writeReport(repoRoot, args.report, report);
	return report;
}

function main() {
	try {
		const args = parseArgs(process.argv.slice(2));
		const report = run(args);
		if (args.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
		else {
			console.log(
				`${report.status}: ${report.findings.length} Rust live proof checks, ${report.blockers} blockers`,
			);
			for (const finding of report.findings)
				console.log(`- ${finding.status}: ${finding.id} — ${finding.detail}`);
		}
		process.exitCode = args.enforce && report.blockers > 0 ? 1 : 0;
	} catch (error) {
		console.error(error?.stack || error);
		process.exitCode = 1;
	}
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : "";
const scriptPath = path.resolve(SCRIPT_PATH);
const isMain =
	process.platform === "win32"
		? invokedPath.toLowerCase() === scriptPath.toLowerCase()
		: invokedPath === scriptPath;
if (isMain) main();
