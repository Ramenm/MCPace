#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { repoRoot } from "./lib/project-metadata.mjs";

const sourceFiles = [
	"src/dashboard/frontend/app.js",
	"src/dashboard/frontend/app.runtime.js",
	"src/dashboard/frontend/app.boot.js",
];
const source = sourceFiles
	.map((relativePath) =>
		fs.readFileSync(path.join(repoRoot, relativePath), "utf8"),
	)
	.join("\n");

const checks = [
	{
		name: "visibility resume does not force overview rebuild",
		pass:
			/else refreshDashboard\(\{ reason: "visible" \}\)/.test(source) &&
			!/reason: "visible"[^\n]+force: true/.test(source),
	},
	{
		name: "refresh overlap is suppressed",
		pass: /state\.refreshing && !options\.forceAbort/.test(source),
	},
	{
		name: "failed refreshes use exponential backoff",
		pass:
			/MAX_REFRESH_FAILURE_BACKOFF_MS/.test(source) &&
			/2 \*\* Math\.min\(state\.failureCount/.test(source),
	},
	{
		name: "page freeze aborts in-flight refresh and defers work",
		pass:
			/document\.addEventListener\("freeze"/.test(source) &&
			/state\.controller\)\s*state\.controller\.abort/.test(source),
	},
	{
		name: "page resume uses cached refresh path",
		pass:
			/document\.addEventListener\("resume"/.test(source) &&
			/refreshDashboard\(\{ reason: "resume" \}\)/.test(source) &&
			!/reason: "resume"[^\n]+force: true/.test(source),
	},
	{
		name: "discarded or bfcache pages recover once without forced rebuild",
		pass:
			/document\.wasDiscarded/.test(source) &&
			/window\.addEventListener\("pageshow"/.test(source) &&
			/reason: "pageshow"/.test(source) &&
			!/reason: "pageshow"[^\n]+force: true/.test(source),
	},
];
const failures = checks.filter((check) => !check.pass);
const report = {
	schema: "mcpace.browserLifecycleProof.v1",
	generatedAt: new Date().toISOString(),
	ok: failures.length === 0,
	files: sourceFiles,
	checks,
};
if (process.argv.includes("--json"))
	process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
else if (report.ok)
	process.stdout.write(
		`PASS browser lifecycle proof: ${checks.length}/${checks.length}\n`,
	);
else
	process.stderr.write(
		failures.map((check) => `FAIL ${check.name}`).join("\n") + "\n",
	);
process.exit(report.ok ? 0 : 1);
