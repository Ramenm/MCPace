import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import {
	createZipFromDirectory,
	listZipEntries,
} from "../../scripts/lib/zip-writer.mjs";
import { repoRoot } from "../../scripts/lib/project-metadata.mjs";
import { runPython } from "./python-runner.mjs";

function writeFixture(root) {
	fs.mkdirSync(path.join(root, "src"), { recursive: true });
	fs.writeFileSync(path.join(root, "README.md"), "# clean fixture\n");
	fs.writeFileSync(path.join(root, "src", "main.rs"), "fn main() {}\n");
	fs.mkdirSync(path.join(root, "data", "runtime", "service"), {
		recursive: true,
	});
	fs.mkdirSync(path.join(root, "data", "runtime", "tool-list-cache"), {
		recursive: true,
	});
	fs.mkdirSync(path.join(root, "data", "runtime", "bin"), { recursive: true });
	fs.writeFileSync(
		path.join(root, "data", "runtime", "mcpace.sqlite"),
		"sqlite-state",
	);
	fs.writeFileSync(
		path.join(root, "data", "runtime", "service", "mcpace-autostart.vbs"),
		'WScript.Echo "bad"\n',
	);
	fs.writeFileSync(
		path.join(root, "data", "runtime", "tool-list-cache", "server.json"),
		"{}\n",
	);
	fs.writeFileSync(
		path.join(root, "data", "runtime", "bin", "mcpace.exe"),
		"binary",
	);
}

function runNodeScript(script, args, options = {}) {
	return spawnSync(process.execPath, [path.join(repoRoot, script), ...args], {
		cwd: repoRoot,
		encoding: "utf8",
		windowsHide: true,
		...options,
	});
}

function parseJson(output, label) {
	try {
		return JSON.parse(output);
	} catch (error) {
		assert.fail(`${label} did not emit valid JSON: ${error?.message || error}`);
	}
}

test("cleanzip drops generated runtime/state artifacts by path prefix, not just directory basename", () => {
	const tmp = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-cleanzip-fixture-"),
	);
	const out = path.join(
		os.tmpdir(),
		`mcpace-cleanzip-${process.pid}-${Date.now()}.zip`,
	);
	try {
		writeFixture(tmp);
		const result = runPython(
			[path.join(repoRoot, "cleanzip_fast.py"), tmp, out],
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
			`runtime entries leaked: ${entries.join(", ")}`,
		);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
		fs.rmSync(out, { force: true });
	}
});

test("cleanzip rejects symlinks instead of archiving files outside the input root", (t) => {
	const tmp = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-cleanzip-symlink-"),
	);
	const outside = path.join(
		os.tmpdir(),
		`mcpace-cleanzip-secret-${process.pid}-${Date.now()}.txt`,
	);
	const out = path.join(
		os.tmpdir(),
		`mcpace-cleanzip-symlink-${process.pid}-${Date.now()}.zip`,
	);
	try {
		fs.writeFileSync(path.join(tmp, "README.md"), "# symlink fixture\n");
		fs.writeFileSync(outside, "SYMLINK_SECRET_PROOF\n");
		try {
			fs.symlinkSync(outside, path.join(tmp, "linked.txt"), "file");
		} catch (error) {
			if (error?.code === "EPERM" || error?.code === "EACCES") {
				t.skip(`file symlinks are unavailable: ${error.code}`);
				return;
			}
			throw error;
		}
		const result = runPython(
			[path.join(repoRoot, "cleanzip_fast.py"), tmp, out],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
			},
		);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		const entries = listZipEntries(out);
		assert.ok(entries.includes("README.md"));
		assert.equal(
			entries.includes("linked.txt"),
			false,
			"symlink target leaked into the ZIP",
		);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
		fs.rmSync(outside, { force: true });
		fs.rmSync(out, { force: true });
	}
});

test("cleanzip rejects symlinked parent directories in fallback and Git modes", (t) => {
	const roots = [];
	const outputs = [];
	try {
		for (const mode of ["fallback", "git"]) {
			const root = fs.mkdtempSync(
				path.join(os.tmpdir(), `mcpace-cleanzip-parent-${mode}-`),
			);
			const out = path.join(
				os.tmpdir(),
				`mcpace-cleanzip-parent-${mode}-${process.pid}-${Date.now()}.zip`,
			);
			roots.push(root);
			outputs.push(out);
			const linked = path.join(root, "linked");
			const real = path.join(root, "real");
			fs.mkdirSync(linked, { recursive: true });
			fs.writeFileSync(
				path.join(linked, "tracked.txt"),
				"PARENT_SYMLINK_SECRET\n",
			);
			if (mode === "git") {
				const initialized = spawnSync("git", ["init", "--quiet"], {
					cwd: root,
					encoding: "utf8",
					windowsHide: true,
				});
				if (initialized.status !== 0) {
					t.skip("git is unavailable for the parent-symlink regression");
					return;
				}
				const added = spawnSync("git", ["add", "linked/tracked.txt"], {
					cwd: root,
					encoding: "utf8",
					windowsHide: true,
				});
				assert.equal(added.status, 0, added.stderr || added.stdout);
			}
			fs.renameSync(linked, real);
			try {
				fs.symlinkSync(
					real,
					linked,
					process.platform === "win32" ? "junction" : "dir",
				);
			} catch (error) {
				if (error?.code === "EPERM" || error?.code === "EACCES") {
					t.skip(`directory links are unavailable: ${error.code}`);
					return;
				}
				throw error;
			}
			const result = runPython(
				[path.join(repoRoot, "cleanzip_fast.py"), root, out],
				{ cwd: repoRoot, encoding: "utf8", windowsHide: true },
			);
			assert.equal(result.status, 0, result.stderr || result.stdout);
			const entries = listZipEntries(out);
			assert.equal(
				entries.includes("linked/tracked.txt"),
				false,
				`${mode} mode followed a parent link`,
			);
			if (mode === "fallback") assert.ok(entries.includes("real/tracked.txt"));
		}
	} finally {
		for (const root of roots) fs.rmSync(root, { recursive: true, force: true });
		for (const out of outputs) fs.rmSync(out, { force: true });
	}
});

test("cleanzip rejects a file that mutates during the stable-read stage", () => {
	const tmp = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-cleanzip-mutation-"),
	);
	const source = path.join(tmp, "source.txt");
	fs.writeFileSync(source, "stable-before-read\n");
	const probe = String.raw`
import importlib.util
import pathlib
import sys
spec = importlib.util.spec_from_file_location("cleanzip_fast", sys.argv[1])
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
source = pathlib.Path(sys.argv[2])
expected = source.lstat()
original = module.shutil.copyfileobj
def mutating_copy(reader, writer, length=1024 * 1024):
    first = reader.read(1)
    writer.write(first)
    with source.open("ab") as changed:
        changed.write(b"changed-during-read")
    original(reader, writer, length)
module.shutil.copyfileobj = mutating_copy
try:
    staged = module.read_regular_file_stable(source, source.parent, expected)
except OSError:
    raise SystemExit(0)
staged.close()
raise SystemExit(1)
`;
	try {
		const result = runPython(
			["-c", probe, path.join(repoRoot, "cleanzip_fast.py"), source],
			{ cwd: repoRoot, encoding: "utf8", windowsHide: true },
		);
		assert.equal(result.status, 0, result.stderr || result.stdout);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});

test("cleanzip rejects parent links that escape the input root in Git and fallback modes", (t) => {
	const roots = [];
	const outsideRoots = [];
	const outputs = [];
	try {
		for (const mode of ["fallback", "git"]) {
			const root = fs.mkdtempSync(
				path.join(os.tmpdir(), `mcpace-parent-escape-${mode}-`),
			);
			const outside = fs.mkdtempSync(
				path.join(os.tmpdir(), `mcpace-parent-secret-${mode}-`),
			);
			const out = path.join(
				os.tmpdir(),
				`mcpace-parent-escape-${mode}-${process.pid}.zip`,
			);
			roots.push(root);
			outsideRoots.push(outside);
			outputs.push(out);
			const linked = path.join(root, "linked");
			fs.mkdirSync(linked);
			fs.writeFileSync(
				path.join(linked, "secret.txt"),
				"ESCAPED_PARENT_SECRET\n",
			);
			if (mode === "git") {
				const init = spawnSync("git", ["init", "--quiet"], {
					cwd: root,
					encoding: "utf8",
					windowsHide: true,
				});
				if (init.status !== 0) {
					t.skip("git is unavailable for the parent-escape regression");
					return;
				}
				const add = spawnSync("git", ["add", "linked/secret.txt"], {
					cwd: root,
					encoding: "utf8",
					windowsHide: true,
				});
				assert.equal(add.status, 0, add.stderr || add.stdout);
			}
			fs.renameSync(
				path.join(linked, "secret.txt"),
				path.join(outside, "secret.txt"),
			);
			fs.rmdirSync(linked);
			try {
				fs.symlinkSync(
					outside,
					linked,
					process.platform === "win32" ? "junction" : "dir",
				);
			} catch (error) {
				if (error?.code === "EPERM" || error?.code === "EACCES") {
					t.skip(`directory links are unavailable: ${error.code}`);
					return;
				}
				throw error;
			}
			const result = runPython(
				[path.join(repoRoot, "cleanzip_fast.py"), root, out],
				{
					cwd: repoRoot,
					encoding: "utf8",
					windowsHide: true,
				},
			);
			assert.equal(result.status, 0, result.stderr || result.stdout);
			assert.equal(listZipEntries(out).includes("linked/secret.txt"), false);
		}
	} finally {
		for (const root of roots) fs.rmSync(root, { recursive: true, force: true });
		for (const root of outsideRoots)
			fs.rmSync(root, { recursive: true, force: true });
		for (const out of outputs) fs.rmSync(out, { force: true });
	}
});

test("cleanzip drops FIFO inputs as non-regular files where supported", (t) => {
	if (process.platform === "win32") {
		t.skip("FIFO regression requires a Unix host");
		return;
	}
	const root = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-cleanzip-fifo-"));
	const out = path.join(os.tmpdir(), `mcpace-cleanzip-fifo-${process.pid}.zip`);
	try {
		const fifo = path.join(root, "blocked.fifo");
		const made = spawnSync("mkfifo", [fifo], { encoding: "utf8" });
		if (made.status !== 0) {
			t.skip("mkfifo is unavailable");
			return;
		}
		const result = runPython(
			[path.join(repoRoot, "cleanzip_fast.py"), root, out],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
			},
		);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		assert.equal(listZipEntries(out).includes("blocked.fifo"), false);
	} finally {
		fs.rmSync(root, { recursive: true, force: true });
		fs.rmSync(out, { force: true });
	}
});

test("cleanzip detects final and parent replacement during a staged read", (t) => {
	if (process.platform === "win32") {
		t.skip("replacement race regression requires Unix rename/link semantics");
		return;
	}
	const tmp = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-cleanzip-replace-"),
	);
	const probe = String.raw`
import importlib.util
import pathlib
import sys
spec = importlib.util.spec_from_file_location("cleanzip_fast", sys.argv[1])
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
root = pathlib.Path(sys.argv[2])
mode = sys.argv[3]
parent = root / "parent"
parent.mkdir()
source = parent / "source.txt"
source.write_text("original-content", encoding="utf-8")
expected = source.lstat()
original_copy = module.shutil.copyfileobj
def replacing_copy(reader, writer, length=1024 * 1024):
    writer.write(reader.read(1))
    if mode == "final":
        source.rename(parent / "source.old")
        source.write_text("replacement-content", encoding="utf-8")
    else:
        outside = root / "outside"
        outside.mkdir()
        (outside / "source.txt").write_text("outside-content", encoding="utf-8")
        parent.rename(root / "parent.old")
        parent.symlink_to(outside, target_is_directory=True)
    original_copy(reader, writer, length)
module.shutil.copyfileobj = replacing_copy
try:
    staged = module.read_regular_file_stable(source, root, expected)
except OSError:
    raise SystemExit(0)
staged.close()
raise SystemExit(1)
`;
	try {
		for (const mode of ["final", "parent"]) {
			const root = path.join(tmp, mode);
			fs.mkdirSync(root);
			const result = runPython(
				["-c", probe, path.join(repoRoot, "cleanzip_fast.py"), root, mode],
				{
					cwd: repoRoot,
					encoding: "utf8",
					windowsHide: true,
				},
			);
			assert.equal(
				result.status,
				0,
				`${mode}: ${result.stderr || result.stdout}`,
			);
		}
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
	}
});

test("clean archive verifier rejects runtime artifacts in directories and ZIPs", () => {
	const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-clean-verify-"));
	const zip = path.join(
		os.tmpdir(),
		`mcpace-clean-verify-${process.pid}-${Date.now()}.zip`,
	);
	try {
		writeFixture(tmp);
		let result = runNodeScript("scripts/verify-clean-archive.mjs", [
			"--json",
			"--source-tree",
			"--repo",
			tmp,
		]);
		assert.notEqual(result.status, 0, result.stdout);
		let report = parseJson(result.stdout, "source-tree verifier");
		assert.equal(report.status, "fail");
		assert.ok(
			report.checks[0].violations.some(
				(issue) => issue.path === "data/runtime/mcpace.sqlite",
			),
		);
		assert.ok(
			report.checks[0].violations.some(
				(issue) => issue.path === "data/runtime/service/mcpace-autostart.vbs",
			),
		);

		createZipFromDirectory(tmp, zip, {
			rootName: "fixture-root",
			date: new Date(0),
		});
		result = runNodeScript("scripts/verify-clean-archive.mjs", [
			"--json",
			"--archive",
			zip,
		]);
		assert.notEqual(result.status, 0, result.stdout);
		report = parseJson(result.stdout, "ZIP verifier");
		assert.equal(report.status, "fail");
		assert.ok(
			report.checks[0].violations.some(
				(issue) => issue.path === "fixture-root/data/runtime/bin/mcpace.exe",
			),
		);
	} finally {
		fs.rmSync(tmp, { recursive: true, force: true });
		fs.rmSync(zip, { force: true });
	}
});

test("repository source tree has no generated runtime/state artifacts checked into the bundle", () => {
	const result = runNodeScript("scripts/verify-clean-archive.mjs", [
		"--json",
		"--source-tree",
		"--repo",
		repoRoot,
	]);
	assert.equal(result.status, 0, result.stderr || result.stdout);
	const report = parseJson(result.stdout, "repository source-tree verifier");
	assert.equal(report.status, "pass");
	assert.deepEqual(
		report.checks.flatMap((check) => check.violations),
		[],
	);
});
