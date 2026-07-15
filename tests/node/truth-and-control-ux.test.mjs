import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(
	path.dirname(fileURLToPath(import.meta.url)),
	"../..",
);
const product = fs.readFileSync(
	path.join(root, "src/dashboard/frontend/product.js"),
	"utf8",
);
const actions = fs.readFileSync(
	path.join(root, "src/dashboard/frontend/app.actions.js"),
	"utf8",
);
const css = fs.readFileSync(
	path.join(root, "src/dashboard/frontend/product.css"),
	"utf8",
);
const dashboard = fs.readFileSync(path.join(root, "src/dashboard.rs"), "utf8");
const dashboardTests = fs.readFileSync(
	path.join(root, "src/dashboard/tests.rs"),
	"utf8",
);

test("home and setup use backend-owned configured-client and routing readiness", () => {
	assert.match(product, /overview\.dashboardFoundation/);
	assert.match(product, /foundationCounts\.clientConfigured === true/);
	assert.match(product, /model\.clientConfigured/);
	assert.match(
		product,
		/foundation\?\.schema === ["']mcpace\.dashboardFoundation\.v1["']/,
	);
	assert.match(
		product,
		/A discovered path is not treated as a verified connection/,
	);
	assert.doesNotMatch(product, /done: model\.clients\.length > 0/);
});

test("bounded activity excludes records without timestamps and explains excluded evidence", () => {
	assert.match(product, /return timestamp !== null && timestamp >= start/);
	assert.match(product, /excludedUnknownTimestamps/);
	assert.match(product, /undated audit entr/);
	assert.match(
		product,
		/model\.events\.filter\(\s*\(?event\)? => !rangeStart \|\| timestampInActivityRange\(event\.timestamp\),?\s*\)/,
	);
});

test("mixed batches are not falsely assigned as exact per-tool outcomes or latency", () => {
	assert.match(
		product,
		/const mixedBatch =\s*allocatedPerCall && record\.successCount > 0 && record\.failedCount > 0/,
	);
	assert.match(
		product,
		/successShare = allocatedPerCall\s*\? record\.successCount \/ Math\.max\(1, record\.callCount\)/,
	);
	assert.match(product, /outcomeEstimatedCalls/);
	assert.match(product, /latencyEstimatedCalls/);
	assert.match(product, /proportional per-tool estimates/);
	assert.match(
		product,
		/Mixed batch outcomes or latency were proportionally allocated/,
	);
});

test("tool UI follows MCP human-readable title precedence and keeps technical identity visible", () => {
	assert.match(
		product,
		/tool\?\.title \|\| annotations\.title \|\| toolTechnicalName\(tool\)/,
	);
	assert.match(product, /class="mc-tool-title"/);
	assert.match(product, /summary\.differs \? `<code>/);
	assert.match(product, /Server-provided annotation; not a trust decision/);
	assert.match(css, /\.mc-tool-card \.mc-tool-title/);
});

test("fallback audit classification avoids broad token false positives and preserves queue timeouts", () => {
	assert.match(
		product,
		/access token\|bearer token\|missing token\|token expired/,
	);
	assert.doesNotMatch(
		product,
		/\/auth\|credential\|token\|unauthorized\|forbidden\//,
	);
	assert.match(
		product,
		/failureStage: \/queue\|lease\/\.test\(source\) \? ["']queue["'] : ["']upstream["']/,
	);
	assert.match(product, /unexpected token/);
});

test("free-text add detection does not classify every phrase containing whitespace as a command", () => {
	assert.match(product, /const launcher =/);
	assert.match(product, /const shellSyntax =/);
	assert.match(product, /const executableWithFlag =/);
	assert.doesNotMatch(product, /\|\| \/\\s\/\.test\(raw\)/);
	assert.match(product, /Likely package name or search phrase/);
	assert.match(product, /You can always choose another option below/);
});

test("server removal uses backend dry-run, exact source path, typed confirmation, and no false undo", () => {
	assert.match(dashboard, /\("POST", "\/api\/actions\/server-remove"\)/);
	assert.match(dashboard, /fn write_server_remove_action/);
	assert.match(dashboard, /"--dry-run"\.to_string\(\)/);
	assert.match(
		dashboardTests,
		/dashboard_server_remove_action_previews_then_removes_exact_source/,
	);
	assert.match(
		actions,
		/runServerAction\(\s*["']server-remove["'],\s*payload,\s*control,\s*["']Reviewing…["']/,
	);
	assert.match(actions, /removalPlan: plan/);
	assert.match(
		actions,
		/const previewPath = String\(plan\.path \|\| payload\.settingsPath/,
	);
	assert.match(
		actions,
		/if \(previewPath\) removePayload\.settingsPath = previewPath/,
	);
	assert.match(product, /data-mc-remove-confirm/);
	assert.match(product, /typedConfirm\.value !== \(names\[0\] \|\| ["']["']\)/);
	assert.match(product, /This action has no dashboard Undo/);
	assert.match(product, /data-server-action="remove"/);
	assert.match(css, /\.mc-server-danger-zone/);
	assert.match(css, /\.mc-destructive-confirm/);
});

test("failed activity events expose explicit error kind and failure stage", () => {
	assert.match(
		product,
		/function activityFailureClassificationMarkup\(audit\)/,
	);
	assert.match(product, /aria-label="Failure classification"/);
	assert.match(product, /audit\.errorKind/);
	assert.match(product, /audit\.failureStage/);
	assert.match(css, /\.mc-event-classification/);
});
