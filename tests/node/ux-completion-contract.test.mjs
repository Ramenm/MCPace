import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";

const repoRoot = path.resolve(import.meta.dirname, "..", "..");
const read = (relativePath) =>
	fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const product = read("src/dashboard/frontend/product.js");
const actions = read("src/dashboard/frontend/app.actions.js");
const css = read("src/dashboard/frontend/product.css");
const compact = (value) => value.replace(/\s+/g, " ");

test("setup guidance is progressive and never equates discovery with a verified connection", () => {
	const source = compact(product);
	assert.match(source, /function setupGuideModel/);
	assert.match(source, /Read the local runtime/);
	assert.match(
		source,
		/A discovered path is not treated as a verified connection/,
	);
	assert.match(source, /Verify tool evidence/);
	assert.match(
		source,
		/configuration, enablement, verification, and client exposure/,
	);
	assert.match(source, /data-mc-setup-progress-detail/);
});

test("live view distinguishes scheduler ownership from completed or executing calls", () => {
	const source = compact(product);
	assert.match(source, /function liveSessionModels/);
	assert.match(source, /function activeLeaseModels/);
	assert.match(source, /Ownership ≠ execution/);
	assert.match(
		source,
		/A lease means MCPace is preserving routing ownership or isolation/,
	);
	assert.match(
		source,
		/It is not proof that a tool is executing at this exact moment/,
	);
	assert.match(source, /\[["']live["'],\s*["']Live now["']\]/);
});

test("application reachability separates observed evidence from potential inventory exposure", () => {
	const source = compact(product);
	assert.match(source, /function observedClientRouteModels/);
	assert.match(
		source,
		/Observed routes require retained calls or active leases/,
	);
	assert.match(source, /Potential access/);
	assert.match(source, /Potential is not observed/);
	assert.match(
		source,
		/does not invent per-client allowlists or a verified connection/,
	);
});

test("server mutations require one impact review and support reversible single toggles", () => {
	const source = compact(product);
	assert.match(source, /function requestServerActionReview/);
	assert.match(source, /Active work may be interrupted/);
	assert.match(
		source,
		/Testing may start a local process or contact a remote endpoint/,
	);
	assert.match(source, /__MCPACE_PRODUCT_CONFIRM_SERVER_ACTION__/);
	assert.match(source, /__MCPACE_PRODUCT_SERVER_ACTION_RESULT__/);
	assert.match(source, /toastAction/);
	assert.match(actions, /confirmServerMutation/);
	assert.match(actions, /mcSkipProductConfirm/);
});

test("screen-share-safe exports redact exact routing context by default", () => {
	const source = compact(product);
	assert.match(source, /exportMode: ["']safe["']/);
	assert.match(source, /mcpace\.activityExport\.safe\.v2/);
	assert.match(source, /redactedFields/);
	assert.match(source, /clientAlias/);
	assert.match(source, /Export safe JSON/);
	assert.match(source, /Privacy-safe is the default/);
	assert.match(source, /containsRawLocalAuditValues/);
});

test("server resolver turns common failures into direct next steps", () => {
	const source = compact(product);
	assert.match(source, /function serverDiagnosis/);
	assert.match(source, /could not authenticate/);
	assert.match(source, /no retained tools\/list result/);
	assert.match(source, /exceeded a time or capacity boundary/);
	assert.match(source, /data-mc-resolve-action/);
});

test("new UX surfaces stay responsive and keyboard visible", () => {
	for (const className of [
		"mc-setup-dialog",
		"mc-action-review-dialog",
		"mc-live-activity",
		"mc-potential-matrix",
		"mc-server-resolver",
		"mc-toast-action",
	]) {
		assert.match(css, new RegExp(className));
	}
	assert.match(css, /focus-visible/);
	assert.match(css, /@media \(max-width: 760px\)/);
	assert.match(css, /prefers-reduced-motion/);
	assert.match(css, /forced-colors/);
});

test("global navigation shortcuts require Alt and announce the active view", () => {
	const source = compact(product);
	assert.match(source, /event\.altKey && event\.key === "\/"/);
	assert.match(source, /Alt\+1–5 switches sections/);
	assert.doesNotMatch(source, /if \(event\.key === "\/" && !editing\)/);
	assert.match(
		source,
		/id="mc-view-announcer" class="mc-sr-only" role="status"/,
	);
	assert.match(source, /Now showing \$\{title\}/);
});

test("rerenders preserve a deterministic focus target", () => {
	const source = compact(product);
	assert.match(source, /function captureDashboardFocus/);
	assert.match(source, /function restoreDashboardFocus/);
	assert.match(source, /focusIdentityAttributes/);
	assert.match(source, /function focusCandidateForToken/);
	assert.match(
		source,
		/const focusToken = state\.pendingFocusToken \|\| captureDashboardFocus\(\)/,
	);
	assert.match(source, /\$\("h1, h2", state\.hosts\[state\.view\]\)/);
});

test("application evidence uses a complete keyboard tab contract", () => {
	const source = compact(product);
	assert.match(
		source,
		/role="tablist" aria-label="Application access evidence"/,
	);
	assert.match(source, /aria-controls="mc-exposure-panel"/);
	assert.match(source, /role="tabpanel" aria-labelledby="mc-exposure-tab-/);
	assert.match(source, /\["ArrowLeft", "ArrowRight", "Home", "End"\]/);
	assert.match(source, /tabindex="\$\{state\.exposureMode/);
	assert.match(
		source,
		/getElementById\(`mc-exposure-tab-\$\{state\.exposureMode\}`\)/,
	);
	assert.match(source, /focus\(\{ preventScroll: true \}\)/);
});

test("usage rows expose explicit controls and toasts remain dismissible", () => {
	const source = compact(product);
	assert.match(source, /class="mc-usage-open" data-mc-open-server/);
	assert.doesNotMatch(source, /class="mc-usage-table-row" tabindex="0"/);
	assert.match(source, /function removeToast/);
	assert.match(
		source,
		/data-mc-toast-dismiss aria-label="Dismiss notification"/,
	);
	assert.match(source, /item\.addEventListener\("focusin", pause\)/);
	assert.match(source, /item\.addEventListener\("focusout", resume\)/);
	assert.match(source, /restoreDashboardFocus\(token\)/);
});

test("dialogs, responsive tabs, and charts preserve accessible context", () => {
	const source = compact(product);
	assert.match(
		source,
		/data-mc-command-close aria-label="Close command center"/,
	);
	assert.match(source, /serverDialogReturnView/);
	assert.match(source, /serverDialogFocusToken/);
	assert.match(source, /switchView\(returnView, \{ focus: false \}\)/);
	assert.match(
		source,
		/else if \(focusToken\) restoreDashboardFocus\(focusToken\)/,
	);
	assert.match(source, /node\.getClientRects\(\)\.length > 0/);
	assert.match(source, /timelineAlternative/);
	assert.match(source, /calls, \$\{formatNumber\(bucket\.failures\)\} failed/);
	assert.match(
		source,
		/window\.innerWidth < 1280 \? "horizontal" : "vertical"/,
	);
	assert.match(source, /dataset\.mcMotion/);
	assert.match(
		css,
		/@media \(max-width: 899\.98px\)[\s\S]*?\.mc-toast-region \{[\s\S]*?var\(--mc-ac-bottom-nav\) \+ 18px/,
	);
	assert.match(
		source,
		/id: "shortcuts"[\s\S]*?run: \(\) => \{ closeCommandDialog\(\); toast\(/,
	);
});
