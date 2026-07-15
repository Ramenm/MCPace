import path from "node:path";

export const FORBIDDEN_SOURCE_ARCHIVE_PREFIXES = Object.freeze([
	"data/runtime",
	"data/server-state",
	"logs",
	"backups",
	"reports/tmp",
	"reports/runtime",
	"reports/local",
	".artifacts",
	".pi-subagents",
	".runtime",
]);

export const FORBIDDEN_SOURCE_ARCHIVE_BASENAMES = Object.freeze([
	"mcpace.sqlite",
	"mcpace-autostart.vbs",
]);

export const ALLOWED_SOURCE_ARCHIVE_DATABASE_PATHS = Object.freeze([
	// Static metadata catalog fixture intentionally shipped with the source tree.
	"metadata/metadata.sqlite3",
]);

export const FORBIDDEN_SOURCE_ARCHIVE_EXTENSIONS = Object.freeze([
	".sqlite",
	".sqlite3",
	".db",
]);

export const FORBIDDEN_SOURCE_ARCHIVE_PATTERNS = Object.freeze([
	/(?:^|\/)data\/runtime\//,
	/(?:^|\/)tool-list-cache\//,
	/(?:^|\/)catalog\/registry-cache(?:-search-[a-f0-9]+)?\.json$/i,
	/(?:^|\/)mcpace-serve-.*\.(?:exe|bin)$/i,
	/(?:^|\/)mcpace-autostart\.vbs$/i,
	/(?:^|\/)project-registry\.json$/i,
	/(?:^|\/)hub\/(?:leases|state)\.json$/i,
	/(?:^|\/)serve\/state\.json$/i,
	/(?:^|\/)[^/]+\.partial\.jsonl$/i,
]);

export function normalizeArchivePath(value) {
	return String(value || "")
		.replace(/\\/g, "/")
		.replace(/^\.\/+/, "")
		.replace(/^\/+/g, "")
		.replace(/\/+/g, "/");
}

function stripSingleRoot(pathValue) {
	const normalized = normalizeArchivePath(pathValue);
	const parts = normalized.split("/").filter(Boolean);
	if (parts.length <= 1) return normalized;
	return parts.slice(1).join("/");
}

function prefixMatch(relativePath, prefix) {
	return relativePath === prefix || relativePath.startsWith(`${prefix}/`);
}

export function sourceArchivePolicyIssue(entryPath, options = {}) {
	const normalized = normalizeArchivePath(entryPath);
	const candidates =
		options.allowSingleRoot === false
			? [normalized]
			: [normalized, stripSingleRoot(normalized)];

	const lowerCandidates = candidates
		.map((candidate) =>
			normalizeArchivePath(candidate).toLocaleLowerCase("en-US"),
		)
		.filter(Boolean);

	if (
		lowerCandidates.some((candidate) =>
			ALLOWED_SOURCE_ARCHIVE_DATABASE_PATHS.includes(candidate),
		)
	) {
		return null;
	}

	for (const relativePath of lowerCandidates) {
		const lower = relativePath;
		for (const prefix of FORBIDDEN_SOURCE_ARCHIVE_PREFIXES) {
			const normalizedPrefix =
				normalizeArchivePath(prefix).toLocaleLowerCase("en-US");
			if (prefixMatch(lower, normalizedPrefix)) {
				return `forbidden generated/runtime path prefix: ${prefix}`;
			}
		}

		const basename = path.posix.basename(lower);
		if (FORBIDDEN_SOURCE_ARCHIVE_BASENAMES.includes(basename)) {
			return `forbidden generated/runtime file: ${basename}`;
		}

		const extension = path.posix.extname(basename);
		if (FORBIDDEN_SOURCE_ARCHIVE_EXTENSIONS.includes(extension)) {
			return `forbidden generated/runtime extension: ${extension}`;
		}

		for (const pattern of FORBIDDEN_SOURCE_ARCHIVE_PATTERNS) {
			if (pattern.test(relativePath)) {
				return `forbidden generated/runtime pattern: ${pattern}`;
			}
		}
	}

	return null;
}

export function sourceArchivePolicyViolations(entries, options = {}) {
	const violations = [];
	for (const entry of entries) {
		const issue = sourceArchivePolicyIssue(entry, options);
		if (issue)
			violations.push({ path: normalizeArchivePath(entry), reason: issue });
	}
	return violations.sort((left, right) => left.path.localeCompare(right.path));
}

export function assertSourceArchivePolicy(entries, options = {}) {
	const violations = sourceArchivePolicyViolations(entries, options);
	if (violations.length > 0) {
		throw new Error(
			`source archive contains generated/runtime artifacts: ${JSON.stringify(violations, null, 2)}`,
		);
	}
	return true;
}
