#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { writeFileAtomicSync } from "./lib/atomic-fs.mjs";
import { generatedReportFreshness } from "./lib/report-freshness.mjs";
import { deriveProjectName } from "./lib/project-metadata.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");
const args = new Set(process.argv.slice(2));
const write = args.has("--write");
const check = args.has("--check");
const jsonOnly = args.has("--json");

function read(relativePath) {
	return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function readJson(relativePath) {
	try {
		return JSON.parse(read(relativePath));
	} catch (error) {
		throw new Error(
			`Invalid JSON in ${relativePath}: ${error instanceof Error ? error.message : String(error)}`,
			{ cause: error },
		);
	}
}

function exists(relativePath) {
	return fs.existsSync(path.join(repoRoot, relativePath));
}

function all(...values) {
	return values.every(Boolean);
}

function parseCatalogCommands() {
	const source = read("src/catalog.rs");
	const commands = [];
	for (const match of source.matchAll(/CommandSpec\s*\{([\s\S]*?)\n\s*\}/g)) {
		const block = match[1];
		const name = block.match(/name:\s*"([^"]+)"/)?.[1];
		if (!name) continue;
		const aliasesBlock = block.match(/aliases:\s*&\[([\s\S]*?)\]/)?.[1] ?? "";
		const aliases = [...aliasesBlock.matchAll(/"([^"]+)"/g)].map(
			(entry) => entry[1],
		);
		const visibility = block.match(
			/visibility:\s*CommandVisibility::([A-Za-z]+)/,
		)?.[1];
		const route = block.match(/route:\s*CommandRoute::([A-Za-z]+)/)?.[1];
		const description =
			block
				.match(/description:\s*"([\s\S]*?)",\n\s*aliases:/)?.[1]
				?.replace(/"\s*\n\s*"/g, "")
				?.replace(/\s+/g, " ")
				?.trim() ?? "";
		if (visibility === "Public") {
			commands.push({ name, aliases, route, description });
		}
	}
	return commands.sort((a, b) => a.name.localeCompare(b.name));
}

function parseActionSet(relativePath) {
	if (!exists(relativePath)) return [];
	const source = read(relativePath);
	const actions = new Set();
	for (const match of source.matchAll(/"([a-z][a-z0-9-]*)"\s*(?:\||=>)/g)) {
		const value = match[1];
		if (
			!["json", "root", "help", "name", "path", "type", "mode"].includes(value)
		)
			actions.add(value);
	}
	for (const match of source.matchAll(/"([a-z][a-z0-9-]*)"\s*\|/g))
		actions.add(match[1]);
	return [...actions]
		.filter((value) => !value.startsWith("-"))
		.sort((a, b) => a.localeCompare(b));
}

function parseWorkflowPlatforms(workflowText) {
	const found = new Set();
	if (/ubuntu|linux/i.test(workflowText)) found.add("linux");
	if (/macos|darwin/i.test(workflowText)) found.add("darwin");
	if (/windows|win32/i.test(workflowText)) found.add("win32");
	return [...found].sort();
}

function targetPlatforms(targets) {
	return [...new Set(targets.map((target) => target.platform))].sort();
}

function requiredOsPresent(values) {
	const set = new Set(values);
	return ["darwin", "linux", "win32"].every((platform) => set.has(platform));
}

function buildSmokeCommands(commands) {
	const safeCommands = [
		{
			args: ["help"],
			expects: "text",
			reason: "small public command router and help",
		},
		{ args: ["version"], expects: "text", reason: "version metadata path" },
		{ args: ["up", "--help"], expects: "text", reason: "convergent onboarding help" },
		{ args: ["start", "--help"], expects: "text", reason: "start lifecycle help" },
		{ args: ["stop", "--help"], expects: "text", reason: "stop lifecycle help" },
		{ args: ["restart", "--help"], expects: "text", reason: "restart lifecycle help" },
		{ args: ["install", "--help"], expects: "text", reason: "server install help" },
		{
			args: ["status", "--json"],
			expects: "jsonOrNonzero",
			reason: "read-only aggregate runtime/startup status",
		},
		{
			args: ["advanced", "doctor", "--json"],
			expects: "json",
			reason: "host and source readiness without runtime start",
		},
		{
			args: ["advanced", "server", "list", "--json"],
			expects: "json",
			reason: "server inventory contract",
		},
		{
			args: ["advanced", "server", "capabilities", "--json"],
			expects: "json",
			reason: "launch/capability metadata contract",
		},
		{
			args: ["advanced", "server", "sources", "--json"],
			expects: "json",
			reason: "MCP settings source discovery contract",
		},
		{
			args: ["advanced", "client", "list", "--json"],
			expects: "json",
			reason: "client catalog visibility contract",
		},
		{
			args: ["advanced", "client", "plan", "--json"],
			expects: "json",
			reason: "client routing plan contract",
		},
		{
			args: ["advanced", "dev", "profile", "--json"],
			expects: "json",
			reason: "maintainer runtime profile read contract",
		},
		{
			args: ["advanced", "dev", "projects", "--json"],
			expects: "json",
			reason: "maintainer project registry read contract",
		},
		{
			args: ["advanced", "dev", "lab", "report", "--json"],
			expects: "json",
			reason: "maintainer evidence corpus contract",
		},
		{
			args: ["advanced", "runtime", "cleanup", "status", "--json"],
			expects: "json",
			reason: "non-destructive cleanup plan",
		},
		{
			args: ["advanced", "dev", "release", "--help"],
			expects: "text",
			reason: "maintainer release help path",
		},
		{
			args: ["advanced", "autostart", "--help"],
			expects: "text",
			reason: "platform startup help path",
		},
		{
			args: ["uninstall", "--help"],
			expects: "text",
			reason: "safe integration removal contract",
		},
	];
	const catalogNames = new Set(commands.map((command) => command.name));
	return safeCommands.map((item) => ({
		...item,
		command: item.args.join(" "),
		coveredTopLevel: catalogNames.has(item.args[0]),
	}));
}

function buildReport() {
	const releaseTargets = readJson("release-targets.json");
	const cliPackage = readJson("packages/npm/cli/package.json");
	const manifest = readJson("release-manifest.json");
	const ciWorkflow = exists(".github/workflows/ci.yml")
		? read(".github/workflows/ci.yml")
		: "";
	const platformWorkflowPath = ".github/workflows/platform-proof.yml";
	const platformWorkflow = exists(platformWorkflowPath)
		? read(platformWorkflowPath)
		: "";
	const commands = parseCatalogCommands();
	const smokeCommands = buildSmokeCommands(commands);
	const publishedTargets = releaseTargets.targets ?? [];
	const plannedTargets = releaseTargets.plannedTargets ?? [];
	const publishedPlatforms = targetPlatforms(publishedTargets);
	const workflowPlatforms = parseWorkflowPlatforms(platformWorkflow);
	const optionalDeps = Object.keys(
		cliPackage.optionalDependencies ?? {},
	).sort();
	const requiredOptionalDeps = publishedTargets
		.map((target) => target.npmPackage ?? target.packageName)
		.sort();
	const missingOptionalDeps = requiredOptionalDeps.filter(
		(name) => !optionalDeps.includes(name),
	);
	const topLevelCommandGaps = commands
		.filter(
			(command) =>
				!smokeCommands.some((smoke) => smoke.args[0] === command.name),
		)
		.map((command) => command.name);

	const checks = [
		{
			id: "release-targets-cover-three-desktop-os",
			status: requiredOsPresent(publishedPlatforms) ? "pass" : "fail",
			detail: `published platforms: ${publishedPlatforms.join(", ") || "none"}`,
		},
		{
			id: "platform-workflow-covers-three-desktop-os",
			status: requiredOsPresent(workflowPlatforms) ? "pass" : "fail",
			detail: `workflow platforms: ${workflowPlatforms.join(", ") || "none"}`,
		},
		{
			id: "platform-workflow-runs-node-rust-and-binary-smoke",
			status: all(
				platformWorkflow.includes("npm run check"),
				platformWorkflow.includes("npm run check:rust"),
				platformWorkflow.includes("npm run platform:binary-smoke"),
				platformWorkflow.includes("cargo build --release"),
				platformWorkflow.includes("dtolnay/rust-toolchain"),
			)
				? "pass"
				: "fail",
			detail:
				"manual platform proof must cover Node contracts, Rust build/test, and native binary smoke commands.",
		},
		{
			id: "optional-npm-packages-match-published-targets",
			status: missingOptionalDeps.length === 0 ? "pass" : "fail",
			detail:
				missingOptionalDeps.length === 0
					? "all published target packages are declared optional deps"
					: `missing: ${missingOptionalDeps.join(", ")}`,
		},
		{
			id: "platform-proof-artifacts-ship-in-source-bundle",
			status: all(
				manifest.includePaths.includes("scripts/platform-proof.mjs"),
				manifest.includePaths.includes("scripts/platform-binary-smoke.mjs"),
				manifest.includePaths.includes("reports/platform-proof.md"),
				manifest.includePaths.includes("reports/platform-proof.json"),
			)
				? "pass"
				: "fail",
			detail:
				"report scripts and generated reports must be reviewable in the source bundle.",
		},
		{
			id: "all-public-commands-inventoried",
			status:
				commands.every((command) => command.route) && commands.length === 10
					? "pass"
					: "fail",
			detail: `${commands.length} public command groups parsed from src/catalog.rs`,
		},
		{
			id: "safe-binary-smoke-covers-operational-groups",
			status: smokeCommands.every((item) => item.coveredTopLevel)
				? "pass"
				: "fail",
			detail: `${smokeCommands.length} non-destructive native smoke commands defined`,
		},
		{
			id: "console-ui-decision-is-lightweight",
			status: "pass",
			detail:
				"Tauri is desktop-webview scope; console UX should stay CLI/dashboard now, with a future Ratatui TUI only after Rust platform proof is green.",
		},
	];
	const failCount = checks.filter((check) => check.status === "fail").length;
	const warnCount = checks.filter((check) => check.status === "warn").length;

	return {
		schema: "mcpace.platformProof.v1",
		generatedAt: new Date().toISOString(),
		root: ".",
		rootName: deriveProjectName(),
		evidenceKind: "static-plan-contract",
		executionEvidence: false,
		scope:
			"Validates platform declarations, workflow shape, command inventory, and smoke coverage; it does not claim that the remote OS matrix executed.",
		overall: failCount > 0 ? "fail" : warnCount > 0 ? "warn" : "pass",
		summary: {
			pass: checks.filter((check) => check.status === "pass").length,
			warn: warnCount,
			fail: failCount,
			publishedTargetCount: publishedTargets.length,
			plannedTargetCount: plannedTargets.length,
			publicCommandCount: commands.length,
			smokeCommandCount: smokeCommands.length,
		},
		uiDecision: {
			requestedTerm: "Taori/Tauri for console view",
			decision:
				"Do not add Tauri as a console dependency now. Tauri is appropriate for a packaged desktop app around the existing web dashboard; a real terminal UI should be Ratatui/crossterm later. Current safe step is a platform-proofed CLI/dashboard with native binary smoke tests.",
			why: [
				"Tauri adds desktop packaging and webview/runtime integration rather than improving terminal rendering.",
				"Ratatui is the right family for a terminal TUI, but adding it should wait until Rust CI is green on Linux/macOS/Windows.",
				"The dashboard already owns the operator model; the first console work should reuse that model rather than fork logic.",
			],
			nextTuiGate:
				"After platform-proof is green, add a Ratatui-based `mcpace tui` as a thin terminal view over userReadiness/operatorPlan instead of duplicating policy logic.",
		},
		platforms: {
			published: publishedPlatforms,
			workflow: workflowPlatforms,
			nodeWorkflow: parseWorkflowPlatforms(ciWorkflow),
		},
		targets: {
			published: publishedTargets.map((target) => ({
				key: target.key,
				platform: target.platform,
				arch: target.arch,
				rustTarget: target.rustTarget,
				runner: target.runner,
				npmPackage: target.npmPackage ?? target.packageName,
				binaryName: target.binaryName,
			})),
			planned: plannedTargets.map((target) => ({
				key: target.key,
				platform: target.platform,
				arch: target.arch,
				rustTarget: target.rustTarget,
				reason: target.reason,
			})),
		},
		commands,
		subcommands: {
			server: parseActionSet("src/server/args.rs"),
			client: parseActionSet("src/client/args.rs"),
			lab: parseActionSet("src/lab/args.rs"),
			serve: parseActionSet("src/serve.rs"),
			hub: parseActionSet("src/hub.rs"),
		},
		smokeCommands,
		topLevelCommandGaps,
		checks,
		correctPlatformFlow: [
			"Static proof on any host: npm run check:platform && npm run check.",
			"Manual GitHub proof: run .github/workflows/platform-proof.yml with full=true.",
			"For each Linux/macOS/Windows host: npm ci --omit=optional, npm run check, install Rust 1.95.0, cargo fmt, clippy, test, build --release.",
			"Run npm run platform:binary-smoke -- --binary target/release/mcpace[.exe] on each host.",
			"Only after all three OS families pass, add a real Ratatui-based mcpace tui command or a Tauri desktop shell.",
		],
	};
}

function renderMarkdown(report) {
	const lines = [];
	lines.push("# MCPace platform proof");
	lines.push("");
	lines.push(
		"Generated by `npm run platform`. This report defines what must be proven on Linux, macOS, and Windows before calling the project cross-platform ready.",
	);
	lines.push("");
	lines.push(`- Static contract status: **${report.overall}**`);
	lines.push(`- Evidence kind: **${report.evidenceKind}**`);
	lines.push(`- Remote execution evidence: **${report.executionEvidence}**`);
	lines.push(`- Scope: ${report.scope}`);
	lines.push(
		"- Caveat: a passing static contract does not prove that the Linux/macOS/Windows matrix executed successfully.",
	);
	lines.push(`- Published targets: ${report.summary.publishedTargetCount}`);
	lines.push(`- Public command groups: ${report.summary.publicCommandCount}`);
	lines.push(`- Native smoke commands: ${report.summary.smokeCommandCount}`);
	lines.push("");
	lines.push("## Console UI decision");
	lines.push("");
	lines.push(`- Decision: ${report.uiDecision.decision}`);
	for (const reason of report.uiDecision.why) lines.push(`- ${reason}`);
	lines.push(`- Next TUI gate: ${report.uiDecision.nextTuiGate}`);
	lines.push("");
	lines.push("## Platform targets");
	lines.push("");
	lines.push(
		"| Target | Platform | Arch | Rust target | Runner | npm package | Binary |",
	);
	lines.push("|---|---|---|---|---|---|---|");
	for (const target of report.targets.published) {
		lines.push(
			`| ${target.key} | ${target.platform} | ${target.arch} | ${target.rustTarget} | ${target.runner} | ${target.npmPackage} | ${target.binaryName} |`,
		);
	}
	lines.push("");
	lines.push("## Non-destructive native smoke commands");
	lines.push("");
	lines.push("| Command | Expectation | Why |");
	lines.push("|---|---|---|");
	for (const smoke of report.smokeCommands) {
		lines.push(`| \`${smoke.command}\` | ${smoke.expects} | ${smoke.reason} |`);
	}
	lines.push("");
	lines.push("## Checks");
	lines.push("");
	lines.push("| Status | Check | Detail |");
	lines.push("|---|---|---|");
	for (const check of report.checks) {
		lines.push(
			`| ${check.status} | ${check.id} | ${check.detail.replace(/\|/g, "\\|")} |`,
		);
	}
	lines.push("");
	lines.push("## Correct platform verification flow");
	lines.push("");
	for (let index = 0; index < report.correctPlatformFlow.length; index += 1) {
		lines.push(`${index + 1}. ${report.correctPlatformFlow[index]}`);
	}
	lines.push("");
	return `${lines.join("\n")}\n`;
}

const report = buildReport();
const markdown = renderMarkdown(report);
const freshnessFindings =
	check || args.has("--ci")
		? generatedReportFreshness({
				repoRoot,
				jsonPath: "reports/platform-proof.json",
				expectedReport: report,
				markdownPath: "reports/platform-proof.md",
				expectedMarkdown: markdown,
			})
		: [];
if (write) {
	fs.mkdirSync(path.join(repoRoot, "reports"), { recursive: true });
	writeFileAtomicSync(
		path.join(repoRoot, "reports/platform-proof.json"),
		JSON.stringify(report, null, 2) + "\n",
		{ mode: 0o644 },
	);
	writeFileAtomicSync(
		path.join(repoRoot, "reports/platform-proof.md"),
		markdown,
		{ mode: 0o644 },
	);
}

if (jsonOnly) {
	process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} else if (check || args.has("--ci")) {
	process.stdout.write(
		`MCPace platform proof: ${report.overall} (${report.summary.pass} pass, ${report.summary.warn} warn, ${report.summary.fail} fail)\n`,
	);
} else if (!write) {
	process.stdout.write(markdown);
}

if ((check || args.has("--ci")) && freshnessFindings.length > 0) {
	process.stderr.write(
		`MCPace platform proof reports are stale:\n- ${freshnessFindings.join("\n- ")}\n`,
	);
}
if (
	(check || args.has("--ci")) &&
	(report.summary.fail > 0 || freshnessFindings.length > 0)
) {
	if (report.summary.fail > 0) {
		process.stderr.write(
			`MCPace platform proof failed: ${report.summary.fail} failed check(s).\n`,
		);
	}
	process.exit(1);
}
