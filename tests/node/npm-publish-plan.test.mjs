import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { repoRoot } from "../../scripts/lib/project-metadata.mjs";

function parseJson(value, label) {
	try {
		return JSON.parse(value);
	} catch (error) {
		assert.fail(`${label} is not valid JSON: ${error?.message ?? error}`);
	}
}

function shellQuote(value) {
	return `'${String(value).replaceAll("'", `'\\''`)}'`;
}

function createFakeNpm(directory) {
	const fakeScript = path.join(directory, "fake-npm.mjs");
	fs.writeFileSync(
		fakeScript,
		`const state = JSON.parse(process.env.FAKE_NPM_STATE ?? "{}");
const spec = process.argv[3] ?? "";
const at = spec.lastIndexOf("@");
const packageName = spec.slice(0, at);
if ((state.errors ?? []).includes(packageName)) {
  console.error("npm error code EAI_AGAIN");
  process.exit(1);
}
if ((state.present ?? []).includes(packageName)) {
  const releaseSha = state.releaseShaByPackage?.[packageName] ?? state.releaseSha ?? null;
  process.stdout.write(JSON.stringify({
    version: spec.slice(at + 1),
    mcpace: { releaseSha },
  }) + "\\n");
  process.exit(0);
}
console.error("npm error code ETARGET");
console.error("No matching version found for " + spec);
process.exit(1);
`,
		"utf8",
	);

	if (process.platform === "win32") {
		fs.writeFileSync(
			path.join(directory, "npm.cmd"),
			`@"${process.execPath}" "${fakeScript}" %*\r\n`,
			"utf8",
		);
	} else {
		const wrapper = path.join(directory, "npm");
		fs.writeFileSync(
			wrapper,
			`#!/bin/sh\nexec ${shellQuote(process.execPath)} ${shellQuote(fakeScript)} "$@"\n`,
			"utf8",
		);
		fs.chmodSync(wrapper, 0o755);
	}
}

const RELEASE_SHA = "a".repeat(40);

function runPlanner(state, envOverrides = {}) {
	const directory = fs.mkdtempSync(
		path.join(os.tmpdir(), "mcpace-publish-plan-"),
	);
	try {
		createFakeNpm(directory);
		return spawnSync(
			process.execPath,
			["scripts/plan-npm-publish.mjs", "--json"],
			{
				cwd: repoRoot,
				encoding: "utf8",
				windowsHide: true,
				env: {
					...process.env,
					PATH: `${directory}${path.delimiter}${process.env.PATH ?? ""}`,
					FAKE_NPM_STATE: JSON.stringify({ releaseSha: RELEASE_SHA, ...state }),
					GITHUB_EVENT_NAME: "push",
					GITHUB_REF: `refs/tags/v${cliPackage.version}`,
					GITHUB_REF_NAME: `v${cliPackage.version}`,
					GITHUB_SHA: RELEASE_SHA,
					GITHUB_RUN_NUMBER: "101",
					MCPACE_PUBLISH_DRY_RUN: "false",
					MCPACE_VERSION_OVERRIDE: "",
					...envOverrides,
				},
			},
		);
	} finally {
		fs.rmSync(directory, { recursive: true, force: true });
	}
}

const cliPackage = parseJson(
	fs.readFileSync(path.join(repoRoot, "packages/npm/cli/package.json"), "utf8"),
	"CLI package metadata",
);
const packageNames = [
	cliPackage.name,
	...Object.keys(cliPackage.optionalDependencies ?? {}),
];

test("npm publish planner schedules a complete absent package set", () => {
	const result = runPlanner({ present: [] });
	assert.equal(result.status, 0, result.stderr);
	const report = parseJson(result.stdout, "absent package plan");
	assert.deepEqual(report.packageNames, packageNames);
	assert.deepEqual(report.publishedPackages, []);
	assert.deepEqual(report.missingPackages, packageNames);
	assert.equal(report.shouldPublish, true);
	assert.equal(report.alreadyPublished, false);
});

test("npm publish planner resumes a partially published package set", () => {
	const present = [packageNames[0], packageNames[2], packageNames[5]];
	const result = runPlanner({ present });
	assert.equal(result.status, 0, result.stderr);
	const report = parseJson(result.stdout, "partial package plan");
	assert.deepEqual(report.publishedPackages, present);
	assert.deepEqual(
		report.missingPackages,
		packageNames.filter((name) => !present.includes(name)),
	);
	assert.equal(report.shouldPublish, true);
	assert.match(report.reason, /resuming partial package set/);
});

test("npm publish planner skips only when all seven versions exist", () => {
	const result = runPlanner({ present: packageNames });
	assert.equal(result.status, 0, result.stderr);
	const report = parseJson(result.stdout, "complete package plan");
	assert.deepEqual(report.missingPackages, []);
	assert.equal(report.shouldPublish, false);
	assert.equal(report.alreadyPublished, true);
});

test("npm publish planner fails closed on registry errors other than absence", () => {
	const result = runPlanner({ present: [], errors: [packageNames[1]] });
	assert.notEqual(result.status, 0);
	assert.match(result.stderr, /unable to check npm registry/);
	assert.match(result.stderr, /EAI_AGAIN/);
});

test("npm publish planner rejects partial packages from another release SHA", () => {
	const packageName = packageNames[2];
	const result = runPlanner({
		present: [packageName],
		releaseShaByPackage: { [packageName]: "b".repeat(40) },
	});
	assert.notEqual(result.status, 0);
	assert.match(result.stderr, /refusing to resume/);
	assert.match(result.stderr, /does not match/);
});

test("npm publish planner does not publish stable versions from main", () => {
	const result = runPlanner(
		{ present: [] },
		{
			GITHUB_REF: "refs/heads/main",
			GITHUB_REF_NAME: "main",
		},
	);
	assert.equal(result.status, 0, result.stderr);
	const report = parseJson(result.stdout, "main branch plan");
	assert.equal(report.channel, "unsupported");
	assert.equal(report.shouldPublish, false);
	assert.match(report.reason, /requires an exact vX\.Y\.Z tag/);
});

test("manual npm dispatch is always a packaging dry-run", () => {
	const result = runPlanner(
		{ present: packageNames },
		{
			GITHUB_EVENT_NAME: "workflow_dispatch",
			MCPACE_PUBLISH_DRY_RUN: "true",
		},
	);
	assert.equal(result.status, 0, result.stderr);
	const report = parseJson(result.stdout, "manual dry-run plan");
	assert.equal(report.channel, "dry-run");
	assert.equal(report.dryRun, true);
	assert.equal(report.shouldPublish, true);
	assert.deepEqual(report.publishedPackages, []);
});

test("manual npm dispatch fails closed if dry-run enforcement is bypassed", () => {
	const result = runPlanner(
		{ present: [] },
		{
			GITHUB_EVENT_NAME: "workflow_dispatch",
			MCPACE_PUBLISH_DRY_RUN: "false",
		},
	);
	assert.notEqual(result.status, 0);
	assert.match(
		result.stderr,
		/manual npm workflow dispatch is packaging dry-run only/,
	);
});
