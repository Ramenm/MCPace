import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
	deriveProjectName,
	deriveProjectVersion,
	repoRoot,
} from "../../scripts/lib/project-metadata.mjs";
import { sha256File } from "../../scripts/lib/rust-build-provenance.mjs";
import {
	listZipEntries,
	listZipEntryMetadata,
} from "../../scripts/lib/zip-writer.mjs";

function parseJson(text, label) {
	try {
		return JSON.parse(text);
	} catch (error) {
		assert.fail(
			`${label} did not return valid JSON: ${error?.message || error}\n${text}`,
		);
	}
}

function makeMiniReleaseRepo(includePaths) {
	const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-release-mini-"));
	fs.mkdirSync(path.join(tmp, "scripts", "lib"), { recursive: true });
	for (const relativePath of [
		"scripts/build-release-artifacts.mjs",
		"scripts/lib/atomic-fs.mjs",
		"scripts/lib/project-metadata.mjs",
		"scripts/lib/repo-files.mjs",
		"scripts/lib/rust-build-provenance.mjs",
		"scripts/lib/zip-writer.mjs",
		"scripts/lib/source-archive-policy.mjs",
	]) {
		fs.cpSync(path.join(repoRoot, relativePath), path.join(tmp, relativePath));
	}
	fs.writeFileSync(
		path.join(tmp, "package.json"),
		JSON.stringify({ name: "mcpace-workspace", version: "0.7.3" }, null, 2),
	);
	fs.writeFileSync(
		path.join(tmp, "Cargo.toml"),
		'[package]\nname = "mcpace"\nversion = "0.7.3"\n',
	);
	fs.writeFileSync(path.join(tmp, "README.md"), "# MCPace\n");
	fs.mkdirSync(path.join(tmp, "docs"), { recursive: true });
	fs.writeFileSync(path.join(tmp, "docs", "README.md"), "# Docs\n");
	fs.mkdirSync(path.join(tmp, "reports"), { recursive: true });
	fs.writeFileSync(path.join(tmp, "reports", "summary.md"), "# Summary\n");
	fs.writeFileSync(
		path.join(tmp, "release-manifest.json"),
		JSON.stringify({ includePaths }, null, 2),
	);
	const initialized = spawnSync("git", ["init", "--quiet"], {
		cwd: tmp,
		encoding: "utf8",
		windowsHide: true,
	});
	assert.equal(initialized.status, 0, initialized.stderr);
	const staged = spawnSync("git", ["add", "--all"], {
		cwd: tmp,
		encoding: "utf8",
		windowsHide: true,
	});
	assert.equal(staged.status, 0, staged.stderr);
	return tmp;
}

function runMiniRelease(tmp) {
	return spawnSync(
		process.execPath,
		[
			"scripts/build-release-artifacts.mjs",
			"--json",
			"--out-dir",
			path.join(tmp, "out"),
			"--timestamp",
			"210526-120099",
		],
		{
			cwd: tmp,
			encoding: "utf8",
			windowsHide: true,
		},
	);
}

test("release artifact builder creates a verified single-root source ZIP from the manifest", () => {
	const outDir = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-release-test-"));
	try {
		const result = spawnSync(
			process.execPath,
			[
				"scripts/build-release-artifacts.mjs",
				"--json",
				"--out-dir",
				outDir,
				"--timestamp",
				"210526-120001",
			],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
			},
		);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const payload = parseJson(result.stdout, "release artifact builder");
		assert.equal(payload.status, "pass");
		assert.equal(
			payload.rootName,
			`mcpace-v${deriveProjectVersion()}-210526-120001`,
		);
		assert.equal(payload.verificationReport.sourceProofStatus, "pass");
		assert.deepEqual(payload.verificationReport.rejectedManifestPaths, []);
		assert.deepEqual(payload.verificationReport.rejectedSourcePaths, []);
		assert.equal(
			fs.existsSync(payload.archive.path),
			true,
			"ZIP archive was not created",
		);
		assert.equal(
			fs.existsSync(payload.manifestPath),
			true,
			"artifact manifest was not created",
		);

		const artifactManifest = parseJson(
			fs.readFileSync(payload.manifestPath, "utf8"),
			"artifact manifest",
		);
		assert.equal(
			artifactManifest.sourceRoot,
			".",
			"artifact manifest must not leak a machine-local absolute root",
		);
		assert.equal(artifactManifest.sourceRootName, deriveProjectName());
		const archiveSha256 = sha256File(payload.archive.path);
		assert.equal(payload.archive.sha256, archiveSha256);
		assert.equal(artifactManifest.archive.sha256, archiveSha256);
		assert.equal(artifactManifest.archive.name, payload.archive.name);
		assert.equal(artifactManifest.archive.status, "verified");
		assert.equal(
			artifactManifest.archive.bytes,
			fs.statSync(payload.archive.path).size,
		);

		const files = listZipEntries(payload.archive.path);
		const zipMetadata = listZipEntryMetadata(payload.archive.path);
		assert.equal(payload.zipVerification.status, "pass");
		assert.equal(payload.zipVerification.entryCount, files.length);
		assert.ok(
			files.every((entry) => entry.startsWith(`${payload.rootName}/`)),
			"archive must contain exactly one root directory",
		);
		for (const required of [
			"README.md",
			"docs/README.md",
			"reports/summary.md",
			"reports/bundle-manifest.json",
			"reports/frontend-qa.json",
			"scripts/build-release-artifacts.mjs",
			"src/dashboard/frontend/app.runtime.js",
			"src/dashboard/frontend/app.render.details.js",
		]) {
			assert.ok(
				files.includes(`${payload.rootName}/${required}`),
				`archive missing ${required}`,
			);
		}

		const npmBin = zipMetadata.find(
			(entry) =>
				entry.name === `${payload.rootName}/packages/npm/cli/bin/mcpace.js`,
		);
		assert.ok(npmBin, "archive missing npm CLI bin shim");
		assert.equal(
			npmBin.hostSystem,
			3,
			"release ZIP should store Unix external attributes",
		);
		assert.notEqual(
			npmBin.unixMode & 0o111,
			0,
			"npm CLI bin shim must keep executable bits in the release ZIP",
		);
		for (const forbidden of [
			"node_modules/",
			".git/",
			"target/",
			"dist/",
			".cache/",
		]) {
			assert.equal(
				files.some((entry) => entry.includes(`/${forbidden}`)),
				false,
				`archive includes forbidden ${forbidden}`,
			);
		}
	} finally {
		fs.rmSync(outDir, { recursive: true, force: true });
	}
});

test("release artifact builder dry-run validates manifest without creating a ZIP", () => {
	const outDir = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-release-dry-run-"),
	);
	try {
		const result = spawnSync(
			process.execPath,
			[
				"scripts/build-release-artifacts.mjs",
				"--json",
				"--dry-run",
				"--out-dir",
				outDir,
				"--timestamp",
				"210526-120002",
			],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
			},
		);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const payload = parseJson(result.stdout, "release artifact dry-run");
		assert.equal(payload.dryRun, true);
		assert.equal(payload.releaseProofStatus, "dry-run");
		assert.equal(payload.verificationReport.sourceProofStatus, "pass");
		assert.deepEqual(payload.verificationReport.rejectedManifestPaths, []);
		assert.equal(
			fs.existsSync(payload.archive.path),
			false,
			"dry-run should not create a ZIP archive",
		);
		assert.equal(
			fs.existsSync(payload.manifestPath),
			true,
			"dry-run should still write a manifest for inspection",
		);
	} finally {
		fs.rmSync(outDir, { recursive: true, force: true });
	}
});

test("release artifact builder rejects manifest path traversal before staging", () => {
	const tmp = makeMiniReleaseRepo([
		"README.md",
		"docs/README.md",
		"reports/summary.md",
		"Cargo.toml",
		"package.json",
		"../outside.txt",
	]);
	try {
		const result = runMiniRelease(tmp);
		assert.notEqual(
			result.status,
			0,
			"path traversal manifest entry must fail release build",
		);
		const payload = parseJson(result.stdout, "path traversal rejection");
		assert.equal(payload.status, "failed");
		assert.match(payload.error, /rejectedManifestPaths/);
		assert.match(payload.error, /\.\. path segments/);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});

test("release artifact builder rejects ignored files under included directories", () => {
	const tmp = makeMiniReleaseRepo([
		"README.md",
		"docs/README.md",
		"reports/summary.md",
		"Cargo.toml",
		"package.json",
		"mcp_settings.d",
	]);
	try {
		const settingsDir = path.join(tmp, "mcp_settings.d");
		fs.mkdirSync(settingsDir, { recursive: true });
		fs.writeFileSync(path.join(settingsDir, "README.md"), "# Settings\n");
		fs.writeFileSync(path.join(tmp, ".gitignore"), "/mcp_settings.d/*.json\n");
		const staged = spawnSync(
			"git",
			["add", ".gitignore", "mcp_settings.d/README.md"],
			{ cwd: tmp, encoding: "utf8", windowsHide: true },
		);
		assert.equal(staged.status, 0, staged.stderr);
		fs.writeFileSync(
			path.join(settingsDir, "local-secret.json"),
			'{"headers":{"Authorization":"Bearer fixture-secret"}}\n',
		);

		const result = runMiniRelease(tmp);
		assert.notEqual(
			result.status,
			0,
			"ignored settings must fail release build",
		);
		const payload = parseJson(result.stdout, "ignored source rejection");
		assert.equal(payload.status, "failed");
		assert.match(payload.error, /ignored by Git/);
		assert.match(payload.error, /mcp_settings\.d\/local-secret\.json/);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});

test("release artifact builder rejects symlinked source entries instead of following them", (t) => {
	const tmp = makeMiniReleaseRepo([
		"README.md",
		"docs/README.md",
		"reports/summary.md",
		"Cargo.toml",
		"package.json",
		"linked-secret.txt",
	]);
	try {
		fs.writeFileSync(
			path.join(tmp, "secret.txt"),
			"must not be copied through a link\n",
		);
		try {
			fs.symlinkSync(
				path.join(tmp, "secret.txt"),
				path.join(tmp, "linked-secret.txt"),
			);
		} catch (error) {
			t.skip(
				`symlink unavailable in this environment: ${error?.message || error}`,
			);
			return;
		}

		const result = runMiniRelease(tmp);
		assert.notEqual(
			result.status,
			0,
			"symlink manifest entry must fail release build",
		);
		const payload = parseJson(result.stdout, "symlink rejection");
		assert.equal(payload.status, "failed");
		assert.match(payload.error, /rejectedSourcePaths/);
		assert.match(payload.error, /symbolic link/);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});
