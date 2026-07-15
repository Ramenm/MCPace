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

function relativeLuminance(hex) {
	const channels = hex
		.slice(1)
		.match(/.{2}/g)
		.map((value) => Number.parseInt(value, 16) / 255)
		.map((value) =>
			value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4,
		);
	return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrastRatio(left, right) {
	const [lighter, darker] = [
		relativeLuminance(left),
		relativeLuminance(right),
	].sort((a, b) => b - a);
	return (lighter + 0.05) / (darker + 0.05);
}

function applyTokenBlocks(tokens, pattern) {
	for (const match of css.matchAll(pattern)) {
		for (const token of match[1].matchAll(/--([\w-]+):\s*(#[0-9a-f]{6})/gi)) {
			tokens[token[1]] = token[2];
		}
	}
	return tokens;
}

function themeTokens(theme) {
	const tokens = applyTokenBlocks({}, /:root\s*\{([^{}]*)\}/g);
	if (theme === "light")
		return applyTokenBlocks(
			tokens,
			/html\[data-mc-theme="light"\]\s*\{([^{}]*)\}/g,
		);
	return applyTokenBlocks(
		tokens,
		new RegExp(`html\\[data-mc-theme="${theme}"\\]\\s*\\{([^{}]*)\\}`, "g"),
	);
}

test("everyday mode is the default and full diagnostics remain explicit", () => {
	const source = compact(product);
	assert.match(source, /detailLevel: ["']essential["']/);
	assert.match(
		source,
		/readPreference\(["']detailLevel["'], ["']essential["']/,
	);
	assert.match(source, /data-mc-detail-level="essential"/);
	assert.match(source, /data-mc-detail-level="full"/);
	assert.match(source, /function setDetailLevel/);
	assert.match(css, /html\[data-mc-detail="essential"\]/);
	assert.match(css, /html\[data-mc-detail="full"\]/);
});

test("home and setup flows use progressive disclosure instead of permanent operator panels", () => {
	const source = compact(product);
	assert.match(source, /function calmHomeMarkup/);
	assert.match(source, /mc-calm-status/);
	assert.match(source, /mc-home-advanced/);
	assert.match(source, /System details/);
	assert.match(source, /mc-add-change-plan/);
	assert.match(source, /What will change/);
	assert.match(source, /mc-integration-more/);
	assert.match(source, /View options/);
	assert.match(source, /mc-app-details/);
	assert.match(source, /Configuration details/);
});

test("monochrome themes and statuses do not rely on hue alone", () => {
	const source = compact(product);
	assert.match(source, /mono-light/);
	assert.match(source, /mono-dark/);
	assert.match(source, /function toneMark/);
	assert.match(source, /good: ["']✓["']/);
	assert.match(source, /warn: ["']!["']/);
	assert.match(source, /bad: ["']×["']/);
	assert.match(css, /html\[data-mc-theme="mono-light"\]/);
	assert.match(css, /html\[data-mc-theme="mono-dark"\]/);
	assert.match(css, /Non-colour status language/);
	assert.match(css, /border-style: dashed/);
	assert.match(css, /forced-colors/);
});

test("server inspector keeps daily tabs simple while preserving technical evidence", () => {
	const source = compact(product);
	assert.match(source, /overviewTab\.textContent = ["']Summary["']/);
	assert.match(source, /routingTab\.textContent = ["']Isolation["']/);
	assert.match(source, /sourceTab\.textContent = ["']Setup["']/);
	assert.match(source, /usageTab\.textContent = ["']Activity["']/);
	assert.match(source, /mc-server-more-menu/);
	assert.match(source, /Protocol evidence/);
	assert.match(source, /Operation history/);
	assert.match(css, /data-mc-technical-tab/);
});

test("keyboard tab movement skips controls hidden by the selected information level", () => {
	assert.ok((product.match(/\.filter\(visible\)/g) || []).length >= 3);
	assert.match(product, /function activityTabKeydown/);
	assert.match(product, /function settingsTabKeydown/);
	assert.match(product, /function serverSheetTabKeydown/);
});

test("theme text, focus, and control-boundary tokens meet WCAG contrast", () => {
	for (const theme of ["light", "mono-light", "mono-dark"]) {
		const tokens = themeTokens(theme);
		for (const foreground of [
			"mc-text",
			"mc-text-soft",
			"mc-muted",
			"mc-faint",
			"mc-accent",
			"mc-good",
			"mc-warn",
			"mc-bad",
		]) {
			assert.ok(
				contrastRatio(tokens[foreground], tokens["mc-panel"]) >= 4.5,
				`${theme} ${foreground} must meet normal-text AA on mc-panel`,
			);
		}
		assert.ok(
			contrastRatio(tokens["mc-border"], tokens["mc-panel-2"]) >= 3,
			`${theme} control boundaries must meet non-text contrast`,
		);
		assert.ok(
			contrastRatio(tokens["mc-accent"], tokens["mc-panel-2"]) >= 4.5,
			`${theme} accent text must meet normal-text AA on mc-panel-2`,
		);
		assert.ok(
			contrastRatio(tokens["mc-faint"], tokens["mc-panel-2"]) >= 4.5,
			`${theme} faint text must meet normal-text AA on mc-panel-2`,
		);
		assert.ok(
			contrastRatio(tokens["mc-warn"], tokens["mc-panel-2"]) >= 4.5,
			`${theme} warning text must meet normal-text AA on mc-panel-2`,
		);
	}
	assert.match(css, /outline:\s*3px solid var\(--mc-focus\)/);
	const dark = themeTokens("unknown-dark-theme");
	for (const foreground of [
		"mc-text",
		"mc-text-soft",
		"mc-muted",
		"mc-faint",
		"mc-accent",
		"mc-good",
		"mc-warn",
		"mc-bad",
	]) {
		assert.ok(
			contrastRatio(dark[foreground], dark["mc-panel"]) >= 4.5,
			`dark ${foreground} must meet normal-text AA on mc-panel`,
		);
	}
	assert.ok(contrastRatio(dark["mc-border"], dark["mc-panel-2"]) >= 3);
});

test("light warning text token meets normal-text AA contrast", () => {
	const lightBlocks = [
		...css.matchAll(/html\[data-mc-theme="light"\]\s*\{([\s\S]*?)\}/g),
	];
	const block = lightBlocks.at(-1)?.[1] || "";
	const background = block.match(/--mc-bg:\s*(#[0-9a-f]{6})/i)?.[1];
	const warning = block.match(/--mc-warn:\s*(#[0-9a-f]{6})/i)?.[1];
	assert.ok(
		background && warning,
		"final light theme should define background and warning tokens",
	);
	assert.ok(contrastRatio(background, warning) >= 4.5);
	assert.equal((css.match(/--mc-warn:\s*#925d00/gi) || []).length, 2);
});

test("monochrome palettes stay neutral and daily source context remains visible", () => {
	assert.match(css, /--mc-bg: #eeeeee;/);
	assert.match(css, /--mc-warn: #4c4c4c;/);
	assert.match(css, /--mc-bg: #080808;/);
	assert.match(css, /--mc-muted: #a2a2a2;/);
	assert.match(css, /\.mc-calm-glance small \{[^}]*color: var\(--mc-muted\)/s);
	assert.match(
		css,
		/html\[data-mc-detail="essential"\] \.mc-server-source-copy code \{[^}]*display: block/s,
	);
});
