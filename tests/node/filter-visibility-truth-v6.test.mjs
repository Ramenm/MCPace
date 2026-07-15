import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";

const css = fs.readFileSync(
	new URL("../../src/dashboard/frontend/product.css", import.meta.url),
	"utf8",
);
const js = fs.readFileSync(
	new URL("../../src/dashboard/frontend/product.js", import.meta.url),
	"utf8",
);

test("adaptive server rows preserve semantic hidden state for search and filters", () => {
	assert.match(css, /#mc-product-shell #server-list \.server-row\[hidden\]/);
	assert.match(css, /display:\s*none\s*!important/);
	assert.match(
		js,
		/server\.row\.hidden = !show \|\| state\.integrationLayout !== ["']list["']/,
	);
});

test("list and connection surfaces preserve semantic hidden state", () => {
	assert.match(css, /\[data-mc-integration-list-shell\]\[hidden\]/);
	assert.match(css, /\[data-mc-route-map\]\[hidden\]/);
	assert.match(
		js,
		/listShell\.hidden = state\.integrationLayout !== ["']list["']/,
	);
	assert.match(
		js,
		/mapShell\.hidden = state\.integrationLayout !== ["']map["']/,
	);
});
