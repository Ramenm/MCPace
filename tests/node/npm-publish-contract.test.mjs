import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import zlib from "node:zlib";
import { repoRoot } from "../../scripts/lib/project-metadata.mjs";
import { trustedNpmCliPath } from "../../scripts/lib/process.mjs";

function runPublishContract(args = ["--json"]) {
	const result = spawnSync(
		process.execPath,
		["scripts/verify-npm-publish-contract.mjs", ...args],
		{
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
		},
	);
	return result;
}

function trimTarText(buffer, start, length) {
	return buffer
		.subarray(start, start + length)
		.toString("utf8")
		.replace(/\0.*$/s, "")
		.trim();
}

function readTgzEntry(tarballPath, desiredPath) {
	const buffer = zlib.gunzipSync(fs.readFileSync(tarballPath));
	let offset = 0;
	while (offset + 512 <= buffer.length) {
		const header = buffer.subarray(offset, offset + 512);
		if (header.every((byte) => byte === 0)) break;
		const name = trimTarText(header, 0, 100);
		const prefix = trimTarText(header, 345, 155);
		const fullName = prefix ? `${prefix}/${name}` : name;
		const sizeText = trimTarText(header, 124, 12).replace(/\s/g, "");
		const size = sizeText ? Number.parseInt(sizeText, 8) : 0;
		const dataStart = offset + 512;
		const dataEnd = dataStart + size;
		if (fullName === desiredPath) {
			return buffer.subarray(dataStart, dataEnd).toString("utf8");
		}
		offset = dataStart + Math.ceil(size / 512) * 512;
	}
	throw new Error(`missing tar entry ${desiredPath}`);
}

function resolveReportPath(reportPath) {
	return path.isAbsolute(reportPath)
		? reportPath
		: path.join(repoRoot, reportPath);
}

function parseJson(value, label) {
	try {
		return JSON.parse(value);
	} catch (error) {
		assert.fail(`${label} is not valid JSON: ${error?.message ?? error}`);
	}
}

test("npm publish contract detects missing native package artifacts before release publish", () => {
	const result = runPublishContract();
	assert.equal(result.status, 0, result.stderr || result.stdout);
	const report = parseJson(result.stdout, "publish contract output");
	assert.equal(report.schema, "mcpace.npmPublishContract.v1");
	assert.equal(report.mainPackageName, "@mcpace/cli");
	assert.equal(report.enabledTargetCount, 6);
	assert.equal(
		report.versionAlignment?.drift.length,
		0,
		JSON.stringify(report.versionAlignment),
	);
	assert.equal(
		report.checks.find((entry) => entry.id === "release-version-alignment")
			?.status,
		"pass",
	);
	assert.equal(report.releaseSha, null);
	assert.equal(
		report.checks.find((entry) => entry.id === "release-sha-metadata")?.status,
		"pass",
	);
	assert.equal(
		report.publishable,
		false,
		"source-only bundle must not be considered directly publishable to npm",
	);
	assert.equal(
		report.binaryPackageGaps.length,
		6,
		"all enabled native target packages must be accounted for before publish",
	);
	assert.ok(
		report.binaryPackageGaps.every((gap) =>
			gap.packageName.startsWith("@mcpace/cli-"),
		),
	);
	assert.ok(
		report.binaryPackageProof.every((entry) =>
			Object.hasOwn(entry, "sourceBinaryPath"),
		),
	);
	assert.ok(
		report.binaryPackageProof.every((entry) =>
			Object.hasOwn(entry, "tarballStatus"),
		),
	);
	assert.ok(
		report.binaryPackageGaps.every((gap) =>
			/native binary|prebuilt npm tarball/.test(gap.reason),
		),
	);
	const binaryCheck = report.checks.find(
		(entry) => entry.id === "binary-packages-or-tarballs-exist",
	);
	assert.equal(binaryCheck?.status, "failed");
});

test("npm publish workflow uses pinned npm for publish and enforces native package contract", () => {
	const workflow = fs.readFileSync(
		path.join(repoRoot, ".github", "workflows", "publish-npm.yml"),
		"utf8",
	);
	assert.match(
		workflow,
		/node scripts\/verify-npm-publish-contract\.mjs --enforce/,
	);
	assert.match(workflow, /build-native-npm-package\.mjs/);
	assert.match(workflow, /Download native package artifacts/);
	assert.match(workflow, /Publish or resume native npm package set/);
	assert.match(workflow, /Publish main npm launcher last/);
	assert.match(workflow, /dry_run_args=\(--dry-run\)/);
	const pinnedPublishes =
		workflow.match(
			/npm exec --yes --package=npm@11\.13\.0 -- npm publish \\\s+[\s\S]*?--provenance/g,
		) ?? [];
	assert.equal(pinnedPublishes.length, 2);
	assert.match(workflow, /registry_has_matching_release/);
	assert.match(workflow, /mcpace\?\.releaseSha/);
	assert.match(workflow, /refusing mixed-SHA resume/);
	assert.match(workflow, /already published; skipping \$package_spec/);
	assert.match(workflow, /E404\|ETARGET/);
	assert.match(workflow, /group: publish-npm/);
	assert.match(workflow, /branches:\s*\n\s*-\s*dev/);
	assert.match(workflow, /tags:\s*\n\s*-\s*["']v\*["']/);
	assert.doesNotMatch(workflow, /branches:\s*\n\s*-\s*main/);
	assert.doesNotMatch(
		workflow,
		/\n\s+npm publish(?:\s|$)/,
		"workflow must not publish with an ambient npm binary",
	);
});

test("npm trusted publisher setup is bulk scripted for all publish packages", () => {
	const script = fs.readFileSync(
		path.join(repoRoot, "scripts", "configure-npm-trusted-publishers.mjs"),
		"utf8",
	);
	const packageJson = parseJson(
		fs.readFileSync(path.join(repoRoot, "package.json"), "utf8"),
		"package.json",
	);
	const manifest = parseJson(
		fs.readFileSync(path.join(repoRoot, "release-manifest.json"), "utf8"),
		"release-manifest.json",
	);
	assert.match(script, /npm'\s*,\s*'trust'\s*,\s*'github'/);
	assert.match(script, /DEFAULT_REPOSITORY = 'Ramenm\/MCPace'/);
	assert.match(script, /DEFAULT_WORKFLOW_FILE = 'publish-npm\.yml'/);
	assert.match(script, /DEFAULT_ENVIRONMENT = 'npm-publish'/);
	assert.match(script, /--allow-publish/);
	assert.match(script, /optionalDependencies/);
	assert.match(script, /npm login --auth-type=web/);
	assert.equal(
		packageJson.scripts["npm:trust:plan"],
		"node scripts/configure-npm-trusted-publishers.mjs",
	);
	assert.equal(
		packageJson.scripts["npm:trust:configure"],
		"node scripts/configure-npm-trusted-publishers.mjs --execute",
	);
	assert.ok(
		manifest.includePaths.includes(
			"scripts/configure-npm-trusted-publishers.mjs",
		),
	);
});

test("npm publish enforce mode fails closed when native packages are not staged", () => {
	const result = runPublishContract(["--enforce"]);
	assert.notEqual(
		result.status,
		0,
		"enforce mode must fail closed until native package artifacts exist",
	);
	const report = parseJson(result.stdout, "enforced publish contract output");
	assert.equal(report.publishable, false);
	assert.ok(
		report.failedChecks.some(
			(entry) => entry.id === "binary-packages-or-tarballs-exist",
		),
	);
});

test("native optional package tarballs do not claim the user-facing mcpace bin", () => {
	const tmp = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-native-bin-contract-"),
	);
	try {
		const outDir = path.join(tmp, "out");
		const binaryPath = path.join(tmp, "mcpace.exe");
		fs.writeFileSync(binaryPath, "native fixture", "utf8");
		fs.writeFileSync(
			path.join(tmp, "mcpace-agent-launcher.exe"),
			"hidden launcher fixture",
			"utf8",
		);
		const build = spawnSync(
			process.execPath,
			[
				"scripts/build-native-npm-package.mjs",
				"--target",
				"win32-x64-msvc",
				"--binary",
				binaryPath,
				"--out-dir",
				outDir,
				"--json",
			],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
			},
		);
		assert.equal(build.status, 0, build.stderr || build.stdout);
		const report = JSON.parse(build.stdout);
		const packageJson = JSON.parse(
			readTgzEntry(
				resolveReportPath(report.tarballPath),
				"package/package.json",
			),
		);
		assert.equal(packageJson.name, "@mcpace/cli-win32-x64-msvc");
		assert.equal(
			packageJson.bin,
			undefined,
			"native packages must not create a competing mcpace bin shim",
		);
		assert.equal(packageJson.mcpace?.mainPackage, "@mcpace/cli");
		assert.equal(packageJson.mcpace?.binaryName, "mcpace.exe");
		assert.deepEqual(packageJson.mcpace?.sidecarBinaries, [
			"mcpace-agent-launcher.exe",
		]);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});

test("npm publish contract does not trust empty native package source directories", () => {
	const script = fs.readFileSync(
		path.join(repoRoot, "scripts", "verify-npm-publish-contract.mjs"),
		"utf8",
	);
	assert.match(script, /sourcePackageBinaryPath/);
	assert.match(script, /path\.join\(packageDir, ["']bin["'], binaryName\)/);
	assert.match(script, /sourceSidecarsPresent[\s\S]*sourceBinaryPath/);
	assert.match(script, /expected native binary/);
	assert.match(script, /packageTargetMetadata/);
	assert.match(
		script,
		/binary-package-target-metadata-matches-release-targets/,
	);
	assert.match(script, /verifyNativePackageTarball/);
	assert.match(script, /requiredSidecarBinariesForTarget/);
	assert.match(script, /package\/package\.json/);
	assert.match(script, /package\/bin\/\$\{binaryName\}/);
	assert.match(script, /package\/bin\/\$\{sidecarName\}/);
	assert.match(script, /mcpace\.sidecarBinaries/);
	assert.match(script, /native package must not define bin\.mcpace/);
});

test("release source ZIP includes the npm publish contract guard script", () => {
	const manifest = parseJson(
		fs.readFileSync(path.join(repoRoot, "release-manifest.json"), "utf8"),
		"release-manifest.json",
	);
	assert.ok(
		manifest.includePaths.includes("scripts/verify-npm-publish-contract.mjs"),
	);
	assert.ok(
		manifest.includePaths.includes("scripts/build-native-npm-package.mjs"),
	);
	assert.ok(manifest.includePaths.includes("docs/release-completion.md"));
});

test("workspace check:publish-contract script also fails closed when native packages are not staged", () => {
	const npmCli = trustedNpmCliPath("npm");
	assert.ok(npmCli, "could not resolve trusted npm CLI path");
	const result = spawnSync(
		process.execPath,
		[npmCli, "run", "check:publish-contract"],
		{
			cwd: repoRoot,
			encoding: "utf8",
			windowsHide: true,
		},
	);
	assert.notEqual(
		result.status,
		0,
		"package.json script must not turn a blocked publish contract into a green check",
	);
	const jsonStart = result.stdout.indexOf("{");
	assert.notEqual(jsonStart, -1, result.stdout);
	const report = parseJson(
		result.stdout.slice(jsonStart),
		"workspace publish contract output",
	);
	assert.equal(report.enforce, true);
	assert.equal(report.publishable, false);
	assert.ok(
		report.failedChecks.some(
			(entry) => entry.id === "binary-packages-or-tarballs-exist",
		),
	);
});

test("npm publish contract rejects a native tarball from another release SHA", () => {
	const tmp = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-native-publish-sha-"),
	);
	const outDir = path.join(repoRoot, "dist", "npm");
	let tarballPath = null;
	try {
		fs.mkdirSync(outDir, { recursive: true });
		const binaryPath = path.join(tmp, "mcpace.exe");
		fs.writeFileSync(binaryPath, "native fixture", "utf8");
		fs.writeFileSync(
			path.join(tmp, "mcpace-agent-launcher.exe"),
			"hidden launcher fixture",
			"utf8",
		);
		const builtSha = "a".repeat(40);
		const expectedSha = "b".repeat(40);
		const build = spawnSync(
			process.execPath,
			[
				"scripts/build-native-npm-package.mjs",
				"--target",
				"win32-x64-msvc",
				"--binary",
				binaryPath,
				"--out-dir",
				outDir,
				"--json",
			],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
				env: { ...process.env, MCPACE_RELEASE_SHA: builtSha },
			},
		);
		assert.equal(build.status, 0, build.stderr || build.stdout);
		const buildReport = parseJson(build.stdout, "cross-SHA native build");
		tarballPath = resolveReportPath(buildReport.tarballPath);

		const verify = spawnSync(
			process.execPath,
			["scripts/verify-npm-publish-contract.mjs", "--json"],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
				env: { ...process.env, MCPACE_RELEASE_SHA: expectedSha },
			},
		);
		assert.equal(verify.status, 0, verify.stderr || verify.stdout);
		const report = parseJson(verify.stdout, "cross-SHA contract report");
		const proof = report.binaryPackageProof.find(
			(entry) => entry.key === "win32-x64-msvc",
		);
		assert.equal(proof?.tarballStatus, "failed");
		assert.match(
			proof?.tarballIssues.join("\n") || "",
			/mcpace\.releaseSha mismatch/,
		);
	} finally {
		if (tarballPath) fs.rmSync(tarballPath, { force: true });
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});

test("npm publish contract verifies Windows native tarballs include the hidden launcher sidecar", () => {
	const tmp = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-native-publish-sidecar-"),
	);
	const outDir = path.join(repoRoot, "dist", "npm");
	let tarballPath = null;
	try {
		fs.mkdirSync(outDir, { recursive: true });
		const binaryPath = path.join(tmp, "mcpace.exe");
		fs.writeFileSync(binaryPath, "native fixture", "utf8");
		fs.writeFileSync(
			path.join(tmp, "mcpace-agent-launcher.exe"),
			"hidden launcher fixture",
			"utf8",
		);
		const build = spawnSync(
			process.execPath,
			[
				"scripts/build-native-npm-package.mjs",
				"--target",
				"win32-x64-msvc",
				"--binary",
				binaryPath,
				"--out-dir",
				outDir,
				"--json",
			],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
			},
		);
		assert.equal(build.status, 0, build.stderr || build.stdout);
		const buildReport = JSON.parse(build.stdout);
		tarballPath = resolveReportPath(buildReport.tarballPath);

		const verify = runPublishContract(["--json"]);
		assert.equal(verify.status, 0, verify.stderr || verify.stdout);
		const report = JSON.parse(verify.stdout);
		const proof = report.binaryPackageProof.find(
			(entry) => entry.key === "win32-x64-msvc",
		);
		assert.equal(
			proof?.tarballStatus,
			"pass",
			JSON.stringify(proof?.tarballIssues),
		);
		assert.deepEqual(proof?.requiredSidecarBinaries, [
			"mcpace-agent-launcher.exe",
		]);
		assert.deepEqual(proof?.tarballSidecarEntryPaths, [
			"package/bin/mcpace-agent-launcher.exe",
		]);
	} finally {
		if (tarballPath) fs.rmSync(tarballPath, { force: true });
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});
