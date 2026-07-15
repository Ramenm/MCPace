import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const js = readFileSync(
	new URL("../../src/dashboard/frontend/product.js", import.meta.url),
	"utf8",
);
const css = readFileSync(
	new URL("../../src/dashboard/frontend/product.css", import.meta.url),
	"utf8",
);

test("Server Atlas keeps Servers and Connections outside the closing View menu", () => {
	assert.match(js, /mc-atlas-persistent-controls/);
	assert.match(js, /data-atlas-persistent-mode="servers"/);
	assert.match(js, /data-atlas-persistent-mode="connections"/);
	assert.match(js, /writeSetting\(STORAGE_VIEW/);
});

test("Server Atlas separates readiness from last-operation outcome", () => {
	assert.match(js, /Last operation/);
	assert.match(js, /without invalidating protocol and tools readiness/);
	assert.match(css, /mc-atlas-fact-guide/);
});

test("Server Atlas presents observed, current, and unobserved connections separately", () => {
	assert.match(js, /Current route ownership/);
	assert.match(js, /Observed use/);
	assert.match(js, /Enabled, not observed/);
	assert.match(js, /Who reaches which MCP server/);
});

test("Server Atlas bounds density without removing information", () => {
	assert.match(js, /rowCount >= 10/);
	assert.match(css, /data-atlas-density="compact"/);
	assert.match(css, /--mc-atlas-row-min: 76px/);
});

test("Server Atlas retains monochrome and reduced-motion behavior", () => {
	assert.match(css, /Monochrome remains structural/);
	assert.match(css, /prefers-reduced-motion: reduce/);
	assert.match(css, /border-style: dashed/);
});

test("Server Atlas ignores only owned observer mutations", () => {
	assert.match(js, /record\.type === ["']attributes["']/);
	assert.match(js, /changedNodes\.length > 0 && changedNodes\.every\(owned\)/);
	assert.match(js, /records\.every\(mutationIsOwned\)/);
	assert.match(js, /mc-atlas-generated-connections/);
	assert.match(js, /requestAnimationFrame\(refresh\)/);
});

test("Server Atlas pressed state has a matching persistent visual selector", () => {
	assert.match(
		css,
		/mc-atlas-surface-switch button\[aria-pressed=["']true["']\]/,
	);
	assert.doesNotMatch(
		css,
		/mc-atlas-surface-switch button\[aria-selected=["']true["']\]/,
	);
});

test("Server Atlas supersedes duplicate native view controls", () => {
	assert.match(js, /deDuplicateNativeViewControls/);
	assert.match(js, /atlasSuperseded/);
	assert.match(css, /data-atlas-superseded/);
});

test("Server Atlas delegates to the canonical integration list and route map", () => {
	assert.match(js, /data-mc-integration-layout=\\?"map\\?"/);
	assert.match(js, /data-mc-integration-layout=\\?"list\\?"/);
	assert.match(js, /data-mc-route-map/);
	assert.match(js, /data-mc-integration-list-shell/);
	const rowCandidateSource = js.slice(
		js.indexOf("function rowCandidates"),
		js.indexOf("function exactButton"),
	);
	assert.doesNotMatch(rowCandidateSource, /\[data-server-name\]/);
});

test("Server Atlas opens a server from Connections without losing context", () => {
	assert.match(js, /data-atlas-open-server/);
	assert.match(js, /atlasRouteOpenBound/);
	assert.match(css, /mc-atlas-route-open/);
});
