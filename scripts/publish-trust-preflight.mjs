#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { repoRoot } from "./lib/project-metadata.mjs";

const workflowPath = path.join(repoRoot, ".github/workflows/publish-npm.yml");
const workflow = fs.readFileSync(workflowPath, "utf8");
function publishTokenFree(text) {
	const tokenReference =
		/\b(?:NPM_TOKEN|NODE_AUTH_TOKEN|NPM_CONFIG_[A-Z0-9_]*TOKEN)\b/i;
	return !tokenReference.test(text);
}

function shellLogicalLines(text) {
	return String(text || "")
		.replace(/\\\r?\n\s*/g, " ")
		.split(/\r?\n/);
}

const checks = [
	{
		name: "workflow uses GitHub OIDC id-token permission",
		pass: /id-token:\s*write/.test(workflow),
	},
	{
		name: "workflow relies on npm OIDC without long-lived token env fallback",
		pass: publishTokenFree(workflow),
	},
	{
		name: "publish lane validates native package contract before publish",
		pass: /verify-npm-publish-contract\.mjs --enforce/.test(workflow),
	},
	{
		name: "publish commands request provenance statements",
		pass: shellLogicalLines(workflow)
			.filter((line) => /^\s*npm\s+exec\b.*\s--\s+npm\s+publish\b/.test(line))
			.every((line) => line.includes("--provenance")),
	},
	{
		name: "real publish uses tag-bound stable and planned dev channels",
		pass:
			/branches:\s*\n\s*-\s*dev/.test(workflow) &&
			/tags:\s*\n\s*-\s*["']v\*["']/.test(workflow) &&
			/plan-npm-publish\.mjs --github-output/.test(workflow) &&
			/needs\.publish-plan\.outputs\.should_publish == 'true'/.test(workflow) &&
			/environment:\s*npm-publish/.test(workflow),
	},
];
const failures = checks.filter((check) => !check.pass);
const report = {
	schema: "mcpace.publishTrustPreflight.v1",
	generatedAt: new Date().toISOString(),
	ok: failures.length === 0,
	workflow: ".github/workflows/publish-npm.yml",
	checks,
};
if (process.argv.includes("--json"))
	process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
else if (report.ok)
	process.stdout.write(
		`PASS npm trusted publishing preflight: ${checks.length}/${checks.length}\n`,
	);
else
	process.stderr.write(
		failures.map((check) => `FAIL ${check.name}`).join("\n") + "\n",
	);
process.exit(report.ok ? 0 : 1);
