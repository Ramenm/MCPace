import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";

const repoRoot = path.resolve(import.meta.dirname, "..", "..");
const read = (relativePath) =>
	fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const readJson = (relativePath) => {
	try {
		return JSON.parse(read(relativePath));
	} catch (error) {
		assert.fail(`${relativePath} must contain valid JSON: ${error.message}`);
	}
};

const runtime = read("src/upstream/lease_runtime.rs");
const app = [
	read("src/dashboard/frontend/app.js"),
	read("src/dashboard/frontend/app.runtime.js"),
].join("\n");
const product = read("src/dashboard/frontend/product.js");
const productCss = read("src/dashboard/frontend/product.css");
const compatibilityLab = readJson(
	"eval/official-server-live-probe-2026-07-11.json",
);

function compactWhitespace(value) {
	return value.replace(/\s+/g, " ");
}

test("tool audit captures latency, payload size, and explicitly separated token evidence", () => {
	assert.match(runtime, /mcpace\.toolAuditMetrics\.v1/);
	for (const field of [
		"queueDurationMs",
		"upstreamDurationMs",
		"totalDurationMs",
		"requestBytes",
		"responseBytes",
		"estimatedInputTokens",
		"estimatedOutputTokens",
		"estimatedTotalTokens",
		"reportedInputTokens",
		"reportedOutputTokens",
		"reportedTotalTokens",
		"tokenUsageSource",
	]) {
		assert.match(
			runtime,
			new RegExp(`"${field}"`),
			`${field} must remain part of the audit contract`,
		);
	}
	assert.match(runtime, /tokenEstimateMethod/);
	assert.match(runtime, /utf8-bytes-div-4/);
	assert.match(runtime, /extract_reported_token_usage/);
	assert.match(runtime, /upstream_result/);
	assert.match(runtime, /request_context/);
	assert.doesNotMatch(runtime, /reportedTotalTokens[^\n]*estimatedTotalTokens/);
});

test("single and pooled tool-call paths expose the same observability envelope", () => {
	assert.ok(
		(
			runtime.match(
				/"observability"\.to_string\(\),\s*JsonValue::object\(metrics\.log_fields\(\)\)/g,
			) || []
		).length >= 4,
	);
	assert.ok(
		(runtime.match(/log_tool_call_audit\(/g) || []).length >= 13,
		"success and failure branches should all produce audit entries",
	);
	assert.ok(
		(runtime.match(/log_tool_batch_audit\(/g) || []).length >= 12,
		"batch success and failure branches should all produce audit entries",
	);
});

test("dashboard retains a useful but explicitly bounded audit window", () => {
	assert.match(app, /\/api\/operations\?limit=5000/);
	assert.match(app, /\/api\/logs\?tail=500/);
	assert.match(product, /active and rotated local log files/);
	assert.match(product, /500-entry log-tail fallback/);
	assert.match(product, /not guaranteed lifetime totals/);
	assert.match(product, /retained window/i);
});

test("usage UI distinguishes measured, reported, and estimated values", () => {
	const source = compactWhitespace(product);
	assert.match(source, /Usage & activity/);
	assert.match(source, /Measured, reported, and estimated are never merged/);
	assert.match(source, /Reported tokens/);
	assert.match(source, /Payload estimate/);
	assert.match(source, /optional usage metadata/);
	assert.match(source, /Approximation from serialized request\/response size/);
	assert.match(source, /tokenEstimateMethod/);
	assert.doesNotMatch(source, /exact token usage from every MCP server/i);
	assert.doesNotMatch(source, /billing tokens/i);
});

test("usage can be investigated by server, tool, client, project, and event", () => {
	assert.match(
		product,
		/\[\s*\[["']live["'],\s*["']Live now["']\],\s*\[["']overview["'],\s*["']Overview["']\],\s*\[["']tools["'],\s*["']Tools["']\],\s*\[["']servers["'],\s*["']Servers["']\],\s*\[["']events["'],\s*["']Events["']\],?\s*\]/,
	);
	assert.match(
		product,
		/const serverGroups = groupAudits\(records, \(?record\)? => record\.server\)/,
	);
	assert.match(
		product,
		/const toolGroups = groupAudits\(records, \(?record\)? => record\.tools\)/,
	);
	assert.match(
		product,
		/const clientGroups = groupAudits\(\s*records\.filter\(\(?record\)? => record\.clientId\),\s*\(?record\)? => record\.clientId,?\s*\)/,
	);
	assert.match(
		product,
		/const projectGroups = groupAudits\(\s*records\.filter\(\(?record\)? => record\.projectRoot\),\s*\(?record\)? => record\.projectRoot,?\s*\)/,
	);
	for (const metric of [
		"successRate",
		"p50",
		"p95",
		"queueP95",
		"requestBytes",
		"responseBytes",
	]) {
		assert.match(product, new RegExp(metric));
	}
});

test("server workspace exposes source, switch, tools, usage, protection, and configuration", () => {
	assert.match(product, /data-server-action="toggle"/);
	assert.match(product, /data-mc-copy-value/);
	assert.match(product, /Open usage/);
	assert.match(product, /ensureCustomTab\(["']usage["'], ["']Usage["']/);
	assert.match(product, /server-dialog-panel-\$\{id\}/);
	assert.match(product, /Technical details/);
	assert.ok(product.includes("function toolRisk(tool = {})"));
	assert.match(product, /sourcePath/);
	assert.match(product, /sourceCommand/);
	assert.match(product, /sourceUrl/);
});

test("configuration provenance is visible for applications and MCPace runtime data", () => {
	assert.match(product, /Configuration map/);
	assert.match(product, /Where MCPace stores and reads state/);
	assert.match(product, /Configuration target/);
	assert.match(product, /Copy path/);
	assert.match(product, /Preview/);
	assert.match(product, /Apply/);
	assert.match(product, /Verify/);
	assert.match(product, /Restore/);
});

test("observability preferences are local, privacy-aware, and keyboard-visible", () => {
	assert.match(product, /mc-settings-tab-observability/);
	assert.match(product, /data-mc-token-estimates="show"/);
	assert.match(product, /data-mc-token-estimates="hide"/);
	assert.match(product, /data-mc-path-visibility="full"/);
	assert.match(product, /data-mc-context-labels="hide"/);
	assert.match(productCss, /mc-observability/);
	assert.match(productCss, /focus-visible/);
	assert.match(productCss, /forced-colors/);
	assert.match(productCss, /prefers-reduced-motion/);
});

test("server row enhancement is mutation-stable and does not self-trigger render loops", () => {
	assert.match(product, /const routeMetaSignature =/);
	assert.match(
		product,
		/routeMeta\.dataset\.mcSignature !== routeMetaSignature/,
	);
	assert.match(product, /const toggleSignature =/);
	assert.match(product, /toggle\.dataset\.mcSignature !== toggleSignature/);
	assert.doesNotMatch(product, /if \(routeMeta\) routeMeta\.innerHTML/);
	assert.match(
		product,
		/mc-row-source,\.mc-row-usage,\.mc-row-route-meta,\.mc-inline-server-toggle/,
	);
	assert.match(
		product,
		/sourceLabel && sourceLabel\.textContent !== ["']Server["']/,
	);
});

test("isolated compatibility evidence covers five materially different MCP servers", () => {
	assert.equal(
		compatibilityLab.schema,
		"mcpace.official-server-compatibility-lab.v1",
	);
	assert.equal(compatibilityLab.results.length, 5);
	assert.deepEqual(compatibilityLab.results.map((result) => result.id).sort(), [
		"everything",
		"filesystem",
		"memory",
		"playwright",
		"sequential-thinking",
	]);
	assert.ok(
		compatibilityLab.results.every(
			(result) => result.connectOk && result.errors.length === 0,
		),
	);
	assert.equal(
		compatibilityLab.results.find((result) => result.id === "filesystem")
			.toolCount,
		14,
	);
	assert.equal(
		compatibilityLab.results.find((result) => result.id === "memory").resources
			.count,
		1,
	);
	assert.equal(
		compatibilityLab.results.find((result) => result.id === "everything")
			.prompts.count,
		4,
	);
	assert.equal(
		compatibilityLab.results.find((result) => result.id === "playwright")
			.openWorldAnnotated,
		24,
	);
	assert.match(compatibilityLab.purpose, /isolated lab/i);
	assert.match(
		compatibilityLab.purpose,
		/not added to MCPace user configuration/i,
	);
});
