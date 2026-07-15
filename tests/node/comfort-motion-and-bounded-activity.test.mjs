import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";

const repoRoot = path.resolve(import.meta.dirname, "..", "..");
const read = (relativePath) =>
	fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const product = read("src/dashboard/frontend/product.js");
const css = read("src/dashboard/frontend/product.css");
const compact = (value) => value.replace(/\s+/g, " ");

test("display comfort preferences remain local and explicit", () => {
	const source = compact(product);
	assert.match(source, /textSize: ["']normal["']/);
	assert.match(source, /effects: ["']soft["']/);
	assert.match(
		source,
		/readPreference\(["']motion["'], ["']system["'], \[\s*["']system["'], ["']reduced["'], ["']off["'],?\s*\]\)/,
	);
	assert.match(source, /data-mc-text-size="large"/);
	assert.match(source, /data-mc-effects="minimal"/);
	assert.match(source, /data-mc-motion="off"/);
	assert.match(source, /function setTextSize/);
	assert.match(source, /function setEffects/);
	assert.match(source, /function syncThemeColor/);
	assert.match(css, /html\[data-mc-text-size="large"\]/);
	assert.match(css, /html\[data-mc-effects="minimal"\]/);
	assert.match(css, /html\[data-mc-motion="off"\]/);
});

test("activity is grouped by date and bounded instead of rendering the retained log at once", () => {
	const source = compact(product);
	assert.match(source, /activityLimit: 16/);
	assert.match(source, /function activityDayLabel/);
	assert.match(source, /function activityEventMarkup/);
	assert.match(source, /function activityStreamMarkup/);
	assert.match(source, /filtered\.slice\(0, state\.activityLimit\)/);
	assert.match(source, /Show \$\{Math\.min\(16, remaining\)\} more/);
	assert.match(source, /state\.activityLimit \+= 16/);
	assert.match(
		source,
		/const shown = Math\.min\(filtered\.length, state\.activityLimit\)/,
	);
	assert.match(
		source,
		/Showing \$\{shown\} of \$\{filtered\.length\} matching entries/,
	);
	assert.match(css, /\.mc-event-day/);
	assert.match(css, /\.mc-event-load-more/);
});

test("frequent integrations can be pinned without mutating runtime configuration", () => {
	const source = compact(product);
	assert.match(source, /pinnedServers: new Set\(\)/);
	assert.match(source, /readSetPreference\(["']pinnedServers["']\)/);
	assert.match(
		source,
		/writeSetPreference\(["']pinnedServers["'], state\.pinnedServers\)/,
	);
	assert.match(source, /data-mc-integration-filter="pinned"/);
	assert.match(source, /pin\.className = ["']mc-row-pin["']/);
	assert.match(source, /is now available in the Pinned filter/);
	assert.match(css, /\.mc-row-pin/);
});

test("server summary answers next action, source, usage, isolation, and access before diagnostics", () => {
	const source = compact(product);
	assert.match(source, /mc-server-daily-summary/);
	assert.match(source, /At a glance/);
	assert.match(source, /Source<\/span>/);
	assert.match(source, /Recent use/);
	assert.match(source, /Isolation/);
	assert.match(source, /Access<\/span>/);
	assert.match(source, /data-mc-daily-tab="tools"/);
	assert.match(source, /data-mc-daily-tab="access"/);
	assert.match(css, /\.mc-server-daily-facts/);
	assert.match(css, /\.mc-server-daily-tools/);
});

test("purposeful motion has system, reduced, and off behavior", () => {
	assert.match(css, /--mc-duration-fast:/);
	assert.match(css, /--mc-duration-medium:/);
	assert.match(css, /--mc-ease-emphasized:/);
	assert.match(css, /@keyframes mc-purposeful-view-in/);
	assert.match(css, /@keyframes mc-purposeful-dialog-in/);
	assert.match(css, /@keyframes mc-purposeful-sheet-in/);
	assert.match(css, /\.mc-server-sheet\[open\] \.server-dialog-card/);
	assert.match(css, /html\[data-mc-motion="reduced"\]/);
	assert.match(css, /@keyframes mc-purposeful-fade-in/);
	assert.match(css, /html\[data-mc-motion="off"\] \*/);
	assert.match(css, /@media \(prefers-reduced-motion: reduce\)/);
});

test("all visual themes remain available and monochrome palettes stay neutral", () => {
	const source = compact(product);
	for (const theme of ["system", "light", "dark", "mono-light", "mono-dark"]) {
		assert.match(source, new RegExp(`data-mc-theme="${theme}"`));
	}
	assert.match(css, /html\[data-mc-theme="mono-light"\]/);
	assert.match(css, /html\[data-mc-theme="mono-dark"\]/);
	assert.match(css, /--mc-bg: #eeeeee;/);
	assert.match(css, /--mc-bg: #080808;/);
	assert.match(css, /forced-colors/);
});

test("older settings are reconciled with the current product map", () => {
	const source = compact(product);
	assert.match(source, /function tidyLegacyPreferences/);
	assert.match(source, /Dashboard refresh/);
	assert.match(source, /Home<\/strong> shows current readiness/);
	assert.match(source, /Integrations<\/strong> manages MCP servers/);
	assert.match(source, /Applications<\/strong> previews and applies/);
	assert.match(source, /Activity<\/strong> shows recent calls/);
});

test("backend DOM observations ignore product-owned mutations", () => {
	assert.match(product, /target\.closest\(["']#mc-product-shell["']\)/);
});

test("usage analytics exposes the latest retained timestamp to server summaries", () => {
	const source = compact(product);
	assert.match(source, /const lastTimestamp = records\.reduce/);
	assert.match(source, /records, lastTimestamp, calls/);
});

test("row overflow actions and status toasts remain accessible and calm", () => {
	const source = compact(product);
	assert.match(source, /aria-label="Open summary for/);
	assert.match(source, /mc-toast:not\(\.mc-toast-action\)/);
	assert.match(css, /Toasts are transient status/);
});

test("bulk-selection targets satisfy the minimum pointer size", () => {
	assert.match(css, /\.mc-row-select \{[^}]*width: 28px; height: 28px;/s);
});

test("inspector avoids repeating recovery copy and hides secondary evidence until requested", () => {
	const source = compact(product);
	assert.match(source, /const recoveryVisible = Boolean\(dailyDiagnosis\)/);
	assert.match(source, /Current integration context/);
	assert.match(source, /summaryAction = recoveryVisible \? ["']["']/);
	assert.match(
		css,
		/The inspector keeps recovery, context, and deep evidence in separate layers/,
	);
	assert.match(
		css,
		/#server-dialog:not\(\.mc-show-technical\).*server-setting-brief/s,
	);
});
