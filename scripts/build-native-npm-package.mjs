#!/usr/bin/env node
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import {
	copyRegularFileNoFollowSync,
	readRegularFileStableSync,
	writeFileAtomicSync,
} from "./lib/atomic-fs.mjs";
import { runCommand } from "./lib/command-runner.mjs";
import {
	deriveProjectVersion,
	readJson,
	repoRoot,
} from "./lib/project-metadata.mjs";

const args = process.argv.slice(2);
const jsonOutput = args.includes("--json");
const WINDOWS_AGENT_LAUNCHER_NAME = "mcpace-agent-launcher.exe";

function argValue(name, fallback = null) {
	const index = args.indexOf(name);
	return index >= 0 ? (args[index + 1] ?? fallback) : fallback;
}

function fail(message) {
	const report = {
		schema: "mcpace.nativeNpmPackageBuild.v1",
		status: "failed",
		error: message,
	};
	if (jsonOutput) {
		process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
	} else {
		process.stderr.write(`${message}\n`);
	}
	process.exit(1);
}

function targetPackageName(target) {
	return target.packageName ?? target.npmPackage ?? `@mcpace/cli-${target.key}`;
}

function packageFileName(packageName, version) {
	const unscoped = packageName.replace(/^@[^/]+\//, "");
	return `${unscoped}-${version}.tgz`;
}

function normalizedReleaseSha(value) {
	const releaseSha = String(value || "")
		.trim()
		.toLowerCase();
	if (!releaseSha) return null;
	if (!/^[a-f0-9]{40}$/.test(releaseSha)) {
		throw new Error(
			`release SHA must be a full 40-character commit SHA; got '${value}'`,
		);
	}
	return releaseSha;
}

function validateTarget(releaseTargets, key) {
	const target = (releaseTargets.targets ?? []).find(
		(candidate) => candidate.key === key,
	);
	if (!target) {
		throw new Error(`unknown release target '${key}'`);
	}
	if (target.publishEnabled === false) {
		throw new Error(`release target '${key}' is not publish-enabled`);
	}
	if (!target.platform || !target.arch || !target.rustTarget) {
		throw new Error(
			`release target '${key}' is missing platform, arch, or rustTarget metadata`,
		);
	}
	return target;
}

function validateBinaryForTarget(binaryPath, target) {
	let stat;
	try {
		stat = fs.lstatSync(binaryPath);
	} catch (error) {
		throw new Error(
			`native binary '${binaryPath}' is not readable: ${error?.message ?? error}`,
		);
	}
	if (stat.isSymbolicLink()) {
		throw new Error(
			`native binary '${binaryPath}' must not be a symbolic link`,
		);
	}
	if (!stat.isFile()) {
		throw new Error(`native binary '${binaryPath}' must be a regular file`);
	}
	if (target.platform !== "win32" && (Number(stat.mode) & 0o111) === 0) {
		throw new Error(
			`native binary '${binaryPath}' must have an executable bit for target '${target.key}'`,
		);
	}
	if (
		target.platform === "win32" &&
		!String(binaryPath).toLowerCase().endsWith(".exe")
	) {
		throw new Error(
			`native binary for Windows target '${target.key}' must use .exe extension`,
		);
	}
	return stat;
}

function validateSidecarBinaryForTarget(binaryPath, target, name) {
	let stat;
	try {
		stat = fs.lstatSync(binaryPath);
	} catch (error) {
		throw new Error(
			`required native sidecar '${name}' for target '${target.key}' is not readable at '${binaryPath}': ${error?.message ?? error}`,
		);
	}
	if (stat.isSymbolicLink()) {
		throw new Error(
			`required native sidecar '${binaryPath}' must not be a symbolic link`,
		);
	}
	if (!stat.isFile()) {
		throw new Error(
			`required native sidecar '${binaryPath}' must be a regular file`,
		);
	}
	if (target.platform !== "win32" && (Number(stat.mode) & 0o111) === 0) {
		throw new Error(
			`required native sidecar '${binaryPath}' must have an executable bit for target '${target.key}'`,
		);
	}
	if (
		target.platform === "win32" &&
		!String(binaryPath).toLowerCase().endsWith(".exe")
	) {
		throw new Error(
			`required native sidecar for Windows target '${target.key}' must use .exe extension`,
		);
	}
	return stat;
}

function sidecarBinariesForTarget(binaryPath, target) {
	if (target.platform !== "win32") return [];
	const launcherPath = path.join(
		path.dirname(binaryPath),
		WINDOWS_AGENT_LAUNCHER_NAME,
	);
	const stat = validateSidecarBinaryForTarget(
		launcherPath,
		target,
		WINDOWS_AGENT_LAUNCHER_NAME,
	);
	return [
		{
			name: WINDOWS_AGENT_LAUNCHER_NAME,
			sourcePath: launcherPath,
			size: stat.size,
			purpose:
				"hidden Windows autostart launcher that starts MCPace Agent without a terminal window",
		},
	];
}

function writePackageJson(packageDir, target, version, releaseSha) {
	const packageName = targetPackageName(target);
	const binaryName =
		target.binaryName ??
		(target.platform === "win32" ? "mcpace.exe" : "mcpace");
	const packageJson = {
		name: packageName,
		version,
		description: `MCPace native binary for ${target.key}.`,
		license: "Apache-2.0",
		repository: {
			type: "git",
			url: "git+https://github.com/Ramenm/MCPace.git",
		},
		bugs: {
			url: "https://github.com/Ramenm/MCPace/issues",
		},
		homepage: "https://github.com/Ramenm/MCPace#readme",
		type: "module",
		files: ["bin", "README.md", "LICENSE", "NOTICE"],
		os: target.os ?? [target.platform],
		cpu: target.cpu ?? [target.arch],
		publishConfig: {
			access: "public",
		},
		mcpace: {
			target: target.key,
			rustTarget: target.rustTarget,
			binaryName,
			sidecarBinaries:
				target.platform === "win32" ? [WINDOWS_AGENT_LAUNCHER_NAME] : [],
			mainPackage: "@mcpace/cli",
			...(releaseSha ? { releaseSha } : {}),
		},
	};
	if (Array.isArray(target.libc) && target.libc.length > 0) {
		packageJson.libc = target.libc;
	}
	writeFileAtomicSync(
		path.join(packageDir, "package.json"),
		`${JSON.stringify(packageJson, null, 2)}\n`,
		{ mode: 0o644 },
	);
}

function writeReadme(packageDir, target) {
	const packageName = targetPackageName(target);
	const binaryName =
		target.binaryName ??
		(target.platform === "win32" ? "mcpace.exe" : "mcpace");
	const sidecarNote =
		target.platform === "win32"
			? ` It also includes \`${WINDOWS_AGENT_LAUNCHER_NAME}\`, the GUI-subsystem login-start sidecar used by \`mcpace autostart\` so Windows does not open a terminal window at user login.`
			: "";
	writeFileAtomicSync(
		path.join(packageDir, "README.md"),
		`# ${packageName}\n\nNative MCPace binary package for \`${target.key}\`.\n\nThis package is installed as an optional dependency of \`@mcpace/cli\`; users normally should not install it directly. It contains the platform-specific \`${binaryName}\` binary and package metadata.${sidecarNote}\n`,
		{ mode: 0o644 },
	);
}

function writeLicense(packageDir) {
	copyRegularFileNoFollowSync(
		path.join(repoRoot, "LICENSE"),
		path.join(packageDir, "LICENSE"),
		{ maxBytes: 1024 * 1024 },
	);
	copyRegularFileNoFollowSync(
		path.join(repoRoot, "NOTICE"),
		path.join(packageDir, "NOTICE"),
		{ maxBytes: 1024 * 1024 },
	);
}

function buildPackage({ target, binaryPath, outDir, version, releaseSha }) {
	const packageName = targetPackageName(target);
	const binaryName =
		target.binaryName ??
		(target.platform === "win32" ? "mcpace.exe" : "mcpace");
	const tempParent = fs.mkdtempSync(
		path.join(os.tmpdir(), `mcpace-native-${target.key}-`),
	);
	const packageDir = path.join(
		tempParent,
		packageName.replace("@", "").replace("/", "-"),
	);
	const binDir = path.join(packageDir, "bin");
	fs.mkdirSync(binDir, { recursive: true });
	try {
		writePackageJson(packageDir, target, version, releaseSha);
		writeReadme(packageDir, target);
		writeLicense(packageDir);
		const binaryCopy = copyRegularFileNoFollowSync(
			binaryPath,
			path.join(binDir, binaryName),
			{
				maxBytes: Number(
					process.env.MCPACE_NATIVE_BINARY_MAX_BYTES || 128 * 1024 * 1024,
				),
			},
		);
		const sidecarCopies = sidecarBinariesForTarget(binaryPath, target).map(
			(sidecar) => {
				const copy = copyRegularFileNoFollowSync(
					sidecar.sourcePath,
					path.join(binDir, sidecar.name),
					{
						maxBytes: Number(
							process.env.MCPACE_NATIVE_BINARY_MAX_BYTES || 128 * 1024 * 1024,
						),
					},
				);
				return {
					name: sidecar.name,
					purpose: sidecar.purpose,
					sourcePath: path
						.relative(repoRoot, sidecar.sourcePath)
						.split(path.sep)
						.join("/"),
					copiedBytes: copy.size,
				};
			},
		);

		fs.mkdirSync(outDir, { recursive: true });
		const pack = runCommand(
			"npm",
			[
				"pack",
				packageDir,
				"--pack-destination",
				outDir,
				"--json",
				"--ignore-scripts",
				"--no-audit",
				"--no-fund",
			],
			{
				cwd: repoRoot,
				timeoutMs: 120_000,
				maxBuffer: 32 * 1024 * 1024,
			},
		);
		if (pack.status !== "pass") {
			throw new Error(
				`npm pack failed for ${packageName}: ${pack.stderrTail || pack.stdoutTail || pack.error || `exit ${pack.exitCode}`}`,
			);
		}
		let packJson;
		try {
			packJson = JSON.parse(pack.stdout || "[]");
		} catch (error) {
			throw new Error(
				`npm pack returned non-JSON output for ${packageName}: ${error?.message ?? error}`,
			);
		}
		const packed = Array.isArray(packJson) ? packJson[0] : packJson;
		const expectedTarball = path.join(
			outDir,
			packageFileName(packageName, version),
		);
		const tarballPath = packed?.filename
			? path.resolve(outDir, packed.filename)
			: expectedTarball;
		if (!fs.existsSync(tarballPath)) {
			throw new Error(
				`npm pack did not create expected tarball for ${packageName}`,
			);
		}
		return {
			schema: "mcpace.nativeNpmPackageBuild.v1",
			status: "pass",
			target: target.key,
			packageName,
			version,
			releaseSha,
			binaryName,
			binarySourcePath: path
				.relative(repoRoot, binaryPath)
				.split(path.sep)
				.join("/"),
			binaryCopiedBytes: binaryCopy.size,
			sidecarBinaries: sidecarCopies,
			tarballPath: path
				.relative(repoRoot, tarballPath)
				.split(path.sep)
				.join("/"),
			npmPack: packed ?? null,
		};
	} finally {
		fs.rmSync(tempParent, { recursive: true, force: true });
	}
}

try {
	const targetKey = argValue("--target") ?? argValue("--target-key");
	const binaryArg = argValue("--binary");
	const outDir = path.resolve(
		argValue("--out-dir", path.join(repoRoot, "dist", "npm")),
	);
	if (!targetKey)
		fail(
			"usage: node scripts/build-native-npm-package.mjs --target <release-target-key> --binary <path> [--release-sha <40-hex>] [--out-dir dist/npm] [--json]",
		);
	if (!binaryArg) fail("missing --binary <path>");
	const releaseTargets = readJson("release-targets.json");
	const target = validateTarget(releaseTargets, targetKey);
	const version = deriveProjectVersion();
	const releaseSha = normalizedReleaseSha(
		argValue(
			"--release-sha",
			process.env.MCPACE_RELEASE_SHA ?? process.env.GITHUB_SHA ?? "",
		),
	);
	const binaryPath = path.resolve(binaryArg);
	validateBinaryForTarget(binaryPath, target);
	readRegularFileStableSync(binaryPath, {
		maxBytes: Number(
			process.env.MCPACE_NATIVE_BINARY_MAX_BYTES || 128 * 1024 * 1024,
		),
	});
	const report = buildPackage({
		target,
		binaryPath,
		outDir,
		version,
		releaseSha,
	});
	if (jsonOutput) {
		process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
	} else {
		process.stdout.write(`Built ${report.tarballPath}\n`);
	}
} catch (error) {
	fail(error?.message ?? String(error));
}
