import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { repoRoot } from "../../scripts/lib/project-metadata.mjs";
import {
	listZipEntries,
	listZipEntryMetadata,
} from "../../scripts/lib/zip-writer.mjs";

function writeFile(file, data, mode = 0o644) {
	fs.mkdirSync(path.dirname(file), { recursive: true });
	fs.writeFileSync(file, data, { mode });
}

function pythonCommand() {
	const candidates = [process.env.PYTHON, "python3", "python"].filter(Boolean);
	for (const command of candidates) {
		const result = spawnSync(command, ["--version"], {
			encoding: "utf8",
			windowsHide: true,
		});
		if (result.status === 0) return command;
	}
	return null;
}

test("cleanzip removes MCPace runtime state and keeps the npm bin shim executable", (t) => {
	const python = pythonCommand();
	if (!python) {
		t.skip("Python is not available in this test environment");
		return;
	}

	const tmp = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-cleanzip-hygiene-"),
	);
	const source = path.join(tmp, "source");
	const out = path.join(tmp, "clean.zip");
	const report = path.join(tmp, "report.json");
	try {
		writeFile(path.join(source, "README.md"), "# fixture\n");
		writeFile(path.join(source, "src", "main.rs"), "fn main() {}\n");
		writeFile(
			path.join(source, "packages", "npm", "cli", "bin", "mcpace.js"),
			'#!/usr/bin/env node\nconsole.log("ok");\n',
			0o644,
		);
		writeFile(
			path.join(source, "data", "runtime", "mcpace.sqlite"),
			"runtime-db\n",
		);
		writeFile(
			path.join(source, "data", "runtime", "service", "mcpace-autostart.vbs"),
			'CreateObject("WScript.Shell")\n',
		);
		writeFile(path.join(source, "data", "server-state", "state.json"), "{}\n");
		writeFile(
			path.join(source, "logs", "mcpace.log"),
			"secret-ish local log\n",
		);
		writeFile(
			path.join(source, "eval", "random-100-npm-sweep.partial.jsonl"),
			"{}\n",
		);

		const result = spawnSync(
			python,
			[
				path.join(repoRoot, "cleanzip_fast.py"),
				source,
				out,
				"--report",
				report,
			],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
			},
		);
		assert.equal(result.status, 0, result.stderr || result.stdout);

		const entries = listZipEntries(out);
		assert.ok(entries.includes("README.md"));
		assert.ok(entries.includes("src/main.rs"));
		assert.equal(
			entries.some((entry) => entry.startsWith("data/runtime/")),
			false,
			"runtime state leaked into clean ZIP",
		);
		assert.equal(
			entries.some((entry) => entry.startsWith("data/server-state/")),
			false,
			"server state leaked into clean ZIP",
		);
		assert.equal(
			entries.some((entry) => entry.startsWith("logs/")),
			false,
			"logs leaked into clean ZIP",
		);
		assert.equal(
			entries.some((entry) => entry.endsWith(".partial.jsonl")),
			false,
			"partial sweep streams leaked into clean ZIP",
		);

		const metadata = listZipEntryMetadata(out);
		const bin = metadata.find(
			(entry) => entry.name === "packages/npm/cli/bin/mcpace.js",
		);
		assert.ok(bin, "npm bin shim missing from clean ZIP");
		assert.notEqual(
			bin.unixMode & 0o111,
			0,
			"npm bin shim must stay executable after clean ZIP extraction",
		);

		const payload = JSON.parse(fs.readFileSync(report, "utf8"));
		const runtimeDrops =
			(payload.dropped_reasons.junk_path_prefix || 0) +
			(payload.dropped_reasons.junk_dir || 0);
		assert.ok(runtimeDrops >= 3, "runtime drops should be counted by reason");
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});

test("release artifact builder honors runtimeDirectories even when a broad source directory is included", () => {
	const tmp = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-release-runtime-dirs-"),
	);
	try {
		fs.mkdirSync(path.join(tmp, "scripts", "lib"), { recursive: true });
		for (const relativePath of [
			"scripts/build-release-artifacts.mjs",
			"scripts/lib/atomic-fs.mjs",
			"scripts/lib/project-metadata.mjs",
			"scripts/lib/zip-writer.mjs",
			"scripts/lib/source-archive-policy.mjs",
			"scripts/lib/repo-files.mjs",
			"scripts/lib/rust-build-provenance.mjs",
		]) {
			fs.cpSync(
				path.join(repoRoot, relativePath),
				path.join(tmp, relativePath),
			);
		}
		writeFile(
			path.join(tmp, "package.json"),
			JSON.stringify({ name: "mcpace-workspace", version: "0.7.8" }, null, 2),
		);
		writeFile(
			path.join(tmp, "Cargo.toml"),
			'[package]\nname = "mcpace"\nversion = "0.7.8"\n',
		);
		writeFile(path.join(tmp, "README.md"), "# MCPace\n");
		writeFile(path.join(tmp, "docs", "README.md"), "# Docs\n");
		writeFile(path.join(tmp, "reports", "summary.md"), "# Summary\n");
		writeFile(
			path.join(tmp, "data", "checked-in-fixture.json"),
			'{"ok":true}\n',
		);
		writeFile(
			path.join(tmp, "data", "runtime", "mcpace.sqlite"),
			"runtime-db\n",
		);
		writeFile(path.join(tmp, "data", "server-state", "state.json"), "{}\n");
		writeFile(path.join(tmp, "logs", "mcpace.log"), "local log\n");
		writeFile(
			path.join(tmp, "release-manifest.json"),
			JSON.stringify(
				{
					includePaths: [
						"README.md",
						"docs/README.md",
						"reports/summary.md",
						"Cargo.toml",
						"package.json",
						"data",
						"logs",
					],
					runtimeDirectories: ["data/runtime", "data/server-state", "logs"],
				},
				null,
				2,
			),
		);
		const gitInit = spawnSync("git", ["init", "--quiet"], {
			cwd: tmp,
			encoding: "utf8",
			windowsHide: true,
		});
		assert.equal(gitInit.status, 0, gitInit.stderr || gitInit.stdout);
		const gitAdd = spawnSync("git", ["add", "--all"], {
			cwd: tmp,
			encoding: "utf8",
			windowsHide: true,
		});
		assert.equal(gitAdd.status, 0, gitAdd.stderr || gitAdd.stdout);

		const result = spawnSync(
			process.execPath,
			[
				"scripts/build-release-artifacts.mjs",
				"--json",
				"--out-dir",
				path.join(tmp, "out"),
				"--timestamp",
				"210526-120333",
			],
			{
				cwd: tmp,
				encoding: "utf8",
				windowsHide: true,
			},
		);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const payload = JSON.parse(result.stdout);
		assert.equal(payload.status, "pass");
		assert.ok(
			payload.verificationReport.skippedPaths.includes("data/runtime"),
			"data/runtime should be skipped as a runtimeDirectory",
		);
		assert.ok(
			payload.verificationReport.skippedPaths.includes("data/server-state"),
			"data/server-state should be skipped as a runtimeDirectory",
		);
		assert.ok(
			payload.verificationReport.skippedPaths.includes("logs"),
			"logs should be skipped as a runtimeDirectory",
		);
		assert.ok(payload.verificationReport.copiedFileCount >= 6);
		assert.equal(payload.verificationReport.forbiddenIncludedPaths.length, 0);

		const entries = listZipEntries(payload.archive.path);
		assert.equal(
			entries.some((entry) => entry.includes("/data/runtime/")),
			false,
		);
		assert.equal(
			entries.some((entry) => entry.includes("/data/server-state/")),
			false,
		);
		assert.equal(
			entries.some((entry) => entry.includes("/logs/")),
			false,
		);
		assert.ok(
			entries.some((entry) => entry.endsWith("/data/checked-in-fixture.json")),
		);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});
