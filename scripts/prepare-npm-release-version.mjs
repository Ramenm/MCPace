#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { repoRoot } from "./lib/project-metadata.mjs";

function parseArgs(argv) {
	const args = {
		version: process.env.MCPACE_RELEASE_VERSION ?? null,
		releaseSha:
			process.env.MCPACE_RELEASE_SHA ?? process.env.GITHUB_SHA ?? null,
		json: argv.includes("--json"),
	};
	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index];
		if (arg === "--version") args.version = argv[++index] ?? null;
		else if (arg === "--release-sha") args.releaseSha = argv[++index] ?? null;
		else if (arg === "--json") args.json = true;
		else if (arg === "--help" || arg === "-h") {
			console.log(
				"Usage: node scripts/prepare-npm-release-version.mjs --version <semver> [--release-sha <40-hex>]",
			);
			process.exit(0);
		} else {
			throw new Error(`unknown argument: ${arg}`);
		}
	}
	if (!args.version) throw new Error("missing --version <semver>");
	if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(args.version)) {
		throw new Error(`invalid npm release version '${args.version}'`);
	}
	args.releaseSha =
		String(args.releaseSha || "")
			.trim()
			.toLowerCase() || null;
	if (args.releaseSha && !/^[a-f0-9]{40}$/.test(args.releaseSha)) {
		throw new Error(
			`release SHA must be a full 40-character commit SHA; got '${args.releaseSha}'`,
		);
	}
	return args;
}

function readJson(relativePath) {
	const text = fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
	try {
		return JSON.parse(text);
	} catch (error) {
		throw new Error(
			`invalid JSON in ${relativePath}: ${error?.message || error}`,
			{ cause: error },
		);
	}
}

function writeJson(relativePath, value) {
	fs.writeFileSync(
		path.join(repoRoot, relativePath),
		`${JSON.stringify(value, null, 2)}\n`,
		"utf8",
	);
}

function replaceTomlPackageVersion(relativePath, version) {
	const filePath = path.join(repoRoot, relativePath);
	const text = fs.readFileSync(filePath, "utf8");
	const pattern = /^version\s*=\s*"[^"]+"/m;
	if (!pattern.test(text))
		throw new Error(
			`${relativePath} does not contain a top-level package version`,
		);
	const updated = text.replace(pattern, `version = "${version}"`);
	fs.writeFileSync(filePath, updated, "utf8");
}

function replaceCargoLockPackageVersion(version) {
	const relativePath = "Cargo.lock";
	const filePath = path.join(repoRoot, relativePath);
	const text = fs.readFileSync(filePath, "utf8");
	const pattern = /(name = "mcpace"\r?\nversion = )"[^"]+"/;
	if (!pattern.test(text))
		throw new Error(
			`${relativePath} does not contain the mcpace package version`,
		);
	const updated = text.replace(pattern, `$1"${version}"`);
	fs.writeFileSync(filePath, updated, "utf8");
}

function updateOptionalDependencies(packageJson, version) {
	for (const name of Object.keys(packageJson.optionalDependencies ?? {})) {
		if (name.startsWith("@mcpace/cli-"))
			packageJson.optionalDependencies[name] = version;
	}
}

function updatePackageLock(version) {
	const lock = readJson("package-lock.json");
	lock.version = version;
	if (lock.packages?.[""]) lock.packages[""].version = version;
	const workspace = lock.packages?.["packages/npm/cli"];
	if (workspace) {
		workspace.version = version;
		updateOptionalDependencies(workspace, version);
	}
	const optionalDependencyNames = Object.keys(
		workspace?.optionalDependencies ?? {},
	)
		.filter((name) => name.startsWith("@mcpace/cli-"))
		.sort((left, right) => left.localeCompare(right));
	const packages =
		lock.packages && typeof lock.packages === "object" ? lock.packages : null;
	if (packages) {
		for (const key of Object.keys(packages)) {
			const isLegacyHoistedNative = /^node_modules\/@mcpace\/cli-/.test(key);
			const isWorkspaceNativeStub =
				/^packages\/npm\/cli\/node_modules\/@mcpace\/cli-/.test(key);
			if (isLegacyHoistedNative || isWorkspaceNativeStub) delete packages[key];
		}
		for (const name of optionalDependencyNames) {
			packages[`packages/npm/cli/node_modules/${name}`] = {
				version,
				optional: true,
			};
		}
	}
	writeJson("package-lock.json", lock);
}

function updateMcpaceConfig(version) {
	const config = readJson("mcpace.config.json");
	config.version = version;
	writeJson("mcpace.config.json", config);
}

function run() {
	const args = parseArgs(process.argv.slice(2));
	const version = args.version;

	const rootPackage = readJson("package.json");
	rootPackage.version = version;
	writeJson("package.json", rootPackage);

	const cliPackage = readJson("packages/npm/cli/package.json");
	cliPackage.version = version;
	cliPackage.mcpace =
		cliPackage.mcpace && typeof cliPackage.mcpace === "object"
			? cliPackage.mcpace
			: {};
	if (args.releaseSha) cliPackage.mcpace.releaseSha = args.releaseSha;
	else delete cliPackage.mcpace.releaseSha;
	updateOptionalDependencies(cliPackage, version);
	writeJson("packages/npm/cli/package.json", cliPackage);

	replaceTomlPackageVersion("Cargo.toml", version);
	replaceCargoLockPackageVersion(version);
	updatePackageLock(version);
	updateMcpaceConfig(version);

	const report = {
		schema: "mcpace.releaseVersionPreparation.v1",
		version,
		releaseSha: args.releaseSha,
		updated: [
			"package.json",
			"packages/npm/cli/package.json",
			"Cargo.toml",
			"Cargo.lock",
			"package-lock.json",
			"mcpace.config.json",
		],
	};
	if (args.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
	else process.stdout.write(`Prepared MCPace release version ${version}\n`);
}

try {
	run();
} catch (error) {
	console.error(error?.stack ?? String(error));
	process.exitCode = 1;
}
