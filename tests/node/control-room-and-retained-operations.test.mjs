import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";

const repoRoot = path.resolve(import.meta.dirname, "..", "..");
const read = (relativePath) =>
	fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const dashboard = read("src/dashboard.rs");
const operations = read("src/dashboard/operations.rs");
const operationsTests = read("src/dashboard/operations/tests.rs");
const runtime = read("src/upstream/lease_runtime.rs");
const app = [
	read("src/dashboard/frontend/app.js"),
	read("src/dashboard/frontend/app.runtime.js"),
].join("\n");
const product = read("src/dashboard/frontend/product.js");
const css = read("src/dashboard/frontend/product.css");
const protocol = read("src/mcp_protocol.rs");

function compact(value) {
	return value.replace(/\s+/g, " ");
}

test("retained operations endpoint merges the active and rotated audit logs safely", () => {
	assert.match(dashboard, /\("GET", "\/api\/operations"\)/);
	assert.match(dashboard, /DEFAULT_OPERATIONS_LIMIT/);
	assert.match(dashboard, /MAX_OPERATIONS_LIMIT/);
	assert.match(operations, /mcpace\.retainedOperations\.v1/);
	assert.match(operations, /active/);
	assert.match(operations, /archive/);
	assert.match(operations, /sort_by_key\(event_timestamp\)/);
	assert.match(operations, /parseErrors/);
	assert.match(operations, /truncated/);
	assert.match(dashboard, /bounded_query_usize/);
	assert.match(operations, /saturating_sub\(limit\)/);
	assert.match(
		operationsTests,
		/retained_operations_reads_archive_then_active_and_keeps_latest_limit/,
	);
});

test("frontend prefers retained operations and preserves a bounded fallback for older backends", () => {
	assert.match(app, /timedFetchJson\("\/api\/operations\?limit=5000"/);
	assert.match(app, /operations:/);
	assert.match(product, /function retainedOperations\(\)/);
	assert.match(product, /source: ["']api\/operations["']/);
	assert.match(product, /source: ["']api\/logs["']/);
	assert.match(product, /Fallback log tail/);
});

test("audit v2 gives every call a stable correlation key and a classified outcome", () => {
	for (const field of [
		"auditSchema",
		"callId",
		"requestKind",
		"outcome",
		"errorKind",
		"failureStage",
	]) {
		assert.match(runtime, new RegExp(`"${field}"`));
	}
	assert.match(runtime, /mcpace\.toolAudit\.v2/);
	assert.match(runtime, /TOOL_AUDIT_SEQUENCE/);
	assert.match(runtime, /next_tool_audit_id/);
	assert.match(runtime, /classify_tool_audit_outcome/);
	for (const outcome of [
		"success",
		"tool_error",
		"policy_denied",
		"authorization",
		"capacity",
		"timeout",
		"validation",
		"transport_error",
		"bridge_error",
	]) {
		assert.match(runtime, new RegExp(outcome));
	}
});

test("home and integrations expose system truth, action priority, scope, and bulk control", () => {
	const source = compact(product);
	assert.match(source, /System truth/);
	assert.match(source, /Follow a request through the whole MCP chain/);
	assert.match(source, /Action center/);
	assert.match(source, /systemActionItems/);
	assert.match(source, /data-mc-integration-scope/);
	assert.match(source, /data-mc-bulk-action/);
	assert.match(source, /selectedServers/);
	assert.match(source, /Remote/);
	assert.match(source, /credential-backed/);
});

test("server workspace separates capabilities, access, usage, and retained events", () => {
	const source = compact(product);
	for (const tab of ["capabilities", "access", "usage", "events"]) {
		assert.match(
			product,
			new RegExp(`ensureCustomTab\\(\\s*["']${tab}["'],`),
			`${tab} must stay in the inspector`,
		);
	}
	assert.match(
		source,
		/Only retained initialize or list evidence is marked measured/,
	);
	assert.match(source, /Reported by initialize evidence/);
	assert.match(source, /Not measured/);
	assert.match(source, /Credential references/);
	assert.match(source, /Configuration provenance/);
	assert.match(source, /Capability is not authorization/);
	assert.match(source, /data-mc-open-event/);
});

test("event detail and exports preserve correlation, latency stages, payload, and token evidence", () => {
	const source = compact(product);
	assert.match(source, /createEventDetailDialog/);
	assert.match(source, /Call ID/);
	assert.match(source, /failure stage/i);
	assert.match(source, /Queue/);
	assert.match(source, /Upstream/);
	assert.match(source, /Total/);
	assert.match(source, /Reported tokens/);
	assert.match(source, /Payload estimate/);
	assert.match(source, /exportActivity/);
	assert.match(source, /Export JSON/);
	assert.match(source, />CSV</);
});

test("client routes and add wizard disclose blast radius before mutation", () => {
	const source = compact(product);
	assert.match(source, /Separate observed use from potential access/);
	assert.match(
		source,
		/Observed routes require retained calls or active leases/,
	);
	assert.match(source, /Potential is not observed/);
	assert.match(source, /addExecutionPlan/);
	assert.match(source, /What MCPace will do/);
	assert.match(
		source,
		/Saving a source, enabling it, and running tools\/list are separate actions/,
	);
	assert.match(source, /Credential values are never rendered here/);
});

test("protocol readiness is honest about the stable target and future migration work", () => {
	const source = compact(product);
	assert.match(protocol, /CURRENT_PROTOCOL_VERSION: &str = "2025-11-25"/);
	assert.match(source, /Stable today, migration-visible tomorrow/);
	assert.match(source, /2025-11-25/);
	assert.match(source, /2026-07-28 preview/);
	assert.match(source, /not silently enabled/i);
	assert.match(source, /Dual-era version routing/);
	assert.match(source, /Extension negotiation/);
	assert.match(source, /Logging migration/);
	assert.match(source, /Authorization hardening/);
	assert.doesNotMatch(source, /2026-07-28 supported/i);
});

test("control-room styling remains responsive, keyboard-visible, and forced-colors aware", () => {
	for (const className of [
		"mc-system-chain",
		"mc-action-center",
		"mc-bulk-bar",
		"mc-capability-list",
		"mc-access-grid",
		"mc-event-detail-dialog",
		"mc-protocol-readiness-card",
	]) {
		assert.match(css, new RegExp(className));
	}
	assert.match(css, /focus-visible/);
	assert.match(css, /forced-colors/);
	assert.match(css, /prefers-reduced-motion/);
	assert.match(css, /@media \(max-width: 700px\)/);
});
