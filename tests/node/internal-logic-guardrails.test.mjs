import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { repoRoot } from "../../scripts/lib/project-metadata.mjs";

const read = (...parts) =>
	fs.readFileSync(path.join(repoRoot, ...parts), "utf8");

function readDashboardJs() {
	return [
		read("src", "dashboard", "frontend", "app.js"),
		read("src", "dashboard", "frontend", "app.runtime.js"),
		read("src", "dashboard", "frontend", "app.model.js"),
		read("src", "dashboard", "frontend", "app.render.js"),
		read("src", "dashboard", "frontend", "app.render.details.js"),
		read("src", "dashboard", "frontend", "app.actions.js"),
		read("src", "dashboard", "frontend", "app.boot.js"),
	].join("\n");
}

function readJson(...parts) {
	try {
		return JSON.parse(read(...parts));
	} catch (error) {
		const detail = error instanceof Error ? error.message : String(error);
		assert.fail(`${path.join(...parts)} must contain valid JSON: ${detail}`);
	}
}
const fnMarker = (name) =>
	new RegExp(String.raw`(?:^|\n)\s*(?:pub(?:\([^)]*\))?\s+)?fn ${name}\(`);

const sliceFn = (source, name, nextName) => {
	const startMatch = fnMarker(name).exec(source);
	assert.ok(startMatch, `missing function ${name}`);
	const start = startMatch.index + (source[startMatch.index] === "\n" ? 1 : 0);
	if (!nextName) return source.slice(start);
	const endMatch = fnMarker(nextName).exec(source.slice(start + 1));
	assert.ok(endMatch, `missing end marker ${nextName}`);
	return source.slice(start, start + 1 + endMatch.index);
};

test("generic source policy prioritizes high-risk/session signals before broad remote fallback", () => {
	const loader = read("src", "server", "loader.rs");
	const body = sliceFn(loader, "infer_generic_source_policy", "source_signals");

	const remoteIndex = body.indexOf("if remote {");
	assert.ok(remoteIndex > -1, "missing broad remote fallback");
	for (const signal of [
		'signals.contains("remote-browser-session")',
		'signals.contains("browser-observation")',
		'signals.contains("browser-or-desktop")',
		'signals.contains("shell-or-process")',
	]) {
		const index = body.indexOf(signal);
		assert.ok(index > -1, `missing prioritized branch ${signal}`);
		assert.ok(
			index < remoteIndex,
			`${signal} must be checked before broad remote fallback`,
		);
	}
});

test("mutable tool evidence prevents readonly network/local policy inference", () => {
	const loader = read("src", "server", "loader.rs");
	const body = sliceFn(loader, "infer_generic_source_policy", "source_signals");

	assert.match(
		body,
		/let mutable_tools = signals\.contains\("mutable-tools"\);/,
	);
	assert.match(body, /signals\.contains\("network-fetch"\) && !mutable_tools/);
	assert.match(
		body,
		/\(signals\.contains\("local-utility"\) \|\| signals\.contains\("readonly-tools"\)\) && !mutable_tools/,
	);
});

test("declared server records inherit real source transport and command metadata", () => {
	const loader = read("src", "server", "loader.rs");
	const normalize = sliceFn(loader, "normalize_server_record", "policy_string");

	assert.match(normalize, /let source_type = if source_record\.is_some\(\)/);
	assert.match(
		normalize,
		/transport_preference: inferred_transport_preference\(object, &source_type\)/,
	);
	assert.match(
		normalize,
		/supported_transports: inferred_supported_transports\(object, &source_type\)/,
	);
	assert.match(
		normalize,
		/required_commands: inferred_required_commands\(object, &source_type, &source_command\)/,
	);

	assert.match(loader, /fn inferred_transport_preference\(/);
	assert.match(loader, /fn inferred_supported_transports\(/);
	assert.match(loader, /fn inferred_required_commands\(/);
	assert.match(loader, /if source_type == "stdio" && !command\.is_empty\(\)/);
});

test("plan-only and not-runnable sources preserve zero-worker semantics through scheduling", () => {
	const loader = read("src", "server", "loader.rs");
	const clientPlan = read("src", "client", "plan.rs");

	assert.match(loader, /ExecutionPolicy::inferred_mode\(/);
	assert.match(loader, /let max_workers = execution\.worker_limit\(\);/);
	assert.match(
		loader,
		/execution\.effective_max_in_flight_per_worker\(&source_type\)/,
	);
	assert.match(loader, /&& !execution\.is_disabled\(\)/);

	const execution = read("src", "execution.rs");
	const inferredMode = sliceFn(execution, "inferred_mode", "is_disabled");
	assert.match(
		inferredMode,
		/concurrency_policy == "plan-only" \|\| scope_class == "not-runnable"/,
	);
	assert.match(inferredMode, /return ExecutionMode::Disabled;/);
	assert.match(execution, /ExecutionMode::Disabled => 0/);
	assert.match(execution, /if self\.is_disabled\(\) \{\s*return 0;/s);

	assert.match(clientPlan, /fn server_is_not_routable\(/);
	assert.match(clientPlan, /name: "disabled-no-route"\.to_string\(\)/);
	assert.match(clientPlan, /parallelism_limit: 0/);
	assert.match(clientPlan, /record\.max_workers == 0/);
});

test("mutating evidence wins over stateless and readonly parallel-safety candidates", () => {
	const loader = read("src", "server", "loader.rs");
	const body = sliceFn(
		loader,
		"infer_parallel_safety_class",
		"policy_is_explicit_stateless",
	);

	const destructiveIndex = body.indexOf("if destructive_tools {");
	const explicitStatelessIndex = body.indexOf(
		"if policy_is_explicit_stateless",
	);
	const networkReadonlyIndex = body.indexOf(
		'signals.contains("network-fetch")',
	);
	assert.ok(destructiveIndex > -1, "missing destructive guard");
	assert.ok(explicitStatelessIndex > -1, "missing stateless branch");
	assert.ok(networkReadonlyIndex > -1, "missing network readonly branch");
	assert.ok(
		destructiveIndex < explicitStatelessIndex,
		"destructive evidence must beat explicit stateless hints",
	);
	assert.ok(
		destructiveIndex < networkReadonlyIndex,
		"destructive evidence must beat readonly network/local hints",
	);
	assert.match(body, /P0_mutating_requires_serialization/);
});

test("schema enums accept the runtime disabled pool model", () => {
	const serverProfile = readJson(
		"schemas",
		"mcpace-server-profile.schema.json",
	);
	const workerPlan = readJson("schemas", "mcpace-worker-plan.schema.json");
	assert.ok(
		serverProfile.properties.defaultPoolModel.enum.includes("disabled"),
	);
	assert.ok(workerPlan.properties.poolModel.enum.includes("disabled"));
});

test("hub-owned stdio warnings only count routable enabled servers", () => {
	const clientPlan = read("src", "client", "plan.rs");
	const loopIndex = clientPlan.indexOf("for record in server_records");
	assert.ok(loopIndex > -1, "missing server loop");
	const loopBody = clientPlan.slice(
		loopIndex,
		clientPlan.indexOf("if context.project_root.is_none()", loopIndex),
	);
	assert.match(
		loopBody,
		/let route_is_routable = server_plan_is_routable\(&plan\);/,
	);
	assert.match(
		loopBody,
		/if route_is_routable && plan\.upstream_transport == "stdio" \{\s*requires_hub_owned_stdio = true;\s*\}/s,
	);
});

test("disabled execution preset is truly non-routable and zero-capacity", () => {
	const policy = read("src", "server", "policy.rs");
	const disabledStart = policy.indexOf('"disabled" => ExecutionPreset');
	assert.ok(disabledStart > -1, "missing disabled preset");
	const disabledArm = policy.slice(
		disabledStart,
		policy.indexOf("        _ => unreachable!()", disabledStart),
	);

	assert.match(disabledArm, /scope_class: "not-runnable"/);
	assert.match(disabledArm, /concurrency_policy: "plan-only"/);
	assert.match(disabledArm, /parallelism_limit: 0/);
	assert.match(disabledArm, /discovery_requires_lease: false/);
	assert.match(policy, /if preset\.mode == "disabled" \{\s*0\s*\} else \{/s);
	assert.match(
		policy,
		/let parallelism = if preset\.mode == "disabled" \{\s*0\s*\}/s,
	);
	assert.match(
		policy,
		/let max_workers = if preset\.mode == "disabled" \{\s*0\s*\} else \{/s,
	);
	assert.match(
		policy,
		/let max_in_flight = if preset\.mode == "disabled" \{\s*0\s*\}/s,
	);

	const configSchema = readJson("schemas", "mcpace-config.schema.json");
	assert.equal(
		configSchema.$defs.executionPolicy.properties.maxWorkers.minimum,
		0,
	);
	assert.equal(
		configSchema.$defs.executionPolicy.properties.maxInFlightPerWorker.minimum,
		0,
	);
	assert.equal(
		configSchema.$defs.serverPolicy.properties.maxInFlightPerWorker.minimum,
		0,
	);
});

test("upstream execution honors disabled runtime policy, not only MCP settings enabled flags", () => {
	const upstreamRoot = read("src", "upstream.rs");
	const serverConfig = read("src", "upstream", "server_config.rs");

	assert.match(
		upstreamRoot,
		/struct UpstreamServerPolicy \{[\s\S]*runtime_enabled: bool,[\s\S]*tool_policies:/,
	);
	assert.match(
		serverConfig,
		/fn server_policy_is_disabled\(raw_server: &JsonValue\) -> bool/,
	);
	for (const field of [
		'["policy", "startupStrategy"]',
		'["policy", "routingGroup"]',
		'["policy", "concurrencyPolicy"]',
		'["policy", "scopeClass"]',
		'["policy", "maxWorkers"]',
		'["execution", "mode"]',
		'["execution", "maxWorkers"]',
	]) {
		assert.match(
			serverConfig,
			new RegExp(field),
			`disabled policy helper must inspect ${field}`,
		);
	}
	assert.match(
		serverConfig,
		/let execution = ExecutionPolicy::for_server\(&execution_defaults, raw_server\);/,
	);
	assert.match(
		serverConfig,
		/let runtime_enabled = !execution\.is_disabled\(\) && !server_policy_is_disabled\(raw_server\);/,
	);
	assert.match(serverConfig, /runtime_enabled,/);
	assert.match(serverConfig, /server is disabled by MCPace runtime policy/);
});

test("server profiles and dashboard preserve disabled servers as zero-capacity/offline", () => {
	const loader = read("src", "server", "loader.rs");
	const dashboard = read("src", "dashboard.rs");
	const overview = read("src", "dashboard", "overview.rs");
	const html = readDashboardJs();

	assert.match(loader, /fn runtime_policy_disabled\(/);
	assert.match(
		loader,
		/let base_effective_enabled = profile_enabled && source_enabled && platform_supported;/,
	);
	assert.match(
		loader,
		/let effective_enabled = base_effective_enabled\s*&& !execution\.is_disabled\(\)\s*&& !runtime_policy_disabled\(/s,
	);
	assert.match(
		loader,
		/let effective_enabled = source_record\.enabled\s*&& !execution\.is_disabled\(\)\s*&& !runtime_policy_disabled\(/s,
	);

	assert.match(dashboard, /if mode != "disabled" \{/);
	assert.doesNotMatch(
		dashboard,
		/mode == "disabled"[\s\S]*push_positive_usize_arg/,
	);
	assert.match(
		overview,
		/if current == "disabled" \{[\s\S]*max_workers: 0,[\s\S]*max_in_flight_per_worker: 0,/,
	);
	assert.match(
		html,
		/if \(serverMode\(server, instances\) === "disabled"\) return 0;/,
	);
	assert.match(
		html,
		/overrides\.maxWorkers \?\? maxWorkers\(server, related\)/,
	);
	assert.match(
		html,
		/firstDefined\(\s*result\.maxWorkers,\s*execution\.maxWorkers,\s*payload\.maxWorkers,\s*server\.maxWorkers,\s*1,?\s*\)/,
	);
});

test("launcher package specs feed internal policy classification without trusting display names", () => {
	const loader = read("src", "server", "loader.rs");
	const signals = sliceFn(loader, "source_signals", "infer_source_type");
	const rawArg = sliceFn(
		loader,
		"raw_arg_is_semantic_signal",
		"looks_like_launcher_package_signal",
	);
	const launcher = sliceFn(
		loader,
		"looks_like_launcher_package_signal",
		"infer_generic_source_policy",
	);

	assert.match(
		signals,
		/let\s+_identity\s*=\s*\(normalized_name,\s*display_name\)/,
	);
	assert.match(signals, /command_semantic_signal\(command\)/);
	assert.match(signals, /args\.join\(" "\)/);
	assert.doesNotMatch(signals, /format!\(\s*"\{\} \{\} \{\} \{\} \{\}"/);
	assert.match(rawArg, /looks_like_launcher_package_signal\(&normalized\)/);
	assert.match(launcher, /value\.starts_with\('@'\)/);
	assert.match(launcher, /value\.contains\("mcp"\)/);
	assert.match(launcher, /value\.starts_with\("http:\/\/"\)/);
	assert.match(
		loader,
		/_ if looks_like_launcher_package_signal\(&command_name\) => command_name,/,
	);
});

test("legacy disabled flags win over enabled flags and toggles write consistent state", () => {
	const loader = read("src", "server", "loader.rs");
	const sourceEnabled = sliceFn(
		loader,
		"source_enabled",
		"supported_transports_for_source_type",
	);
	const writer = read("src", "mcp_sources", "write.rs");

	const disabledIndex = sourceEnabled.indexOf('get("disabled")');
	const enabledIndex = sourceEnabled.indexOf('get("enabled")');
	assert.ok(disabledIndex > -1, "source_enabled must inspect disabled");
	assert.ok(enabledIndex > -1, "source_enabled must inspect enabled");
	assert.ok(
		disabledIndex < enabledIndex,
		"disabled must be evaluated before enabled",
	);
	assert.match(sourceEnabled, /return false;/);

	assert.match(writer, /let raw_disabled = server_object\.get\("disabled"\)/);
	assert.match(
		writer,
		/previous_enabled = raw_enabled\.or_else\(\|\| raw_disabled\.map\(\|disabled\| !disabled\)\);/,
	);
	assert.match(writer, /server_object\.remove\("disabled"\);/);
	assert.match(
		writer,
		/server_object\.insert\("disabled"\.to_string\(\), JsonValue::bool\(true\)\);/,
	);
});

test("server add remote URLs validate authority shape instead of only scheme prefix", () => {
	const helpers = read("src", "mcp_sources", "write_helpers.rs");
	const validator = sliceFn(
		helpers,
		"validate_remote_mcp_url",
		"validate_remote_mcp_authority",
	);
	const authority = sliceFn(
		helpers,
		"validate_remote_mcp_authority",
		"validate_remote_mcp_host",
	);

	assert.match(validator, /trimmed\.contains\('#'\)/);
	assert.match(validator, /split\(\['\/', '\?'\]\)/);
	assert.match(validator, /validate_remote_mcp_authority\(authority\)/);
	assert.match(authority, /authority\.contains\('@'\)/);
	assert.match(authority, /authority\.starts_with\('\['\)/);
	assert.match(authority, /authority\.matches\(':'\)\.count\(\) > 1/);
	assert.match(helpers, /fn validate_remote_mcp_port\(/);
	assert.match(helpers, /parse::<u16>\(\)/);
	assert.match(helpers, /filter\(\|value\| \*value > 0\)/);
});

test("key-value parsing rejects silent overwrites and unsafe HTTP header values", () => {
	const helpers = read("src", "mcp_sources", "write_helpers.rs");
	const parser = helpers.slice(
		helpers.indexOf("fn parse_key_value_pairs("),
		helpers.indexOf("pub(super) fn validate_env_name"),
	);
	assert.ok(parser.length > 0, "missing parse_key_value_pairs body");

	assert.match(parser, /parsed\s*\.keys\(\)\s*\.any\(/);
	assert.match(parser, /eq_ignore_ascii_case\(key\)/);
	assert.match(parser, /contains duplicate key/);
	assert.match(
		parser,
		/flag_name == "--header" && !validate_http_header_value\(value\)/,
	);
	assert.match(helpers, /fn validate_http_header_value\(value: &str\) -> bool/);
	assert.match(
		helpers,
		/text_utils::valid_http_field_value|crate::text_utils::valid_http_field_value/,
	);
	const textUtils = read("src", "text_utils.rs");
	assert.match(
		textUtils,
		/byte == b' ' \|\| \(0x21\.\.=0x7e\)\.contains\(&byte\)/,
	);
	assert.match(textUtils, /fn reserved_mcp_http_header_name/);
	assert.match(helpers, /reserved_mcp_http_header_name\(value\)/);
});

test("dashboard and public URL boundaries reject non-visible or ambiguous authorities", () => {
	const httpBoundary = read("src", "dashboard", "http_boundary.rs");
	const runtimepaths = read("src", "runtimepaths.rs");

	assert.match(httpBoundary, /pub\(super\) fn is_valid_http_header_value/);
	assert.match(
		httpBoundary,
		/byte == b' ' \|\| \(0x21\.\.=0x7e\)\.contains\(&byte\)/,
	);

	assert.match(
		runtimepaths,
		/fn valid_public_url_authority\(authority: &str\) -> bool/,
	);
	assert.match(runtimepaths, /trimmed\.contains\('#'\)/);
	assert.match(runtimepaths, /authority\.contains\('@'\)/);
	assert.match(runtimepaths, /authority\.matches\(':'\)\.count\(\) > 1/);
	assert.match(runtimepaths, /fn valid_public_port\(port: &str\) -> bool/);
	assert.match(runtimepaths, /filter\(\|value\| \*value > 0\)/);
});

test("workspace path matching uses lexical containment and keeps Windows drive roots routable", () => {
	const pathing = read("src", "client", "pathing.rs");
	const pathingTests = read("src", "client", "pathing", "tests.rs");
	const containment = sliceFn(pathing, "path_is_within", "path_compare_key");
	const lexical = pathing.slice(
		pathing.indexOf("fn lexical_path_key("),
		pathing.indexOf("pub(super) fn normalize("),
	);
	assert.ok(lexical.length > 0, "missing lexical_path_key body");

	assert.match(containment, /let path_key = path_compare_key\(path\);/);
	assert.match(containment, /let root_key = path_compare_key\(root\);/);
	assert.doesNotMatch(containment, /trim_trailing_separator/);
	assert.match(
		lexical,
		/match part \{[\s\S]*"" \| "\." => \{\}[\s\S]*"\.\." =>/,
	);
	assert.match(lexical, /parts\.pop\(\);/);
	assert.match(
		pathingTests,
		/fn path_containment_uses_lexical_segments_not_raw_prefixes/,
	);
	assert.match(
		pathingTests,
		/fn path_containment_handles_windows_drive_roots_case_insensitively/,
	);
	assert.match(pathing, /bytes\.len\(\) >= 2 && bytes\[1\] == b':'/);
});

test("unsupported-platform servers are non-routable even if stale metadata says enabled", () => {
	const plan = read("src", "client", "plan.rs");
	const nonRoutable = sliceFn(
		plan,
		"server_is_not_routable",
		"resolve_upstream_transport",
	);
	assert.match(nonRoutable, /!record\.platform_supported/);
	assert.ok(
		nonRoutable.indexOf("!record.platform_supported") <
			nonRoutable.indexOf("!record.effective_enabled"),
		"platform support must be checked before trusting effectiveEnabled snapshots",
	);
});

test("command-like package inference keeps launcher option values from becoming packages", () => {
	const autoInstall = read("src", "mcp_autoinstall.rs");
	const autoInstallTests = read("src", "mcp_autoinstall", "tests.rs");
	const firstArg = sliceFn(
		autoInstall,
		"first_non_option_arg",
		"docker_image_arg",
	);

	assert.match(firstArg, /inline_package_option_value\(arg\)/);
	assert.match(firstArg, /launcher_option_selects_package\(arg\)/);
	assert.match(firstArg, /launcher_option_takes_value\(arg\)/);
	assert.match(firstArg, /--registry/);
	assert.match(firstArg, /--from/);
	assert.match(
		autoInstallTests,
		/command_like_install_prefers_package_flags_for_identity/,
	);
	assert.match(
		autoInstallTests,
		/command_like_install_skips_value_options_before_package/,
	);
});

test("runtime profile and policy keys share MCP source-name normalization", () => {
	const profile = read("src", "profile.rs");
	const loader = read("src", "server", "loader.rs");
	const upstreamConfig = read("src", "upstream", "server_config.rs");

	assert.match(profile, /use crate::mcp_sources;/);
	assert.match(profile, /mcp_sources::normalize_server_name\(server_name\)/);
	assert.doesNotMatch(
		profile,
		/overrides\.insert\(server_name\.trim\(\)\.to_ascii_lowercase\(\)/,
	);

	assert.match(
		loader,
		/let normalized_name = mcp_sources::normalize_server_name\(name\);/,
	);
	assert.doesNotMatch(
		loader,
		/let normalized_name = name\.trim\(\)\.to_ascii_lowercase\(\);/,
	);

	assert.match(
		upstreamConfig,
		/let normalized_server_name = mcp_sources::normalize_server_name\(server_name\);/,
	);
	assert.match(
		upstreamConfig,
		/server_overrides[\s\S]*\.get\(&normalized_server_name\)/,
	);
	assert.doesNotMatch(
		upstreamConfig,
		/server_name\.trim\(\)\.to_ascii_lowercase\(\)/,
	);
});

test("Windows runtime files are replaced without deleting visible state first", () => {
	const runtimepaths = read("src", "runtimepaths.rs");
	const writeTextAtomic = sliceFn(
		runtimepaths,
		"write_text_atomic",
		"replace_file_atomic",
	);
	const replaceAtomic = sliceFn(
		runtimepaths,
		"replace_file_atomic",
		"replace_file_atomic_windows",
	);
	const windowsReplace = sliceFn(
		runtimepaths,
		"replace_file_atomic_windows",
		"replace_existing_file_windows",
	);
	const moveFileEx = runtimepaths.slice(
		runtimepaths.indexOf("fn replace_existing_file_windows"),
		runtimepaths.indexOf("pub(crate) fn unix_time_ms"),
	);
	assert.ok(moveFileEx.length > 0, "missing Windows replacement body");

	assert.match(writeTextAtomic, /replace_file_atomic\(&temp_path, path\)/);
	assert.doesNotMatch(
		writeTextAtomic,
		/remove_file\(path\)/,
		"atomic write must not unlink the visible file before replacement",
	);
	assert.match(replaceAtomic, /#\[cfg\(windows\)\]/);
	assert.match(windowsReplace, /for attempt in 0\.\.=20/);
	assert.match(moveFileEx, /MoveFileExW/);
	assert.match(moveFileEx, /MOVEFILE_REPLACE_EXISTING/);
	assert.match(runtimepaths, /strip_windows_extended_path_prefix/);
	assert.match(runtimepaths, /\\\\\?\\UNC\\/);
});

test("serve background lifecycle avoids locked runners and raw-PID stop races", () => {
	const serve = read("src", "serve.rs");
	const runtimepaths = read("src", "runtimepaths.rs");
	const start = sliceFn(serve, "run_start", "run_stop");
	const stopExisting = sliceFn(
		serve,
		"stop_existing_serve",
		"remove_managed_serve_runner_copy",
	);
	const cleanup = sliceFn(
		serve,
		"remove_managed_serve_runner_copy",
		"run_status",
	);
	const cooperativeStop = sliceFn(
		serve,
		"request_cooperative_serve_stop",
		"remove_managed_serve_runner_copy",
	);

	assert.match(runtimepaths, /fn serve_runner_path_for_start/);
	assert.match(start, /serve_runner_path_for_start\(&state_root\)/);
	assert.doesNotMatch(
		start,
		/serve_runner_path\(&state_root\)/,
		"serve start must not overwrite a potentially locked runner exe",
	);
	assert.match(
		start,
		/remove_managed_serve_runner_copy\(&state_root, &state\)/,
	);
	assert.match(stopExisting, /acquire_lifecycle_coordination\(&state_root\)/);
	assert.match(
		stopExisting,
		/remove_managed_serve_runner_copy\(&state_root, state\)/,
	);
	assert.match(
		cleanup,
		/canonical_runner\.starts_with\(&canonical_runtime_bin\)/,
	);
	assert.match(cooperativeStop, /write_private_text_atomic/);
	assert.match(cooperativeStop, /process_identity::match_process/);
	assert.match(cooperativeStop, /refusing an unsafe raw-PID signal/);
	assert.doesNotMatch(serve, /fn kill_process\(/);
	assert.doesNotMatch(serve, /taskkill|kill_unix_process_group/);
});

test("stdio child environment keeps only one Windows PATH spelling", () => {
	const stdio = read("src", "upstream", "stdio_runtime.rs");
	const envBlock = stdio.slice(
		stdio.indexOf("fn default_child_process_environment"),
		stdio.indexOf("pub(super) fn read_response"),
	);
	assert.ok(envBlock.length > 0, "missing default child environment body");
	assert.match(envBlock, /"Path"/);
	assert.match(envBlock, /env::var\("PATH"\)/);
	assert.doesNotMatch(
		envBlock,
		/"PATH",\s*\n\s*"Path"/,
		"Windows env forwarding must avoid duplicate case-insensitive PATH keys",
	);
});

test("serve start replaces healthy stale endpoint instead of orphaning another runtime", () => {
	const serve = read("src", "serve.rs");
	const start = sliceFn(serve, "run_start", "state_matches_start_request");
	const matcher = sliceFn(
		serve,
		"state_matches_start_request",
		"stop_existing_serve",
	);

	assert.match(
		start,
		/let existing_healthy =\s*health_check\(\s*&existing_state\.host,\s*existing_state\.port,\s*&endpoint\.health_path,\s*\)/s,
	);
	assert.match(
		start,
		/if existing_healthy \{\s*if !state_matches_start_request\(/s,
	);
	assert.match(start, /stop_existing_serve_locked\(&canonical_root\)/);
	assert.ok(
		start.indexOf("acquire_serve_start_lock(&state_root)") <
			start.indexOf("remove_file(serve_stop_request_path(&state_root))"),
		"start must own lifecycle coordination before clearing a stale stop marker",
	);
	assert.match(
		start,
		/\} else \{\s*remove_managed_serve_runner_copy\(&state_root, &existing_state\);\s*let _ = fs::remove_file\(&state_path\);\s*crate::restart_guard::clear\(&restart_guard_path\);\s*\}/s,
	);
	assert.match(matcher, /state\.host == host/);
	assert.match(matcher, /state\.port == port/);
	assert.match(matcher, /state\.max_connections == max_connections/);
	assert.match(matcher, /state\.io_timeout_ms == io_timeout_ms/);
	assert.match(matcher, /state\.max_body_bytes == max_body_bytes/);
	assert.match(matcher, /state\.overview_cache_ms == overview_cache_ms/);
});

test("dashboard HTTP response writer has a typed boundary error", () => {
	const response = read("src", "dashboard", "response.rs");
	assert.match(response, /struct ResponseWriteError/);
	assert.match(response, /impl std::error::Error for ResponseWriteError/);
	assert.match(response, /type ResponseWriteResult<T>/);
	assert.doesNotMatch(
		response,
		/->\s*Result<[^>]+,\s*String>/,
		"response writer should not export stringly write errors",
	);
});

test("setup and serve share the bounded Streamable HTTP probe instead of cloning raw TCP readers", () => {
	const setup = read("src", "setup.rs");
	const serve = read("src", "serve.rs");
	const probe = read("src", "http_probe.rs");
	const httpMcp = sliceFn(setup, "http_mcp_request", "http_mcp_notification");
	const connector = sliceFn(probe, "connect_probe_addr", "raw_response_until");
	const raw = sliceFn(probe, "raw_response_until", "read_response_until");
	const reader = sliceFn(probe, "read_response_until", "response_ready");
	const ready = sliceFn(probe, "response_ready", "parse_json_response");
	const parser = sliceFn(probe, "parse_json_response", "parse_response");

	assert.match(setup, /http_probe::json_get/);
	assert.match(setup, /http_probe::json_response/);
	assert.match(setup, /http_probe::raw_response/);
	assert.match(serve, /http_probe::raw_response/);
	assert.match(serve, /http_probe::parse_response/);
	assert.match(
		httpMcp,
		/MCP-Protocol-Version: \{\}/,
		"setup MCP probe should send protocol version header",
	);
	assert.match(httpMcp, /mcp::CURRENT_PROTOCOL_VERSION/);
	assert.match(
		raw,
		/to_socket_addrs\(\)/,
		"shared probe should try every resolved localhost address",
	);
	assert.match(raw, /connect_probe_addr\(&addr, deadline\)/);
	assert.match(raw, /deadline\s*\.checked_duration_since\(Instant::now\(\)\)/);
	assert.match(connector, /#\[cfg\(windows\)\]/);
	assert.match(connector, /addr\.ip\(\)\.is_loopback\(\)/);
	assert.match(connector, /remaining\.min\(Duration::from_millis\(250\)\)/);
	assert.match(connector, /ErrorKind::TimedOut \| ErrorKind::WouldBlock/);
	assert.match(connector, /Instant::now\(\) < deadline/);
	assert.match(reader, /max_response_bytes/);
	assert.match(
		reader,
		/deadline\s*\.checked_duration_since\(Instant::now\(\)\)/,
	);
	assert.doesNotMatch(
		reader,
		/read_to_string/,
		"shared probe must not wait for EOF on long-lived SSE streams",
	);
	const chunked = sliceFn(probe, "decode_chunked_body", "find_crlf");
	const crlf = sliceFn(probe, "find_crlf", "sse_json_body");
	const sse = sliceFn(probe, "sse_json_body", "probe_host");

	assert.match(ready, /is_event_stream\(\)/);
	assert.match(
		ready,
		/body_bytes\(\)/,
		"readiness should decode chunked/event-stream bodies through HttpResponse::body_bytes",
	);
	assert.match(parser, /sse_json_body\(&body\)/);
	assert.match(
		chunked,
		/b"\\r\\n"/,
		"chunked body parser must look for CRLF, not a literal broken newline",
	);
	assert.match(
		crlf,
		/b"\\r\\n"/,
		"CRLF finder must use an escaped CRLF byte string",
	);
	assert.match(
		sse,
		/replace\("\\r\\n", "\\n"\)/,
		"SSE parser should normalize CRLF without embedding raw newlines in string literals",
	);
});

test("Rust loopback fixtures prove reachability and bound accept waits", () => {
	const lib = read("src", "lib.rs");
	const testSupport = read("src", "test_support_tests.rs");
	assert.match(lib, /mod test_support_tests/);
	assert.match(lib, /use test_support_tests::bind_loopback_test_listener/);
	assert.match(testSupport, /fn bind_loopback_test_listener\(\)/);
	assert.match(testSupport, /for _ in 0\.\.64/);
	assert.match(
		testSupport,
		/TcpStream::connect_timeout\(&addr, Duration::from_millis\(250\)\)/,
	);

	const dashboard = read("src", "dashboard.rs");
	const dashboardTests = read("src", "dashboard", "tests.rs");
	assert.match(dashboard, /accept_deadline/);
	assert.match(dashboard, /listener\.set_nonblocking\(true\)/);
	assert.match(dashboard, /stream\.set_nonblocking\(false\)/);
	assert.match(dashboard, /request_tx\.try_send\(pending\)/);
	assert.match(dashboard, /stream\.shutdown\(Shutdown::Both\)/);
	assert.match(dashboard, /join_request_workers_until/);
	assert.match(dashboardTests, /accept_timeout: Some\(Duration::from_secs\(15\)\)/);
	assert.match(dashboardTests, /bounded_accept_timeout_stops_a_saturated_test_listener/);

	for (const parts of [
		["src", "dashboard", "tests.rs"],
		["src", "http_probe", "tests.rs"],
		["src", "serve", "tests.rs"],
		["src", "upstream", "tests.rs"],
		["src", "upstream", "http_runtime", "tests.rs"],
	]) {
		const fixture = read(...parts);
		assert.match(fixture, /bind_loopback_test_listener\(\)/, parts.join("/"));
		assert.doesNotMatch(fixture, /TcpListener::bind/, parts.join("/"));
		assert.doesNotMatch(
			fixture,
			/listener\.accept\(\)\.unwrap\(\)/,
			parts.join("/"),
		);
	}
});

test("setup completes the MCP lifecycle before listing tools", () => {
	const setup = read("src", "setup.rs");
	const runSetup = setup.slice(
		setup.indexOf("fn run_setup"),
		setup.indexOf("#[cfg(test)]"),
	);
	const notification = sliceFn(
		setup,
		"http_mcp_notification",
		"mcp_session_header",
	);
	const sessionHeader = sliceFn(setup, "mcp_session_header", "usize_at_path");

	assert.match(
		runSetup,
		/method", JsonValue::string\("notifications\/initialized"\)/,
	);
	assert.match(
		runSetup,
		/let initialized_ok = initialized_notification\.is_ok\(\);/,
	);
	assert.match(runSetup, /let tools_list = if initialized_ok \{/);
	assert.match(
		runSetup,
		/&& initialized_ok/,
		"setup ready status must require initialized notification",
	);
	assert.match(runSetup, /"mcpInitializedOk"/);
	assert.match(runSetup, /"mcpInitialized"/);
	assert.match(notification, /matches!\(parsed\.status, 200 \| 202 \| 204\)/);
	assert.match(notification, /MCP-Protocol-Version: \{\}/);
	assert.match(notification, /mcp_session_header\(session_id\)\?/);
	assert.match(sessionHeader, /Mcp-Session-Id: \{\}/);
});

test("projects JSON output uses the same lower-camel keys as the runtime registry", () => {
	const projects = read("src", "projects.rs");
	const serializer = sliceFn(projects, "to_json_value");
	for (const key of [
		"projectId",
		"name",
		"hostPath",
		"detectedType",
		"markers",
		"lastUsedAt",
		"state",
	]) {
		assert.match(serializer, new RegExp(`"${key}"`));
	}
	for (const key of [
		"ProjectId",
		"Name",
		"HostPath",
		"DetectedType",
		"Markers",
		"LastUsedAt",
		"State",
	]) {
		assert.doesNotMatch(serializer, new RegExp(`"${key}"`));
	}
});

test("runtime diagnostics validates stdio and HTTP/HTTPS wrapper targets before marking them callable", () => {
	const diagnostics = read("src", "dashboard", "diagnostics.rs");
	const diagnosticFn = sliceFn(diagnostics, "server_runtime_diagnostic");
	assert.match(
		diagnosticFn,
		/source_type == "stdio" && !source_command\.is_empty\(\)/,
	);
	assert.match(diagnosticFn, /http_upstream_url_is_callable\(source_url\)/);
	assert.match(diagnosticFn, /source_type == "http" && runtime_callable/);
	assert.match(diagnosticFn, /"callable-http-bridge"/);
	assert.doesNotMatch(
		diagnosticFn,
		/external\/remote HTTP server policy is configured, but live HTTP upstream fan-out is not implemented in this HTTP adapter/,
		"diagnostics must not mark every HTTP source as preview-blocked after plain HTTP forwarding exists",
	);
});

test("project registry scan is implemented and uses stable lower-camel records", () => {
	const projects = read("src", "projects.rs");
	const parser = sliceFn(projects, "parse_cli", "read_projects");
	const scanner = sliceFn(projects, "scan_project", "detect_project_markers");
	const upsert = sliceFn(
		projects,
		"upsert_project",
		"normalize_project_record",
	);
	const serializer = sliceFn(projects, "to_json_value");

	assert.match(parser, /scan:\s*action\.as_deref\(\) == Some\("scan"\)/);
	assert.doesNotMatch(projects, /"-scan"\s*=>\s*"--scan"/);
	assert.match(projects, /fn resolve_scan_project_path/);
	assert.match(
		projects,
		/std::env::current_dir\(\)/,
		"relative project scan paths should resolve from the caller cwd, not silently from mcpace --root",
	);
	assert.doesNotMatch(projects, /project scanning is not implemented yet/);
	assert.match(scanner, /detect_project_markers\(&canonical\)/);
	assert.match(scanner, /project_id_for_path\(&host_path\)/);
	assert.match(
		projects,
		/let key = if cfg!\(windows\)/,
		"project ids should only case-fold paths on Windows",
	);
	assert.match(
		upsert,
		/runtimepaths::write_text_atomic\(path, &root\.to_pretty_string\(\)\)/,
	);
	for (const key of [
		"projectId",
		"name",
		"hostPath",
		"detectedType",
		"markers",
		"lastUsedAt",
		"state",
	]) {
		assert.match(serializer, new RegExp(`"${key}"`));
	}
});

test("init seeds the current hub lease-store schema version", () => {
	const init = read("src", "init.rs");
	const hubRuntime = read("src", "hub", "runtime.rs");
	assert.match(
		hubRuntime,
		/fn default_lease_store\(\)[\s\S]*\("version", JsonValue::number\(2\)\)/,
	);
	assert.match(
		init,
		/\("version", JsonValue::number\(2\)\)[\s\S]*"updatedAtMs"[\s\S]*JsonValue::number\(runtimepaths::unix_time_ms\(\)\)/,
	);
	assert.doesNotMatch(
		init,
		/\("version", JsonValue::number\(1\)\),\n\s*\("leases",/,
	);
});

test("client install catalog supports platform-specific VS Code user mcp.json with servers root key", () => {
	const catalog = read("src", "client_catalog.rs");
	const builtin = read("src", "client_catalog", "builtin.rs");
	const actions = read("src", "client", "actions.rs");
	const updater = read("src", "config_edit.rs");
	const vscode = builtin.slice(
		builtin.indexOf('id: "vscode-workspace"'),
		builtin.indexOf('id: "cursor-local"'),
	);

	assert.match(vscode, /display_name: "Visual Studio Code user configuration"/);
	assert.match(vscode, /config_paths: &\["\.vscode\/mcp\.json"/);
	assert.match(vscode, /preferred_scope: "user"/);
	assert.match(
		vscode,
		/preferred_config_path: "~\/\.config\/Code\/User\/mcp\.json"/,
	);
	assert.match(vscode, /servers_object_key: "servers"/);
	assert.match(vscode, /include_type_http: true/);
	assert.match(catalog, /serversObjectKey/);
	assert.match(
		actions,
		/servers_object_key: shape\.servers_object_key\.clone\(\)/,
	);
	assert.match(updater, /entry\(servers_key\.to_string\(\)\)/);
	assert.doesNotMatch(
		vscode,
		/servers_object_key: "mcpServers"/,
		"VS Code mcp.json uses top-level servers, not mcpServers",
	);
});

test("setup readiness counts real upstream sources, not policy-only catalog records", () => {
	const setup = read("src", "setup.rs");
	const runSetup = setup.slice(
		setup.indexOf("fn run_setup"),
		setup.indexOf("#[derive(Clone, Debug)]\nstruct HomeMcpSource"),
	);
	const counts = sliceFn(
		setup,
		"setup_server_counts",
		"import_existing_home_mcp_servers",
	);
	const reporter = sliceFn(setup, "write_text_report");

	assert.match(
		runSetup,
		/let server_counts_before = setup_server_counts\(&root_path\);/,
	);
	assert.match(
		runSetup,
		/let server_count_before = server_counts_before\.source_enabled;/,
	);
	assert.match(runSetup, /let home_import = if server_count_before == 0/);
	assert.doesNotMatch(runSetup, /requested_server_spec|server_spec_to_install/);
	assert.match(counts, /policy_records: records\.len\(\)/);
	assert.match(
		counts,
		/filter\(\|record\| record\.source_enabled\)\s*\.count\(\)/s,
	);
	assert.match(counts, /filter\(\|record\| record\.effective_enabled\)/);
	assert.match(runSetup, /"serversKnown"/);
	assert.match(runSetup, /"sourceEnabledCount"/);
	assert.match(runSetup, /"effectiveEnabledCount"/);
	assert.match(runSetup, /let tools_expected = effective_server_configured;/);
	assert.match(runSetup, /let tools_ready = !tools_expected \|\| tools_ok;/);
	assert.match(reporter, /empty setup is OK/);
	assert.match(reporter, /not expected yet/);
});
