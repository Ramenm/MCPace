#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { readRegularFileStableSync } from "./lib/atomic-fs.mjs";
import {
	deriveProjectVersion,
	readCliPackageJson,
	readJson,
	readText,
	repoRoot,
} from "./lib/project-metadata.mjs";

const WINDOWS_AGENT_LAUNCHER_NAME = "mcpace-agent-launcher.exe";

const args = new Set(process.argv.slice(2));
const jsonOutput = args.has("--json");
const enforce = args.has("--enforce");

function expectedReleaseSha() {
	const value = String(
		process.env.MCPACE_RELEASE_SHA ?? process.env.GITHUB_SHA ?? "",
	)
		.trim()
		.toLowerCase();
	if (!value) return null;
	if (!/^[a-f0-9]{40}$/.test(value)) {
		throw new Error(
			`MCPACE_RELEASE_SHA must be a full 40-character commit SHA; got '${value}'`,
		);
	}
	return value;
}

function readTextIfExists(relativePath) {
	const fullPath = path.join(repoRoot, relativePath);
	return fs.existsSync(fullPath) ? fs.readFileSync(fullPath, "utf8") : "";
}

function walkPackageJsonFiles(root) {
	if (!fs.existsSync(root)) return [];
	const results = [];
	const stack = [root];
	while (stack.length > 0) {
		const current = stack.pop();
		for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
			if (
				entry.name === "node_modules" ||
				entry.name === ".git" ||
				entry.name === "dist" ||
				entry.name === "target"
			) {
				continue;
			}
			const full = path.join(current, entry.name);
			if (entry.isDirectory()) {
				stack.push(full);
			} else if (entry.isFile() && entry.name === "package.json") {
				results.push(full);
			}
		}
	}
	return results.sort();
}

function discoverPackages() {
	const packageDir = path.join(repoRoot, "packages", "npm");
	const packagesByName = new Map();
	for (const packageJsonPath of walkPackageJsonFiles(packageDir)) {
		try {
			const parsed = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
			if (typeof parsed.name === "string") {
				packagesByName.set(parsed.name, {
					name: parsed.name,
					version: parsed.version ?? null,
					relativeDir: path
						.relative(repoRoot, path.dirname(packageJsonPath))
						.split(path.sep)
						.join("/"),
					private: parsed.private === true,
					mcpaceTarget: parsed.mcpace?.target ?? null,
					mcpaceReleaseSha: parsed.mcpace?.releaseSha ?? null,
					mcpaceSidecarBinaries: parsed.mcpace?.sidecarBinaries ?? null,
				});
			}
		} catch {
			// Let package syntax checks report malformed package files; this script only reports publish contract shape.
		}
	}
	return packagesByName;
}

function targetPackageName(target) {
	return target.packageName ?? target.npmPackage ?? `@mcpace/cli-${target.key}`;
}

function tarballNameFragments(packageName, version) {
	const unscoped = packageName.replace(/^@[^/]+\//, "");
	return [
		`${unscoped}-${version}.tgz`,
		`${packageName.replace("@", "").replace("/", "-")}-${version}.tgz`,
	];
}

function tarballCandidatesFor(packageName, version) {
	const candidates = [];
	for (const dir of ["dist", "dist/npm", ".artifacts", ".artifacts/npm"]) {
		for (const fragment of tarballNameFragments(packageName, version)) {
			candidates.push(path.join(repoRoot, dir, fragment));
		}
	}
	return candidates;
}

function trimNullPaddedAscii(buffer, start, length) {
	return buffer
		.subarray(start, start + length)
		.toString("utf8")
		.replace(/\0.*$/s, "")
		.trim();
}

function tarOctal(buffer, start, length) {
	const raw = trimNullPaddedAscii(buffer, start, length).replace(/\s/g, "");
	if (!raw) return 0;
	if (!/^[0-7]+$/.test(raw)) {
		throw new Error(`invalid tar octal field '${raw}'`);
	}
	return Number.parseInt(raw, 8);
}

function tarPathIsUnsafe(name) {
	return (
		!name ||
		name.startsWith("/") ||
		name.includes("\\") ||
		name.split("/").some((part) => part === "" || part === "." || part === "..")
	);
}

function listTarGzEntries(tarballPath) {
	const { data: compressed } = readRegularFileStableSync(tarballPath, {
		maxBytes: 256 * 1024 * 1024,
	});
	const buffer = zlib.gunzipSync(compressed);
	const entries = [];
	let offset = 0;
	while (offset + 512 <= buffer.length) {
		const header = buffer.subarray(offset, offset + 512);
		if (header.every((byte) => byte === 0)) break;
		const name = trimNullPaddedAscii(header, 0, 100);
		const prefix = trimNullPaddedAscii(header, 345, 155);
		const fullName = prefix ? `${prefix}/${name}` : name;
		const mode = tarOctal(header, 100, 8);
		const size = tarOctal(header, 124, 12);
		const type = String.fromCharCode(header[156] || 0);
		const dataStart = offset + 512;
		const dataEnd = dataStart + size;
		if (dataEnd > buffer.length) {
			throw new Error(`tar entry '${fullName}' extends beyond archive size`);
		}
		entries.push({
			path: fullName,
			mode,
			size,
			type: type === "\0" ? "0" : type,
			data: buffer.subarray(dataStart, dataEnd),
		});
		offset = dataStart + Math.ceil(size / 512) * 512;
	}
	return entries;
}

function targetArrayMatches(actual, expected) {
	if (!expected || expected.length === 0) return true;
	return (
		Array.isArray(actual) &&
		expected.length === actual.length &&
		expected.every((value) => actual.includes(value))
	);
}

function requiredSidecarBinariesForTarget(target) {
	return target.platform === "win32" ? [WINDOWS_AGENT_LAUNCHER_NAME] : [];
}

function sidecarArrayMatches(actual, required) {
	if (required.length === 0) return true;
	return (
		Array.isArray(actual) &&
		actual.length === required.length &&
		required.every((value) => actual.includes(value))
	);
}

function verifyNativePackageTarball(tarballPath, target, version, releaseSha) {
	const packageName = target.packageName;
	const binaryName = target.binaryName;
	const relativePath = path
		.relative(repoRoot, tarballPath)
		.split(path.sep)
		.join("/");
	const issues = [];
	let entries = [];
	try {
		entries = listTarGzEntries(tarballPath);
	} catch (error) {
		return {
			path: relativePath,
			status: "failed",
			issues: [`failed to parse tgz: ${error?.message ?? error}`],
		};
	}

	const duplicateEntries = [];
	const seen = new Set();
	for (const entry of entries) {
		if (seen.has(entry.path)) duplicateEntries.push(entry.path);
		seen.add(entry.path);
		if (tarPathIsUnsafe(entry.path))
			issues.push(`unsafe tar entry path: ${entry.path}`);
		if (entry.type === "1" || entry.type === "2")
			issues.push(`link entries are not allowed: ${entry.path}`);
	}
	if (duplicateEntries.length > 0) {
		issues.push(`duplicate tar entries: ${duplicateEntries.sort().join(", ")}`);
	}

	const packageJsonEntry = entries.find(
		(entry) => entry.path === "package/package.json",
	);
	let packageJson = null;
	if (!packageJsonEntry) {
		issues.push("missing package/package.json");
	} else {
		try {
			packageJson = JSON.parse(packageJsonEntry.data.toString("utf8"));
		} catch (error) {
			issues.push(
				`package/package.json is not valid JSON: ${error?.message ?? error}`,
			);
		}
	}

	const binaryEntryPath = `package/bin/${binaryName}`;
	const binaryEntry = entries.find((entry) => entry.path === binaryEntryPath);
	if (!binaryEntry) {
		issues.push(`missing ${binaryEntryPath}`);
	} else if (binaryEntry.type !== "0") {
		issues.push(`${binaryEntryPath} must be a regular file entry`);
	} else if (target.platform !== "win32" && (binaryEntry.mode & 0o111) === 0) {
		issues.push(`${binaryEntryPath} must be executable for ${target.key}`);
	}

	const requiredSidecars = requiredSidecarBinariesForTarget(target);
	const sidecarEntryPaths = [];
	for (const sidecarName of requiredSidecars) {
		const sidecarEntryPath = `package/bin/${sidecarName}`;
		const sidecarEntry = entries.find(
			(entry) => entry.path === sidecarEntryPath,
		);
		if (!sidecarEntry) {
			issues.push(`missing ${sidecarEntryPath}`);
		} else if (sidecarEntry.type !== "0") {
			issues.push(`${sidecarEntryPath} must be a regular file entry`);
		} else {
			sidecarEntryPaths.push(sidecarEntry.path);
		}
	}

	if (packageJson) {
		if (packageJson.name !== packageName)
			issues.push(
				`package name mismatch: expected ${packageName}, got ${packageJson.name ?? null}`,
			);
		if (packageJson.version !== version)
			issues.push(
				`package version mismatch: expected ${version}, got ${packageJson.version ?? null}`,
			);
		if (packageJson.private === true)
			issues.push("native package tarball must not be private");
		if (packageJson.mcpace?.target !== target.key)
			issues.push(
				`mcpace.target mismatch: expected ${target.key}, got ${packageJson.mcpace?.target ?? null}`,
			);
		if (releaseSha && packageJson.mcpace?.releaseSha !== releaseSha) {
			issues.push(
				`mcpace.releaseSha mismatch: expected ${releaseSha}, got ${packageJson.mcpace?.releaseSha ?? null}`,
			);
		}
		if (packageJson.mcpace?.binaryName !== binaryName)
			issues.push(
				`mcpace.binaryName mismatch: expected ${binaryName}, got ${packageJson.mcpace?.binaryName ?? null}`,
			);
		if (
			!sidecarArrayMatches(
				packageJson.mcpace?.sidecarBinaries,
				requiredSidecars,
			)
		) {
			issues.push(
				`mcpace.sidecarBinaries mismatch for ${target.key}: expected ${JSON.stringify(requiredSidecars)}, got ${JSON.stringify(packageJson.mcpace?.sidecarBinaries ?? null)}`,
			);
		}
		if (packageJson.bin?.mcpace) {
			issues.push(
				"native package must not define bin.mcpace; @mcpace/cli owns the user-facing command",
			);
		}
		if (!targetArrayMatches(packageJson.os, target.os))
			issues.push(`os metadata mismatch for ${target.key}`);
		if (!targetArrayMatches(packageJson.cpu, target.cpu))
			issues.push(`cpu metadata mismatch for ${target.key}`);
		if (!targetArrayMatches(packageJson.libc, target.libc))
			issues.push(`libc metadata mismatch for ${target.key}`);
	}

	return {
		path: relativePath,
		status: issues.length === 0 ? "pass" : "failed",
		issues,
		entryCount: entries.length,
		packageName: packageJson?.name ?? null,
		packageVersion: packageJson?.version ?? null,
		packageTargetMetadata: packageJson?.mcpace?.target ?? null,
		packageReleaseSha: packageJson?.mcpace?.releaseSha ?? null,
		packageSidecarMetadata: packageJson?.mcpace?.sidecarBinaries ?? null,
		binaryEntryPath: binaryEntry ? binaryEntry.path : null,
		binaryMode: binaryEntry ? binaryEntry.mode : null,
		sidecarEntryPaths,
	};
}

function tarballProofFor(target, version, releaseSha) {
	for (const candidate of tarballCandidatesFor(target.packageName, version)) {
		if (fs.existsSync(candidate)) {
			return verifyNativePackageTarball(candidate, target, version, releaseSha);
		}
	}
	return null;
}

function sourcePackageBinaryPath(packageInfo, binaryName) {
	if (!packageInfo) return null;
	const packageDir = path.join(repoRoot, packageInfo.relativeDir);
	for (const candidate of [
		path.join(packageDir, "bin", binaryName),
		path.join(packageDir, binaryName),
	]) {
		if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
			return path.relative(repoRoot, candidate).split(path.sep).join("/");
		}
	}
	return null;
}

function sourcePackageSidecarPaths(packageInfo, target) {
	const required = requiredSidecarBinariesForTarget(target);
	if (required.length === 0) return [];
	if (!packageInfo) return null;
	const packageDir = path.join(repoRoot, packageInfo.relativeDir);
	const paths = [];
	for (const sidecarName of required) {
		const candidate = path.join(packageDir, "bin", sidecarName);
		if (!fs.existsSync(candidate) || !fs.statSync(candidate).isFile()) {
			return null;
		}
		paths.push(path.relative(repoRoot, candidate).split(path.sep).join("/"));
	}
	return paths;
}

function check(id, ok, message, details = {}) {
	return {
		id,
		status: ok ? "pass" : "failed",
		message,
		...details,
	};
}

function versionAlignment(version, cliPackage) {
	const rootPackage = readJson("package.json");
	const projectConfig = readJson("mcpace.config.json");
	const packageLock = readJson("package-lock.json");
	const cargoLock = readText("Cargo.lock");
	const cargoLockVersion =
		cargoLock.match(/name = "mcpace"\r?\nversion = "([^"]+)"/)?.[1] ?? null;
	const declared = [
		["package.json", rootPackage.version ?? null],
		["packages/npm/cli/package.json", cliPackage.version ?? null],
		["mcpace.config.json", projectConfig.version ?? null],
		["package-lock.json", packageLock.version ?? null],
		[
			"package-lock.json packages/npm/cli",
			packageLock.packages?.["packages/npm/cli"]?.version ?? null,
		],
		["Cargo.lock", cargoLockVersion],
	];
	const drift = declared
		.filter(([, actual]) => actual !== version)
		.map(([path, actual]) => ({ path, expected: version, actual }));
	return { declared, drift };
}

function buildReport() {
	const version = deriveProjectVersion();
	const releaseSha = expectedReleaseSha();
	const releaseTargets = readJson("release-targets.json");
	const cliPackage = readCliPackageJson();
	const versions = versionAlignment(version, cliPackage);
	const workflow = readTextIfExists(".github/workflows/publish-npm.yml");
	const packagesByName = discoverPackages();
	const enabledTargets = (releaseTargets.targets ?? []).filter(
		(target) => target.publishEnabled !== false,
	);
	const requiredBinaryPackages = enabledTargets.map((target) => ({
		...target,
		packageName: targetPackageName(target),
		binaryName:
			target.binaryName ??
			(target.platform === "win32" ? "mcpace.exe" : "mcpace"),
	}));
	const optionalDependencies = cliPackage.optionalDependencies ?? {};
	const optionalDependencyNames = new Set(Object.keys(optionalDependencies));
	const requiredNames = new Set(
		requiredBinaryPackages.map((target) => target.packageName),
	);
	const missingOptionalDependencies = requiredBinaryPackages
		.filter((target) => !optionalDependencyNames.has(target.packageName))
		.map((target) => target.packageName);
	const extraOptionalDependencies = [...optionalDependencyNames]
		.filter(
			(name) => name.startsWith("@mcpace/cli-") && !requiredNames.has(name),
		)
		.sort();
	const optionalDependencyVersionDrift = Object.entries(optionalDependencies)
		.filter(
			([name, depVersion]) => requiredNames.has(name) && depVersion !== version,
		)
		.map(([name, depVersion]) => ({
			name,
			expected: version,
			actual: depVersion,
		}));

	const binaryPackageGaps = [];
	const binaryPackageProof = [];
	const binaryPackageMetadataDrift = [];
	for (const target of requiredBinaryPackages) {
		const packageInfo = packagesByName.get(target.packageName) ?? null;
		const tarballProof = tarballProofFor(target, version, releaseSha);
		const sourceBinaryPath = sourcePackageBinaryPath(
			packageInfo,
			target.binaryName,
		);
		const sourceSidecarPaths = sourcePackageSidecarPaths(packageInfo, target);
		const requiredSidecars = requiredSidecarBinariesForTarget(target);
		const sourceSidecarMetadataMatches = sidecarArrayMatches(
			packageInfo?.mcpaceSidecarBinaries,
			requiredSidecars,
		);
		const sourceSidecarsPresent =
			requiredSidecars.length === 0 || Array.isArray(sourceSidecarPaths);
		const targetMetadataMatches = packageInfo?.mcpaceTarget === target.key;
		const releaseShaMatches =
			!releaseSha || packageInfo?.mcpaceReleaseSha === releaseSha;
		const hasPublishableSource = Boolean(
			packageInfo &&
				packageInfo.private !== true &&
				packageInfo.version === version &&
				targetMetadataMatches &&
				releaseShaMatches &&
				sourceSidecarMetadataMatches &&
				sourceSidecarsPresent &&
				sourceBinaryPath,
		);
		const hasTarball = tarballProof?.status === "pass";
		binaryPackageProof.push({
			...target,
			packageSourceDir: packageInfo?.relativeDir ?? null,
			packageVersion: packageInfo?.version ?? null,
			packageTargetMetadata: packageInfo?.mcpaceTarget ?? null,
			packageReleaseSha:
				packageInfo?.mcpaceReleaseSha ??
				tarballProof?.packageReleaseSha ??
				null,
			releaseShaMatches: hasTarball
				? tarballProof?.packageReleaseSha === releaseSha || !releaseSha
				: releaseShaMatches,
			sourceBinaryPath,
			requiredSidecarBinaries: requiredSidecars,
			sourceSidecarBinaries: packageInfo?.mcpaceSidecarBinaries ?? null,
			sourceSidecarPaths,
			sourceSidecarMetadataMatches,
			tarballPath: tarballProof?.path ?? null,
			tarballStatus: tarballProof?.status ?? "missing",
			tarballIssues: tarballProof?.issues ?? [],
			tarballEntryCount: tarballProof?.entryCount ?? null,
			tarballSidecarEntryPaths: tarballProof?.sidecarEntryPaths ?? [],
			publishReady: Boolean(hasPublishableSource || hasTarball),
		});
		if (
			packageInfo &&
			packageInfo.private !== true &&
			packageInfo.version === version &&
			!targetMetadataMatches
		) {
			binaryPackageMetadataDrift.push({
				...target,
				expected: target.key,
				actual: packageInfo.mcpaceTarget ?? null,
			});
		}
		if (!hasPublishableSource && !hasTarball) {
			let reason =
				"No publishable platform package source with the expected native binary or prebuilt npm tarball was found for this target.";
			if (tarballProof?.status === "failed") {
				reason = `Prebuilt native npm tarball exists, but failed verification: ${tarballProof.issues.join("; ")}`;
			} else if (
				packageInfo &&
				packageInfo.private !== true &&
				packageInfo.version === version &&
				!targetMetadataMatches
			) {
				reason = `Platform package source exists, but package.json mcpace.target does not match '${target.key}'.`;
			} else if (
				packageInfo &&
				packageInfo.private !== true &&
				packageInfo.version === version &&
				!sourceSidecarMetadataMatches
			) {
				reason = `Platform package source exists, but mcpace.sidecarBinaries does not declare the required Windows hidden launcher sidecar(s): ${requiredSidecars.join(", ")}.`;
			} else if (
				packageInfo &&
				packageInfo.private !== true &&
				packageInfo.version === version &&
				!sourceSidecarsPresent
			) {
				reason = `Platform package source exists, but the required Windows hidden launcher sidecar(s) were not found in package/bin: ${requiredSidecars.join(", ")}.`;
			} else if (
				packageInfo &&
				packageInfo.private !== true &&
				packageInfo.version === version
			) {
				reason = `Platform package source exists, but the expected native binary '${target.binaryName}' was not found in the package.`;
			}
			binaryPackageGaps.push({ ...target, reason });
		}
	}

	const pinnedPublishPattern =
		/npm exec --yes --package=npm@11\.13\.0 -- npm publish(?:\s|$)/;
	const workflowUsesPinnedNpmForPublish = pinnedPublishPattern.test(workflow);
	const workflowEnforcesContract =
		/verify-npm-publish-contract\.mjs --enforce/.test(workflow);
	const checks = [
		check(
			"release-version-alignment",
			versions.drift.length === 0,
			"Cargo, npm workspace metadata, lock files, and mcpace.config.json must use one release version.",
			{ versionDrift: versions.drift },
		),
		check(
			"release-sha-metadata",
			!releaseSha || cliPackage.mcpace?.releaseSha === releaseSha,
			"The launcher package must carry the exact immutable release SHA before publication.",
			{
				expectedReleaseSha: releaseSha,
				actualReleaseSha: cliPackage.mcpace?.releaseSha ?? null,
			},
		),
		check(
			"optional-dependencies-cover-enabled-targets",
			missingOptionalDependencies.length === 0,
			"Main npm package must depend on every enabled platform package.",
			{ missingOptionalDependencies },
		),
		check(
			"optional-dependencies-match-project-version",
			optionalDependencyVersionDrift.length === 0,
			"Platform optionalDependencies must match the project version.",
			{ optionalDependencyVersionDrift },
		),
		check(
			"optional-dependencies-do-not-advertise-disabled-targets",
			extraOptionalDependencies.length === 0,
			"Main npm package must not advertise platform packages outside enabled release targets.",
			{ extraOptionalDependencies },
		),
		check(
			"binary-package-target-metadata-matches-release-targets",
			binaryPackageMetadataDrift.length === 0,
			"Platform package package.json must declare mcpace.target matching release-targets.json.",
			{ binaryPackageMetadataDrift },
		),
		check(
			"binary-packages-or-tarballs-exist",
			binaryPackageGaps.length === 0,
			"Every enabled target must have a publishable platform package source containing matching target metadata and the expected native binary, or a prebuilt tarball before npm publish.",
			{ binaryPackageGaps },
		),
		check(
			"publish-workflow-uses-pinned-npm-for-publish",
			workflowUsesPinnedNpmForPublish,
			"The publish workflow must use the verified npm executable for npm publish, not the ambient npm binary.",
		),
		check(
			"publish-workflow-enforces-native-package-contract",
			workflowEnforcesContract,
			"The publish workflow must enforce this contract before publishing the main launcher.",
		),
	];
	const failedChecks = checks.filter((entry) => entry.status !== "pass");
	return {
		schema: "mcpace.npmPublishContract.v1",
		generatedAt: new Date().toISOString(),
		status: failedChecks.length === 0 ? "pass" : "blocked",
		enforce,
		version,
		releaseSha,
		versionAlignment: versions,
		mainPackageName: cliPackage.name,
		enabledTargetCount: enabledTargets.length,
		requiredBinaryPackages,
		binaryPackageProof,
		binaryPackageGaps,
		binaryPackageMetadataDrift,
		checks,
		failedChecks,
		publishable: failedChecks.length === 0,
	};
}

const report = buildReport();
if (jsonOutput || enforce) {
	process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} else {
	const summary = report.publishable
		? "npm publish contract: pass"
		: `npm publish contract: blocked (${report.failedChecks.length} failed checks)`;
	process.stdout.write(`${summary}\n`);
	for (const failed of report.failedChecks) {
		process.stdout.write(`- ${failed.id}: ${failed.message}\n`);
	}
}

if (enforce && !report.publishable) {
	process.exitCode = 1;
}
