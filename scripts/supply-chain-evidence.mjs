#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { createHash } from "node:crypto";
import {
	cargoLockRefreshFindings,
	readCargoDependencySpecs,
	readCargoLockPackages,
	cargoLockRefreshMessage,
} from "./lib/cargo-policy.mjs";
import {
	readRootPackageJson,
	repoRoot as defaultRepoRoot,
} from "./lib/project-metadata.mjs";

const PUBLIC_NPM_REGISTRY = "https://registry.npmjs.org/";
const FORBIDDEN_RELEASE_PARTS = new Set([
	".git",
	"node_modules",
	"target",
	"dist",
	".artifacts",
	".cache",
	".pytest_cache",
	"__pycache__",
]);

function parseArgs(argv) {
	const args = {
		json: false,
		enforce: false,
		write: false,
		repoRoot: defaultRepoRoot,
		report: "reports/supply-chain-evidence.json",
	};
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (arg === "--json") args.json = true;
		else if (arg === "--enforce") args.enforce = true;
		else if (arg === "--write") args.write = true;
		else if (arg === "--repo") args.repoRoot = path.resolve(argv[++index]);
		else if (arg === "--report") args.report = argv[++index];
		else if (arg === "--help" || arg === "-h") {
			console.log(
				"Usage: node scripts/supply-chain-evidence.mjs [--json] [--enforce] [--write] [--repo DIR] [--report PATH]",
			);
			process.exit(0);
		} else {
			throw new Error(`unknown argument: ${arg}`);
		}
	}
	return args;
}

function readTextIfExists(repoRoot, relativePath) {
	const file = path.join(repoRoot, relativePath);
	return fs.existsSync(file) ? fs.readFileSync(file, "utf8") : "";
}

function readJsonIfExists(repoRoot, relativePath) {
	const text = readTextIfExists(repoRoot, relativePath);
	if (!text) return null;
	try {
		return JSON.parse(text);
	} catch (error) {
		throw new Error(
			`invalid JSON in ${relativePath}: ${error?.message || error}`,
			{ cause: error },
		);
	}
}

function sha256(repoRoot, relativePath) {
	const file = path.join(repoRoot, relativePath);
	if (!fs.existsSync(file)) return null;
	return createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

function finding(id, status, detail, extra = {}) {
	return { id, status, detail, ...extra };
}

function packageLockEvidence(repoRoot) {
	const lock = readJsonIfExists(repoRoot, "package-lock.json");
	if (!lock)
		return finding(
			"npm-lock-present",
			"blocker",
			"package-lock.json is missing",
		);
	const packages =
		lock.packages && typeof lock.packages === "object" ? lock.packages : {};
	const external = Object.entries(packages).filter(
		([name, pkg]) => name.startsWith("node_modules/") && !pkg.link,
	);
	const missingIntegrity = external
		.filter(([, pkg]) => !pkg.integrity && !pkg.link)
		.map(([name]) => name);
	const nonRegistry = external
		.filter(
			([, pkg]) =>
				typeof pkg.resolved === "string" &&
				!pkg.resolved.startsWith(PUBLIC_NPM_REGISTRY),
		)
		.map(([name, pkg]) => ({ package: name, resolved: pkg.resolved }));
	const lifecycleScripts = external
		.filter(
			([, pkg]) =>
				pkg.hasInstallScript === true ||
				(pkg.scripts &&
					Object.keys(pkg.scripts).some((script) =>
						/install|preinstall|postinstall|prepare/i.test(script),
					)),
		)
		.map(([name]) => name);
	const blockers = [];
	if (lock.lockfileVersion !== 3)
		blockers.push(`lockfileVersion is ${lock.lockfileVersion}, expected 3`);
	if (missingIntegrity.length)
		blockers.push(
			`${missingIntegrity.length} external packages lack integrity metadata`,
		);
	if (nonRegistry.length)
		blockers.push(
			`${nonRegistry.length} external packages resolve outside the npm public registry`,
		);
	if (lifecycleScripts.length)
		blockers.push(
			`${lifecycleScripts.length} external packages declare install lifecycle scripts`,
		);
	return finding(
		"npm-lock-integrity",
		blockers.length === 0 ? "pass" : "blocker",
		blockers.length === 0
			? `package-lock.json pins ${external.length} external npm packages with registry integrity evidence`
			: blockers.join("; "),
		{
			lockfileVersion: lock.lockfileVersion,
			externalPackageCount: external.length,
			missingIntegrity: missingIntegrity.slice(0, 20),
			nonRegistry: nonRegistry.slice(0, 20),
			lifecycleScripts: lifecycleScripts.slice(0, 20),
		},
	);
}

function npmConfigEvidence(repoRoot) {
	const npmrc = readTextIfExists(repoRoot, ".npmrc");
	const packageJson =
		repoRoot === defaultRepoRoot
			? readRootPackageJson()
			: readJsonIfExists(repoRoot, "package.json");
	const hasIgnoreScripts = /^ignore-scripts\s*=\s*true\s*$/m.test(npmrc);
	const scripts = Object.values(packageJson?.scripts || {}).join("\n");
	const ciUsesIgnoreScripts =
		/npm ci[^\n]*--ignore-scripts/.test(
			readTextIfExists(repoRoot, ".github/workflows/ci.yml"),
		) &&
		/npm ci[^\n]*--ignore-scripts/.test(
			readTextIfExists(repoRoot, ".github/workflows/release.yml"),
		);
	return finding(
		"npm-install-scripts-disabled",
		hasIgnoreScripts && ciUsesIgnoreScripts ? "pass" : "blocker",
		hasIgnoreScripts && ciUsesIgnoreScripts
			? "npm lifecycle scripts are disabled by local config and CI install commands"
			: "npm lifecycle script hardening is incomplete",
		{
			hasIgnoreScripts,
			ciUsesIgnoreScripts,
			packageScriptCount: Object.keys(packageJson?.scripts || {}).length,
			scriptBytes: scripts.length,
		},
	);
}

function cargoEvidence(repoRoot) {
	const specs = [...readCargoDependencySpecs(repoRoot).values()].sort(
		(left, right) => left.name.localeCompare(right.name),
	);
	const locked = [...readCargoLockPackages(repoRoot).values()].sort(
		(left, right) => left.name.localeCompare(right.name),
	);
	const lockIssues = cargoLockRefreshFindings(repoRoot);
	const pathDeps = specs.filter((dep) => dep.path);
	const blockers = [];
	if (pathDeps.length)
		blockers.push(
			`${pathDeps.length} Cargo dependencies still point at local paths`,
		);
	if (lockIssues.length) blockers.push(cargoLockRefreshMessage(lockIssues));
	return finding(
		"cargo-dependency-evidence",
		blockers.length === 0 ? "pass" : "blocker",
		blockers.length === 0
			? `Cargo.toml has ${specs.length} dependency specs and Cargo.lock has ${locked.length} packages`
			: blockers.join("; "),
		{
			dependencyCount: specs.length,
			lockedPackageCount: locked.length,
			pathDependencies: pathDeps.map((dep) => ({
				name: dep.name,
				path: dep.path,
			})),
			lockIssues,
		},
	);
}

function shellLogicalLines(text) {
	return String(text || "").replace(/\\\r?\n\s*/g, " ");
}

function workflowEvidence(repoRoot) {
	const publish = readTextIfExists(
		repoRoot,
		".github/workflows/publish-npm.yml",
	);
	const publishCommands = shellLogicalLines(publish);
	const release = readTextIfExists(repoRoot, ".github/workflows/release.yml");
	const security = readTextIfExists(repoRoot, ".github/workflows/security.yml");
	const checkCi = readTextIfExists(repoRoot, "scripts/check-ci.mjs");
	const endgame = readTextIfExists(repoRoot, "scripts/endgame-readiness.mjs");
	const releaseRunsCheckCi = /npm run check:ci/.test(release);
	const checks = [
		["publish-oidc", /id-token:\s*write/.test(publish)],
		[
			"publish-provenance",
			/npm publish[^\n]*--provenance/.test(publishCommands),
		],
		["release-artifact-attestation", /actions\/attest@/.test(release)],
		[
			"release-enforces-readiness",
			/check:release-ready:enforce/.test(release) ||
				(releaseRunsCheckCi &&
					/release-readiness\.mjs[\s\S]{0,240}--enforce/.test(checkCi)),
		],
		[
			"release-rust-live-proof",
			/proof:rust-live:enforce/.test(release) ||
				(releaseRunsCheckCi &&
					/endgame-readiness\.mjs[\s\S]{0,240}--enforce/.test(checkCi) &&
					/rust-live-proof\.mjs/.test(endgame)),
		],
		["security-codeql", /github\/codeql-action\/init@/.test(security)],
		["security-scorecard", /ossf\/scorecard-action@/.test(security)],
	];
	const failed = checks.filter(([, ok]) => !ok).map(([id]) => id);
	return finding(
		"workflow-supply-chain-shape",
		failed.length === 0 ? "pass" : "blocker",
		failed.length === 0
			? "publish/release/security workflows expose OIDC, provenance, attestation, CodeQL, and Scorecard controls"
			: `workflow controls missing: ${failed.join(", ")}`,
		{ checks: Object.fromEntries(checks) },
	);
}

function releaseManifestEvidence(repoRoot) {
	const manifest = readJsonIfExists(repoRoot, "release-manifest.json");
	if (!manifest)
		return finding(
			"release-manifest-hygiene",
			"blocker",
			"release-manifest.json is missing",
		);
	const includePaths = Array.isArray(manifest.includePaths)
		? manifest.includePaths
		: [];
	const forbidden = includePaths.filter((item) =>
		item.split(/[\\/]+/).some((part) => FORBIDDEN_RELEASE_PARTS.has(part)),
	);
	const required = [
		"scripts/check-ci.mjs",
		"scripts/mcp-transport-contract.mjs",
		"scripts/release-readiness.mjs",
		"scripts/rust-live-proof.mjs",
		"scripts/supply-chain-evidence.mjs",
		"scripts/endgame-readiness.mjs",
		"docs/release-readiness.md",
		"docs/rust-live-proof.md",
		"docs/endgame-readiness.md",
	];
	const missing = required.filter((item) => !includePaths.includes(item));
	return finding(
		"release-manifest-hygiene",
		forbidden.length === 0 && missing.length === 0 ? "pass" : "blocker",
		forbidden.length === 0 && missing.length === 0
			? "source manifest includes proof gates and excludes forbidden build/runtime paths"
			: "source manifest needs proof-gate/hygiene updates",
		{ forbidden, missing },
	);
}

function writeReport(repoRoot, relativePath, report) {
	const target = path.isAbsolute(relativePath)
		? relativePath
		: path.join(repoRoot, relativePath);
	fs.mkdirSync(path.dirname(target), { recursive: true });
	fs.writeFileSync(target, `${JSON.stringify(report, null, 2)}\n`);
}

function run(args) {
	const repoRoot = args.repoRoot;
	const findings = [
		packageLockEvidence(repoRoot),
		npmConfigEvidence(repoRoot),
		cargoEvidence(repoRoot),
		workflowEvidence(repoRoot),
		releaseManifestEvidence(repoRoot),
	];
	const blockers = findings.filter((item) => item.status === "blocker");
	const warnings = findings.filter((item) => item.status === "warn");
	const report = {
		schema: "mcpace.supplyChainEvidence.v1",
		generatedAt: new Date().toISOString(),
		repoRoot: ".",
		status:
			blockers.length > 0 ? "blocked" : warnings.length > 0 ? "warn" : "pass",
		enforce: args.enforce,
		blockers: blockers.length,
		warnings: warnings.length,
		fileHashes: {
			packageJson: sha256(repoRoot, "package.json"),
			packageLock: sha256(repoRoot, "package-lock.json"),
			cargoToml: sha256(repoRoot, "Cargo.toml"),
			cargoLock: sha256(repoRoot, "Cargo.lock"),
			releaseManifest: sha256(repoRoot, "release-manifest.json"),
		},
		findings,
	};
	if (args.write) writeReport(repoRoot, args.report, report);
	return report;
}

try {
	const args = parseArgs(process.argv.slice(2));
	const report = run(args);
	if (args.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
	else {
		console.log(
			`${report.status}: ${report.findings.length} supply-chain evidence checks, ${report.blockers} blockers, ${report.warnings} warnings`,
		);
		for (const finding of report.findings)
			console.log(`- ${finding.status}: ${finding.id} — ${finding.detail}`);
	}
	process.exitCode = args.enforce && report.blockers > 0 ? 1 : 0;
} catch (error) {
	console.error(error?.stack || error);
	process.exitCode = 1;
}
