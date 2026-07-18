#!/usr/bin/env node
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
	copyRegularFileNoFollowSync,
	lstatStableDirectorySync,
	writeFileAtomicSync,
	withFileLockSync,
} from "./lib/atomic-fs.mjs";
import {
	deriveProjectName,
	deriveProjectVersion,
	readJson,
	repoRoot,
} from "./lib/project-metadata.mjs";
import { createZipFromDirectory, listZipEntries } from "./lib/zip-writer.mjs";
import {
	sourceArchivePolicyIssue,
	assertSourceArchivePolicy,
} from "./lib/source-archive-policy.mjs";
import { listTrackedFiles, listWorkingTreeFiles } from "./lib/repo-files.mjs";
import { sha256File } from "./lib/rust-build-provenance.mjs";

const args = process.argv.slice(2);
const jsonOutput = args.includes("--json");
const dryRun = args.includes("--dry-run");

function argValue(name, fallback = null) {
	const index = args.indexOf(name);
	return index >= 0 ? (args[index + 1] ?? fallback) : fallback;
}

const outDir = path.resolve(
	argValue("--out-dir", path.join(repoRoot, ".artifacts")),
);
const timestampOverride = argValue(
	"--timestamp",
	process.env.MCPACE_RELEASE_TIMESTAMP || null,
);
const forbiddenParts = new Set([
	".git",
	"node_modules",
	"target",
	"dist",
	".cache",
	".pytest_cache",
	"__pycache__",
]);
const forbiddenFiles = new Set([".DS_Store", "Thumbs.db"]);
const DEFAULT_MAX_RELEASE_FILE_BYTES = 32 * 1024 * 1024;
const DEFAULT_MAX_RELEASE_TOTAL_BYTES = 512 * 1024 * 1024;

function positiveIntegerEnv(name, fallback) {
	const raw = process.env[name];
	if (!raw) return fallback;
	const value = Number(raw);
	if (!Number.isSafeInteger(value) || value <= 0) {
		throw new Error(`${name} must be a positive safe integer`);
	}
	return value;
}

const releaseResourceLimits = Object.freeze({
	maxFileBytes: positiveIntegerEnv(
		"MCPACE_RELEASE_MAX_FILE_BYTES",
		DEFAULT_MAX_RELEASE_FILE_BYTES,
	),
	maxTotalBytes: positiveIntegerEnv(
		"MCPACE_RELEASE_MAX_TOTAL_BYTES",
		DEFAULT_MAX_RELEASE_TOTAL_BYTES,
	),
});

function timestamp(now = new Date()) {
	if (timestampOverride) {
		if (!/^\d{6}-\d{6}$/.test(timestampOverride)) {
			throw new Error(
				`invalid --timestamp value '${timestampOverride}', expected ddmmyy-hhmmss`,
			);
		}
		return timestampOverride;
	}
	const pad = (value) => String(value).padStart(2, "0");
	return `${pad(now.getDate())}${pad(now.getMonth() + 1)}${String(now.getFullYear()).slice(-2)}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
}

function isWindowsAbsolutePath(value) {
	return /^[A-Za-z]:[\\/]/.test(value) || /^\\\\/.test(value);
}

function normalizeManifestPath(value) {
	if (typeof value !== "string") {
		throw new Error("manifest path must be a string");
	}
	if (value.length === 0) {
		throw new Error("manifest path must not be empty");
	}
	if (path.isAbsolute(value) || isWindowsAbsolutePath(value)) {
		throw new Error("manifest path must be repository-relative");
	}
	const parts = value.split(/[\\/]+/);
	if (parts.some((part) => part.length === 0)) {
		throw new Error("manifest path must not contain empty path segments");
	}
	if (parts.some((part) => part === "." || part === "..")) {
		throw new Error("manifest path must not contain . or .. path segments");
	}
	if (parts.some((part) => /^[A-Za-z]:$/.test(part))) {
		throw new Error("manifest path must not contain Windows drive segments");
	}
	return parts.join("/");
}

function normalizeManifestPathList(values, label, rejectedManifestPaths) {
	if (values === undefined) return [];
	if (!Array.isArray(values)) {
		rejectedManifestPaths.push({
			path: label,
			reason: `${label} must be an array when present`,
		});
		return [];
	}
	const normalized = [];
	const seen = new Set();
	for (const rawPath of values) {
		try {
			const value = normalizeManifestPath(rawPath).replace(/\/+$/, "");
			if (seen.has(value)) {
				rejectedManifestPaths.push({
					path: String(rawPath),
					reason: `duplicate ${label} path`,
				});
				continue;
			}
			seen.add(value);
			normalized.push(value);
		} catch (error) {
			rejectedManifestPaths.push({
				path: String(rawPath),
				reason: error?.message ?? String(error),
			});
		}
	}
	return normalized;
}

function readManifest() {
	const manifest = readJson("release-manifest.json");
	if (!Array.isArray(manifest.includePaths)) {
		throw new Error("release-manifest.json must contain includePaths array");
	}

	const rejectedManifestPaths = [];
	const includePaths = normalizeManifestPathList(
		manifest.includePaths,
		"includePaths",
		rejectedManifestPaths,
	);
	const runtimeDirectories = normalizeManifestPathList(
		manifest.runtimeDirectories,
		"runtimeDirectories",
		rejectedManifestPaths,
	);

	return {
		...manifest,
		includePaths,
		runtimeDirectories,
		rejectedManifestPaths,
	};
}

function hasRuntimeDirectoryPrefix(relativePath, runtimeDirectories = []) {
	const normalized = normalizeRelativePath(relativePath).replace(
		/^\/+|\/+$/g,
		"",
	);
	return runtimeDirectories.some(
		(runtimeDir) =>
			normalized === runtimeDir || normalized.startsWith(`${runtimeDir}/`),
	);
}

function shouldSkip(relativePath, runtimeDirectories = []) {
	const normalized = normalizeRelativePath(relativePath);
	const parts = normalized.split(/[\\/]+/).filter(Boolean);
	return (
		Boolean(sourceArchivePolicyIssue(normalized, { allowSingleRoot: false })) ||
		parts.some((part) => forbiddenParts.has(part)) ||
		forbiddenFiles.has(path.basename(normalized)) ||
		hasRuntimeDirectoryPrefix(normalized, runtimeDirectories)
	);
}

function normalizeRelativePath(relativePath) {
	return relativePath.split(path.sep).join("/");
}

const WINDOWS_RESERVED_SEGMENT =
	/^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?$/i;

function portablePathKey(relativePath) {
	return relativePath
		.split("/")
		.map((part) => part.normalize("NFC").toLocaleLowerCase("en-US"))
		.join("/");
}

function portablePathIssue(relativePath) {
	const parts = relativePath.split("/");
	for (const part of parts) {
		if (/[\u0000-\u001f]/.test(part)) {
			return "path segment contains a control character";
		}
		if (part.endsWith(" ") || part.endsWith(".")) {
			return "path segment has a trailing space or dot";
		}
		if (WINDOWS_RESERVED_SEGMENT.test(part)) {
			return `path segment uses a Windows reserved device name: ${part}`;
		}
		if (Buffer.byteLength(part, "utf8") > 255) {
			return "path segment exceeds 255 UTF-8 bytes";
		}
	}
	return null;
}

function analyzePortableFileSet(files) {
	const issues = [];
	const collisions = [];
	const seen = new Map();

	for (const file of files) {
		const issue = portablePathIssue(file);
		if (issue) {
			issues.push({ path: file, reason: issue });
		}

		const key = portablePathKey(file);
		const previous = seen.get(key);
		if (previous && previous !== file) {
			collisions.push({ key, paths: [previous, file] });
		} else {
			seen.set(key, file);
		}
	}

	return {
		issues: issues.sort((left, right) => left.path.localeCompare(right.path)),
		collisions: collisions.sort((left, right) =>
			left.key.localeCompare(right.key),
		),
	};
}

function copyPath(
	source,
	destination,
	relativePath,
	runtimeDirectories = [],
	sourcePolicy,
) {
	const normalizedRelativePath = normalizeRelativePath(relativePath);
	if (shouldSkip(normalizedRelativePath, runtimeDirectories)) {
		return {
			skipped: [normalizedRelativePath],
			copied: [],
			copiedBytes: 0,
			missing: [],
			rejected: [],
		};
	}

	let stat;
	try {
		stat = fs.lstatSync(source);
	} catch (error) {
		if (error?.code === "ENOENT" || error?.code === "ENOTDIR") {
			return {
				skipped: [],
				copied: [],
				copiedBytes: 0,
				missing: [normalizedRelativePath],
				rejected: [],
			};
		}
		return {
			skipped: [],
			copied: [],
			copiedBytes: 0,
			missing: [],
			rejected: [
				{
					path: normalizedRelativePath,
					reason: error?.message ?? String(error),
				},
			],
		};
	}

	if (stat.isSymbolicLink()) {
		return {
			skipped: [],
			copied: [],
			copiedBytes: 0,
			missing: [],
			rejected: [
				{
					path: normalizedRelativePath,
					reason: "source entry is a symbolic link",
				},
			],
		};
	}

	if (stat.isDirectory()) {
		const copied = [];
		let copiedBytes = 0;
		const skipped = [];
		const missing = [];
		const rejected = [];
		fs.mkdirSync(destination, { recursive: true });
		let entries;
		try {
			({ entries } = lstatStableDirectorySync(source));
		} catch (error) {
			return {
				skipped: [],
				copied: [],
				copiedBytes: 0,
				missing: [],
				rejected: [
					{
						path: normalizedRelativePath,
						reason: error?.message ?? String(error),
					},
				],
			};
		}
		for (const entry of entries) {
			const childRelative = path.posix.join(normalizedRelativePath, entry.name);
			const child = copyPath(
				path.join(source, entry.name),
				path.join(destination, entry.name),
				childRelative,
				runtimeDirectories,
				sourcePolicy,
			);
			copied.push(...child.copied);
			copiedBytes += child.copiedBytes ?? 0;
			skipped.push(...child.skipped);
			missing.push(...child.missing);
			rejected.push(...child.rejected);
		}
		return { copied, copiedBytes, skipped, missing, rejected };
	}

	if (!stat.isFile()) {
		return {
			skipped: [],
			copied: [],
			copiedBytes: 0,
			missing: [],
			rejected: [
				{
					path: normalizedRelativePath,
					reason: "source entry is not a regular file",
				},
			],
		};
	}

	if (!sourcePolicy.allowed.has(normalizedRelativePath)) {
		return {
			skipped: [],
			copied: [],
			copiedBytes: 0,
			missing: [],
			rejected: [
				{
					path: normalizedRelativePath,
					reason:
						"source entry is ignored by Git and cannot enter a release archive",
				},
			],
		};
	}
	if (
		sourcePolicy.requireTracked &&
		!sourcePolicy.tracked.has(normalizedRelativePath)
	) {
		return {
			skipped: [],
			copied: [],
			copiedBytes: 0,
			missing: [],
			rejected: [
				{
					path: normalizedRelativePath,
					reason:
						"source entry is untracked and cannot enter a non-dry-run release archive",
				},
			],
		};
	}

	try {
		const copiedFile = copyRegularFileNoFollowSync(source, destination, {
			maxBytes: releaseResourceLimits.maxFileBytes,
		});
		return {
			copied: [normalizedRelativePath],
			copiedBytes: copiedFile.size,
			skipped: [],
			missing: [],
			rejected: [],
		};
	} catch (error) {
		if (error?.code === "ENOENT" || error?.code === "ENOTDIR") {
			return {
				skipped: [],
				copied: [],
				copiedBytes: 0,
				missing: [normalizedRelativePath],
				rejected: [],
			};
		}
		return {
			skipped: [],
			copied: [],
			copiedBytes: 0,
			missing: [],
			rejected: [
				{
					path: normalizedRelativePath,
					reason: error?.message ?? String(error),
				},
			],
		};
	}
}

function walkFiles(root) {
	const files = [];
	const stack = [root];
	while (stack.length > 0) {
		const current = stack.pop();
		let entries;
		try {
			({ entries } = lstatStableDirectorySync(current));
		} catch (error) {
			throw new Error(
				`failed to inspect staged release directory '${normalizeRelativePath(path.relative(root, current))}': ${error?.message ?? error}`,
			);
		}
		for (const entry of entries) {
			const full = path.join(current, entry.name);
			const stat = fs.lstatSync(full);
			const relative = normalizeRelativePath(path.relative(root, full));
			if (stat.isSymbolicLink()) {
				throw new Error(`staged release entry is a symbolic link: ${relative}`);
			}
			if (stat.isDirectory()) {
				stack.push(full);
			} else if (stat.isFile()) {
				files.push(relative);
			} else {
				throw new Error(
					`staged release entry is not a regular file: ${relative}`,
				);
			}
		}
	}
	return files.sort();
}

function validateZipContents(archivePath, rootName, stagedFiles) {
	const expected = new Set(stagedFiles.map((file) => `${rootName}/${file}`));
	const actual = listZipEntries(archivePath);
	const outsideRoot = actual.filter(
		(entry) => !entry.startsWith(`${rootName}/`),
	);
	const missing = [...expected].filter((entry) => !actual.includes(entry));
	const extra = actual.filter((entry) => !expected.has(entry));
	return {
		status:
			outsideRoot.length === 0 && missing.length === 0 && extra.length === 0
				? "pass"
				: "failed",
		entryCount: actual.length,
		outsideRoot,
		missing,
		extra,
	};
}

function build() {
	const name = deriveProjectName();
	const version = deriveProjectVersion();
	const stamp = timestamp();
	const rootName = `${name}-v${version}-${stamp}`;
	const archiveName = `${rootName}.zip`;
	const archivePath = path.join(outDir, archiveName);
	const manifestPath = path.join(outDir, `${rootName}.manifest.json`);
	const tempParent = fs.mkdtempSync(path.join(os.tmpdir(), "mcpace-release-"));
	const stagedRoot = path.join(tempParent, rootName);

	try {
		const manifest = readManifest();
		const copied = [];
		const skipped = [];
		const missing = [];
		const rejectedSourcePaths = [];
		let copiedBytes = 0;
		const sourcePolicy = {
			allowed: new Set(
				listWorkingTreeFiles(repoRoot).map((file) =>
					normalizeRelativePath(path.relative(repoRoot, file)),
				),
			),
			tracked: new Set(
				listTrackedFiles(repoRoot).map((file) =>
					normalizeRelativePath(path.relative(repoRoot, file)),
				),
			),
			requireTracked: !dryRun,
		};

		for (const relativePath of manifest.includePaths) {
			const source = path.join(repoRoot, relativePath);
			const result = copyPath(
				source,
				path.join(stagedRoot, relativePath),
				relativePath,
				manifest.runtimeDirectories,
				sourcePolicy,
			);
			copied.push(...result.copied);
			copiedBytes += result.copiedBytes ?? 0;
			skipped.push(...result.skipped);
			missing.push(...result.missing);
			rejectedSourcePaths.push(...result.rejected);
		}

		const required = [
			"README.md",
			"docs/README.md",
			"reports/summary.md",
			"Cargo.toml",
			"package.json",
		];
		const stagedFiles = walkFiles(stagedRoot);
		const missingRequired = required.filter(
			(relativePath) => !stagedFiles.includes(relativePath),
		);
		const forbiddenIncluded = stagedFiles.filter((file) =>
			shouldSkip(file, manifest.runtimeDirectories),
		);
		const portablePathAnalysis = analyzePortableFileSet(stagedFiles);
		const resourceLimitIssues =
			copiedBytes > releaseResourceLimits.maxTotalBytes
				? [
						{
							reason: `release source inputs exceed maximum total size of ${releaseResourceLimits.maxTotalBytes} bytes`,
							copiedBytes,
						},
					]
				: [];

		let archiveSha256 = null;
		let archiveBytes = 0;
		let zipVerification = dryRun
			? {
					status: "dry-run",
					entryCount: 0,
					outsideRoot: [],
					missing: [],
					extra: [],
				}
			: null;

		const verificationReport = {
			sourceProofStatus:
				manifest.rejectedManifestPaths.length === 0 &&
				rejectedSourcePaths.length === 0 &&
				missing.length === 0 &&
				missingRequired.length === 0 &&
				forbiddenIncluded.length === 0 &&
				portablePathAnalysis.issues.length === 0 &&
				portablePathAnalysis.collisions.length === 0 &&
				resourceLimitIssues.length === 0
					? "pass"
					: "failed",
			copiedFileCount: stagedFiles.length,
			skippedPaths: skipped.sort(),
			rejectedManifestPaths: manifest.rejectedManifestPaths.sort(
				(left, right) => left.path.localeCompare(right.path),
			),
			rejectedSourcePaths: rejectedSourcePaths.sort((left, right) =>
				left.path.localeCompare(right.path),
			),
			missingManifestPaths: missing.sort(),
			missingRequiredPaths: missingRequired.sort(),
			forbiddenIncludedPaths: forbiddenIncluded.sort(),
			portablePathIssues: portablePathAnalysis.issues,
			portablePathCollisions: portablePathAnalysis.collisions,
			resourceLimits: releaseResourceLimits,
			copiedBytes,
			resourceLimitIssues,
		};

		if (verificationReport.sourceProofStatus !== "pass") {
			throw new Error(
				`source bundle verification failed: ${JSON.stringify(verificationReport, null, 2)}`,
			);
		}

		fs.mkdirSync(outDir, { recursive: true });

		if (!dryRun) {
			createZipFromDirectory(stagedRoot, archivePath, {
				rootName,
				date: new Date(0),
				maxFileBytes: releaseResourceLimits.maxFileBytes,
				maxTotalUncompressedBytes: releaseResourceLimits.maxTotalBytes,
			});
			zipVerification = validateZipContents(archivePath, rootName, stagedFiles);
			if (zipVerification.status !== "pass") {
				throw new Error(
					`ZIP verification failed: ${JSON.stringify(zipVerification, null, 2)}`,
				);
			}
			assertSourceArchivePolicy(listZipEntries(archivePath));
			archiveSha256 = sha256File(archivePath);
			archiveBytes = fs.statSync(archivePath).size;
		}

		writeFileAtomicSync(
			manifestPath,
			JSON.stringify(
				{
					schema: "mcpace.releaseArtifactManifest.v1",
					generatedAt: new Date().toISOString(),
					rootName,
					archiveName,
					archive: {
						name: archiveName,
						sha256: archiveSha256,
						bytes: archiveBytes,
						status: dryRun ? "dry-run" : "verified",
					},
					sourceRoot: ".",
					sourceRootName: deriveProjectName(),
					includePaths: manifest.includePaths,
					runtimeDirectories: manifest.runtimeDirectories,
					files: stagedFiles,
					verificationReport,
					zipVerification,
				},
				null,
				2,
			) + "\n",
			{ mode: 0o644 },
		);

		return {
			schema: "mcpace.releaseArtifactBuild.v1",
			status: "pass",
			dryRun,
			rootName,
			archive: {
				name: archiveName,
				path: archivePath,
				sha256: archiveSha256,
				bytes: archiveBytes,
			},
			manifestPath,
			releaseProofStatus: dryRun ? "dry-run" : "pass",
			verificationReport,
			zipVerification,
		};
	} finally {
		fs.rmSync(tempParent, { recursive: true, force: true });
	}
}

try {
	const result = withFileLockSync(
		path.join(outDir, ".mcpace-release.lock"),
		build,
	);
	if (jsonOutput) {
		process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
	} else {
		process.stdout.write(`Built ${result.archive.path}\n`);
		process.stdout.write(`Manifest ${result.manifestPath}\n`);
	}
} catch (error) {
	if (jsonOutput) {
		process.stdout.write(
			`${JSON.stringify(
				{
					schema: "mcpace.releaseArtifactBuild.v1",
					status: "failed",
					error: error?.message ?? String(error),
				},
				null,
				2,
			)}\n`,
		);
	} else {
		process.stderr.write(`${error?.stack ?? error}\n`);
	}
	process.exitCode = 1;
}
