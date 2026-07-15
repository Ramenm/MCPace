import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const css = fs.readFileSync(
	new URL("../../src/dashboard/frontend/product.css", import.meta.url),
	"utf8",
);
const js = fs.readFileSync(
	new URL("../../src/dashboard/frontend/product.js", import.meta.url),
	"utf8",
);

test("adaptive composition has four structural viewport modes", () => {
	for (const token of ["wide", "rail", "compact", "phone"])
		assert.match(js, new RegExp(`['"]${token}['"]`));
	for (const breakpoint of ["1280", "900", "680", "420"])
		assert.match(css, new RegExp(breakpoint));
	assert.match(js, /data|dataset/);
	assert.match(js, /layoutSystem/);
	assert.match(js, /adaptiveGrid/);
});

test("responsive layout preserves reflow and mobile safe areas", () => {
	assert.match(css, /100dvh/);
	assert.match(css, /safe-area-inset-bottom/);
	assert.match(css, /overflow-wrap:\s*anywhere/);
	assert.match(css, /min-inline-size:\s*0/);
	assert.match(css, /max-inline-size:\s*100%/);
});

test("tablet is recomposed instead of merely scaled", () => {
	assert.match(css, /navigation rail|Rail:/i);
	assert.match(css, /Compact tablet:/);
	assert.match(css, /grid-template-columns:\s*var\(--mc-ac-rail\)/);
	assert.match(css, /data-layout-role="mobile-nav"/);
	assert.match(css, /data-adaptive-role="settings-nav"/);
	assert.match(css, /data-adaptive-role="server-row"/);
});

test("dialogs and horizontal strips remain operable at narrow widths", () => {
	assert.match(css, /data-dialog-kind="inspector"/);
	assert.match(css, /inline-size:\s*100vw\s*!important/);
	assert.match(css, /data-adaptive-scroll-strip/);
	assert.match(js, /scrollActiveControlIntoView/);
	assert.match(js, /scrollIntoView/);
});

test("motion and high-contrast preferences are retained", () => {
	assert.match(css, /prefers-reduced-motion:\s*reduce/);
	assert.match(css, /data-mc-motion="off"/);
	assert.doesNotMatch(css, /html\[data-motion=/);
	assert.match(js, /dataset\.mcMotion/);
	assert.match(css, /forced-colors:\s*active/);
	assert.match(css, /mc-ac-sheet-in/);
});

test("layout roles and grid slots are wired to actual product chrome", () => {
	assert.match(js, /function setLayoutRole\(element, role\)/);
	assert.match(js, /element\.dataset\.layoutRole = role/);
	assert.match(js, /direct\.dataset\[SLOT\] = role/);
	assert.match(
		css,
		/> \[data-layout-slot="sidebar"\][^{]*\{[^}]*grid-column:\s*1/s,
	);
	assert.match(
		css,
		/> \[data-layout-slot="main"\][^{]*\{[^}]*grid-column:\s*2/s,
	);
	assert.match(css, /margin-inline-start:\s*0\s*!important/);
	assert.match(css, /data-layout-role="topbar"/);
});

test("adaptive discovery targets direct product topbar and structural server row body", () => {
	assert.match(js, /Array\.from\(shell\.children\)\.find/);
	assert.match(
		js,
		/element\.matches\('\.mc-topbar, \[data-layout-role="topbar"\]'\)/,
	);
	assert.doesNotMatch(js, /findByKeywords\(main \|\| shell, \['topbar'/);
	assert.match(js, /body\.dataset\.adaptiveRowBody = ["']true["']/);
	assert.match(css, /data-adaptive-row-body="true"/);
});
