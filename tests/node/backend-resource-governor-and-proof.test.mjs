import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { repoRoot } from "../../scripts/lib/project-metadata.mjs";

function read(relativePath) {
	return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function scripts() {
	const source = read("package.json");
	try {
		return JSON.parse(source).scripts || {};
	} catch (error) {
		assert.fail(`package.json is not valid JSON: ${error?.message || error}`);
	}
}

test("backend exposes a global resource governor before route admission work", () => {
	const dashboard = read("src/dashboard.rs");
	const governor = read("src/dashboard/governor.rs");
	const overview = read("src/dashboard/overview.rs");
	const resources = read("src/resources.rs");

	assert.match(dashboard, /mod governor;/);
	assert.match(dashboard, /resource_governor:\s*GlobalResourceGovernor/);
	assert.match(dashboard, /try_enter_request\(\)/);
	assert.match(dashboard, /http\.resource_governor_rejected/);
	assert.match(dashboard, /503 Service Unavailable/);
	assert.match(governor, /mcpace\.globalResourceGovernor\.v1/);
	assert.match(governor, /activeRequestLimit/);
	assert.match(governor, /rssSoftBytes/);
	assert.match(governor, /fdSoftLimit/);
	assert.match(governor, /threadSoftLimit/);
	assert.match(governor, /http\.server\.active_requests/);
	assert.match(resources, /ENV_GLOBAL_ACTIVE_REQUEST_LIMIT/);
	assert.match(resources, /ENV_PROCESS_RSS_SOFT_BYTES/);
	assert.match(resources, /ENV_PROCESS_FD_SOFT_LIMIT/);
	assert.match(resources, /ENV_PROCESS_THREAD_SOFT_LIMIT/);
	assert.match(
		overview,
		/"processResource", process_resource_snapshot\.clone\(\)/,
	);
	assert.match(overview, /"resourceGovernor", resource_governor_snapshot/);
});

test("HTTP latency snapshots expose OpenTelemetry-compatible aliases without renaming MCPace fields", () => {
	const latency = read("src/dashboard/latency.rs");
	const governor = read("src/dashboard/governor.rs");
	assert.match(latency, /otelAliases/);
	assert.match(latency, /http\.server\.request\.duration/);
	assert.match(latency, /http\.server\.request\.body\.size/);
	assert.match(latency, /http\.request\.header\.size/);
	assert.match(governor, /http\.server\.active_requests/);
});

test("dashboard browser lifecycle proof prevents tab wake-up refresh storms", () => {
	const lifecycle = [
		read("src/dashboard/frontend/app.js"),
		read("src/dashboard/frontend/app.boot.js"),
	].join("\n");
	const proof = read("scripts/browser-lifecycle-proof.mjs");
	const packageScripts = scripts();

	assert.match(lifecycle, /LIFECYCLE_RESUME_MIN_INTERVAL_MS/);
	assert.match(lifecycle, /document\.wasDiscarded/);
	assert.match(lifecycle, /document\.addEventListener\("freeze"/);
	assert.match(lifecycle, /document\.addEventListener\("resume"/);
	assert.match(lifecycle, /window\.addEventListener\("pageshow"/);
	assert.match(lifecycle, /state\.lifecycle\.frozen/);
	assert.match(lifecycle, /refreshDashboard\(\{ reason: "resume" \}\)/);
	assert.doesNotMatch(lifecycle, /reason: "resume"[^\n]+force: true/);
	assert.match(proof, /mcpace\.browserLifecycleProof\.v1/);
	assert.equal(
		packageScripts["proof:browser-lifecycle"],
		"node scripts/browser-lifecycle-proof.mjs",
	);

	const executed = spawnSync(
		process.execPath,
		[path.join(repoRoot, "scripts/browser-lifecycle-proof.mjs"), "--json"],
		{ cwd: repoRoot, encoding: "utf8" },
	);
	assert.equal(executed.status, 0, executed.stderr || executed.stdout);
	let report;
	try {
		report = JSON.parse(executed.stdout);
	} catch (error) {
		assert.fail(
			`browser lifecycle proof did not emit valid JSON: ${error?.message || error}`,
		);
	}
	assert.equal(report.ok, true);
	assert.equal(
		report.checks.every((check) => check.pass),
		true,
	);
});

test("autostart verify records supervised user-level state across platforms", () => {
	const service = read("src/service.rs");
	const config = read("src/service/config.rs");
	const verify = read("src/service/verify.rs");
	const surface = `${service}\n${config}\n${verify}`;
	assert.match(service, /"appliedState", service_applied_state_json\(config\)/);
	assert.match(service, /mcpace\.autostartAppliedState\.v1/);
	assert.match(verify, /visibleAs/);
	assert.match(surface, /MCPace Agent/);
	assert.match(surface, /systemd user service/);
	assert.match(service, /LinuxLaunchMode::Systemd/);
	assert.match(surface, /Windows current-user Run registry/);
	assert.match(surface, /LaunchAgent/);
	assert.match(verify, /supervisedByMcpaceAgent/);
	assert.match(verify, /activatedImmediately/);
	assert.doesNotMatch(config, /linux-xdg-autostart/);
});

test("default up transfers initial runtime ownership to the user supervisor", () => {
	const setup = read("src/setup.rs");
	const service = read("src/service.rs");
	const config = read("src/service/config.rs");
	assert.ok(
		setup.indexOf("let service_install = if parsed.install_service") <
			setup.indexOf("let mut serve_args = vec!"),
		"autostart activation must precede the fallback serve start",
	);
	assert.match(service, /start_user_supervisor_after_enable\(config\)/);
	assert.match(config, /stop_runtime_before_supervisor_start\(config\)/);
	assert.match(config, /vec!\["--user", "daemon-reload"\]/);
	assert.match(config, /vec!\["--user", "start", unit\.as_str\(\)\]/);
	assert.match(config, /spawn_detached_no_window/);
	assert.match(config, /process_image_is\(pid, "mcpace-agent-launcher\.exe"\)/);
	assert.match(config, /macos_launch_agent::start\(super::APP_NAME\)/);
	assert.match(config, /stop_runtime_before_supervisor_start\(config\)/);
	const macosSupervisor = read("src/macos_launch_agent.rs");
	assert.match(macosSupervisor, /"bootstrap"/);
	assert.match(macosSupervisor, /"kickstart"/);
	assert.match(macosSupervisor, /"bootout"/);
	assert.match(config, /if supervisor_runtime_ready\(&endpoint\)/);
	assert.match(
		config,
		/healthy && platform_user_supervisor_is_active\(&endpoint\.root\)/,
	);
});

test("Windows supervisor acknowledges stop before serve restart can spawn", () => {
	const launcher = read("src/bin/mcpace-agent-launcher.rs");
	const serve = read("src/serve.rs");
	assert.match(launcher, /struct SupervisorRegistration/);
	assert.match(
		launcher,
		/let waited = loop[\s\S]*child\.try_wait\(\)[\s\S]*stop_requested\(stop_marker\)[\s\S]*child\.kill\(\)[\s\S]*child\.wait\(\)[\s\S]*acknowledge_stop_request\(stop_marker\)/,
	);
	assert.match(
		serve,
		/request_agent_supervisor_stop\(&canonical_root\)[\s\S]*stop_existing_serve\(&canonical_root\)[\s\S]*wait_for_agent_supervisor_stop\(&canonical_root\)[\s\S]*run_start_after_supervisor_stop\([\s\S]*clear_agent_supervisor_stop_request\(&canonical_root\)/,
	);
	assert.match(
		serve,
		/run_start_impl\(parsed, default_root, stdout, stderr, false\)/,
	);
	assert.match(
		serve,
		/already healthy with different settings; refusing to start a duplicate runtime/,
	);
	assert.match(
		serve,
		/fn run_stop[\s\S]*wait_for_agent_supervisor_stop\(&canonical_root\)[\s\S]*clear_agent_supervisor_stop_request\(&canonical_root\)/,
	);
	assert.match(
		serve,
		/let restart_with_supervisor = agent_supervisor_is_active/,
	);
	assert.match(
		serve,
		/if restart_with_supervisor \{[\s\S]*start_agent_supervisor\(&canonical_root\)/,
	);
});

test("static Rust guard and trusted-publish preflight are wired into CI scripts and release manifest", () => {
	const packageScripts = scripts();
	const manifest = read("release-manifest.json");
	const publishWorkflow = read(".github/workflows/publish-npm.yml");
	const trustPreflight = read("scripts/publish-trust-preflight.mjs");
	const rustGuard = read("scripts/rust-static-guard.mjs");
	const ciEntrypoint = read("scripts/check-ci.mjs");

	assert.equal(
		packageScripts["lint:rust-static"],
		"node scripts/rust-static-guard.mjs --json",
	);
	assert.match(packageScripts["lint:npm"], /lint:rust-static/);
	assert.equal(
		packageScripts["check:publish-trust"],
		"node scripts/publish-trust-preflight.mjs",
	);
	assert.equal(packageScripts["check:ci"], "node scripts/check-ci.mjs");
	assert.match(ciEntrypoint, /["']check:publish-trust["']/);
	assert.match(ciEntrypoint, /["']proof:browser-lifecycle["']/);
	assert.match(manifest, /scripts\/rust-static-guard\.mjs/);
	assert.match(manifest, /scripts\/browser-lifecycle-proof\.mjs/);
	assert.match(manifest, /scripts\/publish-trust-preflight\.mjs/);
	assert.match(publishWorkflow, /id-token:\s*write/);
	assert.match(publishWorkflow, /--provenance/);
	assert.match(publishWorkflow, /environment:\s*npm-publish/);
	assert.doesNotMatch(
		publishWorkflow,
		/NODE_AUTH_TOKEN|NPM_TOKEN|NPM_CONFIG_.*TOKEN/i,
	);
	assert.match(trustPreflight, /mcpace\.publishTrustPreflight\.v1/);
	assert.match(rustGuard, /mcpace\.rustStaticGuard\.v1/);
});
