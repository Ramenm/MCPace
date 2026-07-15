// MCPace dashboard frontend. Backend owns product state; this file renders, validates, and dispatches explicit actions.
const PREF = "mcpace.dashboard.slim.";
const REFRESH_MS = { 15: 15000, 30: 30000, 60: 60000, paused: 0 };
const MAX_SERVER_ROWS = 64;
const HIDDEN_REFRESH_MS = 60000;
const VISIBLE_REFRESH_MIN_INTERVAL_MS = 5000;
const LIFECYCLE_RESUME_MIN_INTERVAL_MS = 5000;
const MAX_REFRESH_FAILURE_BACKOFF_MS = 120000;
const REQUEST_TIMEOUT_MS = 10000;
const ACTION_TIMEOUT_MS = 30000;
const ROUTING_MODES = [
	["serialized", "Safe queue"],
	["shared", "Shared"],
	["pool", "Worker pool"],
	["session-isolated", "Per chat"],
	["project-isolated", "Per project"],
];
const SETUP_TABS = ["import", "discover", "add", "clients", "automation"];
const DIAGNOSTIC_TABS = ["runtime", "trust", "policy", "logs", "preferences"];
const SERVER_DIALOG_TABS = ["overview", "routing", "source"];

const state = {
	overview: null,
	logs: [],
	operations: null,
	query: "",
	setupTab: readPref("setupTab", "import", SETUP_TABS),
	diagnosticTab: readPref("diagnosticTab", "runtime", DIAGNOSTIC_TABS),
	serverDialogTab: "overview",
	enabledOnly: readPref("enabledOnly", "true", ["true", "false"]) === "true",
	refreshMode: readPref("refreshMode", "30", Object.keys(REFRESH_MS)),
	sort: readPref("sort", "risk", ["risk", "name", "instances"]),
	scope: readPref("scope", "normal", ["normal", "attention"]),
	bucket: readPref("bucket", "all", [
		"all",
		"blocked",
		"protected",
		"ready",
		"off",
	]),
	density: readPref("density", "comfortable", ["comfortable", "compact"]),
	selectedServer: null,
	backend: {
		overview: null,
		logs: null,
		operations: null,
		resources: null,
		action: null,
		checkedAt: 0,
	},
	timer: null,
	seq: 0,
	controller: null,
	refreshing: false,
	failureCount: 0,
	lastRefreshStartedAt: 0,
	lastRefreshFinishedAt: 0,
	lastSuccessAt: 0,
	lifecycle: {
		frozen: false,
		freezeCount: 0,
		resumeCount: 0,
		lastResumeAt: 0,
		wasDiscarded: Boolean(document.wasDiscarded),
	},
	serverTests: {},
	discovery: {
		loading: false,
		result: null,
		error: null,
		lastMode: "preview",
	},
	importer: {
		loading: false,
		result: null,
		error: null,
		last: null,
	},
	clientSetup: {
		loading: false,
		result: null,
		error: null,
		last: null,
	},
	update: {
		loading: false,
		requested: false,
		manual: false,
		result: null,
		error: null,
	},
	lastError: null,
};

const $ = (id) => document.getElementById(id);
const els = {
	shell: document.querySelector("[data-controller-root]"),
	mainView: $("main-view"),
	refreshButton: $("refresh-button"),
	updateCheckButton: $("update-check-button"),
	updateNotice: $("update-notice"),
	startButton: $("hub-up-button"),
	stopButton: $("hub-down-button"),
	repairButton: $("repair-button"),
	systemState: $("system-state"),
	systemNote: $("system-note"),
	baseSetup: $("base-setup"),
	baseTitle: $("base-title"),
	baseStateChip: $("base-state-chip"),
	baseBody: $("base-body"),
	baseProgressFill: $("base-progress-fill"),
	baseProgressLabel: $("base-progress-label"),
	baseStepGrid: $("base-step-grid"),
	baseRules: $("base-rules"),
	baseSafety: $("base-safety"),
	baseSafetyTitle: $("base-safety-title"),
	baseSafetyBody: $("base-safety-body"),
	baseSafetyGrid: $("base-safety-grid"),
	baseActionRow: $("base-action-row"),
	accessReview: $("access-review"),
	accessReviewTitle: $("access-review-title"),
	accessReviewBody: $("access-review-body"),
	accessReviewChip: $("access-review-chip"),
	accessReviewList: $("access-review-list"),
	attentionCount: $("attention-count"),
	attentionNote: $("attention-note"),
	serverCount: $("server-count"),
	serverNote: $("server-note"),
	loadState: $("load-state"),
	loadNote: $("load-note"),
	refreshChip: $("refresh-chip"),
	attentionList: $("attention-list"),
	serverSearch: $("server-search"),
	clearSearch: $("clear-search"),
	toggleEnabled: $("toggle-enabled"),
	autoTuneVisible: $("auto-tune-visible"),
	serverAutoTitle: $("server-auto-title"),
	serverAutoBody: $("server-auto-body"),
	serverAutoStats: $("server-auto-stats"),
	serverCommandCenter: $("server-command-center"),
	serverCommandTitle: $("server-command-title"),
	serverCommandBody: $("server-command-body"),
	serverMetricRow: $("server-metric-row"),
	serverWorkbench: $("server-workbench"),
	discoveryPanel: $("server-discovery-panel"),
	serverDiscoverForm: $("server-discover-form"),
	serverDiscoverQuery: $("server-discover-query"),
	serverDiscoverMode: $("server-discover-mode"),
	serverDiscoverRefresh: $("server-discover-refresh"),
	serverDiscoverReview: $("server-discover-review"),
	serverDiscoverButton: $("server-discover-button"),
	serverDiscoverError: $("server-discover-error"),
	serverDiscoveryResults: $("server-discovery-results"),
	setupTools: $("setup-tools"),
	serverImportForm: $("server-import-form"),
	serverImportPath: $("server-import-path"),
	serverImportSettings: $("server-import-settings"),
	serverImportButton: $("server-import-button"),
	serverImportDryRun: $("server-import-dry-run"),
	serverImportDisabled: $("server-import-disabled"),
	serverImportForce: $("server-import-force"),
	serverImportNote: $("server-import-note"),
	serverImportError: $("server-import-error"),
	serverImportResult: $("server-import-result"),
	clientSetupPanel: $("client-setup-panel"),
	clientSetupList: $("client-setup-list"),
	clientSetupResult: $("client-setup-result"),
	clientPreviewAll: $("client-preview-all"),
	clientApplyAll: $("client-apply-all"),
	clientRestoreAll: $("client-restore-all"),
	automationPanel: $("automation-panel"),
	automationTitle: $("automation-title"),
	automationBody: $("automation-body"),
	automationGrid: $("automation-grid"),
	operatorPlanTitle: $("operator-plan-title"),
	operatorPlanBody: $("operator-plan-body"),
	operatorPlanStats: $("operator-plan-stats"),
	operatorPlanLanes: $("operator-plan-lanes"),
	operatorPlanFlow: $("operator-plan-flow"),
	serverInstallForm: $("server-install-form"),
	serverInstallCommand: $("server-install-command"),
	serverInstallName: $("server-install-name"),
	serverInstallButton: $("server-install-button"),
	serverInstallDisabled: $("server-install-disabled"),
	serverInstallForce: $("server-install-force"),
	serverInstallDryRun: $("server-install-dry-run"),
	serverInstallNote: $("server-install-note"),
	serverInstallError: $("server-install-error"),
	serverFleetBoard: $("server-fleet-board"),
	serverGuide: $("server-guide"),
	serverFilterChip: $("server-filter-chip"),
	serverChips: $("server-chips"),
	serverList: $("server-list"),
	serverOverflowNote: $("server-overflow-note"),
	refreshSelect: $("refresh-select"),
	serverSort: $("server-sort"),
	serverScope: $("server-scope"),
	densitySelect: $("density-select"),
	serverDialog: $("server-dialog"),
	serverDialogTitle: $("server-dialog-title"),
	serverDialogSubtitle: $("server-dialog-subtitle"),
	serverDialogBody: $("server-dialog-body"),
	serverDialogClose: $("server-dialog-close"),
	contextList: $("context-list"),
	instanceChip: $("instance-chip"),
	instanceList: $("instance-list"),
	runtimeChip: $("runtime-chip"),
	runtimeList: $("runtime-list"),
	policyChip: $("policy-chip"),
	policyList: $("policy-list"),
	capacityChip: $("capacity-chip"),
	capacityList: $("capacity-list"),
	telemetryChip: $("telemetry-chip"),
	telemetryList: $("telemetry-list"),
	activityChip: $("activity-chip"),
	activityList: $("activity-list"),
	clientChip: $("client-chip"),
	clientList: $("client-list"),
	protocolCompatPanel: $("protocol-compat-panel"),
	protocolCompatChip: $("protocol-compat-chip"),
	protocolCompatGrid: $("protocol-compat-grid"),
	logChip: $("log-chip"),
	auditList: $("audit-list"),
	logList: $("log-list"),
};

function readPref(key, fallback, allowed) {
	try {
		const value = window.localStorage?.getItem(PREF + key);
		if (value && (!allowed || allowed.includes(value))) return value;
	} catch (_) {}
	return fallback;
}

function writePref(key, value) {
	try {
		window.localStorage?.setItem(PREF + key, value);
	} catch (_) {}
}

function escapeHtml(value) {
	return String(value ?? "")
		.replaceAll("&", "&amp;")
		.replaceAll("<", "&lt;")
		.replaceAll(">", "&gt;")
		.replaceAll('"', "&quot;");
}

function safeMarkupUrl(value) {
	const raw = String(value || "").trim();
	if (!raw) return false;
	if (raw.startsWith("#")) return true;
	try {
		const url = new URL(raw, window.location.href);
		return (
			(url.protocol === "http:" || url.protocol === "https:") &&
			url.origin === window.location.origin &&
			!url.username &&
			!url.password
		);
	} catch (_) {
		return false;
	}
}

function setSafeHtml(element, markup) {
	if (!element) return;
	const parsed = new DOMParser().parseFromString(
		String(markup ?? ""),
		"text/html",
	);
	parsed
		.querySelectorAll(
			"script, iframe, object, embed, base, link, meta, template, foreignObject",
		)
		.forEach((node) => node.remove());
	parsed.body.querySelectorAll("*").forEach((node) => {
		for (const attribute of [...node.attributes]) {
			const name = attribute.name.toLowerCase();
			const value = attribute.value;
			if (name.startsWith("on") || name === "srcdoc" || name === "srcset") {
				node.removeAttribute(attribute.name);
			} else if (
				["href", "src", "xlink:href", "action", "formaction"].includes(name) &&
				!safeMarkupUrl(value)
			) {
				node.removeAttribute(attribute.name);
			} else if (
				name === "style" &&
				/(?:url\s*\(|expression\s*\(|@import|javascript:)/i.test(value)
			) {
				node.removeAttribute(attribute.name);
			}
		}
		if (node.tagName === "A" && node.target === "_blank") {
			node.rel = "noopener noreferrer";
		}
	});
	const fragment = document.createDocumentFragment();
	for (const node of [...parsed.body.childNodes]) {
		fragment.appendChild(document.importNode(node, true));
	}
	element.replaceChildren(fragment);
}

function num(value, fallback = 0) {
	const parsed = Number(value);
	return Number.isFinite(parsed) ? parsed : fallback;
}

function text(value, fallback = "—") {
	return value === null || value === undefined || value === ""
		? fallback
		: String(value);
}

function listValues(value) {
	return Array.isArray(value)
		? value.map((item) => String(item || "")).filter(Boolean)
		: [];
}
function requestProductView(view, detail = {}) {
	const control = document.querySelector(
		`#mc-product-shell [data-mc-view="${CSS.escape(String(view || "home"))}"]`,
	);
	if (control) control.click();
	else
		document.dispatchEvent(
			new CustomEvent("mcpace:product-navigate", {
				detail: { ...detail, view },
			}),
		);
}

function setDashboardView(view, options = {}) {
	const productView =
		{
			overview: "home",
			sources: "integrations",
			setup: "integrations",
			diagnostics: "settings",
		}[view] || "home";
	requestProductView(productView, options);
	return productView;
}

function setTabGroup(
	target,
	allowed,
	buttonSelector,
	panelSelector,
	stateKey,
	prefKey,
	options = {},
) {
	const next = allowed.includes(target) ? target : allowed[0];
	state[stateKey] = next;
	if (prefKey) writePref(prefKey, next);
	document.querySelectorAll(buttonSelector).forEach((button) => {
		const active = button.dataset[options.buttonDataKey] === next;
		button.classList.toggle("active", active);
		button.setAttribute("aria-selected", String(active));
		button.tabIndex = active ? 0 : -1;
	});
	document.querySelectorAll(panelSelector).forEach((panel) => {
		const active = panel.dataset[options.panelDataKey] === next;
		panel.hidden = !active;
	});
	return next;
}

function setSetupTab(target) {
	return setTabGroup(
		target,
		SETUP_TABS,
		"[data-setup-target]",
		"[data-setup-panel]",
		"setupTab",
		"setupTab",
		{
			buttonDataKey: "setupTarget",
			panelDataKey: "setupPanel",
		},
	);
}

function setDiagnosticTab(target) {
	return setTabGroup(
		target,
		DIAGNOSTIC_TABS,
		"[data-diagnostic-target]",
		"[data-diagnostic-panel]",
		"diagnosticTab",
		"diagnosticTab",
		{
			buttonDataKey: "diagnosticTarget",
			panelDataKey: "diagnosticPanel",
		},
	);
}

function setServerDialogTab(target) {
	return setTabGroup(
		target,
		SERVER_DIALOG_TABS,
		"[data-server-dialog-tab]",
		"[data-server-dialog-panel]",
		"serverDialogTab",
		"",
		{
			buttonDataKey: "serverDialogTab",
			panelDataKey: "serverDialogPanel",
		},
	);
}

function handleTablistKeydown(event, selector, dataKey, activate) {
	if (!event.target.matches(selector)) return;
	const tabs = [...event.currentTarget.querySelectorAll(selector)].filter(
		(tab) => !tab.disabled,
	);
	const index = tabs.indexOf(event.target);
	if (index < 0) return;
	let nextIndex = index;
	if (event.key === "ArrowRight" || event.key === "ArrowDown")
		nextIndex = (index + 1) % tabs.length;
	else if (event.key === "ArrowLeft" || event.key === "ArrowUp")
		nextIndex = (index - 1 + tabs.length) % tabs.length;
	else if (event.key === "Home") nextIndex = 0;
	else if (event.key === "End") nextIndex = tabs.length - 1;
	else return;
	event.preventDefault();
	const next = tabs[nextIndex];
	activate(next.dataset[dataKey]);
	next.focus();
}

function revealElementById(id, block = "nearest") {
	const node = document.getElementById(id);
	if (!node) return false;
	const setupPanel = node.closest("[data-setup-panel]")?.dataset?.setupPanel;
	if (setupPanel) setSetupTab(setupPanel);
	const diagnosticPanel = node.closest("[data-diagnostic-panel]")?.dataset
		?.diagnosticPanel;
	if (diagnosticPanel) setDiagnosticTab(diagnosticPanel);
	for (let parent = node.parentElement; parent; parent = parent.parentElement) {
		if (parent.tagName === "DETAILS") parent.open = true;
	}
	const motion = document.documentElement.dataset.mcMotion;
	const reducedMotion =
		motion === "reduced" ||
		motion === "off" ||
		window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
	try {
		node.scrollIntoView({
			behavior: reducedMotion ? "auto" : "smooth",
			block,
		});
	} catch (_) {
		node.scrollIntoView();
	}
	return true;
}

function updateSetupToolsState(reason = "") {
	const targetByReason = {
		empty: "import",
		import: "import",
		client: "clients",
		clients: "clients",
		discover: "discover",
		add: "add",
		automation: "automation",
	};
	const target = targetByReason[reason];
	if (!target) return;
	setSetupTab(target);
}

function shellWord(value) {
	const raw = String(value || "").trim();
	if (!raw) return "''";
	if (/^[A-Za-z0-9_./:@=,+-]+$/.test(raw)) return raw;
	return `'${raw.replaceAll("'", "'\\''")}'`;
}

function launchCommand(server) {
	const command = String(server?.sourceCommand || "").trim();
	if (command)
		return [command, ...listValues(server?.sourceArgs)]
			.map(shellWord)
			.join(" ");
	const url = String(server?.sourceUrl || "").trim();
	return url;
}

function shortText(value, limit = 96) {
	const raw = String(value || "").trim();
	return raw.length > limit ? `${raw.slice(0, Math.max(0, limit - 1))}…` : raw;
}

function compactList(value, fallback = "none") {
	const values = listValues(value);
	return values.length ? values.join(", ") : fallback;
}

function setInstallNote(message, tone = "warn") {
	if (!els.serverInstallNote) return;
	setSafeHtml(els.serverInstallNote, `${dot(tone)}${escapeHtml(message)}`);
}

function setImportNote(message, tone = "warn") {
	if (!els.serverImportNote) return;
	setSafeHtml(els.serverImportNote, `${dot(tone)}${escapeHtml(message)}`);
}

function setFieldError(errorElement, inputElement, message = "") {
	const text = String(message || "").trim();
	if (errorElement) {
		errorElement.textContent = text;
		errorElement.hidden = !text;
	}
	if (inputElement) {
		if (text) inputElement.setAttribute("aria-invalid", "true");
		else inputElement.removeAttribute("aria-invalid");
	}
}

function fmtDate(ms) {
	if (!ms) return "—";
	try {
		return new Date(Number(ms)).toLocaleString();
	} catch (_) {
		return String(ms);
	}
}

function fmtMs(ms) {
	const value = num(ms, 0);
	if (!value) return "0ms";
	if (value < 1000) return `${value}ms`;
	const seconds = value / 1000;
	if (seconds < 60) return `${seconds.toFixed(seconds < 10 ? 1 : 0)}s`;
	const minutes = seconds / 60;
	return `${minutes.toFixed(minutes < 10 ? 1 : 0)}m`;
}

function fmtBytes(bytes) {
	const value = num(bytes, 0);
	if (!value) return "0 B";
	const units = ["B", "KB", "MB", "GB"];
	let size = value;
	let unit = 0;
	while (size >= 1024 && unit < units.length - 1) {
		size /= 1024;
		unit += 1;
	}
	return `${size.toFixed(size >= 10 || unit === 0 ? 0 : 1)} ${units[unit]}`;
}

function dot(tone) {
	return `<span class="dot ${escapeHtml(tone || "warn")}"></span>`;
}
function chip(label, tone = "warn") {
	return `<span class="chip">${dot(tone)}${escapeHtml(label)}</span>`;
}
function tags(items) {
	return items
		.filter(Boolean)
		.map((item) => `<span class="tag">${escapeHtml(item)}</span>`)
		.join("");
}
function itemClass(tone) {
	return tone === "bad"
		? "item bad"
		: tone === "warn"
			? "item warn"
			: tone === "good"
				? "item good"
				: "item";
}
function setChip(element, label, tone) {
	if (element) setSafeHtml(element, `${dot(tone)}${escapeHtml(label)}`);
}
function setSurfaceTone(element, tone) {
	const surface = element?.closest?.(
		".signal, .ops-card, .panel, .mini-panel, .connection-map, .setup-queue",
	);
	if (surface) surface.dataset.tone = tone || "warn";
}
function setCardTone(element, tone) {
	if (element) element.dataset.tone = tone || "warn";
}
function setSignalTones(entries) {
	for (const [element, tone] of entries) setSurfaceTone(element, tone);
}

function readinessBand(score) {
	const pct = Math.max(0, Math.min(100, Math.round(Number(score || 0) * 100)));
	if (pct >= 82) return { pct, tone: "good", label: "Ready" };
	if (pct >= 58) return { pct, tone: "warn", label: "Mostly ready" };
	if (pct >= 28) return { pct, tone: "warn", label: "Needs review" };
	return { pct: pct || 12, tone: "bad", label: "Not ready" };
}

function evidenceBand(score, fallback = "Not tested") {
	const pct = Math.max(0, Math.min(100, Math.round(Number(score || 0) * 100)));
	if (pct >= 80) return { pct, tone: "good", label: "Evidence ready" };
	if (pct >= 50) return { pct, tone: "warn", label: "Partial evidence" };
	return { pct: pct || 12, tone: "warn", label: fallback };
}

function countLabel(count, singular, plural = `${singular}s`) {
	const value = num(count, 0);
	return `${value} ${value === 1 ? singular : plural}`;
}

function normalizeServers(value) {
	if (Array.isArray(value)) return value;
	if (Array.isArray(value?.servers)) return value.servers;
	if (Array.isArray(value?.items)) return value.items;
	return [];
}

function normalizeInstances(value) {
	if (Array.isArray(value)) return value;
	if (Array.isArray(value?.instances)) return value.instances;
	if (Array.isArray(value?.items)) return value.items;
	return [];
}

function normalizeClients(value) {
	if (Array.isArray(value)) return value;
	if (Array.isArray(value?.targets)) return value.targets;
	if (Array.isArray(value?.clients)) return value.clients;
	return [];
}

function normalizeLeases(value) {
	if (Array.isArray(value)) return value;
	if (Array.isArray(value?.leases)) return value.leases;
	if (Array.isArray(value?.activeLeases)) return value.activeLeases;
	return [];
}

function groupByServer(instances) {
	const groups = new Map();
	for (const instance of instances) {
		const name = instance.server || instance.serverName || "server";
		if (!groups.has(name)) groups.set(name, []);
		groups.get(name).push(instance);
	}
	return groups;
}

function maxWorkers(server, instances) {
	if (serverMode(server, instances) === "disabled") return 0;
	const fromInstances = instances.reduce(
		(max, instance) => Math.max(max, num(instance.maxWorkers)),
		0,
	);
	return (
		num(server.maxWorkers, 0) ||
		num(server.parallelismLimit, 0) ||
		fromInstances ||
		1
	);
}

function maxInFlight(server, instances) {
	if (serverMode(server, instances) === "disabled") return 0;
	const fromInstances = instances.reduce(
		(max, instance) => Math.max(max, num(instance.maxInFlightPerWorker)),
		0,
	);
	return num(server.maxInFlightPerWorker, 0) || fromInstances || 1;
}

function renderAutoSetup(servers, instances) {
	const plan = autoPolicyPlan(servers, instances);
	const changeCount = plan.changes.length;
	if (els.serverAutoTitle) {
		els.serverAutoTitle.textContent = changeCount
			? `${changeCount} safe resource fix${changeCount === 1 ? "" : "es"} ready`
			: "No server clicking needed";
	}
	if (els.serverAutoBody) {
		els.serverAutoBody.textContent = changeCount
			? "Apply once and MCPace will set enabled servers to the safest low-resource routing available right now."
			: "Enabled sources already have automatic safety limits. Open the source panel only when you need routing or source metadata.";
	}
	if (els.serverAutoStats) {
		setSafeHtml(
			els.serverAutoStats,
			[
				chip(`${plan.enabled} on`, plan.enabled ? "good" : "warn"),
				chip(`${plan.protected} guarded`, plan.protected ? "warn" : "good"),
				chip(`${plan.ready} ready`, plan.ready ? "good" : "warn"),
				chip(`${changeCount} fixes`, changeCount ? "warn" : "good"),
			].join(""),
		);
	}
	if (els.autoTuneVisible) {
		els.autoTuneVisible.disabled = changeCount === 0;
		els.autoTuneVisible.textContent = changeCount
			? `Apply ${changeCount} safe fix${changeCount === 1 ? "" : "es"}`
			: "Safe plan active";
	}
}

function importResultPayload(value) {
	const result =
		value && typeof value === "object" ? value.result || value : {};
	return result && typeof result === "object" ? result : {};
}

function renderServerImportPanel() {
	if (!els.serverImportResult) return;
	updateServerImportPreflight();
	if (state.importer.loading) {
		setSafeHtml(
			els.serverImportResult,
			`<article class="item warn"><div class="item-head"><div class="name">Reading config…</div>${chip("running", "warn")}</div><div class="meta">MCPace is validating the local file and preparing a preview. No secret values are rendered here.</div></article>`,
		);
		return;
	}
	if (state.importer.error) {
		setSafeHtml(
			els.serverImportResult,
			`<article class="item bad"><div class="item-head"><div class="name">Import failed</div>${chip("error", "bad")}</div><div class="meta">${escapeHtml(state.importer.error)}</div></article>`,
		);
		return;
	}
	if (!state.importer.result) {
		setSafeHtml(
			els.serverImportResult,
			`<article class="item"><div class="item-head"><div class="name">No import run yet</div>${chip("idle", "warn")}</div><div class="meta">Recommended first move: import what you already use, then test one server at a time.</div></article>`,
		);
		return;
	}
	const payload = importResultPayload(state.importer.result);
	const entries = Array.isArray(payload.entries) ? payload.entries : [];
	const copied = num(
		payload.copiedCount ?? payload.importedCount ?? payload.updatedCount,
		entries.filter((entry) => entry.action !== "skipped").length,
	);
	const skipped = num(
		payload.skippedCount,
		entries.filter((entry) => entry.action === "skipped").length,
	);
	const dryRun = Boolean(payload.dryRun ?? state.importer.last?.dryRun);
	const disabled = Boolean(
		payload.disabled ?? state.importer.last?.disabled ?? true,
	);
	const force = Boolean(payload.force ?? state.importer.last?.force);
	const addCount = num(
		payload.addedCount ??
			payload.wouldAddCount ??
			payload.importedCount ??
			payload.copiedCount,
		entries.filter((entry) =>
			/add|copy|import|new|would/i.test(
				String(entry.action || entry.status || ""),
			),
		).length || copied,
	);
	const replaceCount = num(
		payload.replacedCount ?? payload.wouldReplaceCount ?? payload.updatedCount,
		entries.filter((entry) =>
			/replace|update|overwrite/i.test(
				String(entry.action || entry.status || ""),
			),
		).length,
	);
	const title = dryRun
		? `${entries.length || copied || 0} server${(entries.length || copied) === 1 ? "" : "s"} in preview`
		: `${copied} source${copied === 1 ? "" : "s"} copied ${disabled ? "disabled" : "as saved"}`;
	const tone = copied || entries.length ? "good" : "warn";
	const diff = `<div class="import-diff-grid" aria-label="Import change summary">
          <article class="import-diff-card"><span>Will add</span><strong>${escapeHtml(addCount)}</strong><p>New sources from the selected config.</p></article>
          <article class="import-diff-card"><span>Will replace</span><strong>${escapeHtml(replaceCount)}</strong><p>${escapeHtml(force ? "Force is on; duplicates may be overwritten." : "Duplicates stay protected unless force is on.")}</p></article>
          <article class="import-diff-card"><span>Will skip</span><strong>${escapeHtml(skipped)}</strong><p>Self entries, duplicates, or unsupported shapes.</p></article>
          <article class="import-diff-card"><span>Saved state</span><strong>${escapeHtml(disabled ? "Off" : "Source")}</strong><p>${escapeHtml(disabled ? "Imported sources stay parked until Review → Enable → Test." : "Imported enabled flags are preserved.")}</p></article>
        </div>`;
	const summary = `<article class="item ${tone}"><div class="item-head"><div class="name">${escapeHtml(title)}</div>${chip(dryRun ? "preview" : "saved", tone)}</div><div class="meta">${escapeHtml(skipped ? `${skipped} skipped. Review duplicates or MCPace self entries before forcing.` : "Next: review the imported row, enable deliberately, then run Test.")}</div></article>`;
	const entryHtml = entries
		.slice(0, 5)
		.map((entry) => {
			const name = entry.name || entry.server || entry.id || "server";
			const action =
				entry.action || entry.status || (dryRun ? "would copy" : "copied");
			const source =
				entry.sourcePath ||
				entry.source ||
				entry.type ||
				entry.command ||
				entry.url ||
				"source hidden";
			const entryTone = /skip|error|fail/i.test(action)
				? "bad"
				: /would|preview|copy|import|update/i.test(action)
					? "good"
					: "warn";
			return `<article class="item ${itemClass(entryTone)}"><div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(action, entryTone)}</div><div class="meta">${escapeHtml(source)}</div></article>`;
		})
		.join("");
	setSafeHtml(
		els.serverImportResult,
		`${diff}${summary}${entryHtml ? `<div class="discovery-candidate-list">${entryHtml}</div>` : ""}`,
	);
}

function clientPreferredConfigPath(client) {
	const supportPath = client?.installSupport?.preferredConfigPath;
	const paths = Array.isArray(client?.configPaths) ? client.configPaths : [];
	const candidates = [supportPath, ...paths]
		.filter(Boolean)
		.map((value) => String(value).trim())
		.filter(Boolean);
	return (
		candidates.find((path) => /\.json(?:$|\b)/i.test(path)) ||
		candidates[0] ||
		""
	);
}

function clientImportPathAllowed(client) {
	const path = clientPreferredConfigPath(client);
	return Boolean(
		path &&
			(/\.json(?:$|\b)/i.test(path) ||
				String(client?.configFormat || "")
					.toLowerCase()
					.includes("json")),
	);
}

function clientSetupTargets(clients) {
	const targets = Array.isArray(clients) ? clients : normalizeClients(clients);
	const weight = (client) => {
		let score = 0;
		if (client?.surfaceClass === "local") score -= 10;
		if (client?.installSupported) score -= 8;
		if (clientImportPathAllowed(client)) score -= 4;
		if (
			/claude|cursor|vscode|vs code|codex/i.test(
				`${client?.displayName || ""} ${client?.id || ""}`,
			)
		)
			score -= 3;
		if (String(client?.surfaceClass || "") === "cloud") score += 12;
		return score;
	};
	return [...targets].sort(
		(left, right) =>
			weight(left) - weight(right) ||
			String(left?.displayName || left?.id || "").localeCompare(
				String(right?.displayName || right?.id || ""),
			),
	);
}

function renderClientSetup(clients = [], _catalog = {}) {
	if (!els.clientSetupList) return;
	const targets = clientSetupTargets(clients);
	const local = targets.filter((client) => client?.surfaceClass === "local");
	const writable = local.filter((client) => client?.installSupported);
	if (!targets.length) {
		setSafeHtml(
			els.clientSetupList,
			`<article class="client-setup-card item warn"><div class="item-head"><div class="name">No client catalog returned</div>${chip("waiting", "warn")}</div><div class="meta">Import and manual server setup still work. Client patch actions appear after client list data loads.</div></article>`,
		);
		renderClientSetupResult();
		return;
	}
	const shown = targets.slice(0, 6);
	setSafeHtml(
		els.clientSetupList,
		shown
			.map((client) => {
				const id = client.id || client.clientTargetId || "client";
				const name = client.displayName || id;
				const localSurface = client.surfaceClass === "local";
				const supported = Boolean(client.installSupported);
				const path = clientPreferredConfigPath(client);
				const canImport = clientImportPathAllowed(client);
				const tone = supported ? "good" : localSurface ? "warn" : "bad";
				const meta = [
					client.surfaceKind || client.surfaceClass || "surface",
					client.configFormat
						? `format ${client.configFormat}`
						: "format unknown",
					path || "no local config path",
				]
					.filter(Boolean)
					.join(" · ");
				const ingress = Array.isArray(client.supportedIngresses)
					? client.supportedIngresses.slice(0, 3).join(", ")
					: "—";
				return `<article class="client-setup-card item ${itemClass(tone)}" data-client-id="${escapeHtml(id)}" data-client-path="${escapeHtml(path)}">
            <div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(supported ? "patchable" : localSurface ? "manual" : "cloud", tone)}</div>
            <div class="meta">${escapeHtml(meta)}</div>
            <div class="tags">${tags([`ingress ${ingress}`, client.installSupport?.preferredScope ? `scope ${client.installSupport.preferredScope}` : "scope manual", canImport ? "importable JSON" : "not import-first"])}</div>
            <div class="client-setup-actions" aria-label="Client actions for ${escapeHtml(name)}">
              ${canImport ? `<button type="button" data-client-setup-action="use-import-path" data-client-path="${escapeHtml(path)}">Use import path</button>` : ""}
              ${supported ? `<button type="button" data-client-setup-action="preview-client" data-client-id="${escapeHtml(id)}">Preview patch</button><button class="primary" type="button" data-client-setup-action="install-client" data-client-id="${escapeHtml(id)}">Apply patch</button><button type="button" data-client-setup-action="restore-client" data-client-id="${escapeHtml(id)}">Restore</button>` : `<button type="button" data-client-setup-action="show-client" data-client-id="${escapeHtml(id)}">Review client</button>`}
            </div>
          </article>`;
			})
			.join("") +
			(targets.length > shown.length
				? `<div class="note">${targets.length - shown.length} more client target(s) stay in Diagnostics.</div>`
				: ""),
	);
	if (els.clientPreviewAll)
		els.clientPreviewAll.disabled =
			!writable.length || state.clientSetup.loading;
	if (els.clientApplyAll)
		els.clientApplyAll.disabled = !writable.length || state.clientSetup.loading;
	if (els.clientRestoreAll)
		els.clientRestoreAll.disabled =
			!writable.length || state.clientSetup.loading;
	renderClientSetupResult();
}

function clientActionPayload(value) {
	return value?.result || value || {};
}

function renderClientSetupResult() {
	if (!els.clientSetupResult) return;
	if (state.clientSetup.loading) {
		setSafeHtml(
			els.clientSetupResult,
			`<article class="item warn"><div class="item-head"><div class="name">Running client action…</div>${chip("working", "warn")}</div><div class="meta">MCPace is using the CLI action and will show changed/would-change status here.</div></article>`,
		);
		return;
	}
	if (state.clientSetup.error) {
		setSafeHtml(
			els.clientSetupResult,
			`<article class="item bad"><div class="item-head"><div class="name">Client action failed</div>${chip("error", "bad")}</div><div class="meta">${escapeHtml(state.clientSetup.error)}</div></article>`,
		);
		return;
	}
	if (!state.clientSetup.result) {
		setSafeHtml(
			els.clientSetupResult,
			`<article class="item"><div class="item-head"><div class="name">No client patch run yet</div>${chip("idle", "warn")}</div><div class="meta">Use Preview first. Apply writes are explicit and restorable.</div></article>`,
		);
		return;
	}
	const payload = clientActionPayload(state.clientSetup.result);
	const installed = Array.isArray(payload.installed)
		? payload.installed
		: payload.clientTargetId
			? [payload]
			: [];
	const failed = Array.isArray(payload.failed) ? payload.failed : [];
	const skipped = Array.isArray(payload.skipped) ? payload.skipped : [];
	const changed = installed.filter(
		(item) => item.changed || item.wouldChange || item.persisted,
	).length;
	const dryRun = Boolean(
		payload.dryRun ?? installed.some((item) => item.dryRun),
	);
	const restoreMode = /restore/i.test(
		String(payload.mode || state.clientSetup.last?.action || ""),
	);
	const title = restoreMode
		? `Restore ${payload.clientTargetId || state.clientSetup.last?.clientId || "client"}`
		: installed.length
			? `${installed.length} client patch${installed.length === 1 ? "" : "es"} ${dryRun ? "previewed" : "processed"}`
			: payload.clientTargetId
				? `${payload.clientTargetId} processed`
				: "Client action complete";
	const tone = failed.length ? "bad" : changed || restoreMode ? "good" : "warn";
	const cards = installed
		.slice(0, 4)
		.map((item) => {
			const name = item.displayName || item.clientTargetId || "client";
			const meta = [
				item.configPath,
				item.dryRun ? "preview only" : item.persisted ? "written" : "no write",
				item.backupId ? `backup ${item.backupId}` : "backup pending",
			]
				.filter(Boolean)
				.join(" · ");
			const itemTone =
				item.persisted || item.changed || item.wouldChange ? "good" : "warn";
			return `<article class="item ${itemClass(itemTone)}"><div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(item.dryRun ? "preview" : item.persisted ? "written" : "checked", itemTone)}</div><div class="meta">${escapeHtml(meta)}</div>${item.diff ? `<pre class="client-result-diff">${escapeHtml(shortText(item.diff, 2000))}</pre>` : ""}</article>`;
		})
		.join("");
	const restoreCard = restoreMode
		? `<article class="item good"><div class="item-head"><div class="name">${escapeHtml(payload.clientTargetId || "client")}</div>${chip("restored", "good")}</div><div class="meta">${escapeHtml([payload.configPath, payload.backupId].filter(Boolean).join(" · ") || "Latest backup restored.")}</div></article>`
		: "";
	const failedCard = failed.length
		? `<article class="item bad"><div class="item-head"><div class="name">${failed.length} failed</div>${chip("check", "bad")}</div><div class="meta">${escapeHtml(
				failed
					.slice(0, 3)
					.map(
						(item) =>
							`${item.clientTargetId || "client"}: ${item.error || "failed"}`,
					)
					.join(" · "),
			)}</div></article>`
		: "";
	const skippedNote = skipped.length
		? `<div class="note">${skipped.length} skipped: ${escapeHtml(skipped.slice(0, 3).join(" · "))}</div>`
		: "";
	setSafeHtml(
		els.clientSetupResult,
		`<article class="item ${tone}"><div class="item-head"><div class="name">${escapeHtml(title)}</div>${chip(dryRun ? "preview" : restoreMode ? "restored" : "done", tone)}</div><div class="meta">${escapeHtml(dryRun ? "Nothing was written. Apply only after the patch looks right." : restoreMode ? "Rollback completed from the selected backup." : "Client config action completed; refresh shows updated catalog state.")}</div></article>${cards}${restoreCard}${failedCard}${skippedNote}`,
	);
}

function fillClientImportPath(path) {
	if (!path || !els.serverImportPath) return;
	updateSetupToolsState("import");
	els.serverImportPath.value = path;
	state.importer.error = null;
	renderServerImportPanel();
	revealElementById("server-import-panel", "center");
	window.setTimeout(() => els.serverImportPath?.focus?.(), 120);
}

async function runClientSetupAction(
	action,
	payload,
	control,
	busyLabel = "Working…",
) {
	const originalText =
		control && "textContent" in control ? control.textContent : "";
	try {
		state.clientSetup.loading = true;
		state.clientSetup.error = null;
		state.clientSetup.last = { action, ...payload };
		renderClientSetupResult();
		if (control) {
			control.disabled = true;
			if (control.tagName === "BUTTON") control.textContent = busyLabel;
		}
		const response = await postServerAction(action, payload);
		state.clientSetup.result = response;
		state.clientSetup.error = null;
		if (action === "client-install" && payload?.dryRun !== true)
			await refreshDashboard({ force: true, reason: action });
		else renderClientSetupResult();
	} catch (error) {
		state.clientSetup.error = apiErrorMessage(error);
		state.lastError = state.clientSetup.error;
		renderError(state.clientSetup.error);
	} finally {
		state.clientSetup.loading = false;
		if (control) {
			control.disabled = false;
			if (control.tagName === "BUTTON") control.textContent = originalText;
		}
		renderClientSetup(
			normalizeClients(state.overview?.clients || []),
			state.overview?.clients || {},
		);
	}
}

function handleClientSetupClick(event) {
	const control = event.target.closest("[data-client-setup-action]");
	if (!control) return;
	const action = control.dataset.clientSetupAction;
	const clientId =
		control.dataset.clientId ||
		control.closest("[data-client-id]")?.dataset?.clientId;
	const path =
		control.dataset.clientPath ||
		control.closest("[data-client-path]")?.dataset?.clientPath;
	if (action === "use-import-path") {
		fillClientImportPath(path);
		return;
	}
	if (action === "show-client") {
		revealElementById("deep-diagnostics", "start");
		return;
	}
	if (!clientId) return;
	if (action === "preview-client") {
		runClientSetupAction(
			"client-install",
			{ clientId, dryRun: true, diff: true },
			control,
			"Previewing…",
		);
		return;
	}
	if (action === "install-client") {
		runClientSetupAction(
			"client-install",
			{ clientId, dryRun: false, diff: false },
			control,
			"Applying…",
		);
		return;
	}
	if (action === "restore-client") {
		if (
			!window.confirm(
				`Restore the latest MCPace client backup for ${clientId}?`,
			)
		)
			return;
		runClientSetupAction(
			"client-restore",
			{ clientId, backup: "latest" },
			control,
			"Restoring…",
		);
	}
}

function renderAutomation(overview = {}, servers = [], instances = []) {
	if (!els.automationGrid) return;
	const plan = autoPolicyPlan(servers, instances);
	const cache = overview.cache || {};
	const cachedTools = overview.cachedToolEvidence || {};
	const automation = overview.automation || {};
	const discoveryControl =
		overview.discoveryControl || automation.discoveryJob || {};
	const runtimeControl = overview.runtimeControlPlane?.summary || {};
	const refreshLabel =
		state.refreshMode === "paused"
			? "Paused"
			: `${Math.round((REFRESH_MS[state.refreshMode] || automation.overviewRefresh?.intervalMs || 0) / 1000)}s`;
	const refreshTone = state.refreshMode === "paused" ? "warn" : "good";
	const cacheTone = cache.stale || cache.refreshError ? "warn" : "good";
	const toolServerCount = num(
		cachedTools.serverCount ?? automation.toolEvidenceCache?.serverCount,
	);
	const toolOk = num(cachedTools.okCount);
	const toolMiss = num(cachedTools.cacheMissCount);
	const toolFailed = num(cachedTools.failedCount);
	const toolTone = toolFailed
		? "bad"
		: toolMiss
			? "warn"
			: toolServerCount
				? "good"
				: "warn";
	const policyTone = plan.changes.length
		? "warn"
		: servers.length
			? "good"
			: "warn";
	const importTone = automation.serverSources?.importSupported
		? "good"
		: "warn";
	const registryCache =
		discoveryControl.registryCache ||
		automation.discoveryJob?.registryCache ||
		{};
	const registryTone = registryCache.exists
		? "good"
		: discoveryControl.enabled || automation.discoveryJob?.enabled
			? "warn"
			: "bad";
	const discoveryTone =
		discoveryControl.enabled || automation.discoveryJob?.enabled
			? "good"
			: "warn";
	if (els.automationTitle) {
		els.automationTitle.textContent = plan.changes.length
			? `${plan.changes.length} conservative policy change${plan.changes.length === 1 ? "" : "s"} ready`
			: "Automatic work is under control";
	}
	if (els.automationBody) {
		els.automationBody.textContent =
			"Live fields refresh automatically; import/discovery/config changes stay explicit. The dashboard separates live state, stored config, derived policy, and hidden secrets.";
	}
	setSafeHtml(
		els.automationGrid,
		[
			`<article class="automation-card ${refreshTone}"><span>Auto refresh</span><strong>${escapeHtml(refreshLabel)}</strong><em>local view preference</em></article>`,
			`<article class="automation-card ${cacheTone}"><span>Overview cache</span><strong>${escapeHtml(cache.hit ? "hit" : cache.bypassed ? "bypass" : "fresh")}</strong><em>age ${escapeHtml(fmtMs(cache.ageMs))} · ttl ${escapeHtml(fmtMs(cache.ttlMs))}</em></article>`,
			`<article class="automation-card ${importTone}"><span>Import existing</span><strong>${escapeHtml(automation.serverSources?.baseFile || "mcp_settings")}</strong><em>${num(automation.serverSources?.includeDirCount)} include dirs · preview first</em></article>`,
			`<article class="automation-card ${discoveryTone}"><span>Discovery</span><strong>${escapeHtml(discoveryControl.mode || automation.discoveryJob?.mode || "manual")}</strong><em>${escapeHtml(discoveryControl.autoInstall || automation.discoveryJob?.autoInstall || "manual-only")} · unknown ${escapeHtml(discoveryControl.installUnknown || automation.discoveryJob?.unknownServers || "plan-only")}</em></article>`,
			`<article class="automation-card ${registryTone}"><span>Registry cache</span><strong>${escapeHtml(registryCache.exists ? "ready" : "missing")}</strong><em>${escapeHtml(registryCache.configuredPath || "catalog/registry-cache.json")}</em></article>`,
			`<article class="automation-card ${toolTone}"><span>Tool cache</span><strong>${toolOk}/${toolServerCount || servers.length}</strong><em>${num(cachedTools.toolCount ?? automation.toolEvidenceCache?.toolCount)} tools · ${toolMiss} miss · ${toolFailed} fail</em></article>`,
			`<article class="automation-card ${policyTone}"><span>Policy plan</span><strong>${plan.changes.length}</strong><em>${num(runtimeControl.serialized)} serialized · ${num(runtimeControl.sharedOk)} shared/pool</em></article>`,
		].join(""),
	);
}

function discoveryPayload(value) {
	return value?.result || value || {};
}

function renderDiscoveryPanel() {
	if (!els.serverDiscoveryResults) return;
	if (state.discovery.loading) {
		setSafeHtml(
			els.serverDiscoveryResults,
			`<article class="item warn"><div class="item-head"><div class="name">Searching candidates…</div>${chip("running", "warn")}</div><div class="meta">Preview checks local approved/registry cache and returns a plan. No server is enabled from this step.</div></article>`,
		);
		return;
	}
	if (state.discovery.error) {
		setSafeHtml(
			els.serverDiscoveryResults,
			`<article class="item bad"><div class="item-head"><div class="name">Discovery failed</div>${chip("error", "bad")}</div><div class="meta">${escapeHtml(state.discovery.error)}</div></article>`,
		);
		return;
	}
	const payload = discoveryPayload(state.discovery.result);
	const candidates = Array.isArray(payload.candidates)
		? payload.candidates
		: [];
	const automatic = Array.isArray(payload.automaticInstallResults)
		? payload.automaticInstallResults
		: [];
	const probes = Array.isArray(payload.postInstallProbeResults)
		? payload.postInstallProbeResults
		: [];
	if (!state.discovery.result) {
		setSafeHtml(
			els.serverDiscoveryResults,
			`<article class="item"><div class="item-head"><div class="name">No discovery run yet</div>${chip("idle", "warn")}</div><div class="meta">Use Preview first. MCPace should not install, enable, or expose a server without an explicit user action.</div></article>`,
		);
		return;
	}
	const decision =
		payload.installDecision ||
		(state.discovery.lastMode === "install" ? "install requested" : "preview");
	const block = payload.installBlockReason || payload.warning || "";
	const summary = `<article class="item ${automatic.length ? "good" : candidates.length ? "warn" : ""}">
          <div class="item-head"><div class="name">${escapeHtml(num(payload.candidateCount, candidates.length))} candidate${num(payload.candidateCount, candidates.length) === 1 ? "" : "s"}</div>${chip(decision, automatic.length ? "good" : candidates.length ? "warn" : "bad")}</div>
          <div class="meta">${escapeHtml(block || "Preview returned a ranked plan. Install still requires an explicit mode and keeps sources disabled by default.")}</div>
          ${automatic.length ? `<div class="tags">${tags(automatic.slice(0, 4).map((item) => `${item.name || "server"}: ${item.decision || "result"}`))}</div>` : ""}
          ${probes.length ? `<div class="tags">${tags(probes.slice(0, 4).map((item) => `${item.server || item.name || "server"} probed`))}</div>` : ""}
        </article>`;
	const candidateHtml = candidates
		.slice(0, 5)
		.map((candidate) => {
			const trust = candidate.trustLevel || "review";
			const tone = candidate.installed
				? "good"
				: trust === "approved" || trust === "trusted"
					? "good"
					: trust === "review"
						? "warn"
						: "bad";
			const title = candidate.title || candidate.name || "candidate";
			const spec =
				candidate.installSpec || candidate.package || candidate.url || "";
			const meta = [
				candidate.source,
				candidate.registryType,
				candidate.transport,
				candidate.recommendedMode ? `mode ${candidate.recommendedMode}` : "",
				candidate.score !== undefined ? `score ${candidate.score}` : "",
			]
				.filter(Boolean)
				.join(" · ");
			return `<article class="item discovery-candidate ${itemClass(tone)}">
            <div>
              <div class="item-head"><div class="name">${escapeHtml(title)}</div>${chip(trust, tone)}</div>
              <div class="meta">${escapeHtml(candidate.description || meta || "No description returned.")}</div>
              ${meta ? `<div class="tags">${tags(meta.split(" · "))}</div>` : ""}
              ${spec ? `<code class="discovery-install-spec">${escapeHtml(spec)}</code>` : ""}
            </div>
            <span class="chip">${candidate.installed ? `${dot("good")}installed` : `${dot("warn")}not installed`}</span>
          </article>`;
		})
		.join("");
	setSafeHtml(
		els.serverDiscoveryResults,
		`${summary}${candidateHtml ? `<div class="discovery-candidate-list">${candidateHtml}</div>` : `<div class="empty-state"><strong>No candidates matched.</strong><p>Try a broader term or paste a command manually.</p><div class="empty-actions"><button class="primary" type="button" data-empty-action="add-server">Paste command</button></div></div>`}`,
	);
}

function normalizeOperatorPlan(value) {
	const plan = value && typeof value === "object" ? value : {};
	return {
		schema: plan.schema || "mcpace.operatorPlan.v0",
		summary: plan.summary || {},
		items: Array.isArray(plan.items) ? plan.items : [],
		flow: Array.isArray(plan.flow) ? plan.flow : [],
	};
}

function operatorPlanForServer(name) {
	const plan = normalizeOperatorPlan(state.overview?.operatorPlan);
	const wanted = String(name || "");
	return plan.items.find((item) => String(item?.name || "") === wanted) || null;
}

function operatorPlanTone(plan) {
	if (!plan) return "warn";
	if (plan.tone) return plan.tone;
	if (plan.lane === "ready") return "good";
	if (plan.lane === "blocked") return "bad";
	return "warn";
}

function operatorCommandChips(commands, limit = 4) {
	const items = Array.isArray(commands) ? commands.slice(0, limit) : [];
	if (!items.length)
		return `<span class="note">No runbook commands yet.</span>`;
	return items
		.map(
			(command) =>
				`<span class="operator-command"><strong>${escapeHtml(command.label || "Run")}</strong>${escapeHtml(shortText(command.command || "", 118))}</span>`,
		)
		.join("");
}

function renderOperatorPlan(rawPlan, servers = [], _instances = []) {
	const plan = normalizeOperatorPlan(rawPlan);
	const summary = plan.summary || {};
	const items = [...plan.items].sort(
		(left, right) =>
			num(left.priority, 9) - num(right.priority, 9) ||
			String(left.name || "").localeCompare(String(right.name || "")),
	);
	const blocked = num(summary.blocked);
	const unchecked = num(summary.unchecked);
	const guarded = num(summary.guarded);
	const ready = num(summary.ready);
	const changes = num(summary.policyChanges);
	const total = num(summary.total, servers.length);
	const top = items[0];
	const title = blocked
		? `${blocked} blocked server${blocked === 1 ? "" : "s"}`
		: unchecked
			? `${unchecked} server${unchecked === 1 ? "" : "s"} need live evidence`
			: changes
				? `${changes} policy correction${changes === 1 ? "" : "s"} ready`
				: ready
					? `${ready} server${ready === 1 ? "" : "s"} ready for brokered use`
					: "No active server plan yet";
	const body = top
		? `${top.nextAction || "Review next action"}. ${top.rationale || "Backend operator plan is active."}`
		: "Add or import an MCP server, keep it parked first, then enable deliberately and run Test to collect initialize + tools/list evidence.";
	if (els.operatorPlanTitle) els.operatorPlanTitle.textContent = title;
	if (els.operatorPlanBody) els.operatorPlanBody.textContent = body;
	if (els.operatorPlanStats) {
		setSafeHtml(
			els.operatorPlanStats,
			[
				chip(`${total} total`, total ? "good" : "warn"),
				chip(`${blocked} blocked`, blocked ? "bad" : "good"),
				chip(`${unchecked} unchecked`, unchecked ? "warn" : "good"),
				chip(`${guarded} guarded`, guarded ? "warn" : "good"),
				chip(`${ready} ready`, ready ? "good" : "warn"),
				chip(`${changes} policy fixes`, changes ? "warn" : "good"),
			].join(""),
		);
	}
	if (els.operatorPlanLanes) {
		const laneOrder = ["blocked", "unchecked", "guarded", "ready", "off"];
		const cards = laneOrder
			.map((lane) => {
				const laneItems = items.filter((item) => item.lane === lane);
				if (!laneItems.length) return "";
				const lead = laneItems[0];
				const tone = operatorPlanTone(lead);
				return `<article class="operator-plan-card ${itemClass(tone)}">
              <div class="label">${escapeHtml(lane)} · ${laneItems.length}</div>
              <strong>${escapeHtml(lead.name || "server")}: ${escapeHtml(lead.nextAction || "review")}</strong>
              <p>${escapeHtml(lead.rationale || lead.evidence || "No rationale available.")}</p>
              <div class="operator-command-list" style="margin-top: 8px;">${operatorCommandChips(lead.commands, 2)}</div>
            </article>`;
			})
			.filter(Boolean);
		setSafeHtml(
			els.operatorPlanLanes,
			cards.length
				? cards.join("")
				: `<div class="empty">No server operator lanes yet.</div>`,
		);
	}
	if (els.operatorPlanFlow) {
		const flow = plan.flow.length
			? plan.flow
			: [
					{
						stage: "Client",
						description:
							"User clients talk to /mcp, not directly to server commands.",
					},
					{
						stage: "Broker",
						description:
							"MCPace checks policy, leases, route state, and evidence.",
					},
					{
						stage: "Source",
						description:
							"Server source stays explicit, reversible, and disabled when blocked.",
					},
					{
						stage: "Evidence",
						description:
							"initialize + tools/list proof decides what the normal view may trust.",
					},
				];
		setSafeHtml(
			els.operatorPlanFlow,
			flow
				.slice(0, 4)
				.map(
					(step, index) =>
						`<article class="flow-card"><span class="flow-index">${String(index + 1).padStart(2, "0")}</span><strong>${escapeHtml(step.stage || step.label || "stage")}</strong><p>${escapeHtml(step.description || step.body || "Safe broker stage.")}</p></article>`,
				)
				.join(""),
		);
	}
}

function runtimeControlForServer(name) {
	const items = state.overview?.runtimeControlPlane?.items;
	if (!Array.isArray(items)) return null;
	return items.find((item) => item.name === name) || null;
}

function renderRuntimeControl(item) {
	if (!item) return "";
	const risk = item.toolRisk || {};
	const parallel = item.parallelism || {};
	const isolation = item.isolation || {};
	const budget = item.resourceBudget || {};
	const labels = [
		`evidence ${text(item.evidenceState, "unknown")}`,
		`risk ${text(risk.risk, "unknown")}`,
		`isolation ${text(isolation.mode, "unknown")}`,
		`budget ${text(budget.class, "unknown")}`,
	];
	const signals =
		Array.isArray(risk.signals) && risk.signals.length
			? risk.signals.slice(0, 4).join(", ")
			: "no tool-risk signals yet";
	return `
          <section class="server-explain-box" aria-label="Runtime control plane">
            <div class="label">Runtime control plane</div>
            <p>${escapeHtml(item.why || "MCPace combines live evidence, tool risk, route policy, isolation, and resource budget before widening concurrency.")}</p>
            <div class="tags">${tags(labels)}</div>
            <div class="detail-grid" style="margin-top: 10px;">
              ${detail("Tool risk", `${text(risk.risk, "unknown")} · approval ${risk.approvalRequired ? "required" : "not required"}`)}
              ${detail("Risk signals", signals)}
              ${detail("Parallelism", `${text(parallel.mode, "serialized")} · ${text(parallel.admission, "lease-gated")}`)}
              ${detail("Locks", text(parallel.lockScope, "none"))}
              ${detail("Isolation", `${text(isolation.mode, "native-restricted")} · container ${isolation.containerCompatible ? "possible" : "not default"}`)}
              ${detail("Resource budget", `${text(budget.class, "guarded")} · ${text(budget.memoryHintMb, "?")} MB hint · ${text(budget.monitor, "monitor when live")}`)}
            </div>
          </section>
        `;
}

function renderServerRunbook(plan) {
	if (!plan) return "";
	const blockers = Array.isArray(plan.blockers) ? plan.blockers : [];
	const safeguards = Array.isArray(plan.safeguards) ? plan.safeguards : [];
	const commands = Array.isArray(plan.commands) ? plan.commands : [];
	const blockerList = blockers.length
		? `<section><div class="label">Blockers</div><ul class="server-checklist">${blockers.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul></section>`
		: "";
	const safeguardList = safeguards.length
		? `<section><div class="label">Safeguards</div><ul class="server-checklist">${safeguards.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul></section>`
		: "";
	return `<section class="server-explain-box">
          <div class="label">Backend operator runbook</div>
          <p><strong>${escapeHtml(plan.nextAction || "Review")}</strong> · ${escapeHtml(plan.rationale || "No backend rationale available.")}</p>
          <div class="operator-command-list">${operatorCommandChips(commands, 6)}</div>
          <div class="server-explain-grid" style="margin-top: 10px;">${blockerList}${safeguardList}</div>
        </section>`;
}

function commandLineLooksComposed(value) {
	const chars = [...String(value || "")];
	let singleQuoted = false;
	let doubleQuoted = false;
	let escaped = false;
	for (let index = 0; index < chars.length; index += 1) {
		const ch = chars[index];
		if (escaped) {
			escaped = false;
			continue;
		}
		if (ch === "\\" && !singleQuoted) {
			escaped = true;
			continue;
		}
		if (ch === "'" && !doubleQuoted) {
			singleQuoted = !singleQuoted;
			continue;
		}
		if (ch === '"' && !singleQuoted) {
			doubleQuoted = !doubleQuoted;
			continue;
		}
		if (singleQuoted || doubleQuoted) continue;
		if (["`", ";", "|", "<", ">"].includes(ch)) return true;
		if (ch === "&" && chars[index + 1] === "&") return true;
		if (ch === "$" && chars[index + 1] === "(") return true;
	}
	return false;
}

function installCommandIntent(value) {
	const raw = String(value || "").trim();
	if (!raw)
		return {
			tone: "warn",
			label: "Waiting",
			body: "Paste one launcher command, package spec, local path, or Streamable HTTP URL.",
		};
	if (commandLineLooksComposed(raw)) {
		return {
			tone: "bad",
			label: "Rejected",
			body: "Use one command or URL only. Remove shell chaining, pipes, redirects, backticks, or command substitutions.",
		};
	}
	if (/^https?:\/\//i.test(raw)) {
		return {
			tone: "good",
			label: "HTTP source",
			body: "Will save a Streamable HTTP source. MCPace will not call it until you explicitly test and enable it.",
		};
	}
	const launcher = raw.split(/\s+/)[0] || "";
	if (
		[
			"npx",
			"pnpm",
			"yarn",
			"bunx",
			"uvx",
			"python",
			"python3",
			"node",
			"deno",
		].includes(launcher)
	) {
		return {
			tone: "warn",
			label: "Launcher",
			body: `Will save ${launcher} as a server launcher. Keep it parked, then enable deliberately and run Test to collect tools/list evidence.`,
		};
	}
	if (/^(\.|~|\/|[A-Za-z]:[\\/])/.test(raw)) {
		return {
			tone: "warn",
			label: "Local path",
			body: "Will save a local command/path. Check working directory and environment names before enabling.",
		};
	}
	return {
		tone: "warn",
		label: "Package or command",
		body: "Will save this as a server source. Prefer trusted registries and review the resolved launch command before enabling.",
	};
}

function updateServerInstallPreflight() {
	const intent = installCommandIntent(els.serverInstallCommand?.value || "");
	setInstallNote(`${intent.label}: ${intent.body}`, intent.tone);
	setFieldError(
		els.serverInstallError,
		els.serverInstallCommand,
		intent.tone === "bad" ? intent.body : "",
	);
	if (els.serverInstallButton)
		els.serverInstallButton.disabled = intent.tone === "bad";
}

function importPathIntent(value) {
	const raw = String(value || "").trim();
	if (!raw)
		return {
			tone: "warn",
			label: "Waiting",
			body: "Paste a local MCP settings JSON path exported by another client.",
		};
	if (
		raw.includes(String.fromCharCode(0)) ||
		raw.includes(String.fromCharCode(10)) ||
		raw.includes(String.fromCharCode(13))
	)
		return {
			tone: "bad",
			label: "Rejected",
			body: "Use a single local file path without newlines or control characters.",
		};
	if (/^https?:\/\//i.test(raw))
		return {
			tone: "bad",
			label: "Remote URL",
			body: "Import accepts local config files only. Add remote HTTP servers through Add server instead.",
		};
	if (!/\.json$/i.test(raw))
		return {
			tone: "warn",
			label: "Check path",
			body: "This can still work, but MCP settings imports are usually JSON files.",
		};
	return {
		tone: "good",
		label: "Ready",
		body: "Preview will read the file and list copied servers without enabling them.",
	};
}

function updateServerImportPreflight() {
	const intent = importPathIntent(els.serverImportPath?.value || "");
	const dryRun = els.serverImportDryRun?.checked !== false;
	setImportNote(`${intent.label}: ${intent.body}`, intent.tone);
	setFieldError(
		els.serverImportError,
		els.serverImportPath,
		intent.tone === "bad" ? intent.body : "",
	);
	if (els.serverImportButton) {
		els.serverImportButton.disabled =
			intent.tone === "bad" || state.importer.loading;
		const disabledLabel =
			els.serverImportDisabled?.checked !== false ? "disabled" : "as saved";
		els.serverImportButton.textContent = dryRun
			? "Preview import"
			: `Import ${disabledLabel}`;
	}
}

function serverControls(server, related, placement = "row") {
	const name = escapeHtml(server.name || "");
	const enabled = Boolean(server.effectiveEnabled);
	const recommendation = recommendedPolicy(server, related);
	const needsTuning = policyNeedsTuning(server, related, recommendation);
	const evidence = serverToolEvidence(server);
	const testLabel = evidence.checked ? "Re-test" : "Test";
	const applyLabel = needsTuning
		? `Apply ${recommendation.label}`
		: `Policy ${recommendation.label}`;
	if (placement === "row") {
		if (!enabled) {
			return `
              <button class="primary" type="button" data-server-name="${name}" data-server-action="enable-test">Enable &amp; test</button>
              <button class="quiet" type="button" data-server-name="${name}" data-server-action="settings">Open source</button>
            `;
		}
		const evidenceNeedsTest = !evidence.checked || !evidence.ok;
		const primaryAction = evidenceNeedsTest
			? "test"
			: needsTuning
				? "routing"
				: "settings";
		const primaryLabel = evidenceNeedsTest
			? testLabel
			: needsTuning
				? "Review routing"
				: "Open source";
		return evidenceNeedsTest
			? `
              <button class="primary" type="button" data-server-name="${name}" data-server-action="${primaryAction}">${escapeHtml(primaryLabel)}</button>
              <button class="quiet" type="button" data-server-name="${name}" data-server-action="settings">Open source</button>
            `
			: `<button class="primary" type="button" data-server-name="${name}" data-server-action="${primaryAction}">${escapeHtml(primaryLabel)}</button>`;
	}
	return `
          <button class="server-toggle ${enabled ? "on" : "off"}" type="button" aria-pressed="${enabled}" data-server-name="${name}" data-server-action="toggle">
            ${enabled ? "Turn off" : "Turn on"}
          </button>
          <button type="button" data-server-name="${name}" data-server-action="auto"${!enabled || !needsTuning ? " disabled" : ""}>${!enabled ? "Enable to apply" : escapeHtml(applyLabel)}</button>
          <button type="button" data-server-name="${name}" data-server-action="${enabled ? "test" : "enable-test"}">${enabled ? escapeHtml(testLabel) : "Enable &amp; test"}</button>
        `;
}

function buildPolicyRows(servers, instances) {
	const groups = groupByServer(instances);
	return servers
		.map((server) => {
			const risk = riskForServer(server, groups.get(server.name) || []);
			const policy =
				server.runtimeType === "stateless"
					? `Can share safely · ${server.effectClass || "read-only"}`
					: `${server.runtimeType || "unknown"} · ${server.concurrencyPolicy || "routing unknown"}`;
			return { server, risk, policy };
		})
		.sort(
			(a, b) =>
				a.risk.rank - b.risk.rank ||
				String(a.server.name || "").localeCompare(String(b.server.name || "")),
		);
}

function shortNames(rows, limit = 6) {
	const names = rows.map(
		(row) => row.server?.name || row.serverName || row.name || "server",
	);
	const visible = names.slice(0, limit).join(", ");
	return names.length > limit
		? `${visible}, +${names.length - limit} more`
		: visible;
}

function buildAttentionItems(hub, readiness, policyRows, _leases) {
	const items = [];
	const hubWarnings = Array.isArray(hub.warnings)
		? hub.warnings.map(String).filter(Boolean)
		: [];
	if (hubWarnings.length) {
		items.push({
			title: "Runtime state can be repaired",
			meta: `${hubWarnings.length} warning${hubWarnings.length === 1 ? "" : "s"}. Use Repair if runtime status looks stale; server auto policy is already handled separately.`,
			tone: "warn",
			tag: "repair",
		});
	}
	if (Array.isArray(readiness.missingRequiredSourceEnablement)) {
		for (const name of readiness.missingRequiredSourceEnablement) {
			items.push({
				title: "Required source is disabled",
				meta: String(name),
				tone: "bad",
				tag: "required",
			});
		}
	}
	if (Array.isArray(readiness.missingRequiredCommands)) {
		for (const command of readiness.missingRequiredCommands) {
			items.push({
				title: "Required command is missing",
				meta: String(command),
				tone: "bad",
				tag: "setup",
			});
		}
	}

	const activePolicyRows = policyRows.filter(
		(row) => row.server?.effectiveEnabled || row.risk.rank <= 1,
	);
	const requiredDisabled = activePolicyRows.filter(
		(row) => row.risk.rank === 1,
	);

	if (requiredDisabled.length) {
		items.push({
			title: `${requiredDisabled.length} server${requiredDisabled.length === 1 ? "" : "s"} need source/profile setup`,
			meta: `${shortNames(requiredDisabled)}. Fix enablement, then run Test to collect tools/list evidence.`,
			tone: "bad",
			tag: "setup",
		});
	}
	const failedEvidence = activePolicyRows.filter((row) => {
		const evidence = serverToolEvidence(row.server);
		return row.server?.effectiveEnabled && evidence.checked && !evidence.ok;
	});
	if (failedEvidence.length) {
		items.push({
			title: `${failedEvidence.length} enabled source${failedEvidence.length === 1 ? "" : "s"} failed Test`,
			meta: `${shortNames(failedEvidence)}. Open the row, review the failure, then retry Test or park the source.`,
			tone: "bad",
			tag: "test failed",
		});
	}
	const uncheckedEvidence = activePolicyRows.filter((row) => {
		const evidence = serverToolEvidence(row.server);
		return row.server?.effectiveEnabled && !evidence.checked;
	});
	if (uncheckedEvidence.length) {
		items.push({
			title: `${uncheckedEvidence.length} source${uncheckedEvidence.length === 1 ? "" : "s"} need Test`,
			meta: `${shortNames(uncheckedEvidence)}. Run Test before relying on tools or treating the source as ready.`,
			tone: "warn",
			tag: "evidence",
		});
	}
	const seen = new Set();
	return items.filter((item) => {
		const key = `${item.title}\n${item.meta}`;
		if (seen.has(key)) return false;
		seen.add(key);
		return true;
	});
}

function updateRefreshChip() {
	let label = "manual";
	let tone = "warn";
	if (state.refreshing) {
		label = "refreshing";
		tone = "warn";
	} else if (state.lastError) {
		label = "refresh failed";
		tone = "bad";
	} else if (state.refreshMode === "paused") {
		label = "paused";
		tone = "warn";
	} else if (document.visibilityState === "hidden") {
		label = "background";
		tone = "warn";
	} else {
		label = `auto ${Math.round((REFRESH_MS[state.refreshMode] || 0) / 1000)}s`;
		tone = "good";
	}
	setChip(els.refreshChip, label, tone);
}

function scheduleRefresh() {
	if (state.timer !== null) window.clearTimeout(state.timer);
	const base = REFRESH_MS[state.refreshMode] || 0;
	if (!base) {
		state.timer = null;
		updateRefreshChip();
		return;
	}
	let delay =
		document.visibilityState === "hidden" || state.lifecycle.frozen
			? Math.max(base, HIDDEN_REFRESH_MS)
			: base;
	if (state.lastError && state.failureCount > 0) {
		const backoff = Math.min(
			MAX_REFRESH_FAILURE_BACKOFF_MS,
			1000 * 2 ** Math.min(state.failureCount, 7),
		);
		delay = Math.max(delay, backoff);
	}
	state.timer = window.setTimeout(
		() => refreshDashboard({ reason: "auto" }),
		delay,
	);
	updateRefreshChip();
}

function setBusy(value) {
	state.refreshing = Boolean(value);
	els.shell?.setAttribute("aria-busy", String(Boolean(value)));
	if (els.refreshButton) {
		els.refreshButton.disabled = Boolean(value);
		els.refreshButton.textContent = value ? "Refreshing…" : "Refresh";
	}
	updateRefreshChip();
}

function timeoutSignal(ms) {
	if (
		typeof AbortSignal !== "undefined" &&
		typeof AbortSignal.timeout === "function"
	) {
		return AbortSignal.timeout(ms);
	}
	if (typeof AbortController === "undefined") return null;
	const controller = new AbortController();
	window.setTimeout(
		() =>
			controller.abort(new DOMException("Request timed out", "TimeoutError")),
		ms,
	);
	return controller.signal;
}

function combineSignals(signals) {
	const active = signals.filter(Boolean);
	if (!active.length) return null;
	if (
		typeof AbortSignal !== "undefined" &&
		typeof AbortSignal.any === "function"
	) {
		return AbortSignal.any(active);
	}
	if (typeof AbortController === "undefined") return active[0];
	const controller = new AbortController();
	const abort = (event) => {
		if (!controller.signal.aborted)
			controller.abort(
				event?.target?.reason ||
					new DOMException("Request aborted", "AbortError"),
			);
	};
	for (const signal of active) {
		if (signal.aborted) {
			abort({ target: signal });
			break;
		}
		signal.addEventListener("abort", abort, { once: true });
	}
	return controller.signal;
}
