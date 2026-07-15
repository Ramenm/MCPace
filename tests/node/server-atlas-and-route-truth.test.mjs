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
const css = fs.readFileSync(
	path.join(root, "src/dashboard/frontend/product.css"),
	"utf8",
);
const appRender = fs.readFileSync(
	path.join(root, "src/dashboard/frontend/app.render.js"),
	"utf8",
);
const compact = (value) => value.replace(/\s+/g, " ");

test("server roster exposes four truthful evidence boundaries", () => {
	const source = compact(product);
	assert.match(source, /function serverLifecycleProfile/);
	assert.match(source, /id: ["']enabled["']/);
	assert.match(source, /id: ["']runtime["']/);
	assert.match(source, /id: ["']protocol["']/);
	assert.match(source, /id: ["']tools["']/);
	assert.match(source, /Definition is exposed to MCPace/);
	assert.match(source, /ownership may be idle/);
	assert.match(source, /not a trust or authorization decision/);
	assert.match(source, /A tools\/list result is retained/);
	assert.match(source, /function serverReadinessHeadline/);
	assert.match(css, /\.mc-row-lifecycle/);
	assert.match(css, /\.mc-life-step/);
});

test("server context joins client, project, latest tool, route, and configuration source", () => {
	const source = compact(product);
	assert.match(source, /function serverContextProfile/);
	assert.match(source, /primaryClient/);
	assert.match(source, /primaryProject/);
	assert.match(source, /latestOperation/);
	assert.match(source, /sourceShort/);
	assert.match(source, /function contextHeadline/);
	assert.match(source, /No client use retained/);
	assert.match(source, /Configuration source/);
	assert.match(css, /\.mc-row-context/);
	assert.match(css, /\.mc-row-context-source/);
});

test("capacity is presented as an understandable maximum rather than worker jargon alone", () => {
	const source = compact(product);
	assert.match(source, /function serverCapacityProfile/);
	assert.match(source, /One request at a time/);
	assert.match(source, /Up to \$\{formatNumber\(total\)\} concurrent/);
	assert.match(source, /worker\$\{workers === 1 \? ["']["'] : ["']s["']\} ×/);
	assert.match(source, /Capacity automatic/);
	assert.match(source, /No route ownership now/);
});

test("evidence freshness distinguishes recent, aging, stale, and unknown evidence", () => {
	const source = compact(product);
	assert.match(source, /function serverEvidenceFreshness/);
	assert.match(source, /Evidence recent/);
	assert.match(source, /Evidence today/);
	assert.match(source, /Evidence aging/);
	assert.match(source, /Evidence stale/);
	assert.match(source, /Evidence time unknown/);
	assert.match(source, /Not tested/);
});

test("route ribbon and connections view use observed calls and route ownership", () => {
	const source = compact(product);
	assert.match(source, /function renderRouteRibbon/);
	assert.match(source, /Observed routes/);
	assert.match(source, /Routes held now/);
	assert.match(source, /ownership may be idle/);
	assert.match(source, /function renderRouteMap/);
	assert.match(source, /OBSERVED CONNECTIONS/);
	assert.match(source, /Who used which MCP server/);
	assert.match(source, /Observed is not the same as available/);
	assert.match(source, /lastTool/);
	assert.match(source, /lastToolOk/);
	assert.match(css, /\.mc-route-ribbon/);
	assert.match(css, /\.mc-connection-card/);
});

test("integration filters support client, project, grouping, and local privacy", () => {
	const source = compact(product);
	assert.match(source, /integrationClient: ["']all["']/);
	assert.match(source, /integrationProject: ["']all["']/);
	assert.match(source, /integrationGroup: ["']none["']/);
	assert.match(source, /data-mc-integration-client/);
	assert.match(source, /data-mc-integration-project/);
	assert.match(source, /data-mc-integration-group/);
	assert.match(source, /state\.contextLabels !== ["']show["']/);
	assert.match(source, /state\.integrationClient = ["']all["']/);
	assert.match(source, /state\.integrationProject = ["']all["']/);
	assert.match(source, /Observed client hidden/);
	assert.match(source, /Observed project hidden/);
});

test("row quick view keeps common context inline and deep settings behind explicit actions", () => {
	const source = compact(product);
	assert.match(source, /expandedServer: null/);
	assert.match(source, /function serverPeekMarkup/);
	assert.match(source, /Readiness/);
	assert.match(source, /Who & where/);
	assert.match(source, /Runtime & capacity/);
	assert.match(source, /Next safe action/);
	assert.match(source, /function toggleServerPeek/);
	assert.match(source, /Quick view/);
	assert.match(css, /\.mc-row-peek/);
	assert.match(css, /\.mc-peek-section/);
});

test("server atlas has bounded responsive compositions and no double adaptive padding", () => {
	assert.match(css, /MCPACE_SERVER_ATLAS_START/);
	assert.match(css, /min-height: 84px !important/);
	assert.match(css, /min-height: 86px !important/);
	assert.match(css, /min-height: 150px !important/);
	assert.match(
		css,
		/grid-template-areas: "source source" "readiness readiness" "context health" "actions actions"/,
	);
	assert.match(
		css,
		/server-row\[data-adaptive-role="server-row"\][^{]*\{[^}]*padding: 0 !important/s,
	);
	assert.match(css, /@media \(max-width: 679px\)/);
	assert.match(css, /@media \(max-width: 419px\)/);
	assert.match(css, /\.mc-mobile-source-short/);
});

test("themes, motion, forced colors, and minimum interaction sizes remain supported", () => {
	assert.match(css, /data-mc-theme-changing="true"/);
	assert.match(css, /data-mc-motion="reduced"/);
	assert.match(css, /data-mc-motion="off"/);
	assert.match(css, /prefers-reduced-motion: reduce/);
	assert.match(css, /forced-colors: active/);
	assert.match(css, /min-height: 27px/);
	assert.match(css, /width: 30px; height: 32px/);
});

test("server atlas uses the retained operation window independently of Activity range", () => {
	const source = compact(product);
	assert.match(
		source,
		/const usageGroups = new Map\(\s*usageAnalytics\(auditRecords\(\)\)/,
	);
	assert.match(
		source,
		/const records = auditRecords\(\)\.filter\(\s*\(?record\)? => record\.server === server\.name,?\s*\)/,
	);
	assert.match(source, /auditRecordCache: null/);
	assert.match(source, /state\.auditRecordCache = null/);
	assert.match(source, /state\.serverModelCache = null/);
	assert.match(source, /renderChrome\(\)/);
});

test("compact route health exposes a human-readable maximum capacity", () => {
	const source = compact(product);
	assert.match(source, /function serverCapacityShort/);
	assert.match(source, /return `max \$\{formatNumber\(capacity\.total\)\}`/);
	assert.match(source, /Isolation · capacity/);
	assert.match(source, /serverCapacityShort\(model\)/);
});

test("mobile atlas composes the full operational card above the fold", () => {
	assert.match(css, /MCPACE_SERVER_ATLAS_COMPOSITION_REFINEMENT_START/);
	assert.match(
		css,
		/grid-template-areas: "source source" "readiness readiness" "context health" "actions actions"/,
	);
	assert.match(
		css,
		/server-evidence-cell > \.server-cell-label \{ display: none !important; \}/,
	);
	assert.match(css, /\.mc-results-status \{[^}]*white-space: nowrap/s);
	assert.match(css, /\.mc-row-peek-toggle::before \{ content: "View"/);
});

test("backend render completion explicitly resynchronizes the product shell", () => {
	assert.match(appRender, /mcpace:dashboard-rendered/);
	assert.match(
		product,
		/addEventListener\(["']mcpace:dashboard-rendered["'], \(\) =>\s*scheduleRender\(0\),?\s*\)/,
	);
});
