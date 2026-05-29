import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

const read = (...parts) => fs.readFileSync(path.join(repoRoot, ...parts), 'utf8');
const readJson = (...parts) => JSON.parse(read(...parts));
const sliceFn = (source, name, nextName) => {
  const start = source.indexOf(`fn ${name}(`);
  assert.notEqual(start, -1, `missing function ${name}`);
  const end = nextName ? source.indexOf(`\nfn ${nextName}(`, start) : source.length;
  assert.notEqual(end, -1, `missing end marker ${nextName}`);
  return source.slice(start, end);
};

test('generic source policy prioritizes high-risk/session signals before broad remote fallback', () => {
  const loader = read('src', 'server', 'loader.rs');
  const body = sliceFn(loader, 'infer_generic_source_policy', 'source_signals');

  const remoteIndex = body.indexOf('if remote {');
  assert.ok(remoteIndex > -1, 'missing broad remote fallback');
  for (const signal of [
    'signals.contains("remote-browser-session")',
    'signals.contains("browser-observation")',
    'signals.contains("browser-or-desktop")',
    'signals.contains("shell-or-process")',
  ]) {
    const index = body.indexOf(signal);
    assert.ok(index > -1, `missing prioritized branch ${signal}`);
    assert.ok(index < remoteIndex, `${signal} must be checked before broad remote fallback`);
  }
});

test('mutable tool evidence prevents readonly network/local policy inference', () => {
  const loader = read('src', 'server', 'loader.rs');
  const body = sliceFn(loader, 'infer_generic_source_policy', 'source_signals');

  assert.match(body, /let mutable_tools = signals\.contains\("mutable-tools"\);/);
  assert.match(body, /signals\.contains\("network-fetch"\) && !mutable_tools/);
  assert.match(
    body,
    /\(signals\.contains\("local-utility"\) \|\| signals\.contains\("readonly-tools"\)\) && !mutable_tools/,
  );
});

test('declared server records inherit real source transport and command metadata', () => {
  const loader = read('src', 'server', 'loader.rs');
  const normalize = sliceFn(loader, 'normalize_server_record', 'policy_string');

  assert.match(normalize, /let source_type = if source_record\.is_some\(\)/);
  assert.match(normalize, /transport_preference: inferred_transport_preference\(object, &source_type\)/);
  assert.match(normalize, /supported_transports: inferred_supported_transports\(object, &source_type\)/);
  assert.match(normalize, /required_commands: inferred_required_commands\(object, &source_type, &source_command\)/);

  assert.match(loader, /fn inferred_transport_preference\(/);
  assert.match(loader, /fn inferred_supported_transports\(/);
  assert.match(loader, /fn inferred_required_commands\(/);
  assert.match(loader, /if source_type == "stdio" && !command\.is_empty\(\)/);
});

test('plan-only and not-runnable sources preserve zero-worker semantics through scheduling', () => {
  const loader = read('src', 'server', 'loader.rs');
  const clientPlan = read('src', 'client', 'plan.rs');

  assert.match(loader, /fn allows_zero_workers\(/);
  assert.match(loader, /scope_class == "not-runnable"/);
  assert.match(loader, /concurrency_policy == "plan-only"/);
  assert.match(loader, /let max_workers = normalized_worker_count\(/);
  assert.match(loader, /let max_in_flight_per_worker = if max_workers == 0 \{\s*0/s);

  assert.match(clientPlan, /fn server_is_not_routable\(/);
  assert.match(clientPlan, /name: "disabled-no-route"\.to_string\(\)/);
  assert.match(clientPlan, /parallelism_limit: 0/);
  assert.match(clientPlan, /record\.max_workers == 0/);
});

test('mutating evidence wins over stateless and readonly parallel-safety candidates', () => {
  const loader = read('src', 'server', 'loader.rs');
  const body = sliceFn(loader, 'infer_parallel_safety_class', 'policy_is_explicit_stateless');

  const destructiveIndex = body.indexOf('if destructive_tools {');
  const explicitStatelessIndex = body.indexOf('if policy_is_explicit_stateless');
  const networkReadonlyIndex = body.indexOf('signals.contains("network-fetch")');
  assert.ok(destructiveIndex > -1, 'missing destructive guard');
  assert.ok(explicitStatelessIndex > -1, 'missing stateless branch');
  assert.ok(networkReadonlyIndex > -1, 'missing network readonly branch');
  assert.ok(destructiveIndex < explicitStatelessIndex, 'destructive evidence must beat explicit stateless hints');
  assert.ok(destructiveIndex < networkReadonlyIndex, 'destructive evidence must beat readonly network/local hints');
  assert.match(body, /P0_mutating_requires_serialization/);
});

test('schema enums accept the runtime disabled pool model', () => {
  const serverProfile = readJson('schemas', 'mcpace-server-profile.schema.json');
  const workerPlan = readJson('schemas', 'mcpace-worker-plan.schema.json');
  assert.ok(serverProfile.properties.defaultPoolModel.enum.includes('disabled'));
  assert.ok(workerPlan.properties.poolModel.enum.includes('disabled'));
});

test('hub-owned stdio warnings only count routable enabled servers', () => {
  const clientPlan = read('src', 'client', 'plan.rs');
  const loopIndex = clientPlan.indexOf('for record in server_records');
  assert.ok(loopIndex > -1, 'missing server loop');
  const loopBody = clientPlan.slice(loopIndex, clientPlan.indexOf('if context.project_root.is_none()', loopIndex));
  assert.match(loopBody, /let route_is_routable = server_plan_is_routable\(&plan\);/);
  assert.match(loopBody, /if route_is_routable && plan\.upstream_transport == "stdio" \{\s*requires_hub_owned_stdio = true;\s*\}/s);
});


test('disabled execution preset is truly non-routable and zero-capacity', () => {
  const policy = read('src', 'server', 'policy.rs');
  const disabledStart = policy.indexOf('"disabled" => ExecutionPreset');
  assert.ok(disabledStart > -1, 'missing disabled preset');
  const disabledArm = policy.slice(disabledStart, policy.indexOf('        _ => unreachable!()', disabledStart));

  assert.match(disabledArm, /scope_class: "not-runnable"/);
  assert.match(disabledArm, /concurrency_policy: "plan-only"/);
  assert.match(disabledArm, /parallelism_limit: 0/);
  assert.match(disabledArm, /discovery_requires_lease: false/);
  assert.match(policy, /if preset\.mode == "disabled" \{\s*0\s*\} else \{/s);
  assert.match(policy, /let parallelism = if preset\.mode == "disabled" \{\s*0\s*\}/s);
  assert.match(policy, /let max_workers = if preset\.mode == "disabled" \{ 0 \}/);
  assert.match(policy, /let max_in_flight = if preset\.mode == "disabled" \{\s*0\s*\}/s);

  const configSchema = readJson('schemas', 'mcpace-config.schema.json');
  assert.equal(configSchema.$defs.executionPolicy.properties.maxWorkers.minimum, 0);
  assert.equal(configSchema.$defs.executionPolicy.properties.maxInFlightPerWorker.minimum, 0);
  assert.equal(configSchema.$defs.serverPolicy.properties.maxInFlightPerWorker.minimum, 0);
});


test('upstream execution honors disabled runtime policy, not only MCP settings enabled flags', () => {
  const upstreamRoot = read('src', 'upstream.rs');
  const serverConfig = read('src', 'upstream', 'server_config.rs');

  assert.match(upstreamRoot, /struct UpstreamServerPolicy \{[\s\S]*runtime_enabled: bool,[\s\S]*tool_policies:/);
  assert.match(serverConfig, /fn server_policy_is_disabled\(raw_server: &JsonValue\) -> bool/);
  for (const field of [
    '\["policy", "startupStrategy"\]',
    '\["policy", "routingGroup"\]',
    '\["policy", "concurrencyPolicy"\]',
    '\["policy", "scopeClass"\]',
    '\["policy", "maxWorkers"\]',
    '\["execution", "mode"\]',
    '\["execution", "maxWorkers"\]',
  ]) {
    assert.match(serverConfig, new RegExp(field), `disabled policy helper must inspect ${field}`);
  }
  assert.match(serverConfig, /let runtime_enabled = !server_policy_is_disabled\(raw_server\);/);
  assert.match(serverConfig, /runtime_enabled,/);
  assert.match(serverConfig, /server is disabled by MCPace runtime policy/);
});


test('server profiles and dashboard preserve disabled servers as zero-capacity/offline', () => {
  const loader = read('src', 'server', 'loader.rs');
  const dashboard = read('src', 'dashboard.rs');
  const overview = read('src', 'dashboard', 'overview.rs');
  const html = read('src', 'dashboard', 'index.html');

  assert.match(loader, /fn runtime_policy_disabled\(/);
  assert.match(loader, /let base_effective_enabled = profile_enabled && source_enabled && platform_supported;/);
  assert.match(loader, /let effective_enabled = base_effective_enabled\s*&& !runtime_policy_disabled\(/s);
  assert.match(loader, /let effective_enabled = source_record\.enabled\s*&& !runtime_policy_disabled\(/s);

  assert.match(dashboard, /if mode != "disabled" \{/);
  assert.doesNotMatch(dashboard, /mode == "disabled"[\s\S]*push_positive_usize_arg/);
  assert.match(overview, /if current == "disabled" \{[\s\S]*max_workers: 0,[\s\S]*max_in_flight_per_worker: 0,/);
  assert.match(html, /if \(serverMode\(server, instances\) === "disabled"\) return 0;/);
  assert.match(html, /overrides\.maxWorkers \?\? maxWorkers\(server, related\)/);
  assert.match(html, /firstDefined\(result\.maxWorkers, execution\.maxWorkers, payload\.maxWorkers, server\.maxWorkers, 1\)/);
});

test('launcher package specs feed internal policy classification without trusting display names', () => {
  const loader = read('src', 'server', 'loader.rs');
  const signals = sliceFn(loader, 'source_signals', 'infer_source_type');
  const rawArg = sliceFn(loader, 'raw_arg_is_semantic_signal', 'looks_like_launcher_package_signal');
  const launcher = sliceFn(loader, 'looks_like_launcher_package_signal', 'infer_generic_source_policy');

  assert.match(signals, /let\s+_identity\s*=\s*\(normalized_name,\s*display_name\)/);
  assert.match(signals, /command_semantic_signal\(command\)/);
  assert.match(signals, /args\.join\(" "\)/);
  assert.doesNotMatch(signals, /format!\(\s*"\{\} \{\} \{\} \{\} \{\}"/);
  assert.match(rawArg, /looks_like_launcher_package_signal\(&normalized\)/);
  assert.match(launcher, /value\.starts_with\('@'\)/);
  assert.match(launcher, /value\.contains\("mcp"\)/);
  assert.match(launcher, /value\.starts_with\("http:\/\/"\)/);
  assert.match(loader, /_ if looks_like_launcher_package_signal\(&command_name\) => command_name,/);
});

test('legacy disabled flags win over enabled flags and toggles write consistent state', () => {
  const loader = read('src', 'server', 'loader.rs');
  const sourceEnabled = sliceFn(loader, 'source_enabled', 'supported_transports_for_source_type');
  const writer = read('src', 'mcp_sources', 'write.rs');

  const disabledIndex = sourceEnabled.indexOf('get("disabled")');
  const enabledIndex = sourceEnabled.indexOf('get("enabled")');
  assert.ok(disabledIndex > -1, 'source_enabled must inspect disabled');
  assert.ok(enabledIndex > -1, 'source_enabled must inspect enabled');
  assert.ok(disabledIndex < enabledIndex, 'disabled must be evaluated before enabled');
  assert.match(sourceEnabled, /return false;/);

  assert.match(writer, /let raw_disabled = server_object\.get\("disabled"\)/);
  assert.match(writer, /previous_enabled = raw_enabled\.or_else\(\|\| raw_disabled\.map\(\|disabled\| !disabled\)\);/);
  assert.match(writer, /server_object\.remove\("disabled"\);/);
  assert.match(writer, /server_object\.insert\("disabled"\.to_string\(\), JsonValue::bool\(true\)\);/);
});

test('server add remote URLs validate authority shape instead of only scheme prefix', () => {
  const helpers = read('src', 'mcp_sources', 'write_helpers.rs');
  const validator = sliceFn(helpers, 'validate_remote_mcp_url', 'validate_remote_mcp_authority');
  const authority = sliceFn(helpers, 'validate_remote_mcp_authority', 'validate_remote_mcp_host');

  assert.match(validator, /trimmed\.contains\('#'\)/);
  assert.match(validator, /split\(\|ch\| ch == '\/' \|\| ch == '\?'\)/);
  assert.match(validator, /validate_remote_mcp_authority\(authority\)/);
  assert.match(authority, /authority\.contains\('@'\)/);
  assert.match(authority, /authority\.starts_with\('\['\)/);
  assert.match(authority, /authority\.matches\(':'\)\.count\(\) > 1/);
  assert.match(helpers, /fn validate_remote_mcp_port\(/);
  assert.match(helpers, /parse::<u16>\(\)/);
  assert.match(helpers, /filter\(\|value\| \*value > 0\)/);
});

test('key-value parsing rejects silent overwrites and unsafe HTTP header values', () => {
  const helpers = read('src', 'mcp_sources', 'write_helpers.rs');
  const parser = helpers.slice(helpers.indexOf('fn parse_key_value_pairs('), helpers.indexOf('pub(super) fn validate_env_name'));
  assert.ok(parser.length > 0, 'missing parse_key_value_pairs body');

  assert.match(parser, /parsed\.contains_key\(key\)/);
  assert.match(parser, /contains duplicate key/);
  assert.match(parser, /flag_name == "--header" && !validate_http_header_value\(value\)/);
  assert.match(helpers, /fn validate_http_header_value\(value: &str\) -> bool/);
  assert.match(helpers, /byte == b' ' \|\| \(0x21\.\.=0x7e\)\.contains\(&byte\)/);
});

test('dashboard and public URL boundaries reject non-visible or ambiguous authorities', () => {
  const httpBoundary = read('src', 'dashboard', 'http_boundary.rs');
  const runtimepaths = read('src', 'runtimepaths.rs');

  assert.match(httpBoundary, /pub\(super\) fn is_valid_http_header_value/);
  assert.match(httpBoundary, /byte == b' ' \|\| \(0x21\.\.=0x7e\)\.contains\(&byte\)/);

  assert.match(runtimepaths, /fn valid_public_url_authority\(authority: &str\) -> bool/);
  assert.match(runtimepaths, /trimmed\.contains\('#'\)/);
  assert.match(runtimepaths, /authority\.contains\('@'\)/);
  assert.match(runtimepaths, /authority\.matches\(':'\)\.count\(\) > 1/);
  assert.match(runtimepaths, /fn valid_public_port\(port: &str\) -> bool/);
  assert.match(runtimepaths, /filter\(\|value\| \*value > 0\)/);
});

test('workspace path matching uses lexical containment and keeps Windows drive roots routable', () => {
  const pathing = read('src', 'client', 'pathing.rs');
  const containment = sliceFn(pathing, 'path_is_within', 'path_compare_key');
  const lexical = pathing.slice(pathing.indexOf('fn lexical_path_key('), pathing.indexOf('pub(super) fn normalize('));
  assert.ok(lexical.length > 0, 'missing lexical_path_key body');

  assert.match(containment, /let path_key = path_compare_key\(path\);/);
  assert.match(containment, /let root_key = path_compare_key\(root\);/);
  assert.doesNotMatch(containment, /trim_trailing_separator/);
  assert.match(lexical, /match part \{[\s\S]*"" \| "\." => \{\}[\s\S]*"\.\." =>/);
  assert.match(lexical, /parts\.pop\(\);/);
  assert.match(pathing, /fn path_containment_uses_lexical_segments_not_raw_prefixes/);
  assert.match(pathing, /fn path_containment_handles_windows_drive_roots_case_insensitively/);
  assert.match(pathing, /bytes\.len\(\) >= 2 && bytes\[1\] == b':'/);
});

test('unsupported-platform servers are non-routable even if stale metadata says enabled', () => {
  const plan = read('src', 'client', 'plan.rs');
  const nonRoutable = sliceFn(plan, 'server_is_not_routable', 'resolve_upstream_transport');
  assert.match(nonRoutable, /!record\.platform_supported/);
  assert.ok(
    nonRoutable.indexOf('!record.platform_supported') < nonRoutable.indexOf('!record.effective_enabled'),
    'platform support must be checked before trusting effectiveEnabled snapshots',
  );
});

test('command-like package inference keeps launcher option values from becoming packages', () => {
  const autoInstall = read('src', 'mcp_autoinstall.rs');
  const firstArg = sliceFn(autoInstall, 'first_non_option_arg', 'docker_image_arg');

  assert.match(firstArg, /inline_package_option_value\(arg\)/);
  assert.match(firstArg, /launcher_option_selects_package\(arg\)/);
  assert.match(firstArg, /launcher_option_takes_value\(arg\)/);
  assert.match(firstArg, /--registry/);
  assert.match(firstArg, /--from/);
  assert.match(autoInstall, /command_like_install_prefers_package_flags_for_identity/);
  assert.match(autoInstall, /command_like_install_skips_value_options_before_package/);
});

test('runtime profile and policy keys share MCP source-name normalization', () => {
  const profile = read('src', 'profile.rs');
  const loader = read('src', 'server', 'loader.rs');
  const upstreamConfig = read('src', 'upstream', 'server_config.rs');

  assert.match(profile, /use crate::mcp_sources;/);
  assert.match(profile, /mcp_sources::normalize_server_name\(server_name\)/);
  assert.doesNotMatch(profile, /overrides\.insert\(server_name\.trim\(\)\.to_ascii_lowercase\(\)/);

  assert.match(loader, /let normalized_name = mcp_sources::normalize_server_name\(name\);/);
  assert.doesNotMatch(loader, /let normalized_name = name\.trim\(\)\.to_ascii_lowercase\(\);/);

  assert.match(upstreamConfig, /let normalized_server_name = mcp_sources::normalize_server_name\(server_name\);/);
  assert.match(upstreamConfig, /server_overrides[\s\S]*\.get\(&normalized_server_name\)/);
  assert.doesNotMatch(upstreamConfig, /server_name\.trim\(\)\.to_ascii_lowercase\(\)/);
});
