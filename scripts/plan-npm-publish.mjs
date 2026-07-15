#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import process from "node:process";
import { readCliPackageJson } from "./lib/project-metadata.mjs";

function parseArgs(argv) {
	return {
		githubOutput: argv.includes("--github-output"),
		json: argv.includes("--json") || !argv.includes("--github-output"),
	};
}

function fail(message) {
	console.error(message);
	process.exit(1);
}

function isStableSemver(version) {
	return /^\d+\.\d+\.\d+$/.test(String(version));
}

function registryVersionMetadata(packageName, version) {
	const npmArgs = ["view", `${packageName}@${version}`, "--json"];
	const command = process.platform === "win32" ? "cmd" : "npm";
	const commandArgs =
		process.platform === "win32"
			? ["/d", "/s", "/c", "npm", ...npmArgs]
			: npmArgs;
	const result = spawnSync(command, commandArgs, {
		encoding: "utf8",
		stdio: ["ignore", "pipe", "pipe"],
		timeout: 60_000,
	});
	if (result.status === 0) {
		let metadata;
		try {
			metadata = JSON.parse(result.stdout || "{}");
		} catch (error) {
			throw new Error(
				`npm registry returned invalid JSON for ${packageName}@${version}: ${error?.message ?? error}`,
				{ cause: error },
			);
		}
		if (metadata?.version !== version) {
			throw new Error(
				`npm registry returned unexpected version for ${packageName}@${version}: ${metadata?.version ?? "<missing>"}`,
			);
		}
		return {
			version: metadata.version,
			releaseSha: metadata.mcpace?.releaseSha ?? null,
		};
	}
	const combined = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
	if (
		/E404|ETARGET|404 Not Found|No match(?:ing)? (?:version )?found/i.test(
			combined,
		)
	) {
		return null;
	}
	const detail =
		result.error?.message ?? (combined.trim() || `exit ${result.status}`);
	throw new Error(
		`unable to check npm registry for ${packageName}@${version}: ${detail}`,
	);
}

function setGithubOutput(values) {
	const outputPath = process.env.GITHUB_OUTPUT;
	if (!outputPath) return;
	const lines = Object.entries(values).map(
		([key, value]) => `${key}=${String(value).replace(/\r?\n/g, " ")}`,
	);
	fs.appendFileSync(outputPath, `${lines.join("\n")}\n`, "utf8");
}

function plan() {
	const cliPackage = readCliPackageJson();
	const packageName = cliPackage.name;
	const sourceVersion = cliPackage.version;
	if (!packageName) fail("packages/npm/cli/package.json is missing name");
	if (!isStableSemver(sourceVersion)) {
		fail(
			`source package version must stay stable (x.y.z); got '${sourceVersion}'`,
		);
	}
	const versionOverride = (process.env.MCPACE_VERSION_OVERRIDE ?? "").trim();
	if (versionOverride && !isStableSemver(versionOverride)) {
		fail(
			`MCPACE_VERSION_OVERRIDE must be stable x.y.z when set; got '${versionOverride}'`,
		);
	}
	const baseVersion = versionOverride || sourceVersion;

	const ref = process.env.GITHUB_REF ?? "";
	const refName = process.env.GITHUB_REF_NAME ?? "";
	const eventName = process.env.GITHUB_EVENT_NAME ?? "";
	const runNumber = process.env.GITHUB_RUN_NUMBER ?? "0";
	const dryRun =
		String(process.env.MCPACE_PUBLISH_DRY_RUN ?? "false").toLowerCase() ===
		"true";
	if (eventName === "workflow_dispatch" && !dryRun) {
		fail("manual npm workflow dispatch is packaging dry-run only");
	}
	const releaseSha = String(process.env.GITHUB_SHA ?? "")
		.trim()
		.toLowerCase();
	if (versionOverride && !dryRun) {
		fail("MCPACE_VERSION_OVERRIDE is allowed only for packaging dry-runs");
	}

	let channel = "unsupported";
	let distTag = "latest";
	let effectiveVersion = baseVersion;
	let reason = "";

	if (eventName === "workflow_dispatch" && dryRun) {
		channel = "dry-run";
		distTag = ref === "refs/heads/dev" ? "dev" : "latest";
		reason = "manual dry-run validates packaging without publishing";
	} else if (ref === "refs/heads/dev" && eventName === "push") {
		channel = "dev";
		distTag = "dev";
		effectiveVersion = `${baseVersion}-dev.${runNumber}`;
		reason =
			"dev branch publishes a unique prerelease version to the dev dist-tag";
	} else if (ref.startsWith("refs/tags/v")) {
		const tagVersion = refName.replace(/^v/, "");
		if (tagVersion !== baseVersion) {
			fail(
				`release tag ${refName} does not match package version ${baseVersion}`,
			);
		}
		channel = "stable";
		distTag = "latest";
		reason =
			"version tag publishes the stable package version to latest when that version is absent from npm";
	} else {
		reason = `ref '${ref || "<missing>"}' is not publishable; stable npm publication requires an exact vX.Y.Z tag`;
	}

	const packageNames = [
		packageName,
		...Object.keys(cliPackage.optionalDependencies ?? {}),
	];
	let publishedPackages = [];
	let publishedPackageMetadata = [];
	let missingPackages = [...packageNames];
	if (channel !== "unsupported" && !dryRun) {
		if (!/^[a-f0-9]{40}$/.test(releaseSha)) {
			fail(
				"real npm publication requires GITHUB_SHA as a full 40-character commit SHA",
			);
		}
		publishedPackageMetadata = packageNames
			.map((name) => ({
				name,
				metadata: registryVersionMetadata(name, effectiveVersion),
			}))
			.filter((entry) => entry.metadata !== null)
			.map((entry) => ({
				name: entry.name,
				version: entry.metadata.version,
				releaseSha: entry.metadata.releaseSha,
			}));
		for (const entry of publishedPackageMetadata) {
			if (entry.releaseSha !== releaseSha) {
				throw new Error(
					`refusing to resume ${entry.name}@${entry.version}: registry release SHA ${entry.releaseSha ?? "<missing>"} does not match ${releaseSha}`,
				);
			}
		}
		publishedPackages = publishedPackageMetadata.map((entry) => entry.name);
		const published = new Set(publishedPackages);
		missingPackages = packageNames.filter((name) => !published.has(name));
	}
	const alreadyPublished = publishedPackages.length === packageNames.length;
	const shouldPublish =
		channel !== "unsupported" && (dryRun || missingPackages.length > 0);
	if (alreadyPublished) {
		reason = `all ${packageNames.length} packages at ${effectiveVersion} already exist on npm; skipping duplicate publish`;
	} else if (!dryRun && publishedPackages.length > 0) {
		reason = `resuming partial package set: ${publishedPackages.length}/${packageNames.length} already published`;
	}

	return {
		schema: "mcpace.npmPublishPlan.v1",
		packageName,
		packageNames,
		sourceVersion,
		baseVersion,
		versionOverride: versionOverride || null,
		effectiveVersion,
		channel,
		distTag,
		dryRun,
		alreadyPublished,
		releaseSha: releaseSha || null,
		publishedPackages,
		publishedPackageMetadata,
		missingPackages,
		shouldPublish,
		ref,
		refName,
		eventName,
		runNumber,
		reason,
	};
}

try {
	const args = parseArgs(process.argv.slice(2));
	const report = plan();
	if (args.githubOutput) {
		setGithubOutput({
			package_name: report.packageName,
			source_version: report.sourceVersion,
			base_version: report.baseVersion,
			version_override: report.versionOverride ?? "",
			effective_version: report.effectiveVersion,
			release_sha: report.releaseSha ?? "",
			channel: report.channel,
			dist_tag: report.distTag,
			dry_run: report.dryRun,
			already_published: report.alreadyPublished,
			should_publish: report.shouldPublish,
			reason: report.reason,
		});
	}
	if (args.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
	else
		process.stdout.write(
			`npm publish plan: ${report.shouldPublish ? "publish" : "skip"} ${report.packageName}@${report.effectiveVersion} (${report.distTag}) — ${report.reason}\n`,
		);
	process.exitCode = 0;
} catch (error) {
	console.error(error?.stack ?? String(error));
	process.exitCode = 1;
}
