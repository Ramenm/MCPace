import fs from "node:fs";
import path from "node:path";
import { isDeepStrictEqual } from "node:util";

function withoutGeneratedAt(report) {
	const copy = structuredClone(report);
	delete copy.generatedAt;
	return copy;
}

function readJson(filePath) {
	return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

export function generatedReportFreshness({
	repoRoot,
	jsonPath,
	expectedReport,
	markdownPath,
	expectedMarkdown,
}) {
	const findings = [];
	const fullJsonPath = path.join(repoRoot, jsonPath);
	try {
		const actual = readJson(fullJsonPath);
		if (typeof actual.generatedAt !== "string" || actual.generatedAt.length === 0) {
			findings.push(`${jsonPath} is missing generatedAt evidence`);
		}
		if (
			!isDeepStrictEqual(
				withoutGeneratedAt(actual),
				withoutGeneratedAt(expectedReport),
			)
		) {
			findings.push(`${jsonPath} is stale; regenerate the checked-in report`);
		}
	} catch (error) {
		findings.push(`${jsonPath} could not be validated: ${error.message}`);
	}

	if (markdownPath && expectedMarkdown !== undefined) {
		const fullMarkdownPath = path.join(repoRoot, markdownPath);
		try {
			const actual = fs.readFileSync(fullMarkdownPath, "utf8").replaceAll("\r\n", "\n");
			const expected = String(expectedMarkdown).replaceAll("\r\n", "\n");
			if (actual !== expected) {
				findings.push(
					`${markdownPath} is stale; regenerate the checked-in report`,
				);
			}
		} catch (error) {
			findings.push(`${markdownPath} could not be validated: ${error.message}`);
		}
	}
	return findings;
}
