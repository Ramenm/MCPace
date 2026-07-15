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
const compact = (value) => value.replace(/\s+/g, " ");
const source = compact(product);

test("server lifecycle keeps source, runtime, MCP, tools, and last operation independent", () => {
	assert.match(source, /function serverRuntimeProfile/);
	assert.match(source, /function serverProbeProfile/);
	assert.match(source, /function serverToolsEvidenceProfile/);
	assert.match(source, /function serverLastOperationProfile/);
	assert.match(source, /function serverOperationalProfile/);
	assert.match(
		source,
		/No live process is expected until an on-demand route needs the server/,
	);
	assert.match(source, /MCP ready · last call failed/);
	assert.match(source, /The server remains protocol-ready/);
	assert.doesNotMatch(
		source,
		/if \(server\.tone === ["']bad["']\) return \{ title: ["']MCP verified/,
	);
});

test("cache miss and zero tools are not presented as protocol failure", () => {
	assert.match(source, /cacheMiss =\s*cacheStatus === ["']cache-miss["']/);
	assert.match(source, /ok === false && !cacheMiss/);
	assert.match(source, /0 tools reported/);
	assert.match(source, /tools\/list completed and returned no tools/);
	assert.match(source, /serverToolsEvidenceProfile\(server\)\.measured/);
	assert.match(source, /No evidence matches the current source/);
});

test("runtime profile reads live process and resource data without claiming a tool is executing", () => {
	assert.match(source, /runtime\.upstreamSessionPool/);
	assert.match(source, /runtime\.serverResourceMonitoring/);
	assert.match(source, /rssBytes/);
	assert.match(source, /fdCount/);
	assert.match(source, /shortestIdleMs/);
	assert.match(source, /This does not prove a tool is executing now/);
	assert.match(source, /ownership may be idle/);
});

test("next safe action is explicit and existing controls remain the mutation authority", () => {
	assert.match(source, /function serverNextActionProfile/);
	assert.match(source, /Test MCP and tools/);
	assert.match(source, /Review credentials/);
	assert.match(source, /Review queue and capacity/);
	assert.match(source, /mc-row-guided-action/);
	assert.match(source, /\$\$\(["']\[data-server-action\]["']/);
	assert.match(css, /\.mc-row-guided-action/);
	assert.match(css, /\.mc-native-row-action/);
});

test("duplicate sources and overlapping tool names are visible but not called errors automatically", () => {
	assert.match(source, /function serverConflictProfile/);
	assert.match(source, /Possible source alias/);
	assert.match(source, /tool name collision/);
	assert.match(source, /Aliases and shared tool names can be intentional/);
	assert.match(source, /Review duplicate source/);
});

test("credentials are represented as references, not proof of availability", () => {
	assert.match(
		source,
		/Header names referenced; value availability is not verified/,
	);
	assert.match(
		source,
		/Environment secret names referenced; value availability is not verified/,
	);
	assert.match(source, /credential availability is not verified/);
	assert.doesNotMatch(source, /Header credentials configured/);
	assert.doesNotMatch(source, /Environment credentials configured/);
});

test("capability evidence includes dynamic-change caveat and keeps authorization separate", () => {
	assert.match(source, /Capability is not authorization/);
	assert.match(source, /Definitions can change while a server runs/);
	assert.match(source, /re-test after a server or source update/);
	assert.match(source, /not a trust or authorization decision/);
});

test("home, grouping, filters, setup, and diagnosis all use operational truth", () => {
	assert.match(
		source,
		/const decorated = model\.servers\.map\(\(?server\)? => \(\{\s*server,\s*operational: serverOperationalProfile\(server\),?\s*\}\)\)/,
	);
	assert.match(source, /const attentionModels = model\.servers\s*\.map/);
	assert.match(
		source,
		/const operational = serverOperationalProfile\(server\)/,
	);
	assert.match(
		source,
		/lifecycle\.protocolMeasured && lifecycle\.tools\.measured/,
	);
	assert.match(source, /MCPace has no source-matched verification/);
	assert.doesNotMatch(
		source,
		/model\.servers\.filter\(\(?server\)? => server\.tone === ["']bad["']\)/,
	);
});

test("quick view and inspector expose runtime, evidence provenance, and capacity progressively", () => {
	assert.match(source, /Runtime & capacity/);
	assert.match(source, /Evidence provenance/);
	assert.match(source, /Isolation & capacity/);
	assert.match(source, /mc-server-runtime-strip/);
	assert.match(css, /MCPACE_LIFECYCLE_TRUTH_V5_START/);
	assert.match(css, /grid-template-columns: repeat\(4, minmax\(0, 1fr\)\)/);
	assert.match(css, /\.mc-server-runtime-strip/);
});

test("view options remains the explicit progressive-disclosure entry point", () => {
	assert.match(source, /<span>View options<\/span>/);
});
