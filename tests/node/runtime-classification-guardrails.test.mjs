import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { repoRoot } from "../../scripts/lib/project-metadata.mjs";

const read = (...parts) =>
	fs.readFileSync(path.join(repoRoot, ...parts), "utf8");

test("runtime signal inference avoids substring false positives for random MCP servers", () => {
	const loader = read("src", "server", "loader.rs");

	assert.match(loader, /fn signal_tokens/);
	assert.match(loader, /fn has_any_token/);
	assert.match(loader, /let code_hosting_api = has_any_token/);
	assert.match(loader, /&& !code_hosting_api/);
	assert.doesNotMatch(
		loader,
		/haystack\.contains\("git"\) \|\| haystack\.contains\("repository"\)/,
	);
	assert.doesNotMatch(
		loader,
		/mutation_terms\.iter\(\)\.any\(\|term\| haystack\.contains\(term\)\)/,
	);
});

test("server loading uses the canonical execution policy for worker limits", () => {
	const loader = read("src", "server", "loader.rs");
	const execution = read("src", "execution.rs");

	assert.match(loader, /let max_workers = execution\.worker_limit\(\)/);
	assert.doesNotMatch(loader, /fn infer_max_workers/);
	assert.match(
		execution,
		/ExecutionMode::Serialized => \(vec!\[\], 10_000, "sticky", 0, 1, 1\)/,
	);
	assert.match(
		execution,
		/ExecutionMode::ProjectIsolated => \{[\s\S]*"sticky-project", 0, 1, 1/,
	);
});

test("stateless detection is not confused with low worker limits", () => {
	const discovery = read("src", "adapter", "discovery.rs");
	const serverIsStateful = discovery.slice(
		discovery.indexOf("fn server_is_stateful"),
		discovery.indexOf("fn server_requires_serialization"),
	);

	assert.match(serverIsStateful, /record\.runtime_type != "stateless"/);
	assert.doesNotMatch(serverIsStateful, /parallelism_limit <= 1/);
});

test("runtime classifier separates read-only browser observation from browser automation", () => {
	const loader = read("src", "server", "loader.rs");

	assert.match(loader, /browser_observation_only/);
	assert.match(
		loader,
		/signals\.insert\("browser-observation"\.to_string\(\)\)/,
	);
	assert.match(loader, /state_binding: "host-readonly"/);
	assert.match(loader, /effect_class: "read-only"/);
	assert.match(loader, /host-readonly-pool/);

	const scenario = read(
		"eval",
		"fixtures",
		"runtime",
		"random-npm-browser-tabs-readonly.json",
	);
	assert.match(scenario, /"browser-observation"/);
	assert.match(scenario, /"effectClass": "read-only"/);
	assert.match(scenario, /"concurrencyPolicy": "multi-reader"/);
});

test("random metadata guardrails avoid broad browser and SDK false positives", () => {
	const loader = read("src", "server", "loader.rs");
	assert.match(loader, /runnable_server_marker/);
	assert.match(loader, /browser_control_action/);
	assert.equal(
		loader.includes(
			'"browser",\n            "browsers",\n            "screenshot"',
		),
		false,
	);
	assert.match(loader, /boondmanager/);
	assert.match(loader, /rancher/);
});

test("runtime classifier does not let broad dependency words override provider evidence", () => {
	const loader = read("src", "server", "loader.rs");

	assert.match(loader, /browser_observation_surface/);
	assert.match(loader, /&& !signals\.contains\("network-or-external-api"\)/);
	assert.match(loader, /&& !signals\.contains\("credentials-or-auth"\)/);
	assert.match(loader, /"contentful"/);
	assert.match(loader, /"wordpress"/);
	assert.equal(loader.includes('"typescript",\n            "tsserver"'), false);
	assert.equal(loader.includes('"code",\n            "code-index"'), false);
});
