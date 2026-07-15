import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const FALLBACK_SKIP_DIRS = new Set([
	".git",
	"node_modules",
	"target",
	"dist",
	".artifacts",
]);

function normalize(value) {
	return value.split(path.sep).join("/");
}

function gitFiles(repoRoot, args) {
	const result = spawnSync("git", ["-C", repoRoot, "ls-files", ...args, "-z"], {
		encoding: "utf8",
		windowsHide: true,
	});
	if (result.status !== 0 || typeof result.stdout !== "string") return null;
	return result.stdout
		.split("\0")
		.filter(Boolean)
		.map((relative) => path.join(repoRoot, ...relative.split("/")))
		.filter((file) => fs.existsSync(file) && fs.statSync(file).isFile())
		.sort();
}

function gitWorkingTreeFiles(repoRoot) {
	return gitFiles(repoRoot, ["--cached", "--others", "--exclude-standard"]);
}

function fallbackWalkFiles(repoRoot) {
	const files = [];
	const stack = [repoRoot];
	while (stack.length > 0) {
		const current = stack.pop();
		if (!fs.existsSync(current)) continue;
		for (const entry of fs
			.readdirSync(current, { withFileTypes: true })
			.sort((left, right) => left.name.localeCompare(right.name))) {
			const full = path.join(current, entry.name);
			const relative = normalize(path.relative(repoRoot, full));
			if (entry.isDirectory()) {
				if (
					!FALLBACK_SKIP_DIRS.has(entry.name) &&
					!relative.split("/").some((part) => FALLBACK_SKIP_DIRS.has(part))
				) {
					stack.push(full);
				}
			} else if (entry.isFile()) {
				files.push(full);
			}
		}
	}
	return files.sort();
}

/**
 * Return tracked plus non-ignored working-tree files when the root is a Git
 * checkout. This keeps local agent/runtime state out of source inspections
 * without hiding generated paths in non-Git fixture directories.
 */
export function listWorkingTreeFiles(repoRoot) {
	return gitWorkingTreeFiles(repoRoot) ?? fallbackWalkFiles(repoRoot);
}

/** Return only paths represented in the Git index; fail closed outside Git. */
export function listTrackedFiles(repoRoot) {
	const files = gitFiles(repoRoot, ["--cached"]);
	if (!files) {
		throw new Error("release source tracking requires a readable Git index");
	}
	return files;
}
