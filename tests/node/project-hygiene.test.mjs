import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import vm from "node:vm";
import {
	cargoCommand,
	readPinnedToolchain,
	runCargo,
} from "../../scripts/cargo-task.mjs";
import {
	readRootPackageJson,
	repoRoot,
} from "../../scripts/lib/project-metadata.mjs";

const textExtensions = new Set([".md", ".yml", ".yaml", ".json"]);

const dashboardJsFiles = Object.freeze([
	"src/dashboard/frontend/app.js",
	"src/dashboard/frontend/app.runtime.js",
	"src/dashboard/frontend/app.model.js",
	"src/dashboard/frontend/app.render.js",
	"src/dashboard/frontend/app.render.details.js",
	"src/dashboard/frontend/app.actions.js",
	"src/dashboard/frontend/app.boot.js",
]);

function readDashboardJs() {
	return dashboardJsFiles
		.map((file) => fs.readFileSync(path.join(repoRoot, file), "utf8"))
		.join("\n");
}

function readJsonFixture(relativePath) {
	const source = fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
	try {
		return JSON.parse(source);
	} catch (error) {
		assert.fail(
			`${relativePath} is not valid JSON: ${error?.message || error}`,
		);
	}
}

function walkFiles(root, predicate = () => true) {
	const files = [];
	const stack = [root];
	while (stack.length > 0) {
		const current = stack.pop();
		if (!fs.existsSync(current)) continue;
		for (const entry of fs
			.readdirSync(current, { withFileTypes: true })
			.sort((left, right) => left.name.localeCompare(right.name))) {
			const full = path.join(current, entry.name);
			if (entry.isDirectory()) stack.push(full);
			else if (entry.isFile() && predicate(full)) files.push(full);
		}
	}
	return files.sort();
}

function normalize(relativePath) {
	return relativePath.split(path.sep).join("/");
}

function referencedScriptsFromText(text) {
	const result = [];
	for (const match of text.matchAll(
		/\bnode\s+(scripts\/[A-Za-z0-9._/-]+\.mjs)\b/g,
	))
		result.push(match[1]);
	return result;
}

function referencedNpmScriptsFromText(text) {
	const result = [];
	for (const match of text.matchAll(/\bnpm\s+run\s+([A-Za-z0-9:_-]+)/g))
		result.push(match[1]);
	return result;
}

function manifestIncludesPath(manifest, relativePath) {
	const normalized = normalize(relativePath);
	return manifest.includePaths.some(
		(entry) => normalized === entry || normalized.startsWith(`${entry}/`),
	);
}

test("project automation references only existing local npm and Node scripts", () => {
	const packageJson = readRootPackageJson();
	const scripts = packageJson.scripts || {};
	const missing = [];

	for (const [name, command] of Object.entries(scripts)) {
		for (const referenced of referencedScriptsFromText(command)) {
			if (!fs.existsSync(path.join(repoRoot, referenced)))
				missing.push(`package.json script ${name} -> ${referenced}`);
		}
	}

	const docs = walkFiles(repoRoot, (file) => {
		const relative = normalize(path.relative(repoRoot, file));
		if (relative.startsWith("node_modules/") || relative.startsWith(".git/"))
			return false;
		if (
			!relative.startsWith(".github/") &&
			!relative.startsWith("docs/") &&
			!relative.startsWith("reports/") &&
			relative !== "README.md" &&
			relative !== "CHANGELOG.md" &&
			relative !== "SECURITY.md"
		)
			return false;
		return textExtensions.has(path.extname(file));
	});

	for (const file of docs) {
		const relative = normalize(path.relative(repoRoot, file));
		const text = fs.readFileSync(file, "utf8");
		for (const referenced of referencedScriptsFromText(text)) {
			if (!fs.existsSync(path.join(repoRoot, referenced)))
				missing.push(`${relative} -> ${referenced}`);
		}
		for (const scriptName of referencedNpmScriptsFromText(text)) {
			if (!scripts[scriptName])
				missing.push(`${relative} -> npm run ${scriptName}`);
		}
	}

	assert.deepEqual(missing, []);
});

test("release manifest includes Node script entrypoints referenced by npm scripts", () => {
	const packageJson = readRootPackageJson();
	const manifest = readJsonFixture("release-manifest.json");
	const missing = [];
	for (const [scriptName, command] of Object.entries(
		packageJson.scripts || {},
	)) {
		for (const referenced of referencedScriptsFromText(command)) {
			if (!manifestIncludesPath(manifest, referenced))
				missing.push(`${scriptName} -> ${referenced}`);
		}
	}
	assert.deepEqual(missing, []);
});

test("npm Rust scripts use the Cargo preflight wrapper instead of raw cargo", () => {
	const scripts = readRootPackageJson().scripts || {};
	assert.equal(scripts.build, "node scripts/cargo-task.mjs build --release");
	assert.equal(
		scripts["test:rust"],
		"node scripts/cargo-task.mjs test -- --test-threads=1",
	);
	assert.equal(scripts["fmt:check"], "node scripts/cargo-task.mjs fmt --check");
	assert.equal(
		scripts.clippy,
		"node scripts/cargo-task.mjs clippy --all-targets -- -D warnings",
	);
	assert.match(scripts["check:rust"], /fmt:check/);
	assert.match(scripts["check:rust"], /clippy/);
	assert.match(scripts["check:rust"], /test:rust/);
});

test("Cargo preflight wrapper is cross-platform and reports missing toolchain clearly", () => {
	assert.equal(cargoCommand("win32"), "cargo.exe");
	assert.equal(cargoCommand("linux"), "cargo");
	assert.equal(readPinnedToolchain(), "1.95.0");
	assert.throws(
		() => runCargo(["--version"], { env: { PATH: "" }, stdio: "ignore" }),
		(error) =>
			error.code === "MCPACE_CARGO_NOT_FOUND" &&
			/rust-toolchain|1\.95\.0|rustup/i.test(error.message),
	);
});

function modulePathsFor(declaringFile, moduleName) {
	const sourceRoot = path.join(repoRoot, "src");
	const relative = path.relative(sourceRoot, declaringFile);
	const parent = path.dirname(declaringFile);
	const candidates = [];
	if (path.basename(declaringFile) === "mod.rs") {
		candidates.push(path.join(parent, `${moduleName}.rs`));
		candidates.push(path.join(parent, moduleName, "mod.rs"));
	} else {
		const siblingDirectory = path.join(
			parent,
			path.basename(declaringFile, ".rs"),
		);
		candidates.push(path.join(siblingDirectory, `${moduleName}.rs`));
		candidates.push(path.join(siblingDirectory, moduleName, "mod.rs"));
		candidates.push(path.join(parent, `${moduleName}.rs`));
		candidates.push(path.join(parent, moduleName, "mod.rs"));
	}
	return candidates.filter(
		(candidate) =>
			normalize(path.relative(sourceRoot, candidate)) !== normalize(relative),
	);
}

function reachableRustFiles() {
	const moduleDeclaration =
		/^\s*(?:#\[[^\n]+\]\s*)*(?:(?:pub(?:\([^)]*\))?)\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;/gm;
	const cargoBinRoots = fs.existsSync(path.join(repoRoot, "src", "bin"))
		? walkFiles(
				path.join(repoRoot, "src", "bin"),
				(file) => path.extname(file) === ".rs",
			)
		: [];
	const stack = [
		path.join(repoRoot, "src", "lib.rs"),
		path.join(repoRoot, "src", "main.rs"),
		...cargoBinRoots,
	];
	const seen = new Set();

	while (stack.length > 0) {
		const file = stack.pop();
		if (seen.has(file) || !fs.existsSync(file)) continue;
		seen.add(file);
		const text = fs.readFileSync(file, "utf8");
		for (const match of text.matchAll(moduleDeclaration)) {
			for (const candidate of modulePathsFor(file, match[1])) {
				if (fs.existsSync(candidate)) {
					stack.push(candidate);
					break;
				}
			}
		}
	}

	return seen;
}

test("Rust source tree has no orphan .rs files outside the crate module graph", () => {
	const srcRoot = path.join(repoRoot, "src");
	const allRustFiles = walkFiles(
		srcRoot,
		(file) => path.extname(file) === ".rs",
	);
	const reachable = reachableRustFiles();
	const orphaned = allRustFiles
		.filter((file) => !reachable.has(file))
		.map((file) => normalize(path.relative(repoRoot, file)));
	assert.deepEqual(orphaned, []);
});

test("public command aliases are unique and cannot silently shadow each other", () => {
	const catalog = fs.readFileSync(
		path.join(repoRoot, "src", "catalog.rs"),
		"utf8",
	);
	const aliases = new Map();
	const duplicates = [];
	for (const blockMatch of catalog.matchAll(
		/CommandSpec\s*\{([\s\S]*?)\n\s*\}/g,
	)) {
		const block = blockMatch[1];
		const name = block.match(/name:\s*"([^"]+)"/)?.[1];
		const aliasBlock = block.match(/aliases:\s*&\[([\s\S]*?)\]/)?.[1] || "";
		if (!name) continue;
		for (const alias of [...aliasBlock.matchAll(/"([^"]+)"/g)].map(
			(match) => match[1],
		)) {
			const previous = aliases.get(alias);
			if (previous && previous !== name)
				duplicates.push(`${alias}: ${previous} <-> ${name}`);
			aliases.set(alias, name);
		}
	}
	assert.deepEqual(duplicates, []);
});

test("source bundle has no case-insensitive path collisions", () => {
	const files = walkFiles(repoRoot, (file) => {
		const relative = normalize(path.relative(repoRoot, file));
		return (
			!relative.startsWith(".git/") &&
			!relative.startsWith("node_modules/") &&
			!relative.startsWith("target/") &&
			!relative.startsWith("dist/")
		);
	});
	const seen = new Map();
	const collisions = [];
	for (const file of files) {
		const relative = normalize(path.relative(repoRoot, file));
		const folded = relative.toLocaleLowerCase("en-US");
		const previous = seen.get(folded);
		if (previous && previous !== relative)
			collisions.push(`${previous} <-> ${relative}`);
		else seen.set(folded, relative);
	}
	assert.deepEqual(collisions, []);
});

test("shared Rust path, platform, and CLI text helpers are centralized instead of reimplemented per command", () => {
	const rustFiles = walkFiles(
		path.join(repoRoot, "src"),
		(file) => path.extname(file) === ".rs",
	);
	const localDefinitions = [];
	const expectedOwners = {
		canonicalize_or_original: "src/runtimepaths.rs",
		user_home_dir: "src/runtimepaths.rs",
		unix_time_ms: "src/runtimepaths.rs",
		unix_time_ms_checked: "src/runtimepaths.rs",
		strip_windows_extended_path_prefix: "src/runtimepaths.rs",
		yes_no: "src/text_utils.rs",
		normalize_flag: "src/text_utils.rs",
		ascii_alnum_dash_underscore: "src/text_utils.rs",
		trimmed_non_empty_owned: "src/text_utils.rs",
		sorted_unique: "src/text_utils.rs",
		join_or_none: "src/text_utils.rs",
		env_usize: "src/resources.rs",
		env_u64: "src/resources.rs",
		env_bool: "src/resources.rs",
		method_is_notification: "src/mcp_protocol.rs",
		json_string_or_null: "src/json_helpers.rs",
		optional_number: "src/json_helpers.rs",
		current_platform_alias: "src/platform_utils.rs",
		normalize_platform: "src/platform_utils.rs",
		supports_current_platform: "src/platform_utils.rs",
	};
	for (const file of rustFiles) {
		const relative = normalize(path.relative(repoRoot, file));
		const text = fs.readFileSync(file, "utf8");
		for (const helper of Object.keys(expectedOwners)) {
			const pattern = new RegExp(
				String.raw`\bfn\s+${helper}(?:<[^()]+>)?\s*\(`,
				"g",
			);
			for (const _match of text.matchAll(pattern)) {
				localDefinitions.push(`${relative} -> ${helper}`);
			}
		}
	}

	assert.deepEqual(
		localDefinitions.sort(),
		Object.entries(expectedOwners)
			.map(([helper, owner]) => `${owner} -> ${helper}`)
			.sort(),
	);
});

test("serve resource args and tool-shaping helpers stay centralized", () => {
	const rustFiles = walkFiles(
		path.join(repoRoot, "src"),
		(file) => path.extname(file) === ".rs",
	);
	const definitions = [];
	const forbiddenDefinitions = [];
	const checkedAboveExpectations = [];
	for (const file of rustFiles) {
		const relative = normalize(path.relative(repoRoot, file));
		const source = fs.readFileSync(file, "utf8");
		for (const match of source.matchAll(
			/\bfn\s+(append_serve_resource_args|shape_tool_for_client)(?:<[^()]+>)?\s*\(/g,
		)) {
			definitions.push(`${relative} -> ${match[1]}`);
		}
		for (const match of source.matchAll(
			/\bfn\s+(shape_http_tool_for_client)\s*\(/g,
		)) {
			forbiddenDefinitions.push(`${relative} -> ${match[1]}`);
		}
		if (
			source.includes('expect("checked above")') ||
			source.includes('expect("validate_request_envelope checked method")')
		) {
			checkedAboveExpectations.push(relative);
		}
	}
	assert.deepEqual(definitions.sort(), [
		"src/adapter/discovery.rs -> shape_tool_for_client",
		"src/resources.rs -> append_serve_resource_args",
	]);
	assert.deepEqual(forbiddenDefinitions, []);
	assert.deepEqual(checkedAboveExpectations, []);
});

test("MCP server config shape selector is centralized", () => {
	const rustFiles = walkFiles(
		path.join(repoRoot, "src"),
		(file) => path.extname(file) === ".rs",
	);
	const helperDefinitions = [];
	const retiredDefinitions = [];
	for (const file of rustFiles) {
		const relative = normalize(path.relative(repoRoot, file));
		const text = fs.readFileSync(file, "utf8");
		for (const match of text.matchAll(
			/\bfn\s+(mcp_servers_object(?:_with_key)?)(?:<[^()]+>)?\s*\(/g,
		)) {
			helperDefinitions.push(`${relative} -> ${match[1]}`);
		}
		for (const match of text.matchAll(/\bfn\s+(source_servers_object)\s*\(/g)) {
			retiredDefinitions.push(`${relative} -> ${match[1]}`);
		}
	}

	assert.deepEqual(helperDefinitions.sort(), [
		"src/json_helpers.rs -> mcp_servers_object",
		"src/json_helpers.rs -> mcp_servers_object_with_key",
	]);
	assert.deepEqual(retiredDefinitions, []);
});

test("shared upstream batch-call item schema is centralized", () => {
	const rustFiles = walkFiles(
		path.join(repoRoot, "src"),
		(file) => path.extname(file) === ".rs",
	);
	const schemaDefinitions = [];
	const retiredDefinitions = [];
	const callers = [];
	for (const file of rustFiles) {
		const relative = normalize(path.relative(repoRoot, file));
		const text = fs.readFileSync(file, "utf8");
		for (const match of text.matchAll(
			/\bfn\s+(upstream_batch_call_item_schema)(?:<[^()]+>)?\s*\(/g,
		)) {
			schemaDefinitions.push(`${relative} -> ${match[1]}`);
		}
		for (const match of text.matchAll(
			/\bfn\s+(http_upstream_batch_call_item_schema)\s*\(/g,
		)) {
			retiredDefinitions.push(`${relative} -> ${match[1]}`);
		}
		if (text.includes("tool_schemas::upstream_batch_call_item_schema()"))
			callers.push(relative);
	}

	assert.deepEqual(schemaDefinitions, [
		"src/tool_schemas.rs -> upstream_batch_call_item_schema",
	]);
	assert.deepEqual(retiredDefinitions, []);
	assert.deepEqual(callers.sort(), [
		"src/dashboard/http_tools.rs",
		"src/mcp_server/tool_surface.rs",
	]);
});

test("source-type alias normalization is centralized for public and runtime views", () => {
	const rustFiles = walkFiles(
		path.join(repoRoot, "src"),
		(file) => path.extname(file) === ".rs",
	);
	const definitions = [];
	const publicCallers = [];
	const runtimeCallers = [];
	const retiredLocalNormalizers = [];
	for (const file of rustFiles) {
		const relative = normalize(path.relative(repoRoot, file));
		const source = fs.readFileSync(file, "utf8");
		for (const match of source.matchAll(
			/\bfn\s+(normalize_public_source_type|normalize_runtime_source_type|infer_public_source_type|infer_runtime_source_type)(?:<[^()]+>)?\s*\(/g,
		)) {
			definitions.push(`${relative} -> ${match[1]}`);
		}
		if (source.includes("source_type::infer_public_source_type("))
			publicCallers.push(relative);
		if (source.includes("source_type::infer_runtime_source_type("))
			runtimeCallers.push(relative);
		if (
			relative !== "src/source_type.rs" &&
			/\bfn\s+normalize_(?:runtime_)?source_type\s*\(/.test(source)
		) {
			retiredLocalNormalizers.push(relative);
		}
	}
	assert.deepEqual(
		definitions.sort(),
		[
			"src/source_type.rs -> infer_public_source_type",
			"src/source_type.rs -> infer_runtime_source_type",
			"src/source_type.rs -> normalize_public_source_type",
			"src/source_type.rs -> normalize_runtime_source_type",
		].sort(),
	);
	assert.deepEqual(publicCallers.sort(), [
		"src/mcp_sources/import.rs",
		"src/mcp_sources/write_helpers.rs",
		"src/server/loader.rs",
		"src/setup.rs",
	]);
	assert.deepEqual(runtimeCallers.sort(), [
		"src/doctor.rs",
		"src/upstream/source_type.rs",
	]);
	assert.deepEqual(retiredLocalNormalizers, []);
});

test("runtime source-type aliases do not treat legacy SSE as modern HTTP", () => {
	const sourceType = fs.readFileSync(
		path.join(repoRoot, "src", "source_type.rs"),
		"utf8",
	);
	assert.match(sourceType, /"streamable-http"\s*=>\s*"http"\.to_string\(\)/);
	assert.match(sourceType, /"sse-legacy"\s*=>\s*"legacy-sse"\.to_string\(\)/);
	assert.match(
		sourceType,
		/"remote-sse"[\s\S]*"sse"[\s\S]*=>\s*\{\s*"sse-legacy"\.to_string\(\)\s*\}/,
	);
});

test("user autostart launches MCPace Agent through the native hidden sidecar instead of VBS or wscript", () => {
	const service = fs.readFileSync(
		path.join(repoRoot, "src", "service.rs"),
		"utf8",
	);
	const serviceConfig = fs.readFileSync(
		path.join(repoRoot, "src", "service", "config.rs"),
		"utf8",
	);
	const serviceSurface = `${service}
${serviceConfig}`;
	assert.match(service, /pub\(crate\) const APP_NAME: &str = "MCPace Agent"/);
	assert.match(serviceConfig, /"agent"\.to_string\(\)/);
	assert.match(serviceConfig, /"--autostart"\.to_string\(\)/);
	assert.match(serviceConfig, /mcpace-agent-launcher\.exe/);
	assert.match(serviceSurface, /native_background/);
	assert.doesNotMatch(serviceSurface, /Command::new\(\s*"wscript(?:\.exe)?"/);
	assert.doesNotMatch(serviceSurface, /mcpace-autostart\.vbs"\)/);
	assert.doesNotMatch(serviceSurface, /windows_command_line_from_strs/);
});

test("release target manifest stays aligned with the npm launcher package", () => {
	const releaseTargets = readJsonFixture("release-targets.json");
	const cliPackage = readJsonFixture("packages/npm/cli/package.json");
	const enabledPackageNames = releaseTargets.targets
		.filter((target) => target.publishEnabled !== false)
		.map((target) => target.packageName)
		.sort();
	const optionalDependencyNames = Object.keys(
		cliPackage.optionalDependencies || {},
	).sort();
	assert.deepEqual(optionalDependencyNames, enabledPackageNames);
	for (const [name, version] of Object.entries(
		cliPackage.optionalDependencies || {},
	)) {
		assert.equal(
			version,
			cliPackage.version,
			`${name} optional dependency version must match @mcpace/cli`,
		);
	}
});

function yamlListValuesAfterKey(text, key) {
	const values = [];
	const lines = text.split(/\r?\n/);
	for (let index = 0; index < lines.length; index += 1) {
		const line = lines[index];
		const inline = line.match(new RegExp(`^\\s*${key}:\\s*\\[(.*)\\]\\s*$`));
		if (inline) {
			values.push(
				...inline[1]
					.split(",")
					.map((value) => value.trim())
					.filter(Boolean),
			);
			continue;
		}
		const block = line.match(new RegExp(`^(\\s*)${key}:\\s*$`));
		if (!block) continue;
		const keyIndent = block[1].length;
		for (let cursor = index + 1; cursor < lines.length; cursor += 1) {
			const child = lines[cursor];
			if (!child.trim()) continue;
			const childIndent = child.match(/^\s*/)[0].length;
			if (childIndent <= keyIndent) break;
			const item = child.match(/^\s*-\s+(.+?)\s*$/);
			if (item) values.push(item[1]);
		}
	}
	return values.map((value) => value.replace(/^['"]|['"]$/g, ""));
}

test("GitHub metadata references only declared repository labels", () => {
	const labelsText = fs.readFileSync(
		path.join(repoRoot, ".github", "labels.yml"),
		"utf8",
	);
	const declared = new Set(
		[...labelsText.matchAll(/^\s*-\s+name:\s+(.+?)\s*$/gm)].map((match) =>
			match[1].replace(/^['"]|['"]$/g, ""),
		),
	);
	const allowExternal = new Set(["*"]);
	const checkedFiles = [
		".github/dependabot.yml",
		".github/release.yml",
		...walkFiles(path.join(repoRoot, ".github", "ISSUE_TEMPLATE"), (file) =>
			/\.ya?ml$/.test(file),
		).map((file) => path.relative(repoRoot, file)),
	];
	const missing = [];
	for (const relative of checkedFiles) {
		const text = fs.readFileSync(path.join(repoRoot, relative), "utf8");
		for (const label of yamlListValuesAfterKey(text, "labels")) {
			if (!declared.has(label) && !allowExternal.has(label))
				missing.push(`${relative} -> ${label}`);
		}
	}
	assert.deepEqual(missing, []);
});

test("GitHub workflows keep current action majors documented behind immutable SHA pins", () => {
	const workflowText = walkFiles(
		path.join(repoRoot, ".github", "workflows"),
		(file) => /\.ya?ml$/.test(file),
	)
		.map((file) => fs.readFileSync(file, "utf8"))
		.join("\n");
	assert.match(workflowText, /actions\/checkout@[0-9a-f]{40}\s+# v6/);
	assert.match(workflowText, /actions\/setup-node@[0-9a-f]{40}\s+# v6/);
	assert.match(workflowText, /actions\/upload-artifact@[0-9a-f]{40}\s+# v7/);
	assert.match(
		workflowText,
		/# actions\/download-artifact v8\s*\n\s*(?:-\s*)?uses: actions\/download-artifact@[0-9a-f]{40}/,
	);
	assert.match(
		workflowText,
		/dtolnay\/rust-toolchain@[0-9a-f]{40}\s+# 1\.95\.0/,
	);
	assert.equal(
		/uses:\s*[^\s#]+@(?:v\d+|stable|\d+\.\d+\.\d+)/.test(workflowText),
		false,
		"third-party actions must use immutable full commit SHA refs",
	);
	assert.match(workflowText, /npm run check:endgame:enforce/);
	const rustProof = fs.readFileSync(
		path.join(repoRoot, "scripts", "rust-live-proof.mjs"),
		"utf8",
	);
	assert.match(rustProof, /"test", "--locked", "--", "--test-threads=1"/);
	assert.equal(
		/toolchain:\s*1\.95\.0/.test(workflowText),
		false,
		"the pinned rust-toolchain action commit already selects Rust 1.95.0",
	);
});

test("optional external tooling preflight uses scanner-specific safe inputs", () => {
	const configPath = path.join(repoRoot, ".github", "zizmor.yml");
	const config = fs.readFileSync(configPath, "utf8");
	assert.match(config, /unpinned-uses/);
	assert.match(config, /actions\/\*:\s*ref-pin/);
	assert.match(config, /dtolnay\/rust-toolchain:\s*ref-pin/);

	const gitleaksConfig = fs.readFileSync(
		path.join(repoRoot, ".gitleaks.toml"),
		"utf8",
	);
	assert.match(gitleaksConfig, /useDefault\s*=\s*true/);
	assert.match(gitleaksConfig, /win32-\(\?:x64\|arm64\)-msvc/);
	assert.match(gitleaksConfig, /generic-api-key/);

	const preflight = fs.readFileSync(
		path.join(repoRoot, "scripts", "tooling-preflight.mjs"),
		"utf8",
	);
	assert.match(preflight, /zizmor\.yml/);
	assert.match(preflight, /--config/);
	assert.match(preflight, /--color/);
	assert.match(preflight, /workflowFileArgs\(\)/);
	assert.doesNotMatch(
		preflight,
		/args:\s*\['\.github\/workflows'\]/,
		"actionlint must receive workflow files, not a directory",
	);
	assert.match(preflight, /gitleaksArgs\(\)/);
	assert.match(preflight, /prepareGitleaksScanSource/);
	assert.doesNotMatch(preflight, /'--source', '\.'/);
	assert.match(preflight, /\.gitleaks\.toml/);
});

test("source comments and manifests do not point at missing local Node scripts", () => {
	const checkedRoots = [
		"package.json",
		"release-manifest.json",
		"scripts",
		"packages",
		"src",
		"docs",
		"reports",
		"README.md",
		"CHANGELOG.md",
		"SECURITY.md",
	];
	const files = [];
	for (const relative of checkedRoots) {
		const absolute = path.join(repoRoot, relative);
		if (!fs.existsSync(absolute)) continue;
		const stat = fs.statSync(absolute);
		if (stat.isFile()) files.push(absolute);
		else
			files.push(
				...walkFiles(absolute, (file) =>
					[".js", ".mjs", ".json", ".md", ".rs"].includes(path.extname(file)),
				),
			);
	}
	const missing = [];
	for (const file of files) {
		const relativeFile = normalize(path.relative(repoRoot, file));
		const text = fs.readFileSync(file, "utf8");
		for (const match of text.matchAll(/scripts\/[A-Za-z0-9._/-]+\.mjs/g)) {
			const scriptPath = match[0];
			if (!fs.existsSync(path.join(repoRoot, scriptPath)))
				missing.push(`${relativeFile} -> ${scriptPath}`);
		}
	}
	assert.deepEqual(missing, []);
});

test("local load-test binary discovery stays aligned with the npm launcher env contract", () => {
	const loadTest = fs.readFileSync(
		path.join(repoRoot, "scripts", "load-test-local.mjs"),
		"utf8",
	);
	assert.match(loadTest, /MCPACE_BINARY_PATH/);
	assert.match(loadTest, /MCPACE_DEV_BINARY/);
	assert.match(loadTest, /assertRunnableBinary/);
	assert.match(loadTest, /target'\s*,\s*'release'/);
	assert.match(loadTest, /target'\s*,\s*'debug'/);
});

test("retired legacy bridge surfaces stay out of source, docs, and automation", () => {
	const forbidden = [
		"manager.settings.json",
		"runtimeProfile",
		"legacyManagerBridge",
		"legacyScriptAliases",
		"MCPACE_PROJECTED_LEGACY_TOP_LEVEL_CONTROLS",
		"legacy-settings",
		"shape_upstream_structured_content",
	];
	const checkedRoots = [
		"package.json",
		"release-manifest.json",
		"schemas",
		"examples",
		"scripts",
		"packages",
		"src",
		"docs",
		"README.md",
		"SECURITY.md",
	];
	const hits = [];
	for (const relative of checkedRoots) {
		const absolute = path.join(repoRoot, relative);
		if (!fs.existsSync(absolute)) continue;
		const stat = fs.statSync(absolute);
		const files = stat.isFile()
			? [absolute]
			: walkFiles(absolute, (file) =>
					[".js", ".mjs", ".json", ".md", ".rs", ".yml", ".yaml"].includes(
						path.extname(file),
					),
				);
		for (const file of files) {
			const text = fs.readFileSync(file, "utf8");
			const relativeFile = normalize(path.relative(repoRoot, file));
			for (const token of forbidden) {
				if (text.includes(token)) hits.push(`${relativeFile} -> ${token}`);
			}
		}
	}
	assert.deepEqual(hits, []);
});

test("repository text files use LF line endings as declared in .editorconfig/.gitattributes", () => {
	const checkedExtensions = new Set([
		".rs",
		".js",
		".mjs",
		".json",
		".lock",
		".md",
		".toml",
		".yml",
		".yaml",
		".html",
	]);
	const checkedNames = new Set([
		".editorconfig",
		".gitattributes",
		".gitignore",
		"LICENSE",
	]);
	const files = walkFiles(repoRoot, (file) => {
		const relative = normalize(path.relative(repoRoot, file));
		if (
			relative.startsWith(".git/") ||
			relative.startsWith(".omx/") ||
			relative.startsWith(".serena/") ||
			relative.startsWith("node_modules/") ||
			relative.startsWith("target/") ||
			relative.startsWith("dist/")
		)
			return false;
		return (
			checkedExtensions.has(path.extname(file)) ||
			checkedNames.has(path.basename(file))
		);
	});
	const crlfFiles = files
		.filter((file) => fs.readFileSync(file).includes(Buffer.from("\r\n")))
		.map((file) => normalize(path.relative(repoRoot, file)));
	assert.deepEqual(crlfFiles, []);
});

test("optional external tooling is wired without slowing the fast local check", () => {
	const packageJson = readRootPackageJson();
	const scripts = packageJson.scripts || {};
	assert.equal(scripts["check:package"], "publint packages/npm/cli");
	assert.equal(scripts["check:ci"], "node scripts/check-ci.mjs");
	assert.match(
		fs.readFileSync(path.join(repoRoot, "scripts/check-ci.mjs"), "utf8"),
		/["']check:package["']/,
	);
	assert.equal(scripts["ci"], "npm run check:ci");
	assert.equal(
		scripts["check:external-tools"],
		"node scripts/tooling-preflight.mjs",
	);
	assert.equal(
		scripts.check.includes("check:package"),
		false,
		"fast npm run check should stay dependency-light",
	);
	assert.ok(
		packageJson.devDependencies?.publint,
		"publint should be locked as a root devDependency for package-boundary checks",
	);

	const manifest = readJsonFixture("release-manifest.json");
	assert.ok(
		manifest.includePaths.includes("package-lock.json"),
		"source bundle should include the npm lockfile for tooling reproducibility",
	);
	assert.ok(
		manifest.includePaths.includes("scripts/tooling-preflight.mjs"),
		"source bundle should include optional external tooling preflight",
	);
});

test("security workflow pull-request jobs are reachable only through declared triggers", () => {
	const securityWorkflow = fs.readFileSync(
		path.join(repoRoot, ".github", "workflows", "security.yml"),
		"utf8",
	);
	assert.match(securityWorkflow, /^ {2}pull_request:\s*$/m);
	assert.match(securityWorkflow, /github\.event_name == 'pull_request'/);
	assert.match(
		securityWorkflow,
		/github\.ref_name == github\.event\.repository\.default_branch/,
	);
	assert.match(securityWorkflow, /MCPACE_ENABLE_PRIVATE_DEPENDENCY_REVIEW/);
});

test("GitHub Node jobs install locked dev tooling before package checks", () => {
	const workflowText = walkFiles(
		path.join(repoRoot, ".github", "workflows"),
		(file) => /\.ya?ml$/.test(file),
	)
		.map((file) => fs.readFileSync(file, "utf8"))
		.join("\n");
	assert.match(workflowText, /cache:\s+npm/);
	assert.match(
		workflowText,
		/npm ci --ignore-scripts --no-audit --no-fund --omit=optional/,
	);
	assert.match(workflowText, /npm run check:package/);
	const publishWorkflow = fs.readFileSync(
		path.join(repoRoot, ".github", "workflows", "publish-npm.yml"),
		"utf8",
	);
	const publishJob =
		publishWorkflow.match(/\n {2}publish:\s*\n[\s\S]*$/)?.[0] || "";
	assert.match(
		publishJob,
		/package-manager-cache:\s*false/,
		"the protected write-bearing publish job must not trust shared caches",
	);
	assert.doesNotMatch(
		publishJob,
		/cache:\s*npm/,
		"the protected write-bearing publish job must install from the lockfile without an npm cache",
	);
});

test("checked-in package lock is public-registry safe and does not leak local mirror URLs", () => {
	const lockfile = fs.readFileSync(
		path.join(repoRoot, "package-lock.json"),
		"utf8",
	);
	const internalRegistryPattern = new RegExp(
		[
			`packages\\.applied-${"caas"}-gateway`,
			`arti${"factory"}`,
			`internal\\.api\\.${"openai"}`,
		].join("|"),
		"i",
	);
	assert.equal(internalRegistryPattern.test(lockfile), false);
	assert.match(
		lockfile,
		/https:\/\/registry\.npmjs\.org\/publint\/-\/publint-/,
	);
});

test("dashboard HTTP surface keeps boundary checks, hardening headers, and telemetry centralized", () => {
	const boundary = fs.readFileSync(
		path.join(repoRoot, "src/dashboard/http_boundary.rs"),
		"utf8",
	);
	const response = fs.readFileSync(
		path.join(repoRoot, "src/dashboard/response.rs"),
		"utf8",
	);
	const dashboard = fs.readFileSync(
		path.join(repoRoot, "src/dashboard.rs"),
		"utf8",
	);
	const overview = fs.readFileSync(
		path.join(repoRoot, "src/dashboard/overview.rs"),
		"utf8",
	);
	const html = fs.readFileSync(
		path.join(repoRoot, "src/dashboard/index.html"),
		"utf8",
	);
	const appJs = readDashboardJs();
	const css = fs.readFileSync(
		path.join(repoRoot, "src/dashboard/frontend/styles.css"),
		"utf8",
	);
	const dashboardFrontend = `${html}\n${css}\n${appJs}`;

	assert.match(boundary, /pub\(crate\) fn is_loopback_host/);
	assert.match(boundary, /parse::<IpAddr>\(\)/);
	assert.match(response, /X-Content-Type-Options: nosniff/);
	assert.match(response, /Referrer-Policy: no-referrer/);
	assert.match(response, /Content-Security-Policy:/);
	assert.match(dashboard, /fn bounded_query_usize/);
	assert.match(boundary, /fn is_valid_http_header_name/);
	assert.match(boundary, /fn is_valid_http_header_value/);
	assert.match(boundary, /values.next\(\).is_some\(\)/);
	assert.match(response, /invalid response header value/);
	assert.match(dashboard, /Malformed HTTP header/);
	assert.match(
		dashboard,
		/JSON action bodies require Content-Type: application\/json/,
	);
	assert.match(dashboard, /fn authorization_bearer_token/);
	assert.match(dashboard, /eq_ignore_ascii_case\("Bearer"\)/);
	assert.equal(
		(
			dashboard.match(
				/reject_forbidden_origin\(stream, request(?:, config)?\)/g,
			) || []
		).length,
		1,
		"origin checks should be applied once at the HTTP entry boundary",
	);
	const mcpHttp = fs.readFileSync(
		path.join(repoRoot, "src/dashboard/mcp_http.rs"),
		"utf8",
	);
	assert.doesNotMatch(mcpHttp, /reject_forbidden_origin/);
	assert.doesNotMatch(mcpHttp, /validate_origin\(request\)/);
	assert.match(overview, /requestDurationAverageMs/);
	assert.match(overview, /requestDurationMaxMs/);
	assert.match(dashboardFrontend, /Request duration/);
	assert.doesNotMatch(dashboardFrontend, /Latency histograms/);
	assert.match(dashboard, /DASHBOARD_CSS/);
	assert.match(dashboard, /DASHBOARD_JS/);
	assert.match(dashboard, /DASHBOARD_RUNTIME_JS/);
	assert.match(dashboard, /DASHBOARD_RENDER_JS/);
	assert.match(dashboard, /DASHBOARD_RENDER_DETAILS_JS/);
	assert.match(dashboard, /DASHBOARD_MODEL_JS/);
	assert.match(dashboard, /DASHBOARD_ACTIONS_JS/);
	assert.match(dashboard, /DASHBOARD_BOOT_JS/);
	assert.match(dashboard, /GET", "\/dashboard\.css/);
	assert.match(dashboard, /GET", "\/dashboard\.js/);
	assert.match(dashboard, /GET", "\/dashboard\.runtime\.js/);
	assert.match(dashboard, /GET", "\/dashboard\.model\.js/);
	assert.match(dashboard, /GET", "\/dashboard\.render\.js/);
	assert.match(dashboard, /GET", "\/dashboard\.render\.details\.js/);
	assert.match(dashboard, /GET", "\/dashboard\.actions\.js/);
	assert.match(dashboard, /GET", "\/dashboard\.boot\.js/);
	assert.match(response, /style-src 'self' 'unsafe-inline'/);
	assert.match(response, /script-src 'self'/);
});

test("dashboard frontend assets stay parseable by current Node tooling", () => {
	const html = fs.readFileSync(
		path.join(repoRoot, "src/dashboard/index.html"),
		"utf8",
	);
	const css = fs.readFileSync(
		path.join(repoRoot, "src/dashboard/frontend/styles.css"),
		"utf8",
	);

	assert.match(html, /<link rel="stylesheet" href="\/dashboard\.css"\s*\/?>/);
	assert.match(
		html,
		/<script src="\/dashboard\.js" defer><\/script>\s*<script src="\/dashboard\.runtime\.js" defer><\/script>\s*<script src="\/dashboard\.model\.js" defer><\/script>\s*<script src="\/dashboard\.render\.js" defer><\/script>\s*<script src="\/dashboard\.render\.details\.js" defer><\/script>\s*<script src="\/dashboard\.actions\.js" defer><\/script>\s*<script src="\/dashboard\.boot\.js" defer><\/script>/,
	);
	assert.doesNotMatch(html, /<script>([\s\S]*?)<\/script>/);
	assert.match(css, /MCPace dashboard styles/);
	for (const file of dashboardJsFiles) {
		const script = fs.readFileSync(path.join(repoRoot, file), "utf8");
		assert.doesNotThrow(() => new vm.Script(script, { filename: file }));
	}
});

test("Node syntax auto jobs retry failed children serially before failing", () => {
	const syntax = fs.readFileSync(
		path.join(repoRoot, "scripts", "check-node-syntax.mjs"),
		"utf8",
	);
	assert.match(syntax, /function retryFailedChecksSerial/);
	assert.match(syntax, /runChecksParallel/);
	assert.match(syntax, /serial-failed-auto-jobs/);
	assert.match(syntax, /uv_cwd\/spawn ENOENT|uv_cwd/);
});

test("public MCP stdio command is exposed without making users type stdio-shim", () => {
	const catalog = fs.readFileSync(
		path.join(repoRoot, "src", "catalog.rs"),
		"utf8",
	);
	const app = fs.readFileSync(path.join(repoRoot, "src", "app.rs"), "utf8");
	const shim = fs.readFileSync(
		path.join(repoRoot, "src", "stdio_shim.rs"),
		"utf8",
	);
	const importer = fs.readFileSync(
		path.join(repoRoot, "src", "mcp_sources", "import.rs"),
		"utf8",
	);
	const setup = fs.readFileSync(path.join(repoRoot, "src", "setup.rs"), "utf8");

	assert.match(catalog, /name:\s*"stdio"/);
	assert.match(catalog, /aliases:\s*&\["stdio-shim",\s*"stdio_shim"\]/);
	assert.match(catalog, /Live MCP stdio launch surface/);
	assert.match(app, /"stdio" \| "stdio-shim" => stdio_shim::run/);
	assert.match(app, /mcpace stdio \[--root <path>\]/);
	assert.match(shim, /Usage: mcpace stdio \[--root <path>\]/);
	assert.match(shim, /compatibility alias/);
	assert.match(
		importer,
		/arg == "mcp-server" \|\| arg == "stdio" \|\| arg == "stdio-shim"/,
	);
	assert.match(setup, /"mcp-server" \| "stdio" \| "stdio-shim" \| "serve"/);
});
