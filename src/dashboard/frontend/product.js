// MCPace Deep UI — product shell over the backend-owned dashboard controls.
function safeProductMarkupUrl(value) {
	const raw = String(value ?? "").trim();
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
	} catch {
		return false;
	}
}

function productMarkupFragment(element, markup) {
	const range = document.createRange();
	range.selectNodeContents(element);
	const fragment = range.createContextualFragment(String(markup ?? ""));
	fragment
		.querySelectorAll(
			"script, iframe, object, embed, base, link, meta, template, foreignObject",
		)
		.forEach((node) => node.remove());
	fragment.querySelectorAll("*").forEach((node) => {
		for (const attribute of [...node.attributes]) {
			const name = attribute.name.toLowerCase();
			const value = attribute.value;
			if (name.startsWith("on") || name === "srcdoc" || name === "srcset") {
				node.removeAttribute(attribute.name);
			} else if (
				["href", "src", "xlink:href", "action", "formaction"].includes(name) &&
				!safeProductMarkupUrl(value)
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
	return fragment;
}

function setProductHtml(element, markup) {
	if (!element) return;
	element.replaceChildren(productMarkupFragment(element, markup));
}

function prependProductHtml(element, markup) {
	if (!element) return;
	element.prepend(productMarkupFragment(element, markup));
}

(() => {
	if (window.__MCPACE_DEEP_UI__) return;
	window.__MCPACE_DEEP_UI__ = true;

	const ICON = Object.freeze({
		logo: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 7.5 12 3.5l7 4v8l-7 4-7-4v-8Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/><path d="m8 9.2 4 2.2 4-2.2M12 11.4v4.4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>',
		home: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m4 10 8-6 8 6v9a1 1 0 0 1-1 1h-5v-6h-4v6H5a1 1 0 0 1-1-1v-9Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/></svg>',
		server:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="4" width="16" height="6" rx="2" fill="none" stroke="currentColor" stroke-width="1.8"/><rect x="4" y="14" width="16" height="6" rx="2" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M8 7h.01M8 17h.01" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"/></svg>',
		apps: '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3.5" y="4" width="17" height="13" rx="2" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M8 20h8M12 17v3" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>',
		activity:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 12h4l2.1-5 4.2 10 2.1-5H21" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>',
		settings:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M19.2 15.1a1.7 1.7 0 0 0 .3 1.9l.1.1-2.5 2.5-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4v-.2a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1-2.5-2.5.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3.4 14H3.2v-4h.2A1.7 1.7 0 0 0 5 9a1.7 1.7 0 0 0-.3-1.9L4.6 7l2.5-2.5.1.1a1.7 1.7 0 0 0 1.9.3 1.7 1.7 0 0 0 1-1.6v-.2h4v.2a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.6 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4h-.2a1.7 1.7 0 0 0-1.6 1.1Z" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linejoin="round"/></svg>',
		search:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="6.5" fill="none" stroke="currentColor" stroke-width="1.9"/><path d="m16 16 4 4" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round"/></svg>',
		plus: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>',
		refresh:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 7v5h-5M4 17v-5h5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/><path d="M18.2 9A7 7 0 0 0 6.5 6.5L4 9m16 6-2.5 2.5A7 7 0 0 1 5.8 15" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>',
		check:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m5 12.5 4.1 4L19 7" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"/></svg>',
		shield:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3 20 6v5c0 5-3.4 8.2-8 10-4.6-1.8-8-5-8-10V6l8-3Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/><path d="m8.5 12 2.1 2.1 4.9-4.7" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>',
		terminal:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="m7 9 3 3-3 3m6 0h4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>',
		compass:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="m15.5 8.5-2 5-5 2 2-5 5-2Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/></svg>',
		import:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v12m0 0 4-4m-4 4-4-4M5 20h14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>',
		list: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 6h12M8 12h12M8 18h12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><circle cx="4" cy="6" r="1" fill="currentColor"/><circle cx="4" cy="12" r="1" fill="currentColor"/><circle cx="4" cy="18" r="1" fill="currentColor"/></svg>',
		map: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m4 6 5-2 6 2 5-2v14l-5 2-6-2-5 2V6Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/><path d="M9 4v14M15 6v14" fill="none" stroke="currentColor" stroke-width="1.7"/></svg>',
		close:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>',
		back: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m14.5 5-7 7 7 7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
		chevron:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 5 7 7-7 7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
		copy: '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="8" y="8" width="11" height="11" rx="2" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" fill="none" stroke="currentColor" stroke-width="1.8"/></svg>',
		warning:
			'<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3 2.8 20h18.4L12 3Z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/><path d="M12 9v5m0 3h.01" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>',
	});

	const VIEW_META = Object.freeze({
		home: ["Home", "Status and next step"],
		integrations: ["Integrations", "Servers and tools"],
		applications: ["Applications", "AI apps and configuration"],
		activity: ["Activity", "Calls, speed, and errors"],
		settings: ["Settings", "Appearance and runtime"],
	});

	const state = {
		view: "home",
		integrationFilter: "all",
		integrationQuery: "",
		integrationSort: "priority",
		integrationLayout: "list",
		integrationScope: "all",
		integrationClient: "all",
		integrationProject: "all",
		integrationGroup: "none",
		expandedServer: null,
		selectedServers: new Set(),
		pinnedServers: new Set(),
		activityFilter: "all",
		activityQuery: "",
		activityView: "events",
		activityRange: "24h",
		activityLimit: 16,
		settingsTab: "general",
		theme: "system",
		density: "comfortable",
		motion: "system",
		textSize: "normal",
		effects: "soft",
		detailLevel: "essential",
		tokenEstimates: "show",
		pathVisibility: "full",
		contextLabels: "show",
		exportMode: "safe",
		exposureMode: "observed",
		setupDismissed: false,
		actionReviewResolver: null,
		actionReviewContext: null,
		addSeed: "",
		addMethod: "",
		movedWizard: null,
		movedWizardPlaceholder: null,
		movedWizardSemantics: null,
		lastFocus: null,
		serverDialogOpener: null,
		serverDialogReturnView: null,
		serverDialogFocusToken: null,
		pendingFocusToken: null,
		renderTimer: 0,
		started: false,
		observer: null,
		commandActions: [],
		nodes: {},
		hosts: {},
		signatures: {},
		eventDetailId: null,
		auditRecordCache: null,
		serverModelCache: null,
		serverConflictCache: null,
	};

	const $ = (selector, root = document) =>
		root?.querySelector?.(selector) || null;
	const $$ = (selector, root = document) => [
		...(root?.querySelectorAll?.(selector) || []),
	];
	const text = (node) => (node?.textContent || "").replace(/\s+/g, " ").trim();
	const lower = (node) => text(node).toLowerCase();
	const escapeHtml = (value) =>
		String(value ?? "").replace(
			/[&<>"']/g,
			(char) =>
				({
					"&": "&amp;",
					"<": "&lt;",
					">": "&gt;",
					'"': "&quot;",
					"'": "&#039;",
				})[char],
		);
	const clamp = (value, min, max) => Math.min(max, Math.max(min, value));
	const unique = (values) => [...new Set(values.filter(Boolean))];
	const focusIdentityAttributes = [
		"data-server-name",
		"data-server-action",
		"data-mc-open-server",
		"data-mc-view",
		"data-mc-home-action",
		"data-mc-activity-view",
		"data-mc-settings-tab",
		"data-mc-exposure-mode",
		"name",
		"aria-label",
	];

	function captureDashboardFocus() {
		const active = document.activeElement;
		if (
			!active ||
			active === document.body ||
			active === document.documentElement
		)
			return null;
		const token = {
			id: active.id || "",
			tagName: active.tagName,
			attributes: focusIdentityAttributes
				.map((name) => [name, active.getAttribute?.(name)])
				.filter(([, value]) => value !== null),
		};
		state.pendingFocusToken = token;
		return token;
	}

	function focusCandidateForToken(token) {
		if (!token) return null;
		if (token.id) {
			const byId = document.getElementById(token.id);
			if (byId) return byId;
		}
		const [first] = token.attributes || [];
		if (!first) return null;
		return $$(`[${first[0]}]`).find(
			(node) =>
				visible(node) &&
				node.tagName === token.tagName &&
				token.attributes.every(
					([name, value]) => node.getAttribute(name) === value,
				),
		);
	}

	function restoreDashboardFocus(token) {
		if (!token) return;
		queueMicrotask(() => {
			const active = document.activeElement;
			if (
				active &&
				active !== document.body &&
				active !== document.documentElement &&
				active.isConnected &&
				visible(active) &&
				!active.disabled
			)
				return;
			const candidate = focusCandidateForToken(token);
			if (candidate && !candidate.hidden && candidate.isConnected) {
				candidate.focus({ preventScroll: true });
				return;
			}
			const heading = $("h1, h2", state.hosts[state.view]);
			if (heading) {
				heading.tabIndex = -1;
				heading.focus({ preventScroll: true });
			}
		});
	}

	window.mcpaceCaptureDashboardFocus = captureDashboardFocus;

	function readPreference(key, fallback, allowed = null) {
		try {
			const value = localStorage.getItem(`mcpace.deep.${key}`);
			return value && (!allowed || allowed.includes(value)) ? value : fallback;
		} catch (_) {
			return fallback;
		}
	}

	function writePreference(key, value) {
		try {
			localStorage.setItem(`mcpace.deep.${key}`, value);
		} catch (_) {}
	}

	function readSetPreference(key) {
		try {
			const value = JSON.parse(
				localStorage.getItem(`mcpace.deep.${key}`) || "[]",
			);
			return new Set(
				Array.isArray(value) ? value.map(String).filter(Boolean) : [],
			);
		} catch (_) {
			return new Set();
		}
	}

	function writeSetPreference(key, values) {
		try {
			localStorage.setItem(`mcpace.deep.${key}`, JSON.stringify([...values]));
		} catch (_) {}
	}

	function syncThemeColor() {
		let meta = $('meta[name="theme-color"]');
		if (!meta) {
			meta = document.createElement("meta");
			meta.name = "theme-color";
			document.head.appendChild(meta);
		}
		const value =
			getComputedStyle(document.documentElement)
				.getPropertyValue("--mc-bg-elevated")
				.trim() ||
			getComputedStyle(document.body).backgroundColor ||
			"#0d100e";
		meta.content = value;
	}

	function toneFrom(value) {
		const source = String(value || "").toLowerCase();
		if (
			/error|failed|failure|offline|unavailable|blocked|denied|invalid|broken|unhealthy/.test(
				source,
			)
		)
			return "bad";
		if (
			/warn|review|required|attention|unknown|pending|not tested|waiting|partial|degraded|manual/.test(
				source,
			)
		)
			return "warn";
		if (/disabled|inactive|stopped|\boff\b|parked/.test(source)) return "off";
		if (
			/ready|working|healthy|online|passed|available|enabled|patchable|ok\b|nominal/.test(
				source,
			)
		)
			return "good";
		return "neutral";
	}

	function statusLabel(tone, source = "") {
		const value = String(source || "").toLowerCase();
		if (tone === "bad")
			return /offline|unavailable/.test(value) ? "Unavailable" : "Failed";
		if (tone === "warn")
			return /not tested/.test(value)
				? "Test required"
				: /manual/.test(value)
					? "Manual"
					: "Review";
		if (tone === "off") return "Disabled";
		if (tone === "good")
			return /patchable/.test(value) ? "Patchable" : "Working";
		return "Not checked";
	}

	function toneMark(tone) {
		return (
			{ good: "✓", warn: "!", bad: "×", off: "–", neutral: "·" }[tone] || "·"
		);
	}

	function initials(value) {
		const words = String(value || "")
			.trim()
			.split(/\s+/)
			.filter(Boolean);
		if (!words.length) return "M";
		return (
			words
				.slice(0, 2)
				.map((word) => word.match(/[A-Za-z0-9]/)?.[0] || "")
				.join("")
				.toUpperCase() || "M"
		);
	}

	function numberFrom(value, pattern, fallback = 0) {
		const match = String(value || "").match(pattern);
		const number = Number(match?.[1]);
		return Number.isFinite(number) ? number : fallback;
	}

	function humanCount(value, singular, plural = `${singular}s`) {
		return `${value} ${value === 1 ? singular : plural}`;
	}

	function visible(node) {
		if (
			!node ||
			node.hidden ||
			node.getAttribute("aria-hidden") === "true" ||
			node.closest?.('[hidden], [aria-hidden="true"]')
		)
			return false;
		const style = getComputedStyle(node);
		return (
			style.display !== "none" &&
			style.visibility !== "hidden" &&
			node.getClientRects().length > 0
		);
	}

	function dashboardApi() {
		return window.__mcpaceDashboard || null;
	}

	function dashboardState() {
		return dashboardApi()?.state || {};
	}

	function overviewData() {
		const value = dashboardState().overview;
		return value && typeof value === "object" ? value : {};
	}

	function overviewServers() {
		const value = overviewData().servers;
		if (Array.isArray(value)) return value;
		if (Array.isArray(value?.servers)) return value.servers;
		if (Array.isArray(value?.items)) return value.items;
		return [];
	}

	function overviewClients() {
		const value = overviewData().clients;
		if (Array.isArray(value)) return value;
		if (Array.isArray(value?.clients)) return value.clients;
		if (Array.isArray(value?.targets)) return value.targets;
		return [];
	}

	function overviewInstances() {
		const value = overviewData().instances;
		if (Array.isArray(value)) return value;
		if (Array.isArray(value?.instances)) return value.instances;
		if (Array.isArray(value?.items)) return value.items;
		return [];
	}

	function overviewLeaseEnvelope() {
		const value = overviewData().leases;
		if (!value)
			return {
				leases: [],
				sessions: [],
				activeLeaseCount: 0,
				activeSessionCount: 0,
				nowMs: null,
			};
		if (Array.isArray(value))
			return {
				leases: value,
				sessions: [],
				activeLeaseCount: value.length,
				activeSessionCount: 0,
				nowMs: null,
			};
		const leases = Array.isArray(value.leases)
			? value.leases
			: Array.isArray(value.activeLeases)
				? value.activeLeases
				: Array.isArray(value.items)
					? value.items
					: [];
		const sessions = Array.isArray(value.sessions)
			? value.sessions
			: Array.isArray(value.activeSessions)
				? value.activeSessions
				: [];
		return {
			...value,
			leases,
			sessions,
			activeLeaseCount: Math.max(
				leases.length,
				finiteNumber(value.activeLeaseCount, leases.length),
			),
			activeSessionCount: Math.max(
				sessions.length,
				finiteNumber(value.activeSessionCount, sessions.length),
			),
			nowMs: finiteNumber(value.nowMs ?? value.refreshedAtMs, null),
		};
	}

	function liveSessionModels() {
		const envelope = overviewLeaseEnvelope();
		const bySession = new Map();
		envelope.sessions.forEach((session, index) => {
			const id = String(
				session?.sessionLeaseId || session?.sessionId || `session-${index + 1}`,
			);
			bySession.set(id, {
				id,
				clientId: String(session?.clientId || "unknown client"),
				sessionId: String(session?.sessionId || ""),
				projectRoot: String(session?.projectRoot || ""),
				servers: Array.isArray(session?.servers)
					? session.servers.map(String).filter(Boolean)
					: [],
				activeLeaseCount: Math.max(
					0,
					finiteNumber(
						session?.activeLeaseCount,
						Array.isArray(session?.activeLeaseIds)
							? session.activeLeaseIds.length
							: 0,
					),
				),
				startedAtMs: finiteNumber(session?.startedAtMs, null),
				lastSeenAtMs: finiteNumber(
					session?.lastLeaseSeenAtMs ?? session?.refreshedAtMs,
					null,
				),
				source: "session-summary",
			});
		});
		envelope.leases.forEach((lease, index) => {
			const id = String(
				lease?.sessionLeaseId ||
					lease?.sessionId ||
					`lease-session-${index + 1}`,
			);
			const current = bySession.get(id) || {
				id,
				clientId: String(lease?.clientId || "unknown client"),
				sessionId: String(lease?.sessionId || ""),
				projectRoot: String(lease?.projectRoot || ""),
				servers: [],
				activeLeaseCount: 0,
				startedAtMs: finiteNumber(lease?.acquiredAtMs, null),
				lastSeenAtMs: finiteNumber(
					lease?.renewedAtMs ?? lease?.acquiredAtMs,
					null,
				),
				source: "lease-derived",
			};
			const server = String(lease?.server || lease?.serverName || "");
			if (server && !current.servers.includes(server))
				current.servers.push(server);
			if (current.source === "lease-derived")
				current.activeLeaseCount = Math.max(current.activeLeaseCount, 0) + 1;
			current.startedAtMs =
				current.startedAtMs === null
					? finiteNumber(lease?.acquiredAtMs, null)
					: Math.min(
							current.startedAtMs,
							finiteNumber(lease?.acquiredAtMs, current.startedAtMs),
						);
			current.lastSeenAtMs =
				Math.max(
					current.lastSeenAtMs || 0,
					finiteNumber(lease?.renewedAtMs ?? lease?.acquiredAtMs, 0),
				) || null;
			bySession.set(id, current);
		});
		return [...bySession.values()]
			.map((session) => ({
				...session,
				servers: unique(session.servers).sort(),
			}))
			.sort(
				(left, right) => (right.lastSeenAtMs || 0) - (left.lastSeenAtMs || 0),
			);
	}

	function activeLeaseModels() {
		return overviewLeaseEnvelope()
			.leases.map((lease, index) => ({
				id: String(lease?.leaseId || `lease-${index + 1}`),
				server: String(lease?.server || lease?.serverName || "unknown server"),
				clientId: String(lease?.clientId || "unknown client"),
				sessionId: String(lease?.sessionId || ""),
				sessionLeaseId: String(lease?.sessionLeaseId || ""),
				projectRoot: String(lease?.projectRoot || ""),
				acquiredAtMs: finiteNumber(lease?.acquiredAtMs, null),
				renewedAtMs: finiteNumber(lease?.renewedAtMs, null),
				expiresAtMs: finiteNumber(lease?.expiresAtMs, null),
				strategy: String(
					lease?.route?.requestStrategy || lease?.requestStrategy || "",
				),
				lane: String(lease?.route?.schedulerLane || lease?.schedulerLane || ""),
				transport: String(
					lease?.route?.upstreamTransport || lease?.transport || "",
				),
			}))
			.sort(
				(left, right) =>
					(right.renewedAtMs || right.acquiredAtMs || 0) -
					(left.renewedAtMs || left.acquiredAtMs || 0),
			);
	}

	function retainedOperations() {
		const value = dashboardState().operations;
		return value && typeof value === "object" && Array.isArray(value.events)
			? value
			: null;
	}

	function rawLogs() {
		const retained = retainedOperations();
		if (retained?.events?.length) return retained.events;
		const value = dashboardState().logs;
		return Array.isArray(value) ? value : [];
	}

	function retainedWindow() {
		const retained = retainedOperations();
		if (!retained) {
			const logs = Array.isArray(dashboardState().logs)
				? dashboardState().logs
				: [];
			return {
				schema: "fallback-log-tail",
				returned: logs.length,
				totalParsed: logs.length,
				truncated: logs.length >= 500,
				parseErrors: 0,
				files: [],
				oldestTsMs: logs[0]?.tsMs || null,
				newestTsMs: logs.at(-1)?.tsMs || null,
				source: "api/logs",
			};
		}
		return { ...retained, source: "api/operations" };
	}

	function finiteNumber(value, fallback = null) {
		if (value === null || value === undefined || value === "") return fallback;
		const number = Number(value);
		return Number.isFinite(number) ? number : fallback;
	}

	function boolValue(value, fallback = false) {
		return typeof value === "boolean" ? value : fallback;
	}

	function formatNumber(value) {
		const number = finiteNumber(value, 0);
		try {
			return new Intl.NumberFormat(undefined, {
				maximumFractionDigits: 1,
			}).format(number);
		} catch (_) {
			return String(Math.round(number * 10) / 10);
		}
	}

	function formatDuration(value) {
		const ms = finiteNumber(value, null);
		if (ms === null) return "not measured";
		if (ms < 1) return "<1 ms";
		if (ms < 1000) return `${Math.round(ms)} ms`;
		if (ms < 60000) return `${(ms / 1000).toFixed(ms < 10000 ? 1 : 0)} s`;
		return `${(ms / 60000).toFixed(1)} min`;
	}

	function formatBytes(value) {
		const bytes = finiteNumber(value, 0);
		if (bytes < 1024) return `${Math.round(bytes)} B`;
		if (bytes < 1024 ** 2)
			return `${(bytes / 1024).toFixed(bytes < 10240 ? 1 : 0)} KB`;
		if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
		return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
	}

	function formatRelativeTimestamp(value) {
		const timestamp = finiteNumber(value, null);
		if (timestamp === null) return "time unavailable";
		const delta = Math.max(0, Date.now() - timestamp);
		if (delta < 5000) return "just now";
		if (delta < 60000) return `${Math.floor(delta / 1000)}s ago`;
		if (delta < 3600000) return `${Math.floor(delta / 60000)}m ago`;
		if (delta < 86400000) return `${Math.floor(delta / 3600000)}h ago`;
		if (delta < 604800000) return `${Math.floor(delta / 86400000)}d ago`;
		try {
			return new Intl.DateTimeFormat(undefined, {
				month: "short",
				day: "numeric",
				hour: "2-digit",
				minute: "2-digit",
			}).format(timestamp);
		} catch (_) {
			return new Date(timestamp).toLocaleString();
		}
	}

	function percentile(values, fraction) {
		const sorted = values
			.map((value) => finiteNumber(value, null))
			.filter((value) => value !== null)
			.sort((a, b) => a - b);
		if (!sorted.length) return null;
		const index = Math.max(0, Math.ceil(sorted.length * fraction) - 1);
		return sorted[index];
	}

	function compactPath(value) {
		const path = String(value || "");
		if (!path || state.pathVisibility === "full" || path.length <= 52)
			return path;
		const normalized = path.replace(/\\/g, "/");
		const parts = normalized.split("/").filter(Boolean);
		if (parts.length < 4) return `…${path.slice(-48)}`;
		return `${normalized.startsWith("/") ? "/" : ""}${parts[0]}/…/${parts.slice(-2).join("/")}`;
	}

	function pathRow(label, value, note = "", tone = "neutral") {
		const raw = String(value || "").trim();
		if (!raw) return "";
		return `<article class="mc-path-row" data-tone="${tone}"><div><span>${escapeHtml(label)}</span><code title="${escapeHtml(raw)}">${escapeHtml(compactPath(raw))}</code>${note ? `<small>${escapeHtml(note)}</small>` : ""}</div><button type="button" data-mc-copy-value="${escapeHtml(raw)}" aria-label="Copy ${escapeHtml(label)}">${ICON.copy}<span>Copy</span></button></article>`;
	}

	function serverRecord(name) {
		return (
			overviewServers().find(
				(server) => String(server?.name || "") === String(name || ""),
			) || {}
		);
	}

	function cachedToolEntry(name) {
		const cache = overviewData().cachedToolEvidence || {};
		const servers = Array.isArray(cache.servers) ? cache.servers : [];
		return (
			servers.find(
				(server) => String(server?.name || "") === String(name || ""),
			) || {}
		);
	}

	function cachedToolDefinitions(name) {
		const tools = cachedToolEntry(name).tools;
		return Array.isArray(tools) ? tools : [];
	}

	function toolTechnicalName(tool = {}) {
		return String(tool?.name || "unnamed-tool");
	}

	function toolDisplayName(tool = {}) {
		const annotations =
			tool?.annotations && typeof tool.annotations === "object"
				? tool.annotations
				: {};
		return String(tool?.title || annotations.title || toolTechnicalName(tool));
	}

	function toolDisplaySummary(tool = {}) {
		const display = toolDisplayName(tool);
		const technical = toolTechnicalName(tool);
		return { display, technical, differs: display !== technical };
	}

	function cachedToolDefinitionByName(name, serverName = "") {
		const technical = String(name || "");
		if (!technical) return null;
		const serverNames = serverName
			? [serverName]
			: overviewServers()
					.map((server) => String(server?.name || ""))
					.filter(Boolean);
		for (const candidate of serverNames) {
			const definition = cachedToolDefinitions(candidate).find(
				(tool) => toolTechnicalName(tool) === technical,
			);
			if (definition) return definition;
		}
		return null;
	}

	function toolRisk(tool = {}) {
		const annotations =
			tool.annotations && typeof tool.annotations === "object"
				? tool.annotations
				: {};
		const name = toolTechnicalName(tool);
		const description = String(tool.description || "");
		const source = `${name} ${description}`.toLowerCase();
		const readOnly = annotations.readOnlyHint === true;
		const destructiveHint = annotations.destructiveHint === true;
		const destructiveSignal =
			/delete|remove|drop|truncate|overwrite|write|create|update|execute|run|click|navigate|upload|send|publish|commit|push|install/.test(
				source,
			);
		const openWorldHint = annotations.openWorldHint === true;
		const openWorldSignal =
			/browser|network|http|url|web|email|slack|github|database|shell|terminal|command/.test(
				source,
			);
		const idempotent = annotations.idempotentHint === true;
		const categories = [];
		if (readOnly)
			categories.push({
				label: "Server hint: read-only",
				tone: "good",
				source: "annotation",
			});
		if (!readOnly && destructiveHint)
			categories.push({
				label: "Server hint: destructive",
				tone: "bad",
				source: "annotation",
			});
		else if (!readOnly && destructiveSignal)
			categories.push({
				label: "May change data",
				tone: "bad",
				source: "heuristic",
			});
		if (openWorldHint)
			categories.push({
				label: "Server hint: open-world",
				tone: "warn",
				source: "annotation",
			});
		else if (openWorldSignal)
			categories.push({
				label: "May access external systems",
				tone: "warn",
				source: "heuristic",
			});
		if (idempotent)
			categories.push({
				label: "Server hint: idempotent",
				tone: "neutral",
				source: "annotation",
			});
		if (!categories.length)
			categories.push({
				label: "Risk not described",
				tone: "neutral",
				source: "unknown",
			});
		return categories;
	}

	function auditTimestamp(entry = {}) {
		return finiteNumber(
			entry.tsMs ?? entry.timestampMs ?? entry.atMs ?? entry.timeMs,
			null,
		);
	}

	function fallbackAuditClassification(entry = {}) {
		if (entry.bridgeOk !== false && entry.upstreamOk !== false)
			return {
				outcome: "success",
				errorKind: "none",
				failureStage: "complete",
			};
		if (entry.bridgeOk !== false)
			return {
				outcome: "tool_error",
				errorKind: "upstream_tool_error",
				failureStage: "upstream",
			};
		const source = String(entry.error || "").toLowerCase();
		if (/policy|risk|blocked|denied|not allowed|allowunknown/.test(source))
			return {
				outcome: "denied",
				errorKind: "policy_denied",
				failureStage: "policy",
			};
		if (
			/unauthorized|forbidden|authentication|authorization|access token|bearer token|missing token|token expired|credential/.test(
				source,
			)
		)
			return {
				outcome: "denied",
				errorKind: "authorization",
				failureStage: "authorization",
			};
		if (/timeout|timed out/.test(source))
			return {
				outcome: "timeout",
				errorKind: "timeout",
				failureStage: /queue|lease/.test(source) ? "queue" : "upstream",
			};
		if (/queue|lease|capacity|busy/.test(source))
			return {
				outcome: "rejected",
				errorKind: "capacity",
				failureStage: "queue",
			};
		if (
			/unknown tool|schema|argument|invalid|duplicate|unexpected token/.test(
				source,
			)
		)
			return {
				outcome: "invalid",
				errorKind: "validation",
				failureStage: "validation",
			};
		if (/spawn|stdio|http|connect|transport|status|process/.test(source))
			return {
				outcome: "transport_error",
				errorKind: "transport",
				failureStage: "upstream",
			};
		return {
			outcome: "bridge_error",
			errorKind: "internal",
			failureStage: "bridge",
		};
	}

	function auditRecords() {
		if (Array.isArray(state.auditRecordCache)) return state.auditRecordCache;
		const records = rawLogs()
			.filter(
				(entry) =>
					entry &&
					(entry.event === "tool_call_audit" ||
						entry.event === "tool_batch_audit"),
			)
			.map((entry, index) => {
				const batch = entry.event === "tool_batch_audit";
				const tools = batch
					? Array.isArray(entry.tools)
						? entry.tools.map(String).filter(Boolean)
						: []
					: [String(entry.tool || "unknown-tool")];
				const callCount = Math.max(
					1,
					finiteNumber(entry.callCount, batch ? tools.length || 1 : 1),
				);
				const failedCount = batch
					? Math.max(
							0,
							finiteNumber(
								entry.upstreamFailedCount,
								entry.upstreamOk === false ? callCount : 0,
							),
						)
					: entry.bridgeOk === true && entry.upstreamOk === true
						? 0
						: 1;
				const successCount = Math.max(0, callCount - failedCount);
				const timestamp = auditTimestamp(entry);
				const classification = entry.outcome
					? {
							outcome: String(entry.outcome),
							errorKind: String(entry.errorKind || "unknown"),
							failureStage: String(entry.failureStage || "unknown"),
						}
					: fallbackAuditClassification(entry);
				const callId = String(entry.callId || `${timestamp || 0}-${index}`);
				return {
					raw: entry,
					id: callId,
					callId,
					auditSchema: String(entry.auditSchema || "mcpace.toolAudit.pre-v2"),
					requestKind: String(
						entry.requestKind || (batch ? "tools/call.batch" : "tools/call"),
					),
					outcome: classification.outcome,
					errorKind: classification.errorKind,
					failureStage: classification.failureStage,
					timestamp,
					event: entry.event,
					batch,
					server: String(entry.server || "unknown-server"),
					tools,
					toolLabel: batch ? `${callCount} calls` : tools[0],
					callCount,
					successCount,
					failedCount,
					ok:
						classification.outcome === "success" &&
						failedCount === 0 &&
						entry.bridgeOk !== false,
					bridgeOk: entry.bridgeOk !== false,
					upstreamOk: entry.upstreamOk !== false && failedCount === 0,
					queueMs: finiteNumber(entry.queueDurationMs, null),
					upstreamMs: finiteNumber(entry.upstreamDurationMs, null),
					totalMs: finiteNumber(entry.totalDurationMs, null),
					requestBytes: finiteNumber(entry.requestBytes, 0),
					responseBytes: finiteNumber(entry.responseBytes, 0),
					estimatedInputTokens: finiteNumber(entry.estimatedInputTokens, null),
					estimatedOutputTokens: finiteNumber(
						entry.estimatedOutputTokens,
						null,
					),
					estimatedTotalTokens: finiteNumber(entry.estimatedTotalTokens, null),
					reportedInputTokens: finiteNumber(entry.reportedInputTokens, null),
					reportedOutputTokens: finiteNumber(entry.reportedOutputTokens, null),
					reportedTotalTokens: finiteNumber(entry.reportedTotalTokens, null),
					tokenUsageSource: String(entry.tokenUsageSource || ""),
					estimateMethod: String(entry.tokenEstimateMethod || ""),
					metricsSchema: String(entry.metricsSchema || ""),
					clientId: String(entry.clientId || ""),
					sessionId: String(entry.sessionId || ""),
					projectRoot: String(entry.projectRoot || ""),
					transport: String(entry.transport || ""),
					trace: String(entry.trace || ""),
					error: String(entry.error || ""),
					leaseId: String(entry.leaseId || ""),
					pooled: Boolean(entry.pooled),
				};
			})
			.sort((left, right) => (right.timestamp || 0) - (left.timestamp || 0));
		state.auditRecordCache = records;
		return records;
	}

	function activityRangeStart() {
		const durations = { "1h": 3600000, "24h": 86400000, "7d": 604800000 };
		return durations[state.activityRange]
			? Date.now() - durations[state.activityRange]
			: null;
	}

	function timestampInActivityRange(timestamp) {
		const start = activityRangeStart();
		if (!start) return true;
		return timestamp !== null && timestamp >= start;
	}

	function rangedAuditRecords(records = auditRecords()) {
		return records.filter((record) =>
			timestampInActivityRange(record.timestamp),
		);
	}

	function groupAudits(records, keyOf, labelOf = null) {
		const groups = new Map();
		records.forEach((record) => {
			const keys = [].concat(keyOf(record) || []).filter(Boolean);
			keys.forEach((key, index) => {
				if (!groups.has(key))
					groups.set(key, {
						key,
						label: labelOf ? labelOf(record, key, index) : key,
						calls: 0,
						successes: 0,
						failures: 0,
						durations: [],
						queues: [],
						requestBytes: 0,
						responseBytes: 0,
						reportedTokens: 0,
						reportedRecords: 0,
						estimatedTokens: 0,
						estimatedRecords: 0,
						outcomeEstimatedCalls: 0,
						latencyEstimatedCalls: 0,
						lastTimestamp: null,
						clients: new Set(),
						projects: new Set(),
						servers: new Set(),
					});
				const group = groups.get(key);
				const divisor = Math.max(1, keys.length);
				const allocatedPerCall = keys.length > 1;
				const callShare = allocatedPerCall ? 1 : record.callCount;
				const mixedBatch =
					allocatedPerCall && record.successCount > 0 && record.failedCount > 0;
				const successShare = allocatedPerCall
					? record.successCount / Math.max(1, record.callCount)
					: record.successCount;
				const failureShare = allocatedPerCall
					? record.failedCount / Math.max(1, record.callCount)
					: record.failedCount;
				group.calls += callShare;
				group.successes += successShare;
				group.failures += failureShare;
				if (mixedBatch) group.outcomeEstimatedCalls += callShare;
				if (record.totalMs !== null) {
					group.durations.push(
						allocatedPerCall
							? record.totalMs / Math.max(1, record.callCount)
							: record.totalMs,
					);
					if (allocatedPerCall && record.callCount > 1)
						group.latencyEstimatedCalls += callShare;
				}
				if (record.queueMs !== null)
					group.queues.push(
						allocatedPerCall
							? record.queueMs / Math.max(1, record.callCount)
							: record.queueMs,
					);
				group.requestBytes += record.requestBytes / divisor;
				group.responseBytes += record.responseBytes / divisor;
				if (record.reportedTotalTokens !== null) {
					group.reportedTokens += record.reportedTotalTokens / divisor;
					group.reportedRecords += 1;
				}
				if (record.estimatedTotalTokens !== null) {
					group.estimatedTokens += record.estimatedTotalTokens / divisor;
					group.estimatedRecords += 1;
				}
				if (
					!group.lastTimestamp ||
					(record.timestamp || 0) > group.lastTimestamp
				)
					group.lastTimestamp = record.timestamp;
				if (record.clientId) group.clients.add(record.clientId);
				if (record.projectRoot) group.projects.add(record.projectRoot);
				if (record.server) group.servers.add(record.server);
			});
		});
		return [...groups.values()]
			.map((group) => ({
				...group,
				p50: percentile(group.durations, 0.5),
				p95: percentile(group.durations, 0.95),
				queueP95: percentile(group.queues, 0.95),
				successRate: group.calls ? group.successes / group.calls : null,
				outcomeEstimated: group.outcomeEstimatedCalls > 0,
				latencyEstimated: group.latencyEstimatedCalls > 0,
			}))
			.sort(
				(left, right) =>
					right.calls - left.calls ||
					(right.lastTimestamp || 0) - (left.lastTimestamp || 0),
			);
	}

	function usageAnalytics(records = rangedAuditRecords(), options = {}) {
		const lastTimestamp = records.reduce(
			(latest, record) =>
				record.timestamp !== null &&
				(latest === null || record.timestamp > latest)
					? record.timestamp
					: latest,
			null,
		);
		const calls = records.reduce((sum, record) => sum + record.callCount, 0);
		const successes = records.reduce(
			(sum, record) => sum + record.successCount,
			0,
		);
		const failures = records.reduce(
			(sum, record) => sum + record.failedCount,
			0,
		);
		const durations = records
			.map((record) => record.totalMs)
			.filter((value) => value !== null);
		const queues = records
			.map((record) => record.queueMs)
			.filter((value) => value !== null);
		const requestBytes = records.reduce(
			(sum, record) => sum + record.requestBytes,
			0,
		);
		const responseBytes = records.reduce(
			(sum, record) => sum + record.responseBytes,
			0,
		);
		const reportedRecords = records.filter(
			(record) => record.reportedTotalTokens !== null,
		);
		const estimatedRecords = records.filter(
			(record) => record.estimatedTotalTokens !== null,
		);
		const reportedTokens = reportedRecords.reduce(
			(sum, record) => sum + record.reportedTotalTokens,
			0,
		);
		const estimatedTokens = estimatedRecords.reduce(
			(sum, record) => sum + record.estimatedTotalTokens,
			0,
		);
		const allForRange = Array.isArray(options.allRecords)
			? options.allRecords
			: auditRecords();
		const excludedUnknownTimestamps = activityRangeStart()
			? allForRange.filter((record) => record.timestamp === null).length
			: 0;
		const mixedBatchEntries = records.filter(
			(record) =>
				record.batch && record.successCount > 0 && record.failedCount > 0,
		).length;
		const serverGroups = groupAudits(records, (record) => record.server);
		const toolGroups = groupAudits(records, (record) => record.tools);
		toolGroups.forEach((group) => {
			const labels = unique(
				[...group.servers]
					.map((serverName) => {
						const definition = cachedToolDefinitionByName(
							group.key,
							serverName,
						);
						return definition ? toolDisplayName(definition) : group.key;
					})
					.filter(Boolean),
			);
			group.label = labels.length === 1 ? labels[0] : group.key;
			group.technicalLabel = group.key;
		});
		const clientGroups = groupAudits(
			records.filter((record) => record.clientId),
			(record) => record.clientId,
		);
		const projectGroups = groupAudits(
			records.filter((record) => record.projectRoot),
			(record) => record.projectRoot,
		);
		return {
			records,
			lastTimestamp,
			calls,
			successes,
			failures,
			successRate: calls ? successes / calls : null,
			p50: percentile(durations, 0.5),
			p95: percentile(durations, 0.95),
			queueP95: percentile(queues, 0.95),
			requestBytes,
			responseBytes,
			reportedTokens,
			reportedCoverage: records.length
				? reportedRecords.length / records.length
				: 0,
			estimatedTokens,
			estimatedCoverage: records.length
				? estimatedRecords.length / records.length
				: 0,
			durationCoverage: records.length ? durations.length / records.length : 0,
			metricsCoverage: records.length
				? records.filter(
						(record) => record.metricsSchema === "mcpace.toolAuditMetrics.v1",
					).length / records.length
				: 0,
			excludedUnknownTimestamps,
			mixedBatchEntries,
			servers: serverGroups,
			tools: toolGroups,
			clients: clientGroups,
			projects: projectGroups,
		};
	}

	function usageForServer(name) {
		const records = auditRecords().filter((record) => record.server === name);
		return usageAnalytics(rangedAuditRecords(records), { allRecords: records });
	}

	function rangeLabel() {
		return (
			{
				"1h": "Last hour",
				"24h": "Last 24 hours",
				"7d": "Last 7 days",
				all: "Retained window",
			}[state.activityRange] || "Retained window"
		);
	}

	function successLabel(rate) {
		return rate === null
			? "No calls"
			: `${(rate * 100).toFixed(rate > 0.99 ? 1 : 0)}%`;
	}

	function usageTokenMarkup(analytics, compact = false) {
		const reported =
			analytics.reportedTokens > 0
				? `<strong>${formatNumber(analytics.reportedTokens)}</strong><small>reported by upstream metadata · ${(analytics.reportedCoverage * 100).toFixed(0)}% coverage</small>`
				: `<strong>Not reported</strong><small>No standard MCP token counter was observed in this window.</small>`;
		const estimated =
			state.tokenEstimates === "show"
				? `<span class="mc-token-estimate"><b>≈ ${formatNumber(analytics.estimatedTokens)}</b> payload tokens <em>UTF-8 bytes ÷ 4 · not model billing</em></span>`
				: `<span class="mc-token-estimate"><b>Hidden</b> payload estimate <em>Enable in Observability settings</em></span>`;
		return `<div class="mc-token-readout${compact ? " compact" : ""}">${reported}${estimated}</div>`;
	}

	function timelineBuckets(records, count = 12) {
		const timestampedRecords = records.filter(
			(record) => record.timestamp !== null,
		);
		if (!timestampedRecords.length) return [];
		const timestamps = timestampedRecords.map((record) => record.timestamp);
		const now = Date.now();
		const configuredStart = activityRangeStart();
		const start = configuredStart || Math.min(...timestamps, now - 3600000);
		const end = Math.max(now, ...timestamps);
		const width = Math.max(1, (end - start) / count);
		const buckets = Array.from({ length: count }, (_, index) => ({
			start: start + index * width,
			end: start + (index + 1) * width,
			calls: 0,
			failures: 0,
		}));
		timestampedRecords.forEach((record) => {
			const timestamp = record.timestamp;
			const index = clamp(
				Math.floor((timestamp - start) / width),
				0,
				count - 1,
			);
			buckets[index].calls += record.callCount;
			buckets[index].failures += record.failedCount;
		});
		return buckets;
	}

	function usageGroupRows(groups, kind = "server", limit = null) {
		const rows = limit ? groups.slice(0, limit) : groups;
		if (!rows.length)
			return `<div class="mc-large-empty">${ICON.activity}<strong>No measured calls in this view</strong><span>Run tools through MCPace; audit statistics will appear after the backend records them.</span></div>`;
		const p95Heading = kind === "tool" ? "P95 / call" : "Operation p95";
		return `<div class="mc-usage-table" role="table" aria-label="${escapeHtml(kind)} usage"><div class="mc-usage-table-head" role="row"><span role="columnheader">${kind === "tool" ? "Tool" : kind === "client" ? "Client" : kind === "project" ? "Project" : "Integration"}</span><span role="columnheader">Calls</span><span role="columnheader">Success</span><span role="columnheader">${p95Heading}</span><span role="columnheader">Payload</span><span role="columnheader">Last seen</span></div>${rows
			.map((group) => {
				const icon =
					kind === "tool"
						? ICON.terminal
						: kind === "server"
							? ICON.server
							: kind === "client"
								? ICON.apps
								: ICON.activity;
				const technical =
					kind === "tool" &&
					group.technicalLabel &&
					group.technicalLabel !== group.label
						? ` · ${group.technicalLabel}`
						: "";
				const secondary =
					kind === "tool"
						? `${group.servers.size} integration${group.servers.size === 1 ? "" : "s"}${technical}`
						: `${group.clients.size} client${group.clients.size === 1 ? "" : "s"} · ${group.projects.size} project${group.projects.size === 1 ? "" : "s"}`;
				const openControl =
					kind === "server"
						? `<button type="button" class="mc-usage-open" data-mc-open-server="${escapeHtml(group.key)}">Open details<span class="mc-sr-only"> for ${escapeHtml(group.label)}</span></button>`
						: "";
				const success = `${group.outcomeEstimated ? "≈ " : ""}${successLabel(group.successRate)}`;
				const latency = `${group.latencyEstimated ? "≈ " : ""}${formatDuration(group.p95)}`;
				const estimateNote =
					kind === "tool" && (group.outcomeEstimated || group.latencyEstimated)
						? ' title="Mixed batch outcomes or latency were proportionally allocated because the audit envelope did not identify each individual call result."'
						: "";
				return `<div class="mc-usage-table-row" role="row" data-tone="${group.failures ? "warn" : "good"}"><div role="cell" class="mc-usage-identity"><span>${icon}</span><div><strong>${escapeHtml(group.label)}</strong><small>${escapeHtml(secondary)}</small></div>${openControl}</div><strong role="cell">${formatNumber(group.calls)}</strong><span role="cell"${estimateNote}><i class="mc-mini-status" data-tone="${group.failures ? "warn" : "good"}"></i>${escapeHtml(success)}</span><span role="cell"${estimateNote}>${escapeHtml(latency)}</span><span role="cell">${escapeHtml(formatBytes(group.requestBytes + group.responseBytes))}</span><span role="cell">${escapeHtml(formatRelativeTimestamp(group.lastTimestamp))}</span></div>`;
			})
			.join("")}</div>`;
	}

	function usageTimelineMarkup(records) {
		const buckets = timelineBuckets(records, 16);
		if (!buckets.length)
			return `<div class="mc-large-empty">${ICON.activity}<strong>No call timeline yet</strong><span>The graph uses actual retained audit timestamps.</span></div>`;
		const max = Math.max(1, ...buckets.map((bucket) => bucket.calls));
		const timelineAlternative = buckets
			.map(
				(bucket) =>
					`${new Date(bucket.start).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}: ${formatNumber(bucket.calls)} calls, ${formatNumber(bucket.failures)} failed`,
			)
			.join("; ");
		return `<div class="mc-usage-timeline" role="img" aria-label="Tool calls over ${escapeHtml(rangeLabel().toLowerCase())}. ${escapeHtml(timelineAlternative)}"><div class="mc-timeline-bars">${buckets
			.map((bucket, index) => {
				const height = clamp(
					(bucket.calls / max) * 100,
					bucket.calls ? 7 : 1,
					100,
				);
				const failureHeight = bucket.calls
					? clamp((bucket.failures / bucket.calls) * 100, 0, 100)
					: 0;
				return `<div class="mc-timeline-column" title="${formatNumber(bucket.calls)} calls · ${formatNumber(bucket.failures)} failed"><span style="--bar:${height}%;--errors:${failureHeight}%"></span>${index % 4 === 0 ? `<small>${new Date(bucket.start).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</small>` : "<small></small>"}</div>`;
			})
			.join(
				"",
			)}</div><div class="mc-timeline-legend"><span><i data-tone="good"></i>Calls</span><span><i data-tone="bad"></i>Failures inside bar</span><strong>Peak ${formatNumber(max)}</strong></div></div>`;
	}

	function coverageItem(label, coverage, detail, tone = null) {
		const pct = clamp(Math.round((coverage || 0) * 100), 0, 100);
		const derivedTone = tone || (pct >= 95 ? "good" : pct ? "warn" : "off");
		return `<article class="mc-coverage-item" data-tone="${derivedTone}"><header><strong>${escapeHtml(label)}</strong><span>${pct}%</span></header><div><i style="--coverage:${pct}%"></i></div><small>${escapeHtml(detail)}</small></article>`;
	}

	function configurationPaths() {
		const overview = overviewData();
		const root = String(overview.rootPath || "");
		const hub = overview.hub || {};
		const automation = overview.automation || {};
		const baseFile = String(
			automation.serverSources?.baseFile || "mcp_settings.json",
		);
		const join = (base, leaf) =>
			!base ? leaf : `${base.replace(/[\\/]$/, "")}/${leaf}`;
		const paths = [
			{
				label: "MCPace project root",
				value: root,
				note: "Runtime and project-owned configuration root",
				tone: "good",
			},
			{
				label: "Server inventory",
				value: root ? join(root, baseFile) : baseFile,
				note: "Base MCP source file observed by the dashboard",
				tone: "good",
			},
			{
				label: "Server fragments",
				value: root ? join(root, "mcp_settings.d/") : "mcp_settings.d/",
				note: "Per-source include directory",
				tone: "neutral",
			},
			{
				label: "Control-plane config",
				value: root ? join(root, "mcpace.config.json") : "mcpace.config.json",
				note: "Runtime, discovery, and policy configuration",
				tone: "warn",
			},
			{
				label: "Runtime state",
				value: String(hub.stateRoot || ""),
				note: "Hub state, caches, leases, and generated runtime data",
				tone: "neutral",
			},
			{
				label: "Runtime log",
				value: String(hub.logPath || ""),
				note: "Backend event and tool-call audit stream",
				tone: "neutral",
			},
		];
		return paths.filter((path) => path.value);
	}

	async function copyText(value, label = "Value") {
		const textValue = String(value || "");
		try {
			if (navigator.clipboard?.writeText)
				await navigator.clipboard.writeText(textValue);
			else {
				const input = document.createElement("textarea");
				input.value = textValue;
				input.style.position = "fixed";
				input.style.opacity = "0";
				document.body.appendChild(input);
				input.select();
				document.execCommand("copy");
				input.remove();
			}
			toast(`${label} copied`, textValue);
		} catch (_) {
			toast(
				"Copy failed",
				"Select the path manually from the configuration map.",
			);
		}
	}

	function discoverNodes() {
		state.nodes = {
			baseShell: $(".shell", state.baseRoot || document),
			serverList: $("#server-list"),
			serverOverflow: $("#server-overflow-note"),
			originalEnabledToggle: $("#toggle-enabled"),
			clientPanel: $("#client-setup-panel"),
			clientList: $("#client-setup-list"),
			clientResult: $("#client-setup-result"),
			serverDialog: $("#server-dialog"),
			serverDialogTitle: $("#server-dialog-title"),
			serverDialogSubtitle: $("#server-dialog-subtitle"),
			serverDialogBody: $("#server-dialog-body"),
			serverDialogTabs: $(".server-dialog-tabs"),
			importPanel: $("#server-import-panel"),
			discoverPanel: $("#server-discovery-panel"),
			manualPanel: $("#server-install-panel"),
			importInput: $("#server-import-path"),
			discoverInput: $("#server-discover-query"),
			manualInput: $("#server-install-command"),
			importResult: $("#server-import-result"),
			discoverResult: $("#server-discovery-results"),
			manualResult: $("#server-install-note"),
			activityList: $("#activity-list"),
			activityChip: $("#activity-chip"),
			auditList: $("#audit-list"),
			logList: $("#log-list"),
			logChip: $("#log-chip"),
			runtimePanel: $("#diagnostic-runtime"),
			trustPanel: $("#diagnostic-trust"),
			policyPanel: $("#server-advanced"),
			logsPanel: $("#diagnostic-logs"),
			preferencesPanel: $("#diagnostic-preferences"),
			automationPanel: $("#automation-panel"),
			accessReview: $("#access-review"),
			updateNotice: $("#update-notice"),
			updateCheckButton: $("#update-check-button"),
			startButton: $("#hub-up-button"),
			stopButton: $("#hub-down-button"),
			repairButton: $("#repair-button"),
			systemState: $("#system-state"),
			loadState: $("#load-state"),
			loadNote: $("#load-note"),
			refreshChip: $("#refresh-chip"),
		};
	}

	function productShellMarkup() {
		const nav = [
			["home", "Home", ICON.home],
			["integrations", "Integrations", ICON.server],
			["applications", "Applications", ICON.apps],
			["activity", "Activity", ICON.activity],
			["settings", "Settings", ICON.settings],
		]
			.map(
				([id, label, icon], index) => `
      <button type="button" class="mc-nav-item" data-mc-view="${id}" ${id === "home" ? 'aria-current="page"' : ""}>
        <span class="mc-nav-icon">${icon}</span><span class="mc-nav-copy"><strong>${label}</strong><small>${VIEW_META[id][1]}</small></span>
        ${id === "integrations" ? '<span class="mc-nav-alert" data-mc-issue-count hidden>0</span>' : ""}<kbd>Alt+${index + 1}</kbd>
      </button>`,
			)
			.join("");

		return `
      <a class="mc-skip-link" href="#mc-product-main">Skip to MCPace content</a>
      <aside class="mc-sidebar" aria-label="MCPace navigation">
        <a class="mc-brand" href="#home" data-mc-home-link aria-label="MCPace home">
          <span class="mc-brand-mark">${ICON.logo}</span>
          <span><strong>MCPace</strong><small>Local MCP manager</small></span>
        </a>
        <div class="mc-nav-caption">MCPace</div>
        <nav class="mc-nav" aria-label="Primary sections">${nav}</nav>
        <div class="mc-sidebar-status" data-mc-sidebar-status data-tone="neutral">
          <span class="mc-status-orb"></span><span><strong>Checking runtime</strong><small>Local state is loading</small></span>
        </div>
      </aside>
      <header class="mc-topbar">
        <button type="button" class="mc-command-button" data-mc-command-open>${ICON.search}<span>Search integrations, tools, or actions</span><kbd>⌘K</kbd></button>
        <button type="button" class="mc-topbar-setup" data-mc-setup aria-label="Open setup guide"><span data-mc-setup-progress>0/5</span><strong>Setup guide</strong></button>
        <button type="button" class="mc-topbar-live" data-mc-open-live hidden><i></i><span><strong data-mc-live-count>0 sessions</strong><small data-mc-live-detail>0 leases</small></span></button>
        <div class="mc-topbar-status" data-mc-topbar-status data-tone="neutral" role="status" aria-live="polite" aria-atomic="true"><span></span><strong>Checking</strong><small>runtime state</small></div>
        <button type="button" class="mc-icon-button" data-mc-refresh aria-label="Refresh runtime" title="Refresh runtime">${ICON.refresh}</button>
        <button type="button" class="mc-primary-button" data-mc-add>${ICON.plus}<span>Add integration</span></button>
      </header>
      <div id="mc-view-announcer" class="mc-sr-only" role="status" aria-live="polite" aria-atomic="true"></div>
      <main class="mc-stage" id="mc-product-main" tabindex="-1">
        <section class="mc-view" data-mc-view-host="home"></section>
        <section class="mc-view" data-mc-view-host="integrations" hidden></section>
        <section class="mc-view" data-mc-view-host="applications" hidden></section>
        <section class="mc-view" data-mc-view-host="activity" hidden></section>
        <section class="mc-view" data-mc-view-host="settings" hidden></section>
      </main>
      <nav class="mc-mobile-nav" aria-label="Mobile sections">
        ${[
					["home", "Home", ICON.home],
					["integrations", "Integrations", ICON.server],
					["applications", "Applications", ICON.apps],
					["activity", "Activity", ICON.activity],
					["settings", "Settings", ICON.settings],
				]
					.map(
						([id, label, icon]) =>
							`<button type="button" data-mc-view="${id}" ${id === "home" ? 'aria-current="page"' : ""}>${icon}<span>${label}</span>${id === "integrations" ? "<i data-mc-issue-dot hidden></i>" : ""}</button>`,
					)
					.join("")}
      </nav>
      <div class="mc-toast-region" aria-live="polite" aria-atomic="false"></div>`;
	}

	function buildProductShell() {
		const originalChildren = [...document.body.children];
		const baseRoot = document.createElement("div");
		baseRoot.id = "mc-base-root";
		baseRoot.setAttribute("aria-hidden", "true");
		originalChildren.forEach((child) => baseRoot.appendChild(child));
		state.baseRoot = baseRoot;

		const product = document.createElement("div");
		product.id = "mc-product-shell";
		setProductHtml(product, productShellMarkup());
		document.body.append(product, baseRoot);
		document.body.classList.add("mc-deep-ui");

		state.hosts = Object.fromEntries(
			$$("[data-mc-view-host]", product).map((host) => [
				host.dataset.mcViewHost,
				host,
			]),
		);
		discoverNodes();

		state.theme = readPreference("theme", "system", [
			"system",
			"dark",
			"light",
			"mono-dark",
			"mono-light",
		]);
		state.density = readPreference("density", "comfortable", [
			"comfortable",
			"compact",
		]);
		state.motion = readPreference("motion", "system", [
			"system",
			"reduced",
			"off",
		]);
		state.textSize = readPreference("textSize", "normal", ["normal", "large"]);
		state.effects = readPreference("effects", "soft", ["soft", "minimal"]);
		state.detailLevel = readPreference("detailLevel", "essential", [
			"essential",
			"full",
		]);
		state.integrationLayout = readPreference("integrationLayout", "list", [
			"list",
			"map",
		]);
		state.integrationSort = readPreference("integrationSort", "priority", [
			"priority",
			"name",
			"tools",
			"activity",
			"latency",
		]);
		state.integrationGroup = readPreference("integrationGroup", "none", [
			"none",
			"status",
			"source",
			"client",
			"project",
		]);
		state.activityView = readPreference("activityView", "events", [
			"live",
			"overview",
			"tools",
			"servers",
			"events",
		]);
		if (
			state.detailLevel === "essential" &&
			["tools", "servers"].includes(state.activityView)
		)
			state.activityView = "events";
		state.activityRange = readPreference("activityRange", "24h", [
			"1h",
			"24h",
			"7d",
			"all",
		]);
		state.tokenEstimates = readPreference("tokenEstimates", "show", [
			"show",
			"hide",
		]);
		state.pathVisibility = readPreference("pathVisibility", "full", [
			"full",
			"compact",
		]);
		state.contextLabels = readPreference("contextLabels", "show", [
			"show",
			"hide",
		]);
		state.exportMode = readPreference("exportMode", "safe", ["safe", "full"]);
		state.exposureMode = readPreference("exposureMode", "observed", [
			"observed",
			"potential",
		]);
		state.setupDismissed =
			readPreference("setupDismissed", "false", ["true", "false"]) === "true";
		state.pinnedServers = readSetPreference("pinnedServers");
		document.documentElement.dataset.mcTheme = state.theme;
		document.documentElement.dataset.mcDensity = state.density;
		document.documentElement.dataset.mcMotion = state.motion;
		document.documentElement.dataset.mcTextSize = state.textSize;
		document.documentElement.dataset.mcEffects = state.effects;
		document.documentElement.dataset.mcDetail = state.detailLevel;
		document.documentElement.dataset.mcCurrentView = "home";
		requestAnimationFrame(syncThemeColor);

		if (state.nodes.serverDialog) {
			state.nodes.serverDialog.removeAttribute("aria-hidden");
			document.body.appendChild(state.nodes.serverDialog);
		}

		buildViewSkeletons();
		createAddDialog();
		createCommandDialog();
		createEventDetailDialog();
		createSetupGuideDialog();
		createActionReviewDialog();
		installServerActionHooks();
		bindGlobalEvents();
	}

	function pageHeading(eyebrow, title, description, actions = "") {
		return `<header class="mc-page-heading"><div><span>${escapeHtml(eyebrow)}</span><h1>${escapeHtml(title)}</h1><p>${escapeHtml(description)}</p></div>${actions ? `<div class="mc-page-actions">${actions}</div>` : ""}</header>`;
	}

	function buildViewSkeletons() {
		buildHomeSkeleton();
		buildIntegrationsSkeleton();
		buildApplicationsSkeleton();
		buildActivitySkeleton();
		buildSettingsSkeleton();
	}

	function buildHomeSkeleton() {
		setProductHtml(
			state.hosts.home,
			`
      <div data-mc-home-content></div>`,
		);
	}

	function buildIntegrationsSkeleton() {
		const host = state.hosts.integrations;
		setProductHtml(
			host,
			`
      ${pageHeading("MCP servers", "Integrations", "Understand each server at a glance: readiness, observed clients, configuration source, health, isolation, and the next safe action.", `<button type="button" class="mc-primary-button" data-mc-add-inline>${ICON.plus}<span>Add integration</span></button>`)}
      <section class="mc-integration-workbench mc-server-atlas">
        <div class="mc-server-controlbar">
          <label class="mc-search-field">${ICON.search}<span class="mc-sr-only">Search integrations</span><input type="search" data-mc-integration-search placeholder="Search server, tool, client, project, or source" autocomplete="off"><kbd>/</kbd></label>
          <div class="mc-atlas-summary" role="group" aria-label="Filter integrations by state">
            <button type="button" data-mc-summary-filter="all" aria-pressed="true"><span>${ICON.server}</span><strong data-mc-summary-total>0</strong><small>servers</small></button>
            <button type="button" data-mc-summary-filter="attention" aria-pressed="false" data-tone="warn"><span aria-hidden="true">!</span><strong data-mc-summary-review>0</strong><small>review</small></button>
            <button type="button" data-mc-summary-filter="active" aria-pressed="false" data-tone="good"><span>${ICON.activity}</span><strong data-mc-summary-routes>0</strong><small>routes held</small></button>
            <button type="button" data-mc-summary-filter="working" aria-pressed="false"><span aria-hidden="true">✓</span><strong data-mc-summary-ready>0</strong><small>working</small></button>
            <button type="button" data-mc-summary-filter="disabled" aria-pressed="false"><span aria-hidden="true">–</span><strong data-mc-summary-disabled>0</strong><small>off</small></button>
            <button type="button" class="mc-pinned-filter" data-mc-integration-filter="pinned" aria-pressed="false"><span aria-hidden="true">☆</span><strong data-mc-pinned-count>0</strong><small>pinned</small></button>
          </div>
          <details class="mc-integration-more"><summary>${ICON.settings}<span>View options</span>${ICON.chevron}</summary><div>
            <label class="mc-sort-field mc-scope-field"><span>Scope</span><select data-mc-integration-scope><option value="all">All integrations</option><option value="local">Local only</option><option value="remote">Remote only</option><option value="credentials">Uses credentials</option><option value="risk">Needs risk review</option><option value="unused">No retained calls</option></select></label>
            <label class="mc-sort-field"><span>Sort</span><select data-mc-integration-sort><option value="priority">Needs attention first</option><option value="activity">Current and recent use</option><option value="latency">Slowest p95</option><option value="tools">Most tools</option><option value="name">Name</option></select></label>
            <label class="mc-sort-field"><span>Group</span><select data-mc-integration-group><option value="none">No grouping</option><option value="status">Current state</option><option value="source">Runtime location</option><option value="client">Observed client</option><option value="project">Observed project</option></select></label>
            <button type="button" class="mc-select-visible" data-mc-select-visible aria-pressed="false"><span aria-hidden="true"></span>Select visible</button>
            <div class="mc-layout-toggle" role="group" aria-label="Integration presentation"><button type="button" data-mc-integration-layout="list" aria-pressed="true" title="Compact server roster">${ICON.list}<span>Servers</span></button><button type="button" data-mc-integration-layout="map" aria-pressed="false" title="Observed client-to-server connections">${ICON.map}<span>Connections</span></button></div>
          </div></details>
        </div>
        <div class="mc-context-filter-strip">
          <span><strong>Observed use</strong><small>Retained calls and current route ownership</small></span>
          <label><span>Client</span><select data-mc-integration-client><option value="all">All observed clients</option></select></label>
          <label><span>Project</span><select data-mc-integration-project><option value="all">All observed projects</option></select></label>
          <button type="button" data-mc-clear-context hidden>Clear</button>
        </div>
        <div class="mc-route-ribbon" data-mc-route-ribbon aria-label="Observed client-to-server routes"></div>
        <div class="mc-results-line">
          <div class="mc-results-status" role="status" aria-live="polite" data-mc-integration-results>Waiting for server inventory.</div>
          <details class="mc-results-help"><summary>How to read this</summary><div><p><strong>On</strong> means the definition is exposed to MCPace. It does not prove the process is running.</p><p><strong>Runtime</strong> shows current ownership or retained historical evidence. A route lease can be idle.</p><p><strong>MCP</strong> means protocol evidence was measured. It is not an authorization or trust decision.</p><p><strong>Tools</strong> means MCPace retained a tools/list result.</p></div></details>
        </div>
        <div class="mc-bulk-bar" data-mc-bulk-bar hidden><div><strong data-mc-bulk-count>0 selected</strong><span>Actions run sequentially through the existing backend controls.</span></div><div><button type="button" data-mc-bulk-action="test">Test</button><button type="button" data-mc-bulk-action="enable">Enable</button><button type="button" data-mc-bulk-action="disable">Disable</button><button type="button" class="mc-text-button" data-mc-bulk-action="clear">Clear</button></div></div>
        <div class="mc-integration-list-shell" data-mc-integration-list-shell>
          <div class="mc-integration-columns" aria-hidden="true"><span>Server</span><span>Readiness</span><span>Who & where</span><span>Health & route</span><span>Actions</span></div>
          <div data-mc-server-list-mount></div>
          <div class="mc-inline-empty" data-mc-integration-empty hidden></div>
        </div>
        <div class="mc-route-map-shell" data-mc-route-map hidden></div>
      </section>`,
		);

		const mount = $("[data-mc-server-list-mount]", host);
		if (state.nodes.serverList) {
			state.nodes.serverList.removeAttribute("aria-hidden");
			mount.appendChild(state.nodes.serverList);
		} else {
			setProductHtml(
				mount,
				'<div class="mc-inline-empty">Server inventory is not available yet.</div>',
			);
		}
		if (state.nodes.serverOverflow)
			mount.appendChild(state.nodes.serverOverflow);

		$("[data-mc-add-inline]", host)?.addEventListener("click", () =>
			openAddDialog(),
		);
		$("[data-mc-integration-search]", host)?.addEventListener(
			"input",
			(event) => {
				state.integrationQuery = event.target.value;
				renderIntegrations();
			},
		);
		$$("[data-mc-summary-filter]", host).forEach((button) =>
			button.addEventListener("click", () => {
				state.integrationFilter = button.dataset.mcSummaryFilter;
				renderIntegrations();
			}),
		);
		$$("[data-mc-integration-filter]", host).forEach((button) =>
			button.addEventListener("click", () => {
				state.integrationFilter = button.dataset.mcIntegrationFilter;
				renderIntegrations();
			}),
		);
		$("[data-mc-integration-sort]", host)?.addEventListener(
			"change",
			(event) => {
				state.integrationSort = event.target.value;
				writePreference("integrationSort", state.integrationSort);
				renderIntegrations();
			},
		);
		$("[data-mc-integration-scope]", host)?.addEventListener(
			"change",
			(event) => {
				state.integrationScope = event.target.value;
				renderIntegrations();
			},
		);
		$("[data-mc-integration-group]", host)?.addEventListener(
			"change",
			(event) => {
				state.integrationGroup = event.target.value;
				writePreference("integrationGroup", state.integrationGroup);
				renderIntegrations();
			},
		);
		$("[data-mc-integration-client]", host)?.addEventListener(
			"change",
			(event) => {
				state.integrationClient = event.target.value;
				renderIntegrations();
			},
		);
		$("[data-mc-integration-project]", host)?.addEventListener(
			"change",
			(event) => {
				state.integrationProject = event.target.value;
				renderIntegrations();
			},
		);
		$("[data-mc-clear-context]", host)?.addEventListener("click", () => {
			state.integrationClient = "all";
			state.integrationProject = "all";
			renderIntegrations();
		});
		$("[data-mc-select-visible]", host)?.addEventListener("click", () =>
			toggleVisibleServerSelection(),
		);
		$$("[data-mc-bulk-action]", host).forEach((button) =>
			button.addEventListener("click", () =>
				runBulkServerAction(button.dataset.mcBulkAction, button),
			),
		);
		$$("[data-mc-integration-layout]", host).forEach((button) =>
			button.addEventListener("click", () => {
				state.integrationLayout = button.dataset.mcIntegrationLayout;
				writePreference("integrationLayout", state.integrationLayout);
				renderIntegrations();
			}),
		);
	}

	function buildApplicationsSkeleton() {
		const host = state.hosts.applications;
		setProductHtml(
			host,
			`
      ${pageHeading("AI applications", "Applications", "Connect AI apps and see exactly where MCPace writes their configuration.")}
      <section class="mc-app-workspace"><header><div><span>YOUR APPLICATIONS</span><h2>Connect and verify</h2><p>Preview shows the file change first. Apply writes it. Verify confirms the app can reach MCPace. Restore uses the saved backup.</p></div></header><div data-mc-client-panel-mount></div></section>
      <section class="mc-exposure-card" data-mc-client-exposure></section>
      <details class="mc-app-details" ${state.detailLevel === "full" ? "open" : ""}><summary><span>${ICON.apps}</span><span><strong>Configuration details</strong><small>Connection diagram, exact paths, and MCPace-owned files</small></span>${ICON.chevron}</summary><div class="mc-app-details-body"><section class="mc-client-overview" data-mc-client-overview></section><section class="mc-configuration-map-card"><header><div><span>Configuration map</span><h2>Where MCP settings live</h2><p>Paths come from the backend catalog and current overview. MCPace does not invent missing locations.</p></div><button type="button" class="mc-text-button" data-mc-open-observability-settings>Path display settings ${ICON.chevron}</button></header><div data-mc-configuration-map></div></section></div></details>`,
		);
		const mount = $("[data-mc-client-panel-mount]", host);
		if (state.nodes.clientPanel) {
			state.nodes.clientPanel.hidden = false;
			state.nodes.clientPanel.removeAttribute("aria-hidden");
			mount.appendChild(state.nodes.clientPanel);
		} else
			setProductHtml(
				mount,
				'<div class="mc-inline-empty">Client catalog will appear after the runtime loads.</div>',
			);
		$("[data-mc-open-observability-settings]", host)?.addEventListener(
			"click",
			() => {
				switchView("settings");
				setSettingsTab("observability");
			},
		);
	}

	function buildActivitySkeleton() {
		const host = state.hosts.activity;
		setProductHtml(
			host,
			`
      ${pageHeading("Recorded operations", "Activity", "See what used your integrations, what failed, and how long calls took.", `<div class="mc-activity-heading-actions"><div class="mc-export-menu"><button type="button" class="mc-secondary-button" data-mc-export-activity="json">Export JSON</button><button type="button" class="mc-text-button" data-mc-export-activity="csv">CSV</button></div><button type="button" class="mc-secondary-button" data-mc-activity-refresh>${ICON.refresh}<span>Refresh</span></button></div>`)}
      <span class="mc-sr-only">Usage & activity</span>
      <section class="mc-observability-head">
        <div class="mc-observability-tabs" role="tablist" aria-label="Usage and activity views">
          ${[
						["live", "Live now"],
						["overview", "Overview"],
						["tools", "Tools"],
						["servers", "Servers"],
						["events", "Events"],
					]
						.map(([id, label]) => {
							const visibleLabel =
								{
									events: "Recent",
									overview: "Usage",
									live: "Right now",
									tools: "By tool",
									servers: "By server",
								}[id] || label;
							return `<button type="button" role="tab" id="mc-activity-tab-${id}" aria-controls="mc-activity-panel-${id}" aria-selected="${id === state.activityView}" tabindex="${id === state.activityView ? "0" : "-1"}" data-mc-activity-view="${id}">${visibleLabel}<span class="mc-sr-only">${label}</span></button>`;
						})
						.join("")}
        </div>
        <label class="mc-range-select"><span>Window</span><select data-mc-activity-range><option value="1h">Last hour</option><option value="24h">Last 24 hours</option><option value="7d">Last 7 days</option><option value="all">Retained logs</option></select></label>
      </section>
      <section class="mc-usage-summary" data-mc-activity-summary aria-label="Usage summary"></section>
      <div class="mc-observability-panels">
        <section role="tabpanel" id="mc-activity-panel-live" aria-labelledby="mc-activity-tab-live" data-mc-activity-panel="live" hidden><div data-mc-live-activity></div></section>
        <section role="tabpanel" id="mc-activity-panel-overview" aria-labelledby="mc-activity-tab-overview" data-mc-activity-panel="overview"><div data-mc-usage-overview></div></section>
        <section role="tabpanel" id="mc-activity-panel-tools" aria-labelledby="mc-activity-tab-tools" data-mc-activity-panel="tools" hidden><div data-mc-usage-tools></div></section>
        <section role="tabpanel" id="mc-activity-panel-servers" aria-labelledby="mc-activity-tab-servers" data-mc-activity-panel="servers" hidden><div data-mc-usage-servers></div></section>
        <section role="tabpanel" id="mc-activity-panel-events" aria-labelledby="mc-activity-tab-events" data-mc-activity-panel="events" hidden>
          <section class="mc-activity-workbench">
            <div class="mc-activity-toolbar"><div class="mc-filter-tabs" role="group" aria-label="Activity type filter"><button type="button" data-mc-activity-filter="all" aria-pressed="true">All</button><button type="button" data-mc-activity-filter="tool" aria-pressed="false">Tool calls</button><button type="button" data-mc-activity-filter="error" aria-pressed="false">Errors</button><button type="button" data-mc-activity-filter="runtime" aria-pressed="false">Runtime</button></div><label class="mc-search-field mc-search-field-small">${ICON.search}<span class="mc-sr-only">Search activity</span><input type="search" data-mc-activity-search placeholder="Search event, server, client, or trace" autocomplete="off"></label></div>
            <div class="mc-results-status" role="status" aria-live="polite" data-mc-activity-results></div><div class="mc-event-stream" data-mc-event-stream></div>
          </section>
        </section>
      </div>
      <details class="mc-raw-drawer"><summary>${ICON.terminal}<span><strong>Technical event data</strong><small>Raw retained operations from local log files</small></span>${ICON.chevron}</summary><div data-mc-raw-telemetry></div></details>`,
		);
		$("[data-mc-activity-refresh]", host)?.addEventListener(
			"click",
			refreshRuntime,
		);
		$$("[data-mc-export-activity]", host).forEach((button) =>
			button.addEventListener("click", () =>
				exportActivity(button.dataset.mcExportActivity),
			),
		);
		$$("[data-mc-activity-view]", host).forEach((button) => {
			button.addEventListener("click", () => {
				state.activityView = button.dataset.mcActivityView;
				state.activityLimit = 16;
				writePreference("activityView", state.activityView);
				renderActivity();
			});
			button.addEventListener("keydown", activityTabKeydown);
		});
		$("[data-mc-activity-range]", host)?.addEventListener("change", (event) => {
			state.activityRange = event.target.value;
			state.activityLimit = 16;
			writePreference("activityRange", state.activityRange);
			renderActivity();
		});
		$$("[data-mc-activity-filter]", host).forEach((button) =>
			button.addEventListener("click", () => {
				state.activityFilter = button.dataset.mcActivityFilter;
				state.activityLimit = 16;
				renderActivity();
			}),
		);
		$("[data-mc-activity-search]", host)?.addEventListener("input", (event) => {
			state.activityQuery = event.target.value;
			state.activityLimit = 16;
			renderActivity();
		});
	}

	function activityTabKeydown(event) {
		const tabs = $$(
			"[data-mc-activity-view]",
			event.currentTarget.closest('[role="tablist"]'),
		).filter(visible);
		const current = tabs.indexOf(event.currentTarget);
		if (current < 0) return;
		let next = null;
		if (event.key === "ArrowRight" || event.key === "ArrowDown")
			next = (current + 1) % tabs.length;
		else if (event.key === "ArrowLeft" || event.key === "ArrowUp")
			next = (current - 1 + tabs.length) % tabs.length;
		else if (event.key === "Home") next = 0;
		else if (event.key === "End") next = tabs.length - 1;
		if (next === null) return;
		event.preventDefault();
		tabs[next].focus();
		tabs[next].click();
	}

	function buildSettingsSkeleton() {
		const host = state.hosts.settings;
		const tabOrientation = window.innerWidth < 1280 ? "horizontal" : "vertical";
		const categories = [
			["general", "General", "Appearance and maintenance"],
			["security", "Security", "Trust, authorization, and protocol"],
			["discovery", "Discovery", "Catalog and automatic work"],
			["observability", "Observability", "Usage, paths, and privacy"],
			["advanced", "Advanced", "Runtime, policy, and capacity"],
		];
		setProductHtml(
			host,
			`
      ${pageHeading("Preferences", "Settings", "Choose how MCPace looks and how the local runtime behaves.")}
      <div class="mc-settings-layout"><div class="mc-settings-tabs" role="tablist" aria-label="Settings categories" aria-orientation="${tabOrientation}">${categories.map(([id, label, description], index) => `<button type="button" role="tab" id="mc-settings-tab-${id}" aria-controls="mc-settings-panel-${id}" aria-selected="${id === "general"}" tabindex="${id === "general" ? "0" : "-1"}" data-mc-settings-tab="${id}"><span>0${index + 1}</span><span><strong>${label}</strong><small>${description}</small></span>${ICON.chevron}</button>`).join("")}</div><div class="mc-settings-panels">
        <section role="tabpanel" id="mc-settings-panel-general" aria-labelledby="mc-settings-tab-general" data-mc-settings-panel="general">
          <section class="mc-preferences-card mc-appearance-card">
            <header><span>Interface</span><h2>Choose how much MCPace shows</h2><p>Essentials keeps daily work calm. Full detail keeps every diagnostic and operator surface available.</p></header>
            <div class="mc-appearance-preview" aria-hidden="true"><div><i></i><span></span><span></span></div><div><b></b><span></span><span></span></div><div><b></b><span></span><span></span></div></div>
            <div class="mc-preference-row"><div><strong>Information level</strong><span>Essentials uses plain language and hides deep runtime evidence until requested.</span></div><div class="mc-segmented"><button type="button" data-mc-detail-level="essential">Essentials</button><button type="button" data-mc-detail-level="full">Full detail</button></div></div>
            <div class="mc-preference-row"><div><strong>Theme</strong><span>System follows the operating system. Monochrome themes use lightness, symbols, and borders instead of hue.</span></div><div class="mc-theme-grid"><button type="button" data-mc-theme="system"><i data-swatch="system"></i><span>System</span></button><button type="button" data-mc-theme="light"><i data-swatch="light"></i><span>Light</span></button><button type="button" data-mc-theme="dark"><i data-swatch="dark"></i><span>Dark</span></button><button type="button" data-mc-theme="mono-light"><i data-swatch="mono-light"></i><span>Mono light</span></button><button type="button" data-mc-theme="mono-dark"><i data-swatch="mono-dark"></i><span>Mono dark</span></button></div></div>
            <div class="mc-preference-row"><div><strong>Density</strong><span>Comfortable is easier to scan; compact fits more rows without changing meaning.</span></div><div class="mc-segmented"><button type="button" data-mc-density="comfortable">Comfortable</button><button type="button" data-mc-density="compact">Compact</button></div></div>
            <div class="mc-preference-row"><div><strong>Text size</strong><span>Large text increases reading comfort without enabling the denser operator layout.</span></div><div class="mc-segmented"><button type="button" data-mc-text-size="normal">Normal</button><button type="button" data-mc-text-size="large">Large</button></div></div>
            <div class="mc-preference-row"><div><strong>Visual effects</strong><span>Minimal removes glow, translucency, and decorative gradients while keeping hierarchy intact.</span></div><div class="mc-segmented"><button type="button" data-mc-effects="soft">Soft</button><button type="button" data-mc-effects="minimal">Minimal</button></div></div>
            <div class="mc-preference-row"><div><strong>Motion</strong><span>Motion explains where content came from. Reduced removes travel and Off removes transitions entirely.</span></div><div class="mc-segmented"><button type="button" data-mc-motion="system">System</button><button type="button" data-mc-motion="reduced">Reduced</button><button type="button" data-mc-motion="off">Off</button></div></div>
          </section>
          <section class="mc-maintenance-card"><header><span>Version & maintenance</span><h2>Keep MCPace current</h2><p>MCPace checks npm after the dashboard opens and caches the result for six hours. Installing an update remains user-triggered; nothing is rewritten silently.</p></header><div class="mc-maintenance-action" data-mc-update-action></div><div data-mc-update-notice></div></section><div class="mc-runtime-settings" data-mc-settings-runtime="general"></div>
        </section>
        <section role="tabpanel" id="mc-settings-panel-security" aria-labelledby="mc-settings-tab-security" data-mc-settings-panel="security" hidden><div class="mc-settings-context"><span>${ICON.shield}</span><div><strong>Trust before execution</strong><p>Origin, access scope, credentials, protocol, and tool evidence are separate checks.</p></div></div><div data-mc-access-review></div><section class="mc-protocol-readiness-card" data-mc-protocol-readiness></section><div class="mc-runtime-settings" data-mc-settings-runtime="security"></div></section>
        <section role="tabpanel" id="mc-settings-panel-discovery" aria-labelledby="mc-settings-tab-discovery" data-mc-settings-panel="discovery" hidden><div class="mc-settings-context"><span>${ICON.compass}</span><div><strong>Discovery stays preview-first</strong><p>Search, installation, enablement, and tools/list remain explicit stages.</p></div><button type="button" class="mc-secondary-button" data-mc-open-catalog>Explore catalog</button></div><div class="mc-runtime-settings" data-mc-settings-runtime="discovery"></div></section>
        <section role="tabpanel" id="mc-settings-panel-observability" aria-labelledby="mc-settings-tab-observability" data-mc-settings-panel="observability" hidden><div class="mc-settings-context"><span>${ICON.activity}</span><div><strong>Measured, reported, and estimated are never merged</strong><p>Latency and payload bytes come from the MCPace tool boundary. Token totals are exact only when optional metadata reports them.</p></div><button type="button" class="mc-secondary-button" data-mc-open-usage>Open usage</button></div><section class="mc-preferences-card"><header><span>Local display & privacy</span><h2>Observability presentation</h2><p>These preferences change only this browser. Backend audit collection continues according to runtime behavior.</p></header><div class="mc-preference-row"><div><strong>Payload token estimates</strong><span>Show UTF-8 payload bytes ÷ 4 as an approximation, clearly separated from reported tokens.</span></div><div class="mc-segmented"><button type="button" data-mc-token-estimates="show">Show estimate</button><button type="button" data-mc-token-estimates="hide">Hide estimate</button></div></div><div class="mc-preference-row"><div><strong>Configuration paths</strong><span>Full paths help troubleshooting; compact paths reduce visual exposure.</span></div><div class="mc-segmented"><button type="button" data-mc-path-visibility="full">Full</button><button type="button" data-mc-path-visibility="compact">Compact</button></div></div><div class="mc-preference-row"><div><strong>Client and project labels</strong><span>Hide context labels from derived UI while raw backend entries remain available to the local operator.</span></div><div class="mc-segmented"><button type="button" data-mc-context-labels="show">Show</button><button type="button" data-mc-context-labels="hide">Hide</button></div></div><div class="mc-preference-row"><div><strong>Activity exports</strong><span>Privacy-safe is the default: exact client, project, session, lease, trace, error text, log paths, and raw payload are removed or replaced with export-local aliases.</span></div><div class="mc-segmented"><button type="button" data-mc-export-mode="safe">Privacy-safe</button><button type="button" data-mc-export-mode="full">Full local</button></div></div></section><section class="mc-observability-settings-card"><header><span>Audit quality</span><h2>Current retained window</h2></header><div data-mc-observability-quality></div></section><section class="mc-configuration-map-card"><header><div><span>Data locations</span><h2>Where MCPace stores and reads state</h2><p>Paths are observed from the live overview; missing paths are not invented.</p></div></header><div class="mc-path-list" data-mc-data-paths></div></section><section class="mc-disclosure-card"><strong>Retention and scope</strong><p>The dashboard requests a bounded retained-operations window from the active and rotated local log files, with a 500-entry log-tail fallback for older backends. Counts are retained-window totals, not guaranteed lifetime totals. Argument content is represented in audit by a fingerprint; raw tool results are not copied into the summary cards.</p></section></section>
        <section role="tabpanel" id="mc-settings-panel-advanced" aria-labelledby="mc-settings-tab-advanced" data-mc-settings-panel="advanced" hidden><div class="mc-settings-context"><span>${ICON.terminal}</span><div><strong>Expert runtime controls</strong><p>Capacity, route policy, endpoint details, and compatibility remain outside everyday server controls.</p></div></div><section class="mc-operations-card"><header><span>Runtime maintenance</span><h2>Repair or restart deliberately</h2><p>These actions affect the local control plane and use existing backend confirmation.</p></header><div class="mc-operations-grid" data-mc-runtime-actions></div></section><div class="mc-runtime-settings" data-mc-settings-runtime="advanced"></div></section>
      </div></div>`,
		);
		const generalMount = $('[data-mc-settings-runtime="general"]', host);
		const securityMount = $('[data-mc-settings-runtime="security"]', host);
		const discoveryMount = $('[data-mc-settings-runtime="discovery"]', host);
		const advancedMount = $('[data-mc-settings-runtime="advanced"]', host);
		moveRuntimePanel(state.nodes.preferencesPanel, generalMount, "preferences");
		moveRuntimePanel(state.nodes.trustPanel, securityMount, "trust");
		moveRuntimePanel(state.nodes.automationPanel, discoveryMount, "automation");
		moveRuntimePanel(state.nodes.runtimePanel, advancedMount, "runtime");
		moveRuntimePanel(state.nodes.policyPanel, advancedMount, "policy");
		moveAccessReview($("[data-mc-access-review]", host));
		moveUpdateControls(
			$("[data-mc-update-action]", host),
			$("[data-mc-update-notice]", host),
		);
		moveRuntimeActions($("[data-mc-runtime-actions]", host));
		$$("[data-mc-settings-tab]", host).forEach((tab) => {
			tab.addEventListener("click", () =>
				setSettingsTab(tab.dataset.mcSettingsTab),
			);
			tab.addEventListener("keydown", settingsTabKeydown);
		});
		$$("[data-mc-theme]", host).forEach((button) =>
			button.addEventListener("click", () => setTheme(button.dataset.mcTheme)),
		);
		$$("[data-mc-detail-level]", host).forEach((button) =>
			button.addEventListener("click", () =>
				setDetailLevel(button.dataset.mcDetailLevel),
			),
		);
		$$("[data-mc-density]", host).forEach((button) =>
			button.addEventListener("click", () =>
				setDensity(button.dataset.mcDensity),
			),
		);
		$$("[data-mc-text-size]", host).forEach((button) =>
			button.addEventListener("click", () =>
				setTextSize(button.dataset.mcTextSize),
			),
		);
		$$("[data-mc-effects]", host).forEach((button) =>
			button.addEventListener("click", () =>
				setEffects(button.dataset.mcEffects),
			),
		);
		$$("[data-mc-motion]", host).forEach((button) =>
			button.addEventListener("click", () =>
				setMotion(button.dataset.mcMotion),
			),
		);
		$$("[data-mc-token-estimates]", host).forEach((button) =>
			button.addEventListener("click", () => {
				state.tokenEstimates = button.dataset.mcTokenEstimates;
				writePreference("tokenEstimates", state.tokenEstimates);
				renderAll();
			}),
		);
		$$("[data-mc-path-visibility]", host).forEach((button) =>
			button.addEventListener("click", () => {
				state.pathVisibility = button.dataset.mcPathVisibility;
				writePreference("pathVisibility", state.pathVisibility);
				renderAll();
			}),
		);
		$$("[data-mc-context-labels]", host).forEach((button) =>
			button.addEventListener("click", () => {
				state.contextLabels = button.dataset.mcContextLabels;
				writePreference("contextLabels", state.contextLabels);
				renderAll();
			}),
		);
		$$("[data-mc-export-mode]", host).forEach((button) =>
			button.addEventListener("click", () => {
				state.exportMode = button.dataset.mcExportMode;
				writePreference("exportMode", state.exportMode);
				renderAll();
			}),
		);
		$("[data-mc-open-catalog]", host)?.addEventListener("click", () =>
			openAddDialog("catalog"),
		);
		$("[data-mc-open-usage]", host)?.addEventListener("click", () =>
			switchView("activity"),
		);
		updatePreferenceControls();
		renderObservabilitySettings();
	}

	function moveRuntimePanel(panel, mount, key) {
		if (!panel || !mount) {
			if (mount)
				setProductHtml(
					mount,
					'<div class="mc-inline-empty">This runtime section is not available in the current backend payload.</div>',
				);
			return;
		}
		panel.hidden = false;
		panel.removeAttribute("aria-hidden");
		panel.dataset.mcRuntimePanel = key;
		mount.appendChild(panel);
		if (key === "preferences") tidyLegacyPreferences(panel);
	}

	function tidyLegacyPreferences(panel) {
		["server-sort", "server-scope", "density-select"].forEach((id) => {
			const field = $(`#${id}`, panel)?.closest(".field");
			if (field) field.hidden = true;
		});
		const context = $("#view-context", panel);
		const label = $(".label", context);
		const heading = $("h3", context);
		const note = $(".section-note", context);
		if (label) label.textContent = "Background behavior";
		if (heading) heading.textContent = "Dashboard refresh";
		if (note)
			note.textContent =
				"Auto refresh changes only how often this browser requests local runtime state.";
		const guide = $("#help-page", panel);
		if (guide) {
			const guideLabel = $(".label", guide);
			const guideHeading = $("h3", guide);
			if (guideLabel) guideLabel.textContent = "Product map";
			if (guideHeading) guideHeading.textContent = "Where each task lives now";
			const lists = $$(".help-list", guide);
			if (lists[0])
				setProductHtml(
					lists[0],
					"<span><strong>Home</strong> shows current readiness and the next useful action.</span><span><strong>Integrations</strong> manages MCP servers, tools, isolation, and setup.</span><span><strong>Applications</strong> previews and applies AI-client configuration.</span><span><strong>Activity</strong> shows recent calls, failures, and live route ownership.</span>",
				);
			if (lists[1])
				setProductHtml(
					lists[1],
					"<span><strong>Settings</strong> controls appearance, trust, discovery, privacy, and runtime behavior.</span><span><strong>Essentials</strong> keeps daily work quiet.</span><span><strong>Full detail</strong> reveals protocol evidence and operator controls.</span><span><strong>Secret values</strong> are never rendered by the product UI.</span>",
				);
		}
	}

	function moveAccessReview(mount) {
		if (!mount) return;
		if (!state.nodes.accessReview) {
			setProductHtml(
				mount,
				'<div class="mc-inline-empty">Access review is not available in this backend build.</div>',
			);
			return;
		}
		state.nodes.accessReview.hidden = false;
		state.nodes.accessReview.removeAttribute("aria-hidden");
		state.nodes.accessReview.dataset.mcRuntimePanel = "access-review";
		mount.appendChild(state.nodes.accessReview);
	}

	function moveUpdateControls(actionMount, noticeMount) {
		if (actionMount && state.nodes.updateCheckButton) {
			const button = state.nodes.updateCheckButton;
			button.textContent = "Check for updates";
			button.classList.add("mc-secondary-button");
			actionMount.appendChild(button);
		} else if (actionMount) {
			setProductHtml(
				actionMount,
				'<span class="mc-inline-note">Update checks are unavailable in this backend build.</span>',
			);
		}
		if (noticeMount && state.nodes.updateNotice) {
			state.nodes.updateNotice.removeAttribute("aria-hidden");
			noticeMount.appendChild(state.nodes.updateNotice);
		}
	}

	function moveRuntimeActions(mount) {
		if (!mount) return;
		const actions = [
			[
				state.nodes.repairButton,
				"Repair runtime",
				"Rebuild local runtime wiring and client configuration where needed.",
				"repair",
			],
			[
				state.nodes.startButton,
				"Start hub",
				"Start the optional local hub process.",
				"start",
			],
			[
				state.nodes.stopButton,
				"Stop hub",
				"Stop the local hub; active clients may temporarily lose routing.",
				"stop",
			],
		];
		const available = actions.filter(([button]) => button);
		if (!available.length) {
			setProductHtml(
				mount,
				'<div class="mc-inline-empty">Runtime maintenance actions are unavailable in this backend build.</div>',
			);
			return;
		}
		available.forEach(([button, label, description, tone]) => {
			const item = document.createElement("div");
			item.className = "mc-operation-item";
			item.dataset.tone = tone;
			setProductHtml(
				item,
				`<div><strong>${escapeHtml(label)}</strong><span>${escapeHtml(description)}</span></div>`,
			);
			button.textContent = label;
			button.classList.add(
				tone === "stop"
					? "mc-danger-button"
					: tone === "repair"
						? "mc-primary-button"
						: "mc-secondary-button",
			);
			item.appendChild(button);
			mount.appendChild(item);
		});
	}

	function serverModels() {
		if (Array.isArray(state.serverModelCache)) return state.serverModelCache;
		const rows = $$(".server-row", state.nodes.serverList);
		const records = overviewServers();
		const instances = overviewInstances();
		const operatorItems = Array.isArray(overviewData().operatorPlan?.items)
			? overviewData().operatorPlan.items
			: [];
		const controlItems = Array.isArray(
			overviewData().runtimeControlPlane?.items,
		)
			? overviewData().runtimeControlPlane.items
			: [];
		const usageGroups = new Map(
			usageAnalytics(auditRecords()).servers.map((group) => [group.key, group]),
		);
		const leaseModels = activeLeaseModels();
		const sessionModels = liveSessionModels();
		const models = rows.map((row, index) => {
			const name =
				row.dataset.serverName ||
				text($(".server-source-cell .name", row)) ||
				`Server ${index + 1}`;
			const backend =
				records.find((server) => String(server?.name || "") === name) || {};
			const operator =
				operatorItems.find((item) => String(item?.name || "") === name) || {};
			const control =
				controlItems.find((item) => String(item?.name || "") === name) || {};
			const enabled =
				backend.effectiveEnabled !== undefined
					? Boolean(backend.effectiveEnabled)
					: row.dataset.enabled !== "false";
			const domEvidenceTitle = text(
				$(".server-evidence-cell .server-cell-primary", row),
			);
			const domEvidenceBody = text(
				$(".server-evidence-cell .server-cell-secondary", row),
			);
			const domRouteTitle = text(
				$(".server-routing-cell .server-cell-primary", row),
			);
			const domRouteBody = text(
				$(".server-routing-cell .server-cell-secondary", row),
			);
			const evidenceTitle = String(
				operator.evidence ||
					operator.nextAction ||
					domEvidenceTitle ||
					control.evidenceState ||
					"Evidence unavailable",
			);
			const evidenceBody = String(
				operator.rationale ||
					control.why ||
					domEvidenceBody ||
					"The backend did not return an evidence explanation.",
			);
			const routeTitle = String(
				operator.currentMode ||
					control.parallelism?.mode ||
					domRouteTitle ||
					"automatic",
			);
			const routeBody = String(
				control.parallelism?.reason ||
					operator.rationale ||
					domRouteBody ||
					"The backend did not return a routing explanation.",
			);
			const sourceMeta = text(
				$(".server-source-cell .server-cell-secondary", row),
			);
			const toolDefinitions = cachedToolDefinitions(name);
			const previewTools = $$(".server-tool-preview .tag", row)
				.map(text)
				.filter((value) => value && !/^\+\d+/.test(value));
			const tools = toolDefinitions
				.map((tool) => String(tool?.name || ""))
				.filter(Boolean).length
				? toolDefinitions
						.map((tool) => String(tool?.name || ""))
						.filter(Boolean)
				: previewTools;
			const toolLabels = toolDefinitions.length
				? toolDefinitions.map(toolDisplayName)
				: tools;
			const toolCount = finiteNumber(
				cachedToolEntry(name).toolCount,
				finiteNumber(
					backend.toolCount,
					numberFrom(evidenceTitle, /(\d+)\s+tools?/i, tools.length),
				),
			);
			const lane = String(operator.lane || "").toLowerCase();
			const operatorTone = ["good", "warn", "bad", "off", "neutral"].includes(
				String(operator.tone || ""),
			)
				? String(operator.tone)
				: "";
			let tone = !enabled
				? "off"
				: operatorTone ||
					(row.classList.contains("bad")
						? "bad"
						: row.classList.contains("warn")
							? "warn"
							: toneFrom(`${evidenceTitle} ${evidenceBody}`));
			if (
				enabled &&
				tone === "neutral" &&
				(lane === "ready" ||
					/available|completed|passed/i.test(
						`${evidenceTitle} ${evidenceBody}`,
					))
			)
				tone = "good";
			const status =
				!enabled || lane === "off"
					? "Disabled"
					: lane === "blocked"
						? "Blocked"
						: lane === "unchecked"
							? "Test required"
							: lane === "guarded"
								? "Protected review"
								: lane === "ready" || tone === "good"
									? "Working"
									: /test failed/i.test(evidenceTitle)
										? "Test failed"
										: statusLabel(tone, `${evidenceTitle} ${evidenceBody}`);
			const normalizedMode = String(
				control.parallelism?.mode || operator.currentMode || routeTitle || "",
			).toLowerCase();
			const routeMode = /project/.test(normalizedMode)
				? "Per project"
				: /session|chat/.test(normalizedMode)
					? "Per chat"
					: /serial|safe queue/.test(normalizedMode)
						? "Serialized"
						: /pool/.test(normalizedMode)
							? "Reusable sessions"
							: /shared/.test(normalizedMode)
								? "Shared"
								: routeTitle || "Automatic";
			const sourceType = String(
				backend.sourceType || (/http/i.test(sourceMeta) ? "http" : "stdio"),
			);
			const sourcePath = String(backend.sourcePath || "");
			const sourceCommand = String(
				backend.sourceCommand || backend.command || "",
			);
			const sourceUrl = String(backend.sourceUrl || backend.url || "");
			const sourceArgs = Array.isArray(backend.sourceArgs)
				? backend.sourceArgs.map(String)
				: [];
			const sourceLocation =
				sourcePath ||
				sourceUrl ||
				[sourceCommand, ...sourceArgs].filter(Boolean).join(" ") ||
				sourceMeta;
			const usage = usageGroups.get(name) || {
				calls: 0,
				successes: 0,
				failures: 0,
				p50: null,
				p95: null,
				queueP95: null,
				lastTimestamp: null,
				reportedTokens: 0,
				reportedRecords: 0,
				estimatedTokens: 0,
				estimatedRecords: 0,
				clients: new Set(),
				projects: new Set(),
			};
			const activeInstances = instances.filter(
				(instance) =>
					String(
						instance?.server || instance?.serverName || instance?.name || "",
					) === name,
			);
			const activeLeases = leaseModels.filter((lease) => lease.server === name);
			const liveSessions = sessionModels.filter((session) =>
				session.servers.includes(name),
			);
			const risks = toolDefinitions.flatMap(toolRisk);
			const riskCounts = risks.reduce((map, risk) => {
				map[risk.tone] = (map[risk.tone] || 0) + 1;
				return map;
			}, {});
			return {
				row,
				backend,
				name,
				key: name,
				initials: initials(name),
				enabled,
				evidenceTitle,
				evidenceBody,
				routeTitle,
				routeBody,
				routeMode,
				sourceMeta,
				sourceType,
				sourcePath,
				sourceCommand,
				sourceUrl,
				sourceArgs,
				sourceLocation,
				sourceEnvNames: Array.isArray(backend.sourceEnvNames)
					? backend.sourceEnvNames.map(String)
					: [],
				sourceHeaderNames: Array.isArray(backend.sourceHeaderNames)
					? backend.sourceHeaderNames.map(String)
					: [],
				scopeClass: String(backend.scopeClass || ""),
				concurrencyPolicy: String(backend.concurrencyPolicy || ""),
				stateBinding: String(backend.stateBinding || ""),
				credentialBinding: String(backend.credentialBinding || ""),
				runtimeType: String(backend.runtimeType || ""),
				stateClass: String(backend.stateClass || ""),
				effectClass: String(backend.effectClass || ""),
				transportPreference: String(backend.transportPreference || ""),
				transportStatus: String(backend.transportStatus || ""),
				maxWorkers: finiteNumber(backend.maxWorkers, null),
				maxInFlightPerWorker: finiteNumber(backend.maxInFlightPerWorker, null),
				tools,
				toolLabels,
				toolDefinitions,
				toolCount,
				operator,
				control,
				lane,
				riskDecision:
					control.toolRisk && typeof control.toolRisk === "object"
						? control.toolRisk
						: null,
				tone,
				status,
				usage,
				activeInstances,
				activeLeases,
				liveSessions,
				riskCounts,
				searchable:
					`${name} ${evidenceTitle} ${evidenceBody} ${routeTitle} ${routeBody} ${sourceMeta} ${sourceLocation} ${sourceType} ${backend.scopeClass || ""} ${backend.stateClass || ""} ${backend.effectClass || ""} ${tools.join(" ")} ${toolLabels.join(" ")}`.toLowerCase(),
			};
		});
		state.serverModelCache = models;
		return models;
	}

	function isLoopbackUrl(value) {
		try {
			const url = new URL(String(value || ""));
			return ["localhost", "127.0.0.1", "::1", "[::1]"].includes(
				url.hostname.toLowerCase(),
			);
		} catch (_) {
			return false;
		}
	}

	function serverCapabilityProfile(server) {
		const cache = cachedToolEntry(server.name);
		const evidence =
			cache.capabilities ||
			server.backend.protocolCapabilities ||
			server.backend.capabilities ||
			{};
		const explicit = (key) =>
			Object.hasOwn(evidence, key)
				? Boolean(evidence[key])
				: Object.hasOwn(server.backend, `${key}Supported`)
					? Boolean(server.backend[`${key}Supported`])
					: null;
		const toolsEvidence = serverToolsEvidenceProfile(server);
		const toolsMeasured = toolsEvidence.measured;
		const capabilities = [
			{
				id: "tools",
				label: "Tools",
				state: toolsMeasured
					? "measured"
					: server.enabled
						? "not-measured"
						: "disabled",
				detail: toolsMeasured
					? toolsEvidence.detail
					: server.enabled
						? "Run Test to collect tools/list evidence"
						: "Enable before testing",
			},
			...[
				["resources", "Resources"],
				["prompts", "Prompts"],
				["tasks", "Tasks"],
				["logging", "Logging"],
				["completions", "Completions"],
			].map(([id, label]) => {
				const value = explicit(id);
				return {
					id,
					label,
					state:
						value === true
							? "reported"
							: value === false
								? "not-reported"
								: "not-measured",
					detail:
						value === true
							? "Reported by initialize evidence"
							: value === false
								? "Explicitly not reported"
								: "No retained initialize evidence",
				};
			}),
		];
		const measured = capabilities.filter((item) =>
			["measured", "reported", "not-reported"].includes(item.state),
		).length;
		return {
			protocolVersion: String(
				cache.protocolVersion || server.backend.protocolVersion || "",
			),
			serverName: String(
				cache.serverInfo?.name || server.backend.serverInfo?.name || "",
			),
			serverVersion: String(
				cache.serverInfo?.version || server.backend.serverInfo?.version || "",
			),
			capabilities,
			measured,
			total: capabilities.length,
			coverage: capabilities.length ? measured / capabilities.length : 0,
		};
	}

	function serverAccessProfile(server) {
		const remote =
			server.sourceType === "http" &&
			!isLoopbackUrl(server.sourceUrl || server.sourceLocation);
		const loopback =
			server.sourceType === "http" &&
			isLoopbackUrl(server.sourceUrl || server.sourceLocation);
		const credentialNames = unique([
			...server.sourceEnvNames,
			...server.sourceHeaderNames,
		]);
		const backendRisk = server.riskDecision || {};
		const backendCategories = Array.isArray(backendRisk.categories)
			? backendRisk.categories.map((value) => String(value).toLowerCase())
			: [];
		const destructive = Math.max(
			server.riskCounts.bad || 0,
			String(backendRisk.risk || "").toLowerCase() === "destructive" ? 1 : 0,
		);
		const external = Math.max(
			server.riskCounts.warn || 0,
			remote ||
				backendCategories.some((value) =>
					/network|external|open-world|credential/.test(value),
				)
				? 1
				: 0,
		);
		const dataScope = remote
			? "Remote network"
			: loopback
				? "Local HTTP"
				: "Local process";
		const auth = server.sourceHeaderNames.length
			? "Header names referenced; value availability is not verified"
			: server.sourceEnvNames.length
				? "Environment secret names referenced; value availability is not verified"
				: remote
					? "Authorization evidence not retained"
					: "Process environment; credential availability is not verified";
		const exposure = server.enabled
			? "Available through the MCPace endpoint to configured clients"
			: "Not exposed while disabled";
		const approvalRequired = backendRisk.approvalRequired === true;
		const tone =
			remote && !credentialNames.length
				? "warn"
				: destructive || approvalRequired
					? "warn"
					: server.enabled
						? "good"
						: "off";
		return {
			remote,
			loopback,
			credentialNames,
			destructive,
			external,
			approvalRequired,
			dataScope,
			auth,
			exposure,
			tone,
		};
	}

	function auditFailureGroups(records = rangedAuditRecords()) {
		const groups = new Map();
		records
			.filter((record) => !record.ok)
			.forEach((record) => {
				const key = record.errorKind || "unknown";
				if (!groups.has(key))
					groups.set(key, {
						key,
						label: key.replace(/_/g, " "),
						count: 0,
						calls: 0,
						stage: record.failureStage || "unknown",
						latest: null,
						servers: new Set(),
					});
				const group = groups.get(key);
				group.count += 1;
				group.calls += Math.max(1, record.failedCount);
				group.latest =
					Math.max(group.latest || 0, record.timestamp || 0) || null;
				group.servers.add(record.server);
			});
		return [...groups.values()].sort(
			(left, right) =>
				right.calls - left.calls || (right.latest || 0) - (left.latest || 0),
		);
	}

	function systemActionItems(model) {
		const items = [];
		const coveredServers = new Set();
		if (model.runtime.offline)
			items.push({
				tone: "bad",
				title: "Reconnect the local runtime",
				detail:
					model.runtime.note || "The dashboard cannot verify current routes.",
				action: "refresh",
				label: "Refresh",
			});
		model.servers.forEach((server) => {
			const operational = serverOperationalProfile(server);
			if (!["bad", "warn"].includes(operational.tone)) return;
			coveredServers.add(server.name);
			items.push({
				tone: operational.tone,
				title: `${server.name}: ${operational.title}`,
				detail: operational.next?.reason || operational.detail,
				server: server.name,
				tab: operational.next?.tab || "overview",
				label: operational.next?.label || "Review",
			});
		});
		const failures = auditFailureGroups()
			.filter((group) =>
				[...group.servers].some((name) => !coveredServers.has(name)),
			)
			.slice(0, 2);
		failures.forEach((group) =>
			items.push({
				tone:
					group.key === "authorization" || group.key === "policy_denied"
						? "warn"
						: "bad",
				title: `${group.calls} ${group.label} failure${group.calls === 1 ? "" : "s"}`,
				detail: `${group.servers.size} integration${group.servers.size === 1 ? "" : "s"} · stage ${group.stage}`,
				action: "activity-errors",
				label: "Inspect",
			}),
		);
		const retained = retainedWindow();
		if (retained.parseErrors)
			items.push({
				tone: "warn",
				title: "Some retained events could not be parsed",
				detail: `${retained.parseErrors} malformed log line${retained.parseErrors === 1 ? "" : "s"} were skipped.`,
				action: "observability",
				label: "Review data",
			});
		if (retained.source === "api/logs")
			items.push({
				tone: "neutral",
				title: "Extended retained history is unavailable",
				detail: "Statistics currently use the bounded 500-entry log tail.",
				action: "observability",
				label: "See scope",
			});
		return items.slice(0, 6);
	}

	function clientModels() {
		return $$(".client-setup-card[data-client-id]", state.nodes.clientList).map(
			(card) => {
				const name = text($(".name", card)) || card.dataset.clientId;
				const chip = text($(".chip", card));
				const meta = text($(".meta", card));
				const actions = $$("[data-client-setup-action]", card).map(
					(button) => ({
						action: button.dataset.clientSetupAction,
						label: text(button),
						button,
					}),
				);
				const category = /patchable/i.test(chip)
					? "patchable"
					: /manual/i.test(chip)
						? "manual"
						: /cloud/i.test(chip)
							? "cloud"
							: "unknown";
				const tone =
					category === "patchable"
						? "good"
						: category === "manual"
							? "warn"
							: category === "cloud"
								? "neutral"
								: toneFrom(`${chip} ${meta}`);
				const status =
					category === "patchable"
						? "Patch supported"
						: category === "manual"
							? "Manual setup"
							: category === "cloud"
								? "Cloud surface"
								: chip || "Not checked";
				return {
					card,
					id: card.dataset.clientId,
					path: card.dataset.clientPath || "",
					name,
					initials: initials(name),
					chip,
					meta,
					actions,
					category,
					tone,
					status,
				};
			},
		);
	}

	function runtimeSnapshot() {
		const system = text(state.nodes.systemState);
		const load = text(state.nodes.loadState);
		const note = text(state.nodes.loadNote);
		const refresh = text(state.nodes.refreshChip);
		const source = `${system} ${load} ${note} ${refresh}`;
		let tone = toneFrom(source);
		if (
			/waiting|loading|initializing|checking|—/.test(source.toLowerCase()) &&
			!/online|ready|healthy|linked|ok\b/.test(source.toLowerCase())
		)
			tone = "neutral";
		const offline = /offline|unavailable|failed|error|disconnected/.test(
			source.toLowerCase(),
		);
		const ready =
			!offline &&
			/online|ready|healthy|linked|ok\b|nominal/.test(source.toLowerCase());
		return {
			system,
			load,
			note,
			refresh,
			source,
			tone: offline ? "bad" : ready ? "good" : tone,
			offline,
			ready,
		};
	}

	function activityModels() {
		const events = [];
		auditRecords().forEach((record, index) => {
			const duration =
				record.totalMs === null ? "" : ` · ${formatDuration(record.totalMs)}`;
			const context =
				state.contextLabels === "show"
					? [
							record.clientId,
							record.projectRoot ? compactPath(record.projectRoot) : "",
						]
							.filter(Boolean)
							.join(" · ")
					: "";
			const meta = [
				record.server,
				context,
				`${record.callCount} call${record.callCount === 1 ? "" : "s"}${duration}`,
				record.error,
			]
				.filter(Boolean)
				.join(" · ");
			events.push({
				id: record.id,
				type: record.ok ? "tool" : "error",
				title: record.batch
					? `Batch · ${record.tools.slice(0, 3).join(", ")}${record.tools.length > 3 ? ` +${record.tools.length - 3}` : ""}`
					: record.tools[0],
				meta,
				chip: record.ok ? "ok" : "failed",
				tone: record.ok ? "good" : "bad",
				source: "Tool audit",
				payload: JSON.stringify(record.raw, null, 2),
				timestamp: record.timestamp,
				audit: record,
				index,
			});
		});

		rawLogs().forEach((entry, index) => {
			if (
				!entry ||
				entry.event === "tool_call_audit" ||
				entry.event === "tool_batch_audit"
			)
				return;
			const title = String(entry.event || "Runtime event");
			const level = String(entry.level || "info");
			const tone =
				level === "error"
					? "bad"
					: level === "warn"
						? "warn"
						: toneFrom(`${title} ${level}`);
			const type =
				tone === "bad" || /error|fail/i.test(title) ? "error" : "runtime";
			const detail = [entry.server, entry.message, entry.status, entry.error]
				.filter(Boolean)
				.map(String)
				.join(" · ");
			events.push({
				id: `runtime-${auditTimestamp(entry) || 0}-${index}`,
				type,
				title,
				meta: detail || "Backend event",
				chip: level,
				tone,
				source: "Backend log",
				payload: JSON.stringify(entry, null, 2),
				timestamp: auditTimestamp(entry),
				index: 1000 + index,
			});
		});

		const snapshots = $$("#activity-list article").filter(
			(item) =>
				text(item) &&
				!/no active (?:locks|route leases) returned/i.test(text(item)),
		);
		snapshots.forEach((item, index) => {
			const title = text($(".name", item)) || `Runtime state ${index + 1}`;
			const meta = text($(".meta", item));
			const chip = text($(".chip", item));
			const tone = toneFrom(`${chip} ${title} ${meta}`);
			events.push({
				id: `snapshot-${index}`,
				type: tone === "bad" ? "error" : "runtime",
				title,
				meta,
				chip: chip || "current",
				tone,
				source: "Current snapshot",
				current: true,
				timestamp: Date.now(),
				index: 2000 + index,
			});
		});
		return events.sort(
			(left, right) =>
				(right.timestamp || 0) - (left.timestamp || 0) ||
				left.index - right.index,
		);
	}

	function metrics() {
		const servers = serverModels();
		const clients = clientModels();
		const events = activityModels();
		const runtime = runtimeSnapshot();
		const usage = usageAnalytics();
		const operational = servers.map((server) => [
			server,
			serverOperationalProfile(server),
		]);
		const ready = operational.filter(
			([server, profile]) => server.enabled && profile.tone === "good",
		).length;
		const review = operational.filter(
			([server, profile]) =>
				server.enabled && ["warn", "bad"].includes(profile.tone),
		).length;
		const disabled = operational.filter(
			([server, profile]) => !server.enabled || profile.tone === "off",
		).length;
		const tools = servers.reduce((sum, server) => sum + server.toolCount, 0);
		const leaseEnvelope = overviewLeaseEnvelope();
		const liveSessions = liveSessionModels();
		const activeLeases = activeLeaseModels();
		const activeLocks = Math.max(
			leaseEnvelope.activeLeaseCount,
			numberFrom(text(state.nodes.activityChip), /(\d+)\s+active/i, 0),
		);
		const toolEvents = usage.calls;
		const errors =
			usage.failures +
			events.filter((event) => event.type === "error" && !event.audit).length;
		const overview = overviewData();
		const foundation =
			overview.dashboardFoundation &&
			typeof overview.dashboardFoundation === "object"
				? overview.dashboardFoundation
				: null;
		const foundationCounts =
			foundation?.counts && typeof foundation.counts === "object"
				? foundation.counts
				: {};
		const configuredClientKey = String(
			overview.clients?.configuredClientKeyName || "",
		).trim();
		const clientConfigured =
			foundationCounts.clientConfigured === true ||
			Boolean(configuredClientKey);
		const runtimeReady =
			typeof foundationCounts.runtimeReady === "boolean"
				? foundationCounts.runtimeReady
				: Boolean(runtime.ready && !runtime.offline);
		const routingReady =
			typeof foundationCounts.routingReady === "boolean"
				? foundationCounts.routingReady
				: Boolean(
						runtimeReady && clientConfigured && ready > 0 && review === 0,
					);
		return {
			servers,
			clients,
			events,
			runtime,
			usage,
			ready,
			review,
			disabled,
			tools,
			activeLocks,
			activeLeases,
			liveSessions,
			toolEvents,
			errors,
			foundation,
			foundationCounts,
			clientConfigured,
			runtimeReady,
			routingReady,
		};
	}

	function foundationActionKey(action) {
		const key = String(action || "refresh");
		if (key === "clients") return "applications";
		if (["import-server", "add-server"].includes(key)) return "add";
		if (key === "servers") return "integrations";
		if (key === "repair") return "settings";
		return key;
	}

	function homeState(model) {
		if (model.runtime.offline) {
			return {
				tone: "bad",
				eyebrow: "Local service unavailable",
				title: "MCPace cannot reach its local service",
				description:
					model.runtime.note ||
					"Refresh the local service before changing integrations.",
				primary: ["Refresh runtime", "refresh"],
				secondary: ["Open activity", "activity"],
			};
		}
		const checking =
			!model.runtime.ready &&
			/waiting|loading|initializing|checking|unknown|not loaded|—/.test(
				`${model.runtime.source} ${model.runtime.load} ${model.runtime.note}`.toLowerCase(),
			);
		if (checking) {
			return {
				tone: "neutral",
				eyebrow: "Checking runtime",
				title: "Reading local MCP state",
				description:
					"MCPace is waiting for the local service before it recommends an action.",
				primary: ["Refresh runtime", "refresh"],
				secondary: ["Open activity", "activity"],
			};
		}
		const foundation = model.foundation;
		if (foundation?.schema === "mcpace.dashboardFoundation.v1") {
			const next =
				foundation.nextStep && typeof foundation.nextStep === "object"
					? foundation.nextStep
					: {};
			const stateKey = String(
				foundation.nextStepKey || foundation.stateKey || next.key || "ready",
			);
			const tone =
				foundation.status === "bad"
					? "bad"
					: foundation.status === "good"
						? "good"
						: "warn";
			const label = String(
				next.actionLabel || foundation.actions?.[0]?.label || "Open",
			);
			const action = foundationActionKey(
				next.action || foundation.actions?.[0]?.action || "refresh",
			);
			const eyebrow =
				stateKey === "ready"
					? "Ready"
					: stateKey === "client"
						? "Client setup"
						: stateKey === "routing"
							? "Routing review"
							: "Next safe step";
			return {
				tone,
				eyebrow,
				title: String(foundation.title || next.title || "Finish MCPace setup"),
				description: String(
					foundation.body ||
						next.body ||
						"Complete the next backend-selected step before normal use.",
				),
				primary: [label, action],
				secondary:
					stateKey === "ready"
						? ["View activity", "activity"]
						: stateKey === "source"
							? ["Open applications", "applications"]
							: ["Open integrations", "integrations"],
			};
		}
		if (!model.servers.length) {
			return {
				tone: "neutral",
				eyebrow: "Start here",
				title: "Add the first MCP integration",
				description:
					"Choose a catalog candidate, import an existing config, or enter a command or URL. MCPace keeps it reviewable before normal use.",
				primary: ["Add integration", "add"],
				secondary: model.clients.length
					? ["Review applications", "applications"]
					: null,
			};
		}
		if (!model.clientConfigured) {
			const targetNote = model.clients.length
				? `${humanCount(model.clients.length, "AI app target")} detected, but none is confirmed as configured.`
				: "No supported AI app target is available yet.";
			return {
				tone: "warn",
				eyebrow: "Client setup",
				title: "Connect an AI application",
				description: `${targetNote} Preview and apply a reversible client patch before treating the route as ready.`,
				primary: ["Open applications", "applications"],
				secondary: ["Open integrations", "integrations"],
			};
		}
		if (model.review) {
			return {
				tone: "warn",
				eyebrow: "Attention needed",
				title: `${humanCount(model.review, "integration")} ${model.review === 1 ? "needs" : "need"} review`,
				description: `${model.ready} working integration${model.ready === 1 ? "" : "s"} remain available while the affected server waits for testing or configuration.`,
				primary: [`Review ${model.review}`, "review"],
				secondary: ["Open integrations", "integrations"],
			};
		}
		if (!model.routingReady) {
			return {
				tone: "warn",
				eyebrow: "Routing review",
				title: "Finish the safe route",
				description:
					"The backend has not marked the complete client-to-tool route as ready. Review current tool evidence and isolation before normal use.",
				primary: ["Open integrations", "integrations"],
				secondary: ["Setup guide", "setup"],
			};
		}
		return {
			tone: "good",
			eyebrow: "Ready",
			title: "Your MCP route is ready to use",
			description: `${humanCount(model.ready, "working integration")} provide ${humanCount(model.tools, "reported tool")} through a configured AI application.`,
			primary: ["Open integrations", "integrations"],
			secondary: ["View activity", "activity"],
		};
	}

	function calmHomeMarkup(model, status) {
		const guide = setupGuideModel(model);
		const decorated = model.servers.map((server) => ({
			server,
			operational: serverOperationalProfile(server),
		}));
		const issues = decorated.filter((item) =>
			["bad", "warn"].includes(item.operational.tone),
		);
		const toneRank = { bad: 0, warn: 1, good: 2, off: 3, neutral: 4 };
		const ordered = [...decorated].sort(
			(left, right) =>
				(toneRank[left.operational.tone] ?? 5) -
					(toneRank[right.operational.tone] ?? 5) ||
				right.server.usage.calls - left.server.usage.calls ||
				left.server.name.localeCompare(right.server.name),
		);
		const serverRows =
			ordered
				.slice(0, 5)
				.map(({ server, operational }) => {
					const action =
						operational.tone === "good"
							? "Open"
							: operational.next?.label ||
								(server.enabled ? "Review" : "Enable");
					const detail =
						operational.tone === "good"
							? `${operational.lifecycle.tools.label}${server.usage.calls ? ` · ${formatNumber(server.usage.calls)} retained calls` : ""}`
							: operational.title;
					return `<button type="button" class="mc-calm-server" data-tone="${operational.tone}" data-mc-open-server="${escapeHtml(server.name)}"><span class="mc-tone-mark" aria-hidden="true">${toneMark(operational.tone)}</span><span><strong>${escapeHtml(server.name)}</strong><small>${escapeHtml(detail)}</small></span><em>${escapeHtml(action)}</em>${ICON.chevron}</button>`;
				})
				.join("") ||
			`<button type="button" class="mc-calm-empty" data-mc-home-action="add">${ICON.plus}<span><strong>Add your first integration</strong><small>Import an existing server or enter a command or URL.</small></span></button>`;

		const recent = model.events.filter((event) => !event.current).slice(0, 4);
		const eventRows =
			recent
				.map(
					(event) =>
						`<button type="button" class="mc-calm-event" data-tone="${event.tone}" data-mc-open-event="${escapeHtml(event.id || "")}"><span class="mc-tone-mark" aria-hidden="true">${toneMark(event.tone)}</span><span><strong>${escapeHtml(event.title)}</strong><small>${escapeHtml(event.meta || event.source)}</small></span><time>${escapeHtml(formatRelativeTimestamp(event.timestamp))}</time></button>`,
				)
				.join("") ||
			`<div class="mc-calm-empty-static">${ICON.activity}<span><strong>No recent calls yet</strong><small>Tool use and errors will appear here after an AI app uses MCPace.</small></span></div>`;

		const liveText = model.liveSessions.length
			? `${humanCount(model.liveSessions.length, "app session")} using ${humanCount(unique(model.liveSessions.flatMap((session) => session.servers)).length, "integration")}`
			: "No app session is holding a server right now";
		const nextStep = guide.steps.find((step) => !step.done);
		const setup =
			guide.finished && state.setupDismissed
				? ""
				: `<section class="mc-calm-setup"><div class="mc-calm-setup-copy"><span>SETUP ${guide.complete}/${guide.total}</span><strong>${escapeHtml(nextStep?.title || "Setup complete")}</strong><small>${escapeHtml(nextStep?.detail || "Your first safe MCP route is ready.")}</small></div><div class="mc-calm-progress" aria-label="${guide.complete} of ${guide.total} setup steps complete"><i style="--progress:${guide.total ? (guide.complete / guide.total) * 100 : 0}%"></i></div><button type="button" class="mc-secondary-button" data-mc-home-action="setup">${guide.finished ? "Review setup" : "Continue"}</button></section>`;

		return `<section class="mc-calm-home" data-tone="${status.tone}">
      <header class="mc-calm-status">
        <div class="mc-calm-status-mark" data-tone="${status.tone}" aria-hidden="true">${toneMark(status.tone)}</div>
        <div class="mc-calm-status-copy"><span>${escapeHtml(status.eyebrow)}</span><h1>${escapeHtml(status.title)}</h1><p>${escapeHtml(status.description)}</p></div>
        <div class="mc-calm-status-actions"><button type="button" class="mc-primary-button" data-mc-home-action="${status.primary[1]}">${escapeHtml(status.primary[0])}</button>${status.secondary ? `<button type="button" class="mc-secondary-button" data-mc-home-action="${status.secondary[1]}">${escapeHtml(status.secondary[0])}</button>` : ""}</div>
      </header>
      <section class="mc-calm-glance" aria-label="MCPace at a glance">
        <button type="button" data-mc-home-action="integrations"><span>Integrations</span><strong>${model.ready}/${model.servers.length || 0}</strong><small>${model.review ? `${model.review} need attention` : "working"}</small></button>
        <button type="button" data-mc-home-action="activity"><span>Tool calls</span><strong>${formatNumber(model.usage.calls)}</strong><small>${escapeHtml(rangeLabel().toLowerCase())}</small></button>
        <button type="button" data-mc-home-action="applications"><span>AI apps</span><strong>${model.clientConfigured ? "1+" : model.clients.length}</strong><small>${model.clientConfigured ? "configured route" : model.clients.length ? `${model.clients.length} target${model.clients.length === 1 ? "" : "s"} found · not configured` : "none configured"}</small></button>
        <button type="button" data-mc-home-action="live"><span>Right now</span><strong>${model.liveSessions.length || "—"}</strong><small>${escapeHtml(liveText)}</small></button>
      </section>
      ${setup}
      <div class="mc-calm-grid">
        <section class="mc-calm-panel"><header><div><span>SERVERS</span><h2>${issues.length ? `${issues.length} need attention` : "Everything is working"}</h2></div><button type="button" class="mc-text-button" data-mc-home-action="integrations">View all</button></header><div class="mc-calm-list">${serverRows}</div></section>
        <section class="mc-calm-panel"><header><div><span>RECENT</span><h2>What happened</h2></div><button type="button" class="mc-text-button" data-mc-home-action="activity">Open activity</button></header><div class="mc-calm-list">${eventRows}</div></section>
      </div>
    </section>`;
	}

	function renderHome() {
		const host = $("[data-mc-home-content]", state.hosts.home);
		if (!host) return;
		const model = metrics();
		const status = homeState(model);
		const firstIssue = model.servers.find((server) =>
			["bad", "warn"].includes(serverOperationalProfile(server).tone),
		);
		const actions = systemActionItems(model);
		const retained = retainedWindow();
		const capabilityProfiles = model.servers.map(serverCapabilityProfile);
		const capabilityMeasured = capabilityProfiles.reduce(
			(sum, profile) => sum + profile.measured,
			0,
		);
		const capabilityTotal = capabilityProfiles.reduce(
			(sum, profile) => sum + profile.total,
			0,
		);
		const remoteServers = model.servers.filter(
			(server) => serverAccessProfile(server).remote,
		).length;
		const recent = model.events.filter((event) => !event.current).slice(0, 5);
		const routeModes = model.servers.reduce(
			(map, server) =>
				map.set(server.routeMode, (map.get(server.routeMode) || 0) + 1),
			new Map(),
		);
		const routeModeRows = [...routeModes.entries()]
			.sort((a, b) => b[1] - a[1])
			.slice(0, 5);

		const clientNodes = model.clients
			.slice(0, 4)
			.map(
				(client) =>
					`<div class="mc-fabric-node" data-tone="${client.tone}"><span>${escapeHtml(client.initials)}</span><div><strong>${escapeHtml(client.name)}</strong><small>${escapeHtml(client.status)}</small></div><i></i></div>`,
			)
			.join("");
		const serverNodes = model.servers
			.slice(0, 6)
			.map((server) => {
				const operational = serverOperationalProfile(server);
				return `<button type="button" class="mc-fabric-node" data-tone="${operational.tone}" data-mc-open-server="${escapeHtml(server.name)}"><span>${escapeHtml(server.initials)}</span><div><strong>${escapeHtml(server.name)}</strong><small>${escapeHtml(operational.title)} · ${escapeHtml(operational.lifecycle.tools.label)}</small></div><i></i></button>`;
			})
			.join("");

		const attentionModels = model.servers
			.map((server) => ({
				server,
				operational: serverOperationalProfile(server),
			}))
			.filter((item) => ["bad", "warn"].includes(item.operational.tone));
		const attention = attentionModels.length
			? attentionModels
					.slice(0, 4)
					.map(
						({ server, operational }) =>
							`<button type="button" class="mc-attention-row" data-mc-open-server="${escapeHtml(server.name)}"><span class="mc-status-orb" data-tone="${operational.tone}"></span><span><strong>${escapeHtml(server.name)}</strong><small>${escapeHtml(operational.next?.reason || operational.detail)}</small></span><em>${escapeHtml(operational.title)}</em>${ICON.chevron}</button>`,
					)
					.join("")
			: `<div class="mc-clear-state">${ICON.check}<div><strong>No integration needs review</strong><span>Every enabled route has current source-matched MCP evidence.</span></div></div>`;

		const activity = recent.length
			? recent
					.map(
						(event) =>
							`<button type="button" class="mc-home-event" data-tone="${event.tone}" data-mc-open-event="${escapeHtml(event.id || "")}"><span class="mc-event-icon">${event.type === "tool" ? ICON.terminal : event.type === "error" ? ICON.warning : ICON.activity}</span><div><strong>${escapeHtml(event.title)}</strong><p>${escapeHtml(event.meta || event.source)}</p></div><em>${escapeHtml(event.chip)}</em></button>`,
					)
					.join("")
			: `<div class="mc-empty-state-small"><strong>No recorded tool activity yet</strong><span>The current backend log window does not contain tool-call audit entries.</span></div>`;

		const distribution = routeModeRows.length
			? routeModeRows
					.map(
						([name, count]) =>
							`<div class="mc-distribution-row"><span>${escapeHtml(name)}</span><div><i style="--value:${clamp((count / Math.max(model.servers.length, 1)) * 100, 6, 100)}%"></i></div><strong>${count}</strong></div>`,
					)
					.join("")
			: '<div class="mc-empty-state-small"><strong>No protection modes yet</strong><span>Add an integration to create a route.</span></div>';
		const actionRows = actions.length
			? actions
					.map(
						(item, index) =>
							`<button type="button" class="mc-system-action" data-tone="${item.tone}" ${item.server ? `data-mc-system-server="${escapeHtml(item.server)}" data-mc-system-tab="${escapeHtml(item.tab || "overview")}"` : `data-mc-system-action="${escapeHtml(item.action || "")}"`}><span>${index + 1}</span><div><strong>${escapeHtml(item.title)}</strong><small>${escapeHtml(item.detail)}</small></div><em>${escapeHtml(item.label || "Open")}</em>${ICON.chevron}</button>`,
					)
					.join("")
			: `<div class="mc-clear-state">${ICON.check}<div><strong>No urgent operator action</strong><span>Runtime, routes, and retained audit evidence are currently readable.</span></div></div>`;
		const liveNow =
			model.liveSessions.length || model.activeLeases.length
				? `<button type="button" class="mc-live-now-strip" data-mc-home-action="live"><span class="mc-live-pulse"><i></i></span><div><small>Live routing ownership</small><strong>${humanCount(model.liveSessions.length, "session")} · ${humanCount(model.activeLeases.length, "lease")}</strong><p>${escapeHtml(
						model.liveSessions
							.slice(0, 3)
							.map(
								(session) =>
									`${session.clientId} → ${session.servers.join(", ") || "route pending"}`,
							)
							.join(" · "),
					)}</p></div><em>Lease state is not proof that a tool is currently executing.</em>${ICON.chevron}</button>`
				: `<button type="button" class="mc-live-now-strip mc-live-now-empty" data-mc-home-action="live"><span class="mc-live-pulse"><i></i></span><div><small>Live routing ownership</small><strong>No active session leases</strong><p>Current clients are not holding a retained route lease.</p></div><em>Open Live now</em>${ICON.chevron}</button>`;
		const chain = [
			[
				"Client targets",
				model.clients.length,
				model.clients.length ? "Configured targets listed" : "No AI app listed",
				model.clients.length ? "good" : "warn",
			],
			[
				"Local broker",
				model.runtime.offline ? "Offline" : "Ready",
				model.runtime.offline
					? "Backend state unavailable"
					: "Control plane responding",
				model.runtime.offline ? "bad" : "good",
			],
			[
				"Integrations",
				model.servers.length,
				`${model.ready} working · ${model.review} review`,
				model.review ? "warn" : model.servers.length ? "good" : "neutral",
			],
			[
				"Capability evidence",
				capabilityTotal ? `${capabilityMeasured}/${capabilityTotal}` : "—",
				"Measured or explicitly reported fields",
				capabilityMeasured ? "good" : "warn",
			],
			[
				"Retained operations",
				retained.returned || 0,
				retained.source === "api/operations"
					? `${retained.files?.filter((file) => file.exists).length || 0} log files · ${retained.truncated ? "limited" : "complete window"}`
					: "Fallback log tail",
				retained.parseErrors
					? "warn"
					: retained.source === "api/operations"
						? "good"
						: "warn",
			],
		]
			.map(
				([label, value, detail, tone], index) =>
					`<div class="mc-chain-step" data-tone="${tone}"><span>${index + 1}</span><div><small>${escapeHtml(label)}</small><strong>${escapeHtml(value)}</strong><em>${escapeHtml(detail)}</em></div>${index < 4 ? '<i aria-hidden="true"></i>' : ""}</div>`,
			)
			.join("");

		setProductHtml(
			host,
			`
      <section class="mc-home-hero" data-tone="${status.tone}">
        <div class="mc-home-hero-copy"><div class="mc-state-label"><span class="mc-status-orb" data-tone="${status.tone}"></span>${escapeHtml(status.eyebrow)}</div><h1>${escapeHtml(status.title)}</h1><p>${escapeHtml(status.description)}</p><div class="mc-hero-actions"><button type="button" class="mc-primary-button" data-mc-home-action="${status.primary[1]}">${escapeHtml(status.primary[0])}</button>${status.secondary ? `<button type="button" class="mc-secondary-button" data-mc-home-action="${status.secondary[1]}">${escapeHtml(status.secondary[0])}</button>` : ""}<button type="button" class="mc-text-button" data-mc-home-action="setup">Setup guide</button></div></div>
        <div class="mc-home-facts" aria-label="Current MCPace facts">
          <div><span>Working</span><strong>${model.ready}</strong><small>enabled routes</small></div>
          <div><span>Review</span><strong>${model.review}</strong><small>need attention</small></div>
          <div><span>Tools</span><strong>${model.tools}</strong><small>reported names</small></div>
          <div><span>Clients</span><strong>${model.clients.length}</strong><small>targets listed</small></div>
        </div>
      </section>
      <button type="button" class="mc-home-usage-strip" data-mc-home-action="activity" aria-label="Open measured usage and activity">
        <span>${ICON.activity}</span><div><small>${escapeHtml(rangeLabel())}</small><strong>${formatNumber(model.usage.calls)} tool calls</strong></div><div><small>Success</small><strong>${successLabel(model.usage.successRate)}</strong></div><div><small>Operation p95</small><strong>${escapeHtml(formatDuration(model.usage.p95))}</strong></div><div><small>Payload</small><strong>${escapeHtml(formatBytes(model.usage.requestBytes + model.usage.responseBytes))}</strong></div><div class="mc-home-token-mini"><small>Tokens</small><strong>${model.usage.reportedTokens ? `${formatNumber(model.usage.reportedTokens)} reported` : state.tokenEstimates === "show" ? `≈ ${formatNumber(model.usage.estimatedTokens)} payload` : "not reported"}</strong></div>${ICON.chevron}
      </button>
      ${liveNow}
      <section class="mc-control-room">
        <header><div><span>System truth</span><h2>Follow a request through the whole MCP chain</h2><p>Each step is backed by current inventory or retained runtime evidence. Unknown capability and authorization state stays unknown.</p></div><div class="mc-control-room-badges"><span>${remoteServers} remote</span><span>${model.servers.length - remoteServers} local</span><span>${model.usage.failures} failed calls</span></div></header>
        <div class="mc-system-chain">${chain}</div>
        <div class="mc-action-center"><header><div><span>Action center</span><h3>${actions.length ? `${actions.length} item${actions.length === 1 ? "" : "s"} to review` : "System is quiet"}</h3></div><button type="button" class="mc-text-button" data-mc-home-action="activity">Open operations</button></header><div>${actionRows}</div></div>
      </section>
      <section class="mc-fabric-section">
        <header><div><span>Configured fabric</span><h2>One local route between clients and tools</h2><p>This map reflects the current server and client inventory. It does not invent request traffic.</p></div><button type="button" class="mc-text-button" data-mc-home-action="integrations">Inspect integrations ${ICON.chevron}</button></header>
        <div class="mc-fabric-canvas">
          <div class="mc-fabric-side"><small>APPLICATION TARGETS</small><div>${clientNodes || '<div class="mc-fabric-empty">No AI app listed</div>'}</div></div>
          <div class="mc-fabric-connector" aria-hidden="true"><i></i><i></i></div>
          <div class="mc-fabric-core" data-tone="${status.tone}"><span>${ICON.logo}</span><strong>MCPace</strong><small>${model.runtime.offline ? "runtime unavailable" : model.activeLocks ? `${model.activeLocks} current route owners` : "local control plane"}</small></div>
          <div class="mc-fabric-connector" aria-hidden="true"><i></i><i></i></div>
          <div class="mc-fabric-side mc-fabric-side-servers"><small>MCP INTEGRATIONS</small><div>${serverNodes || '<button type="button" class="mc-fabric-empty" data-mc-home-action="add">Add the first integration</button>'}</div></div>
        </div>
      </section>
      <div class="mc-home-grid">
        <section class="mc-home-panel"><header><div><span>Next actions</span><h2>${model.review ? "Needs attention" : "Operational queue"}</h2></div><strong class="mc-panel-count">${model.review}</strong></header><div>${attention}</div>${firstIssue ? `<footer><button type="button" class="mc-text-button" data-mc-open-server="${escapeHtml(firstIssue.name)}">Review ${escapeHtml(firstIssue.name)} ${ICON.chevron}</button></footer>` : ""}</section>
        <section class="mc-home-panel"><header><div><span>Actual events</span><h2>Recent activity</h2></div><button type="button" class="mc-text-button" data-mc-home-action="activity">Open all</button></header><div class="mc-home-events">${activity}</div></section>
        <section class="mc-home-panel"><header><div><span>Current configuration</span><h2>Protection distribution</h2></div>${ICON.shield}</header><div class="mc-distribution">${distribution}</div></section>
      </div>`,
		);

		const advancedChildren = [...host.children];
		const advanced = document.createElement("details");
		advanced.className = "mc-home-advanced";
		advanced.open = state.detailLevel === "full";
		setProductHtml(
			advanced,
			`<summary><span>${ICON.terminal}</span><span><strong>System details</strong><small>Topology, retained evidence, protection distribution, and operator diagnostics</small></span>${ICON.chevron}</summary><div class="mc-home-advanced-body"></div>`,
		);
		const advancedBody = $(".mc-home-advanced-body", advanced);
		advancedChildren.forEach((child) => advancedBody.appendChild(child));
		host.prepend(advanced);
		prependProductHtml(host, calmHomeMarkup(model, status));

		$$("[data-mc-home-action]", host).forEach((button) =>
			button.addEventListener("click", () =>
				handleHomeAction(button.dataset.mcHomeAction),
			),
		);
		$$("[data-mc-open-server]", host).forEach((button) =>
			button.addEventListener("click", () =>
				openServer(button.dataset.mcOpenServer, "overview"),
			),
		);
		$$("[data-mc-open-event]", host).forEach((button) =>
			button.addEventListener("click", () =>
				openEventDetail(button.dataset.mcOpenEvent),
			),
		);
		$$("[data-mc-system-server]", host).forEach((button) =>
			button.addEventListener("click", () =>
				openServer(
					button.dataset.mcSystemServer,
					button.dataset.mcSystemTab || "overview",
				),
			),
		);
		$$("[data-mc-system-action]", host).forEach((button) =>
			button.addEventListener("click", () =>
				handleSystemAction(button.dataset.mcSystemAction),
			),
		);
	}

	function handleSystemAction(action) {
		if (action === "refresh") refreshRuntime();
		else if (action === "activity-errors") {
			state.activityView = "events";
			state.activityFilter = "error";
			switchView("activity");
			renderActivity();
		} else if (action === "observability") {
			switchView("settings");
			setSettingsTab("observability");
		} else if (action) switchView(action);
	}

	function handleHomeAction(action) {
		if (action === "add") openAddDialog();
		else if (action === "setup") openSetupGuide();
		else if (action === "live") {
			state.activityView = "live";
			writePreference("activityView", "live");
			switchView("activity");
			renderActivity();
		} else if (action === "refresh") refreshRuntime();
		else if (action === "review") {
			state.integrationFilter = "attention";
			switchView("integrations");
		} else switchView(action);
	}

	function compactContextValue(value, fallback = "") {
		const raw = String(value || "").trim();
		if (!raw) return fallback;
		const normalized = raw.replace(/\\/g, "/");
		if (normalized.length <= 34) return normalized;
		const parts = normalized.split("/").filter(Boolean);
		return parts.length > 2
			? `…/${parts.slice(-2).join("/")}`
			: `…${normalized.slice(-31)}`;
	}

	function projectDisplayName(value) {
		const raw = String(value || "").trim();
		if (!raw) return "No project retained";
		const normalized = raw.replace(/\\/g, "/").replace(/\/$/, "");
		return normalized.split("/").filter(Boolean).at(-1) || raw;
	}

	function sourceDisplayName(server) {
		if (server.sourceType === "http") {
			try {
				const url = new URL(server.sourceUrl || server.sourceLocation || "");
				return `${url.hostname}${url.port ? `:${url.port}` : ""}`;
			} catch (_) {
				return compactContextValue(
					server.sourceUrl || server.sourceLocation,
					"Remote endpoint",
				);
			}
		}
		if (server.sourcePath)
			return compactContextValue(server.sourcePath, "Config file");
		if (server.sourceCommand)
			return compactContextValue(server.sourceCommand, "Local command");
		return compactContextValue(
			server.sourceLocation || server.sourceMeta,
			"Source not returned",
		);
	}

	function serverContextProfile(server) {
		const records = auditRecords().filter(
			(record) => record.server === server.name,
		);
		const sessions = server.liveSessions || [];
		const leases = server.activeLeases || [];
		const latestRecord = records[0] || null;
		const latestSession = sessions[0] || null;
		const latestLease = leases[0] || null;
		const clients = unique([
			...records.map((record) => record.clientId),
			...sessions.map((session) => session.clientId),
			...leases.map((lease) => lease.clientId),
		]);
		const projects = unique([
			...records.map((record) => record.projectRoot),
			...sessions.map((session) => session.projectRoot),
			...leases.map((lease) => lease.projectRoot),
		]);
		const latestToolName = latestRecord?.tools?.[0] || "";
		const definition = latestToolName
			? cachedToolDefinitionByName(latestToolName, server.name)
			: null;
		const latestOperation = definition
			? toolDisplayName(definition)
			: latestToolName;
		const lastTimestamp =
			Math.max(
				latestRecord?.timestamp || 0,
				latestSession?.lastSeenAtMs || 0,
				latestLease?.renewedAtMs || latestLease?.acquiredAtMs || 0,
				server.usage?.lastTimestamp || 0,
			) || null;
		return {
			records,
			sessions,
			leases,
			clients,
			projects,
			live: sessions.length > 0 || leases.length > 0,
			primaryClient:
				latestRecord?.clientId ||
				latestSession?.clientId ||
				latestLease?.clientId ||
				clients[0] ||
				"",
			primaryProject:
				latestRecord?.projectRoot ||
				latestSession?.projectRoot ||
				latestLease?.projectRoot ||
				projects[0] ||
				"",
			lastTimestamp,
			activeLeaseCount:
				leases.length ||
				sessions.reduce(
					(sum, session) => sum + finiteNumber(session.activeLeaseCount, 0),
					0,
				),
			sourceShort: sourceDisplayName(server),
			latestOperation,
			latestOperationTechnical: latestToolName,
			latestOperationOk: latestRecord ? latestRecord.ok : null,
			latestOperationAt: latestRecord?.timestamp || null,
		};
	}

	function serverCapacityProfile(server) {
		const workers = finiteNumber(server.maxWorkers, null);
		const inFlight = finiteNumber(server.maxInFlightPerWorker, null);
		if (workers !== null && inFlight !== null) {
			const total = Math.max(0, workers * inFlight);
			return {
				total,
				label:
					total === 1
						? "One request at a time"
						: `Up to ${formatNumber(total)} concurrent`,
				detail: `${workers} worker${workers === 1 ? "" : "s"} × ${inFlight} request${inFlight === 1 ? "" : "s"} each`,
			};
		}
		if (workers !== null)
			return {
				total: null,
				label: `${workers} worker${workers === 1 ? "" : "s"}`,
				detail: "Per-worker in-flight limit not returned",
			};
		if (inFlight !== null)
			return {
				total: null,
				label: `${inFlight} in flight per worker`,
				detail: "Worker count not returned",
			};
		return {
			total: null,
			label: "Capacity automatic",
			detail: "MCPace did not return an explicit maximum",
		};
	}

	function serverCapacityLabel(server) {
		return serverCapacityProfile(server).label;
	}

	function serverCapacityShort(server) {
		const capacity = serverCapacityProfile(server);
		if (capacity.total !== null) return `max ${formatNumber(capacity.total)}`;
		if (/^\d+ worker/.test(capacity.label))
			return capacity.label.replace(/ workers?/, "w");
		if (/in flight/.test(capacity.label))
			return capacity.label.replace(" in flight per worker", "/worker");
		return "auto";
	}

	function serverRouteExplanation(server) {
		const context = serverContextProfile(server);
		const capacity = serverCapacityProfile(server);
		const ownership = context.activeLeaseCount
			? `${context.activeLeaseCount} route lease${context.activeLeaseCount === 1 ? "" : "s"} held; ownership may be idle`
			: server.activeInstances.length
				? `${server.activeInstances.length} runtime instance${server.activeInstances.length === 1 ? "" : "s"} observed`
				: "No route ownership now";
		return {
			title: server.routeMode || "Automatic isolation",
			capacity,
			ownership,
		};
	}

	function serverRuntimeProfile(server) {
		const runtime = overviewData().runtime || {};
		const pool = runtime.upstreamSessionPool || {};
		const monitor = runtime.serverResourceMonitoring || {};
		const sessions = (Array.isArray(pool.sessions) ? pool.sessions : []).filter(
			(item) => String(item?.server || "") === server.name,
		);
		const resource =
			(Array.isArray(monitor.items) ? monitor.items : []).find(
				(item) => String(item?.server || "") === server.name,
			) || {};
		const pids = unique([
			...sessions.map((item) =>
				item?.pid === undefined || item?.pid === null ? "" : String(item.pid),
			),
			...(Array.isArray(resource.pids) ? resource.pids.map(String) : []),
		]);
		const rssBytes = Math.max(
			0,
			finiteNumber(
				resource.rssBytes,
				sessions.reduce(
					(sum, item) =>
						sum + Math.max(0, finiteNumber(item?.resource?.rssBytes, 0)),
					0,
				),
			),
		);
		const virtualMemoryBytes = Math.max(
			0,
			finiteNumber(
				resource.virtualMemoryBytes,
				sessions.reduce(
					(sum, item) =>
						sum +
						Math.max(0, finiteNumber(item?.resource?.virtualMemoryBytes, 0)),
					0,
				),
			),
		);
		const fdCount = Math.max(
			0,
			finiteNumber(
				resource.fdCount,
				sessions.reduce(
					(sum, item) =>
						sum + Math.max(0, finiteNumber(item?.resource?.fdCount, 0)),
					0,
				),
			),
		);
		const threads = Math.max(
			0,
			finiteNumber(
				resource.threads,
				sessions.reduce(
					(sum, item) =>
						sum + Math.max(0, finiteNumber(item?.resource?.threads, 0)),
					0,
				),
			),
		);
		const callCount = Math.max(
			0,
			finiteNumber(
				resource.callCount,
				sessions.reduce(
					(sum, item) => sum + Math.max(0, finiteNumber(item?.callCount, 0)),
					0,
				),
			),
		);
		const ages = sessions
			.map((item) => finiteNumber(item?.ageMs, null))
			.filter((value) => value !== null);
		const idles = sessions
			.map((item) => finiteNumber(item?.idleMs, null))
			.filter((value) => value !== null);
		const oldestAgeMs = ages.length ? Math.max(...ages) : null;
		const shortestIdleMs = idles.length ? Math.min(...idles) : null;
		const routeOwners =
			server.activeLeases.length ||
			server.liveSessions.reduce(
				(sum, item) =>
					sum + Math.max(0, finiteNumber(item.activeLeaseCount, 0)),
				0,
			);
		const processObserved = sessions.length > 0 || pids.length > 0;
		const runtimeInstanceObserved = server.activeInstances.length > 0;
		const current =
			processObserved || runtimeInstanceObserved || routeOwners > 0;
		const location = processObserved
			? `${sessions.length || finiteNumber(resource.sessions, 0)} pooled process session${(sessions.length || finiteNumber(resource.sessions, 0)) === 1 ? "" : "s"}`
			: routeOwners
				? `${routeOwners} route owner${routeOwners === 1 ? "" : "s"}; ownership may be idle`
				: runtimeInstanceObserved
					? `${server.activeInstances.length} runtime instance${server.activeInstances.length === 1 ? "" : "s"} reported`
					: "No live runtime evidence";
		return {
			sessions,
			resource,
			pids,
			rssBytes,
			virtualMemoryBytes,
			fdCount,
			threads,
			callCount,
			oldestAgeMs,
			shortestIdleMs,
			routeOwners,
			processObserved,
			runtimeInstanceObserved,
			current,
			location,
		};
	}

	function probeFailureText(value) {
		const source = String(value || "").toLowerCase();
		return (
			/(?:test|probe|initialize|handshake|protocol|tools\s*\/\s*list|tool discovery).{0,80}(?:failed|failure|error|timed out|timeout|rejected|unsupported)/.test(
				source,
			) ||
			/(?:failed|failure|error|timed out|timeout|rejected|unsupported).{0,80}(?:test|probe|initialize|handshake|protocol|tools\s*\/\s*list|tool discovery)/.test(
				source,
			)
		);
	}

	function serverProbeProfile(server) {
		const cache = cachedToolEntry(server.name);
		const cacheStatus = String(cache.status || "").toLowerCase();
		const cacheMiss =
			cacheStatus === "cache-miss" ||
			cacheStatus === "not-checked" ||
			/no cached tools|cache miss/.test(
				String(cache.error || "").toLowerCase(),
			);
		const sourceCallable =
			cache.runtimeCallable !== false &&
			server.backend?.platformSupported !== false &&
			server.backend?.sourceEnabled !== false &&
			server.lane !== "blocked";
		const explicitFailure =
			(Object.hasOwn(cache, "ok") && cache.ok === false && !cacheMiss) ||
			probeFailureText(`${server.evidenceTitle} ${server.evidenceBody}`);
		const toolsEnvelopeMeasured =
			cache.ok === true ||
			cacheStatus === "cached-tools" ||
			cacheStatus === "listed-tools" ||
			server.toolDefinitions.length > 0 ||
			server.toolCount > 0 ||
			Object.hasOwn(server.backend || {}, "toolCount");
		const protocolVersion = String(
			cache.protocolVersion || server.backend?.protocolVersion || "",
		);
		const verified =
			!explicitFailure && (toolsEnvelopeMeasured || Boolean(protocolVersion));
		if (!sourceCallable) {
			return {
				state: "blocked",
				tone: "bad",
				label: "Source blocked",
				short: "MCP",
				verified: false,
				failed: false,
				detail:
					server.evidenceBody ||
					"The current source cannot be launched or reached on this platform.",
				tab: "source",
				cache,
				cacheMiss,
				protocolVersion,
			};
		}
		if (explicitFailure) {
			return {
				state: "failed",
				tone: "bad",
				label: "MCP test failed",
				short: "MCP",
				verified: false,
				failed: true,
				detail:
					server.evidenceBody ||
					String(
						cache.error || "The latest initialize or tools/list check failed.",
					),
				tab: "events",
				cache,
				cacheMiss,
				protocolVersion,
			};
		}
		if (verified) {
			return {
				state: "complete",
				tone: "good",
				label: "MCP verified",
				short: "MCP",
				verified: true,
				failed: false,
				detail: protocolVersion
					? `Protocol ${protocolVersion} evidence is retained.`
					: "Initialize or tools/list evidence is retained.",
				tab: "capabilities",
				cache,
				cacheMiss,
				protocolVersion,
			};
		}
		return {
			state: "unknown",
			tone: server.enabled ? "warn" : "off",
			label: "MCP unknown",
			short: "MCP",
			verified: false,
			failed: false,
			detail: cacheMiss
				? "No evidence matches the current source definition. Run Test."
				: "No retained protocol verification.",
			tab: "capabilities",
			cache,
			cacheMiss,
			protocolVersion,
		};
	}

	function serverToolsEvidenceProfile(
		server,
		probe = serverProbeProfile(server),
	) {
		const cache = probe.cache || cachedToolEntry(server.name);
		const cacheStatus = String(cache.status || "").toLowerCase();
		const measured =
			probe.verified &&
			(cache.ok === true ||
				cacheStatus === "cached-tools" ||
				cacheStatus === "listed-tools" ||
				server.toolDefinitions.length > 0 ||
				server.toolCount > 0 ||
				Object.hasOwn(server.backend || {}, "toolCount"));
		if (measured) {
			const count = Math.max(0, finiteNumber(server.toolCount, 0));
			return {
				measured: true,
				count,
				state: "complete",
				tone: count ? "good" : "neutral",
				label: count
					? `${count} tool${count === 1 ? "" : "s"}`
					: "0 tools reported",
				detail: count
					? "A tools/list result is retained for the current source definition; this is not a trust or authorization decision."
					: "tools/list completed and returned no tools; this server may expose other MCP capabilities, and the result is not a trust or authorization decision.",
				tab: "tools",
			};
		}
		if (probe.failed)
			return {
				measured: false,
				count: 0,
				state: "failed",
				tone: "bad",
				label: "Tools unavailable",
				detail: "Tool discovery did not complete successfully.",
				tab: "tools",
			};
		return {
			measured: false,
			count: 0,
			state: "unknown",
			tone: server.enabled ? "warn" : "off",
			label: "Tools unknown",
			detail:
				"No retained tools/list result for the current source definition.",
			tab: "tools",
		};
	}

	function serverEvidenceFreshness(server) {
		const cache = cachedToolEntry(server.name);
		const timestamps = [
			cache.checkedAtMs,
			cache.updatedAtMs,
			cache.refreshedAtMs,
			cache.testedAtMs,
			cache.lastCheckedAtMs,
			cache.timestampMs,
			server.backend?.lastTestedAtMs,
			server.backend?.lastCheckedAtMs,
		]
			.map((value) => finiteNumber(value, null))
			.filter((value) => value !== null);
		const timestamp = timestamps.length ? Math.max(...timestamps) : null;
		if (timestamp !== null) {
			const age = Math.max(0, Date.now() - timestamp);
			if (age <= 15 * 60 * 1000)
				return {
					timestamp,
					tone: "good",
					label: "Evidence recent",
					detail: formatRelativeTimestamp(timestamp),
					source: "live",
				};
			if (age <= 24 * 60 * 60 * 1000)
				return {
					timestamp,
					tone: "neutral",
					label: "Evidence today",
					detail: formatRelativeTimestamp(timestamp),
					source: "measured",
				};
			if (age <= 7 * 24 * 60 * 60 * 1000)
				return {
					timestamp,
					tone: "warn",
					label: "Evidence aging",
					detail: formatRelativeTimestamp(timestamp),
					source: "measured",
				};
			return {
				timestamp,
				tone: "warn",
				label: "Evidence stale",
				detail: formatRelativeTimestamp(timestamp),
				source: "measured",
			};
		}
		if (
			cache.cacheHit === true ||
			String(cache.status || "").toLowerCase() === "cached-tools"
		) {
			const ttl = finiteNumber(cache.cacheTtlMs, null);
			return {
				timestamp: null,
				tone: "neutral",
				label: "Evidence time unknown",
				detail: `Matched the current source definition${ttl !== null ? ` · cache window ${formatDuration(ttl)}` : ""}; exact collection time was not returned.`,
				source: "cached",
			};
		}
		if (serverToolsEvidenceProfile(server).measured) {
			return {
				timestamp: null,
				tone: "neutral",
				label: "Evidence time unknown",
				detail:
					"A source-matched tools/list result is retained, but its collection time was not returned.",
				source: "retained",
			};
		}
		return {
			timestamp: null,
			tone: "warn",
			label: "Not tested",
			detail:
				"No evidence matches the current source. Run Test after first setup or after changing the definition.",
			source: "missing",
		};
	}

	function serverLifecycleProfile(server) {
		const context = serverContextProfile(server);
		const runtime = serverRuntimeProfile(server);
		const probe = serverProbeProfile(server);
		const tools = serverToolsEvidenceProfile(server, probe);
		const runtimeNow = runtime.current;
		const runtimeSeen =
			runtimeNow || context.records.length > 0 || tools.measured;
		const steps = [
			{
				id: "enabled",
				label: server.enabled ? "Enabled" : "Off",
				short: "On",
				tone: server.enabled ? "good" : "off",
				state: server.enabled ? "complete" : "off",
				detail: server.enabled
					? "Definition is exposed to MCPace."
					: "Definition is saved but not exposed.",
				tab: "source",
			},
			{
				id: "runtime",
				label: runtimeNow
					? runtime.processObserved
						? "Process observed"
						: "Route held"
					: runtimeSeen
						? "Seen before"
						: "On demand",
				short: "Runtime",
				tone: runtimeNow
					? "good"
					: runtimeSeen
						? "neutral"
						: server.enabled
							? "neutral"
							: "off",
				state: runtimeNow ? "current" : runtimeSeen ? "historical" : "idle",
				detail: runtimeNow
					? `${runtime.location}. This does not prove a tool is executing now.`
					: runtimeSeen
						? "Historical runtime or operation evidence exists, but no live process or route owner is visible now."
						: "No live process is expected until an on-demand route needs the server.",
				tab: "usage",
			},
			{
				id: "protocol",
				label: probe.label,
				short: "MCP",
				tone: probe.tone,
				state: probe.state,
				detail: probe.detail,
				tab: probe.tab,
			},
			{
				id: "tools",
				label: tools.label,
				short: "Tools",
				tone: tools.tone,
				state: tools.state,
				detail: tools.detail,
				tab: tools.tab,
			},
		];
		return {
			steps,
			complete: steps.filter((step) =>
				["complete", "current", "historical", "idle"].includes(step.state),
			).length,
			total: steps.length,
			runtimeNow,
			runtimeSeen,
			protocolMeasured: probe.verified,
			failed: probe.failed,
			sourceBlocked: probe.state === "blocked",
			probe,
			tools,
			runtime,
		};
	}

	function normalizedSourceIdentity(server) {
		if (server.sourceType === "http") {
			try {
				const url = new URL(server.sourceUrl || server.sourceLocation || "");
				url.hash = "";
				if (url.pathname.length > 1)
					url.pathname = url.pathname.replace(/\/+$/, "");
				return `http:${url.toString().toLowerCase()}`;
			} catch (_) {
				return server.sourceUrl || server.sourceLocation
					? `http:${String(server.sourceUrl || server.sourceLocation)
							.trim()
							.toLowerCase()}`
					: "";
			}
		}
		const command = [server.sourceCommand, ...(server.sourceArgs || [])]
			.filter(Boolean)
			.join(" ")
			.replace(/\s+/g, " ")
			.trim();
		return command ? `stdio:${command}` : "";
	}

	function serverConflictProfile(server, models = serverModels()) {
		const sourceIdentity = normalizedSourceIdentity(server);
		const duplicateSources = sourceIdentity
			? models.filter(
					(candidate) =>
						candidate.name !== server.name &&
						normalizedSourceIdentity(candidate) === sourceIdentity,
				)
			: [];
		const names = new Set(
			(server.toolDefinitions.length
				? server.toolDefinitions.map(toolTechnicalName)
				: server.tools
			).filter(Boolean),
		);
		const collisions = new Map();
		if (names.size) {
			models
				.filter(
					(candidate) => candidate.name !== server.name && candidate.enabled,
				)
				.forEach((candidate) => {
					const candidateNames = new Set(
						(candidate.toolDefinitions.length
							? candidate.toolDefinitions.map(toolTechnicalName)
							: candidate.tools
						).filter(Boolean),
					);
					[...names]
						.filter((name) => candidateNames.has(name))
						.forEach((name) => {
							if (!collisions.has(name)) collisions.set(name, []);
							collisions.get(name).push(candidate.name);
						});
				});
		}
		return {
			duplicateSources,
			toolCollisions: [...collisions.entries()].map(([tool, servers]) => ({
				tool,
				servers: unique(servers),
			})),
			hasDuplicateSource: duplicateSources.length > 0,
			hasToolCollisions: collisions.size > 0,
		};
	}

	function serverLastOperationProfile(server) {
		const context = serverContextProfile(server);
		const record = context.records[0] || null;
		if (!record)
			return {
				known: false,
				failed: false,
				record: null,
				tone: "neutral",
				title: "No retained operation",
				detail: "No completed tool call is retained for this server.",
			};
		const failed =
			record.ok === false ||
			record.outcome === "tool_error" ||
			record.outcome === "bridge_error";
		const errorKind = String(record.errorKind || "").toLowerCase();
		const stage = String(record.failureStage || "").toLowerCase();
		const tool = context.latestOperation || record.tools?.[0] || "Tool call";
		return {
			known: true,
			failed,
			record,
			errorKind,
			stage,
			tone: failed ? "warn" : "good",
			title: failed
				? `Last call failed · ${tool}`
				: `Last call succeeded · ${tool}`,
			detail: `${record.timestamp ? formatRelativeTimestamp(record.timestamp) : "time not retained"}${failed ? ` · ${errorKind || "unclassified error"}${stage ? ` at ${stage}` : ""}` : ""}`,
		};
	}

	function serverNextActionProfile(server) {
		const lifecycle = serverLifecycleProfile(server);
		const last = serverLastOperationProfile(server);
		const conflicts = serverConflictProfile(server);
		if (!server.enabled)
			return {
				urgent: true,
				tone: "off",
				label: "Enable and test",
				reason: "The definition is saved but not exposed to applications.",
				kind: "server",
				action: "enable-test",
				tab: "source",
			};
		if (lifecycle.sourceBlocked)
			return {
				urgent: true,
				tone: "bad",
				label: "Fix setup",
				reason: lifecycle.probe.detail,
				kind: "tab",
				action: "source",
				tab: "source",
			};
		if (lifecycle.probe.failed)
			return {
				urgent: true,
				tone: "bad",
				label: "Retry MCP test",
				reason: lifecycle.probe.detail,
				kind: "server",
				action: "test",
				tab: "events",
			};
		if (!lifecycle.protocolMeasured || !lifecycle.tools.measured)
			return {
				urgent: true,
				tone: "warn",
				label: "Test MCP and tools",
				reason: "No tools/list evidence matches the current source definition.",
				kind: "server",
				action: "test",
				tab: "tools",
			};
		if (last.failed) {
			if (last.errorKind === "authorization")
				return {
					urgent: true,
					tone: "warn",
					label: "Review credentials",
					reason:
						"The MCP route is verified, but the latest tool call failed authorization.",
					kind: "tab",
					action: "access",
					tab: "access",
				};
			if (last.errorKind === "policy_denied")
				return {
					urgent: true,
					tone: "warn",
					label: "Review isolation",
					reason: "The server is verified, but policy denied the latest call.",
					kind: "tab",
					action: "routing",
					tab: "routing",
				};
			if (
				["timeout", "capacity"].includes(last.errorKind) ||
				last.stage === "queue"
			)
				return {
					urgent: true,
					tone: "warn",
					label: "Review queue and capacity",
					reason:
						"The server is verified, but the latest call waited too long or hit a capacity boundary.",
					kind: "tab",
					action: "routing",
					tab: "routing",
				};
			if (last.errorKind === "transport")
				return {
					urgent: true,
					tone: "warn",
					label: "Check connection",
					reason:
						"The server passed earlier discovery, but the latest call failed at the transport boundary.",
					kind: "tab",
					action: "source",
					tab: "source",
				};
			return {
				urgent: true,
				tone: "warn",
				label: "Inspect last failure",
				reason:
					"Protocol and tools are verified; only the latest operation failed.",
				kind: "tab",
				action: "events",
				tab: "events",
			};
		}
		if (conflicts.hasDuplicateSource)
			return {
				urgent: false,
				tone: "neutral",
				label: "Review duplicate source",
				reason: `The same command or endpoint is also configured as ${conflicts.duplicateSources.map((item) => item.name).join(", ")}. Aliases may be intentional.`,
				kind: "tab",
				action: "source",
				tab: "source",
			};
		if (server.lane === "blocked")
			return {
				urgent: true,
				tone: "bad",
				label: "Review backend block",
				reason:
					server.evidenceBody ||
					"The backend marked this route blocked after source and policy evaluation.",
				kind: "tab",
				action: "events",
				tab: "events",
			};
		if (
			server.lane === "guarded" ||
			serverAccessProfile(server).approvalRequired
		)
			return {
				urgent: false,
				tone: "warn",
				label: "Review protection",
				reason:
					server.evidenceBody ||
					"The backend requires an access or policy review before broad use.",
				kind: "tab",
				action: "routing",
				tab: "routing",
			};
		if (conflicts.hasToolCollisions)
			return {
				urgent: false,
				tone: "neutral",
				label: "Review tool-name collisions",
				reason: `${conflicts.toolCollisions.length} technical tool name${conflicts.toolCollisions.length === 1 ? "" : "s"} also appear on other enabled servers.`,
				kind: "tab",
				action: "tools",
				tab: "tools",
			};
		return {
			urgent: false,
			tone: "good",
			label: "No urgent action",
			reason:
				"The definition, MCP evidence, and tool discovery are ready. Runtime may start only when needed.",
			kind: "tab",
			action: "overview",
			tab: "overview",
		};
	}

	function serverOperationalProfile(server) {
		const lifecycle = serverLifecycleProfile(server);
		const last = serverLastOperationProfile(server);
		const next = serverNextActionProfile(server);
		const progress = lifecycleProgress(server);
		if (!server.enabled)
			return {
				title: "Saved, not exposed",
				detail: "Enable when an AI workflow needs this server.",
				tone: "off",
				progress,
				lifecycle,
				last,
				next,
			};
		if (lifecycle.sourceBlocked)
			return {
				title: "Source cannot run",
				detail: lifecycle.probe.detail,
				tone: "bad",
				progress,
				lifecycle,
				last,
				next,
			};
		if (lifecycle.probe.failed)
			return {
				title: "MCP test failed",
				detail: lifecycle.probe.detail,
				tone: "bad",
				progress,
				lifecycle,
				last,
				next,
			};
		if (!lifecycle.protocolMeasured)
			return {
				title: "MCP not verified",
				detail: "Run Test before relying on this source.",
				tone: "warn",
				progress,
				lifecycle,
				last,
				next,
			};
		if (!lifecycle.tools.measured)
			return {
				title: "Tool discovery not measured",
				detail: "The current source has no retained tools/list result.",
				tone: "warn",
				progress,
				lifecycle,
				last,
				next,
			};
		if (last.failed)
			return {
				title: "MCP ready · last call failed",
				detail: `${last.title}. The server remains protocol-ready; inspect the operation separately.`,
				tone: "warn",
				progress,
				lifecycle,
				last,
				next,
			};
		if (server.lane === "blocked")
			return {
				title: "MCP verified · backend blocked",
				detail:
					server.evidenceBody ||
					"A structured backend policy or source condition blocks normal use.",
				tone: "bad",
				progress,
				lifecycle,
				last,
				next,
			};
		if (
			server.lane === "guarded" ||
			serverAccessProfile(server).approvalRequired
		)
			return {
				title: "MCP ready · review recommended",
				detail:
					server.evidenceBody || "Review access or isolation before broad use.",
				tone: "warn",
				progress,
				lifecycle,
				last,
				next,
			};
		const runtimeNote = lifecycle.runtimeNow
			? lifecycle.runtime.location
			: "No process or route owner is held now; the server can start on demand.";
		const toolsNote = lifecycle.tools.count
			? `${lifecycle.tools.count} tool${lifecycle.tools.count === 1 ? "" : "s"} available.`
			: "tools/list completed with zero tools.";
		return {
			title: lifecycle.runtimeNow
				? "Ready · runtime observed"
				: "Ready on demand",
			detail: `${toolsNote} ${runtimeNote}`,
			tone: "good",
			progress,
			lifecycle,
			last,
			next,
		};
	}

	function lifecycleMarkup(server, { compact = false } = {}) {
		const lifecycle = serverLifecycleProfile(server);
		return lifecycle.steps
			.map(
				(step) =>
					`<button type="button" class="mc-life-step" data-tone="${step.tone}" data-state="${step.state}" data-mc-life-tab="${step.tab}" title="${escapeHtml(`${step.label}. ${step.detail}`)}" aria-label="${escapeHtml(`${step.short}: ${step.label}. ${step.detail}`)}"><i aria-hidden="true">${toneMark(step.tone)}</i><span><small>${escapeHtml(step.short)}</small>${compact ? "" : `<strong>${escapeHtml(step.label)}</strong>`}</span></button>`,
			)
			.join("");
	}

	function lifecycleProgress(server) {
		const lifecycle = serverLifecycleProfile(server);
		return `${lifecycle.complete}/${lifecycle.total}`;
	}

	function serverReadinessHeadline(server) {
		const profile = serverOperationalProfile(server);
		return {
			title: profile.title,
			detail: profile.detail,
			tone: profile.tone,
			progress: profile.progress,
		};
	}

	function contextHeadline(context) {
		const labelsVisible = state.contextLabels === "show";
		const client = labelsVisible
			? context.primaryClient || "Client not retained"
			: context.primaryClient
				? "Client hidden locally"
				: "Client not retained";
		const project = labelsVisible
			? projectDisplayName(context.primaryProject)
			: context.primaryProject
				? "Project hidden locally"
				: "No project retained";
		if (context.live) {
			const detail = [
				context.activeLeaseCount
					? `${context.activeLeaseCount} lease${context.activeLeaseCount === 1 ? "" : "s"}`
					: "",
				context.latestOperation ? `last ${context.latestOperation}` : "",
				context.lastTimestamp
					? formatRelativeTimestamp(context.lastTimestamp)
					: "",
			]
				.filter(Boolean)
				.join(" · ");
			return {
				title: `${client} · ${project}`,
				detail: `${detail || "Current route ownership"}; ownership may be idle`,
				tone: context.latestOperationOk === false ? "warn" : "good",
			};
		}
		if (context.records.length)
			return {
				title: `${client} · ${project}`,
				detail: `${context.latestOperation ? `Last ${context.latestOperation} · ` : ""}${formatRelativeTimestamp(context.lastTimestamp)}`,
				tone: context.latestOperationOk === false ? "warn" : "neutral",
			};
		return {
			title: "No client use retained",
			detail: "No calls in retained operation history",
			tone: "off",
		};
	}

	function serverPeekMarkup(server) {
		const context = serverContextProfile(server);
		const contextInfo = contextHeadline(context);
		const operational = serverOperationalProfile(server);
		const runtime = operational.lifecycle.runtime;
		const route = serverRouteExplanation(server);
		const access = serverAccessProfile(server);
		const freshness = serverEvidenceFreshness(server);
		const conflict = serverConflictProfile(server);
		const processDetail = runtime.processObserved
			? `${runtime.pids.length ? `PID ${runtime.pids.join(", ")}` : `${runtime.sessions.length} pooled session${runtime.sessions.length === 1 ? "" : "s"}`}${runtime.rssBytes ? ` · ${formatBytes(runtime.rssBytes)} RSS` : ""}${runtime.shortestIdleMs !== null ? ` · idle ${formatDuration(runtime.shortestIdleMs)}` : ""}`
			: `${route.ownership} · ${route.capacity.label}`;
		const conflictDetail = conflict.hasDuplicateSource
			? `Possible source alias: ${conflict.duplicateSources.map((item) => item.name).join(", ")}`
			: conflict.hasToolCollisions
				? `${conflict.toolCollisions.length} tool name collision${conflict.toolCollisions.length === 1 ? "" : "s"} across enabled servers`
				: "No duplicate command or endpoint detected";
		const next = operational.next;
		return `<div class="mc-peek-section" data-tone="${operational.tone}"><span>Readiness</span><strong>${escapeHtml(operational.title)}</strong><small>${escapeHtml(freshness.label)} · ${escapeHtml(freshness.detail)}</small><button type="button" data-mc-peek-tab="${escapeHtml(operational.lifecycle.probe.tab)}">Open evidence</button></div><div class="mc-peek-section"><span>Who & where</span><strong>${escapeHtml(contextInfo.title)}</strong><small>${escapeHtml(contextInfo.detail)} · ${escapeHtml(context.sourceShort)}</small><button type="button" data-mc-peek-tab="usage">Open activity</button></div><div class="mc-peek-section"><span>Runtime & capacity</span><strong>${escapeHtml(runtime.location)}</strong><small>${escapeHtml(processDetail)}</small><button type="button" data-mc-peek-tab="routing">Open isolation</button></div><div class="mc-peek-section" data-tone="${next.tone}"><span>Next safe action</span><strong>${escapeHtml(next.label)}</strong><small>${escapeHtml(next.reason)}${conflictDetail ? ` · ${escapeHtml(conflictDetail)}` : ""}</small><button type="button" data-mc-peek-action="${escapeHtml(next.action)}" data-mc-peek-kind="${escapeHtml(next.kind)}" data-mc-peek-tab="${escapeHtml(next.tab)}">${escapeHtml(next.label)}</button></div>`;
	}

	function syncContextSelect(
		select,
		values,
		selected,
		allLabel,
		formatter = (value) => value,
	) {
		if (!select) return;
		const uniqueValues = unique(values).sort((a, b) =>
			String(a).localeCompare(String(b)),
		);
		const signature = JSON.stringify([allLabel, uniqueValues]);
		if (select.dataset.mcSignature !== signature) {
			select.dataset.mcSignature = signature;
			setProductHtml(
				select,
				`<option value="all">${escapeHtml(allLabel)}</option>${uniqueValues.map((value) => `<option value="${escapeHtml(value)}">${escapeHtml(formatter(value))}</option>`).join("")}`,
			);
		}
		select.value = uniqueValues.includes(selected) ? selected : "all";
	}

	function serverGroupProfile(server) {
		const context = serverContextProfile(server);
		if (state.integrationGroup === "status") {
			const operational = serverOperationalProfile(server);
			return {
				key: operational.tone,
				label:
					operational.tone === "bad"
						? "Blocked or failed"
						: operational.tone === "warn"
							? "Needs action"
							: operational.tone === "good"
								? "Ready"
								: operational.tone === "off"
									? "Off"
									: "Not checked",
				tone: operational.tone,
			};
		}
		if (state.integrationGroup === "source") {
			const access = serverAccessProfile(server);
			if (access.remote)
				return { key: "remote", label: "Remote endpoints", tone: "warn" };
			if (server.sourceType === "http")
				return {
					key: "local-http",
					label: "Local HTTP endpoints",
					tone: "neutral",
				};
			return { key: "local-process", label: "Local processes", tone: "good" };
		}
		if (state.integrationGroup === "client")
			return {
				key: context.primaryClient || "unobserved",
				label:
					state.contextLabels === "show"
						? context.primaryClient || "No observed client"
						: context.primaryClient
							? "Observed client hidden"
							: "No observed client",
				tone: context.live ? "good" : "neutral",
			};
		if (state.integrationGroup === "project")
			return {
				key: context.primaryProject || "unobserved",
				label:
					state.contextLabels === "show"
						? projectDisplayName(context.primaryProject)
						: context.primaryProject
							? "Observed project hidden"
							: "No observed project",
				tone: context.live ? "good" : "neutral",
			};
		return { key: "all", label: "All servers", tone: "neutral" };
	}

	function arrangeServerRows(models) {
		const list = state.nodes.serverList;
		if (!list) return;
		const grouped = state.integrationGroup !== "none";
		const desired = [];
		let previousGroup = null;
		models.forEach((server) => {
			const group = serverGroupProfile(server);
			server.row.dataset.mcAtlasGroup = group.key;
			if (grouped && group.key !== previousGroup) {
				desired.push({ type: "group", key: group.key, group });
				previousGroup = group.key;
			}
			desired.push({ type: "row", key: server.name, server });
		});
		const signature = JSON.stringify([
			state.integrationGroup,
			desired.map((item) => `${item.type}:${item.key}`),
		]);
		if (list.dataset.mcAtlasOrder === signature) return;
		list.dataset.mcAtlasOrder = signature;
		$$(".mc-server-group-header", list).forEach((header) => header.remove());
		const fragment = document.createDocumentFragment();
		desired.forEach((item) => {
			if (item.type === "row") fragment.appendChild(item.server.row);
			else {
				const header = document.createElement("div");
				header.className = "mc-server-group-header";
				header.dataset.mcGroupKey = item.key;
				header.dataset.tone = item.group.tone;
				setProductHtml(
					header,
					`<span>${toneMark(item.group.tone)}</span><strong>${escapeHtml(item.group.label)}</strong><small data-mc-group-count></small>`,
				);
				fragment.appendChild(header);
			}
		});
		list.appendChild(fragment);
	}

	function syncServerPeeks() {
		$$(".server-row", state.nodes.serverList).forEach((row) => {
			const open = state.expandedServer === row.dataset.serverName;
			row.classList.toggle("mc-peek-open", open);
			const peek = $(".mc-row-peek", row);
			if (peek) peek.hidden = !open;
			const button = $(".mc-row-peek-toggle", row);
			if (button) button.setAttribute("aria-expanded", String(open));
		});
	}

	function toggleServerPeek(name) {
		state.expandedServer = state.expandedServer === name ? null : name;
		syncServerPeeks();
		if (state.expandedServer)
			requestAnimationFrame(() =>
				$(".server-row.mc-peek-open .mc-row-peek")?.scrollIntoView?.({
					block: "nearest",
					behavior:
						["reduced", "off"].includes(
							document.documentElement.dataset.mcMotion,
						) || matchMedia("(prefers-reduced-motion: reduce)").matches
							? "auto"
							: "smooth",
				}),
			);
	}

	function renderRouteRibbon(host, servers) {
		if (!host) return;
		const visibleNames = new Set(
			servers.filter(integrationMatches).map((server) => server.name),
		);
		const routes = observedClientRouteModels().filter((route) =>
			visibleNames.has(route.server),
		);
		const current = routes.filter((route) => route.live);
		const shown = (current.length ? current : routes).slice(0, 5);
		if (!shown.length) {
			setProductHtml(
				host,
				`<span class="mc-route-ribbon-label">${ICON.activity}<strong>Observed routes</strong></span><p>No client-to-server use is retained for this view. Configured availability is kept separate from observed use.</p><button type="button" data-mc-open-connections>Connections</button>`,
			);
		} else {
			setProductHtml(
				host,
				`<span class="mc-route-ribbon-label">${ICON.activity}<strong>${current.length ? "Routes held now" : "Recent routes"}</strong><small>${current.length ? "ownership may be idle" : "retained calls"}</small></span><div class="mc-route-ribbon-scroll">${shown
					.map((route) => {
						const client =
							state.contextLabels === "show" ? route.clientId : "Client hidden";
						const project =
							state.contextLabels === "show"
								? projectDisplayName([...route.projects][0])
								: route.projects.size
									? "Project hidden"
									: "No project";
						return `<button type="button" class="mc-route-chip" data-tone="${route.live ? "good" : route.failures ? "warn" : "neutral"}" data-mc-open-server="${escapeHtml(route.server)}" title="${escapeHtml(route.live ? "Route ownership is current but may be idle." : "Retained historical use; not currently held.")}" aria-label="${escapeHtml(`${client} to ${route.server}. ${route.live ? "Route held now; ownership may be idle." : `${route.calls} retained calls.`}`)}"><strong>${escapeHtml(client)} <i aria-hidden="true">→</i> ${escapeHtml(route.server)}</strong><small>${escapeHtml(project)}${route.lastTool ? ` · ${route.lastTool}` : ""}</small></button>`;
					})
					.join(
						"",
					)}</div><button type="button" data-mc-open-connections>All connections</button>`,
			);
		}
		$$("[data-mc-open-server]", host).forEach((button) =>
			button.addEventListener("click", () =>
				openServer(button.dataset.mcOpenServer, "usage"),
			),
		);
		$("[data-mc-open-connections]", host)?.addEventListener("click", () => {
			state.integrationLayout = "map";
			writePreference("integrationLayout", state.integrationLayout);
			renderIntegrations();
		});
	}

	function annotateServerRows(models) {
		models.forEach((model) => {
			const row = model.row;
			const context = serverContextProfile(model);
			const contextInfo = contextHeadline(context);
			const readiness = serverReadinessHeadline(model);
			const route = serverRouteExplanation(model);
			const freshness = serverEvidenceFreshness(model);
			row.dataset.mcTone = readiness.tone;
			row.dataset.mcAtlasRow = "true";
			row.removeAttribute("tabindex");
			const privateContext =
				state.contextLabels === "show"
					? `${context.primaryClient || "client not retained"}. ${context.primaryProject ? projectDisplayName(context.primaryProject) : "project not retained"}.`
					: `${context.primaryClient ? "client hidden locally" : "client not retained"}. ${context.primaryProject ? "project hidden locally" : "project not retained"}.`;
			const rowLabel = `${model.name}. ${readiness.title}. ${serverLifecycleProfile(model).tools.label}. ${model.usage.calls || 0} calls in retained history. ${privateContext} ${model.routeMode}. ${serverCapacityLabel(model)}.`;
			if (row.getAttribute("aria-label") !== rowLabel)
				row.setAttribute("aria-label", rowLabel);

			let status = $(".mc-row-status", row);
			if (!status) {
				status = document.createElement("span");
				status.className = "mc-row-status";
				setProductHtml(status, "<i></i><span></span>");
				(
					$(".server-title-row", row) || $(".server-source-cell", row)
				)?.appendChild(status);
			}
			status.dataset.tone = readiness.tone;
			const statusMark = $("i", status);
			if (statusMark) {
				statusMark.textContent = toneMark(readiness.tone);
				statusMark.setAttribute("aria-hidden", "true");
			}
			const operationalStatus =
				readiness.tone === "good"
					? "Working"
					: readiness.tone === "bad"
						? "Needs action"
						: readiness.tone === "warn"
							? "Review"
							: readiness.tone === "off"
								? "Disabled"
								: "Not checked";
			const statusText = $("span", status);
			if (statusText && statusText.textContent !== operationalStatus)
				statusText.textContent = operationalStatus;

			let pin = $(".mc-row-pin", row);
			if (!pin) {
				pin = document.createElement("button");
				pin.type = "button";
				pin.className = "mc-row-pin";
				(
					$(".server-title-row", row) || $(".server-source-cell", row)
				)?.appendChild(pin);
				pin.addEventListener("click", (event) => {
					event.preventDefault();
					event.stopPropagation();
					const name = row.dataset.serverName || model.name;
					if (state.pinnedServers.has(name)) state.pinnedServers.delete(name);
					else state.pinnedServers.add(name);
					writeSetPreference("pinnedServers", state.pinnedServers);
					renderIntegrations();
					toast(
						state.pinnedServers.has(name)
							? "Pinned integration"
							: "Unpinned integration",
						`${name} ${state.pinnedServers.has(name) ? "is now available in the Pinned filter." : "was removed from the Pinned filter."}`,
						"neutral",
					);
				});
			}
			const pinned = state.pinnedServers.has(model.name);
			pin.setAttribute("aria-pressed", String(pinned));
			pin.setAttribute("aria-label", `${model.name} pinned`);
			pin.title = pinned ? "Unpin integration" : "Pin integration";
			pin.textContent = pinned ? "★" : "☆";
			row.classList.toggle("mc-pinned", pinned);

			const sourceLabel = $(".server-source-cell .server-cell-label", row);
			if (sourceLabel && sourceLabel.textContent !== "Server")
				sourceLabel.textContent = "Server";
			const evidenceLabel = $(".server-evidence-cell .server-cell-label", row);
			if (evidenceLabel && evidenceLabel.textContent !== "Readiness")
				evidenceLabel.textContent = "Readiness";
			const routeLabel = $(".server-routing-cell .server-cell-label", row);
			if (routeLabel && routeLabel.textContent !== "Isolation")
				routeLabel.textContent = "Isolation";

			let selection = $(".mc-row-select", row);
			if (!selection) {
				selection = document.createElement("label");
				selection.className = "mc-row-select";
				setProductHtml(
					selection,
					`<input type="checkbox" aria-label="Select ${escapeHtml(model.name)}"><span aria-hidden="true">${ICON.check}</span>`,
				);
				$(".server-source-cell", row)?.prepend(selection);
				$("input", selection)?.addEventListener("change", (event) => {
					if (event.target.checked) state.selectedServers.add(model.name);
					else state.selectedServers.delete(model.name);
					renderIntegrations();
				});
			}
			const selectionInput = $("input", selection);
			if (selectionInput)
				selectionInput.checked = state.selectedServers.has(model.name);
			row.classList.toggle(
				"mc-selected",
				state.selectedServers.has(model.name),
			);

			let mobileSource = $(".mc-mobile-source-short", row);
			if (!mobileSource) {
				mobileSource = document.createElement("span");
				mobileSource.className = "mc-mobile-source-short";
				$(".server-source-cell .server-cell-secondary", row)?.after(
					mobileSource,
				);
			}
			const mobileSourceText = `${model.sourceType === "http" ? "HTTP" : "stdio"} · ${context.sourceShort}`;
			if (mobileSource && mobileSource.textContent !== mobileSourceText) {
				mobileSource.textContent = mobileSourceText;
				mobileSource.title = model.sourceLocation || context.sourceShort;
			}

			let source = $(".mc-row-source", row);
			if (!source) {
				source = document.createElement("div");
				source.className = "mc-row-source";
				mobileSource?.after(source);
			}
			const sourceSignature = `${model.sourceType}|${model.sourceLocation}|${state.pathVisibility}`;
			if (source && source.dataset.mcSignature !== sourceSignature) {
				source.dataset.mcSignature = sourceSignature;
				setProductHtml(
					source,
					`<span>${escapeHtml(model.sourceType === "http" ? "Remote endpoint" : model.sourcePath ? "Config source" : "Launch source")}</span><code title="${escapeHtml(model.sourceLocation || "Source not returned")}">${escapeHtml(compactPath(model.sourceLocation || "Source not returned"))}</code>${model.sourceLocation ? `<button type="button" data-mc-copy-value="${escapeHtml(model.sourceLocation)}" aria-label="Copy source for ${escapeHtml(model.name)}">${ICON.copy}</button>` : ""}`,
				);
			}

			const evidenceCell = $(".server-evidence-cell", row);
			let readinessNode = $(".mc-readiness-summary", row);
			if (!readinessNode) {
				readinessNode = document.createElement("div");
				readinessNode.className = "mc-readiness-summary";
				evidenceCell?.prepend(readinessNode);
			}
			const readinessSignature = JSON.stringify(readiness);
			if (
				readinessNode &&
				readinessNode.dataset.mcSignature !== readinessSignature
			) {
				readinessNode.dataset.mcSignature = readinessSignature;
				readinessNode.dataset.tone = readiness.tone;
				setProductHtml(
					readinessNode,
					`<span><strong>${escapeHtml(readiness.title)}</strong><small>${escapeHtml(readiness.detail)}</small></span><em title="${escapeHtml(`${readiness.progress} evidence boundaries complete`)}">${escapeHtml(readiness.progress)}</em>`,
				);
			}
			let lifecycle = $(".mc-row-lifecycle", row);
			if (!lifecycle) {
				lifecycle = document.createElement("div");
				lifecycle.className = "mc-row-lifecycle";
				readinessNode?.after(lifecycle);
			}
			const lifecycleSignature = JSON.stringify(
				serverLifecycleProfile(model).steps.map((step) => [
					step.id,
					step.label,
					step.tone,
					step.state,
				]),
			);
			if (lifecycle && lifecycle.dataset.mcSignature !== lifecycleSignature) {
				lifecycle.dataset.mcSignature = lifecycleSignature;
				setProductHtml(lifecycle, lifecycleMarkup(model));
				$$("[data-mc-life-tab]", lifecycle).forEach((button) =>
					button.addEventListener("click", (event) => {
						event.preventDefault();
						event.stopPropagation();
						openServer(model.name, button.dataset.mcLifeTab);
					}),
				);
			}

			let contextNode = $(".mc-row-context", row);
			if (!contextNode) {
				contextNode = document.createElement("div");
				contextNode.className = "mc-row-context";
				evidenceCell?.after(contextNode);
			}
			const contextSignature = JSON.stringify([
				contextInfo,
				context.sourceShort,
				state.contextLabels,
				context.live,
			]);
			if (contextNode && contextNode.dataset.mcSignature !== contextSignature) {
				contextNode.dataset.mcSignature = contextSignature;
				contextNode.dataset.tone = contextInfo.tone;
				setProductHtml(
					contextNode,
					`<span class="mc-row-context-owner"><small>${context.live ? "Route ownership" : "Last observed use"}</small><strong>${escapeHtml(contextInfo.title)}</strong><em>${escapeHtml(contextInfo.detail)}</em></span><span class="mc-row-context-source"><small>Configuration source</small><code title="${escapeHtml(model.sourceLocation || context.sourceShort)}">${escapeHtml(context.sourceShort)}</code></span>`,
				);
			}

			let health = $(".mc-row-health", row);
			if (!health) {
				health = document.createElement("div");
				health.className = "mc-row-health";
				contextNode?.after(health);
			}
			const healthSignature = JSON.stringify([
				model.usage.calls,
				model.usage.failures,
				model.usage.successRate,
				model.usage.p95,
				route.title,
				route.capacity.label,
				route.ownership,
				freshness,
			]);
			if (health && health.dataset.mcSignature !== healthSignature) {
				health.dataset.mcSignature = healthSignature;
				const usageMain = model.usage.calls
					? `${formatNumber(model.usage.calls)} calls · ${successLabel(model.usage.successRate)}`
					: "No retained calls";
				const latency =
					model.usage.p95 !== null
						? `p95 ${formatDuration(model.usage.p95)}`
						: "latency not measured";
				setProductHtml(
					health,
					`<span class="mc-row-health-main"><small>Health</small><strong>${escapeHtml(usageMain)}</strong><em>${escapeHtml(latency)} · ${escapeHtml(freshness.label)}</em></span><span class="mc-row-health-meta"><small>Isolation · capacity</small><strong>${escapeHtml(route.title)} · ${escapeHtml(serverCapacityShort(model))}</strong><em>${escapeHtml(route.capacity.label)}</em></span>`,
				);
			}

			let plainProtection = $(".mc-row-plain-protection", row);
			if (!plainProtection) {
				plainProtection = document.createElement("div");
				plainProtection.className = "mc-row-plain-protection";
				evidenceCell?.appendChild(plainProtection);
			}
			const plainProtectionSignature = JSON.stringify([
				model.routeMode,
				context.activeLeaseCount,
			]);
			if (
				plainProtection &&
				plainProtection.dataset.mcSignature !== plainProtectionSignature
			) {
				plainProtection.dataset.mcSignature = plainProtectionSignature;
				setProductHtml(
					plainProtection,
					`<span>${ICON.shield}<small>Isolation</small><strong>${escapeHtml(model.routeMode || "Automatic")}</strong></span>${context.activeLeaseCount ? `<em>${context.activeLeaseCount} held</em>` : ""}`,
				);
			}

			let routeMeta = $(".mc-row-route-meta", row);
			if (!routeMeta) {
				routeMeta = document.createElement("div");
				routeMeta.className = "mc-row-route-meta";
				$(".server-routing-cell", row)?.appendChild(routeMeta);
			}
			const routeMetaSignature = JSON.stringify([
				route.ownership,
				route.capacity.detail,
			]);
			if (routeMeta && routeMeta.dataset.mcSignature !== routeMetaSignature) {
				routeMeta.dataset.mcSignature = routeMetaSignature;
				setProductHtml(
					routeMeta,
					`<span>${escapeHtml(route.ownership)}</span><span>${escapeHtml(route.capacity.detail)}</span>`,
				);
			}

			const actionArea = $(".server-quick-controls", row);
			let toggle = $(".mc-inline-server-toggle", actionArea);
			if (!toggle && actionArea) {
				toggle = document.createElement("button");
				toggle.type = "button";
				toggle.className = "mc-inline-server-toggle";
				toggle.dataset.serverAction = "toggle";
				actionArea.prepend(toggle);
			}
			if (toggle) {
				toggle.dataset.serverName = model.name;
				toggle.setAttribute("aria-pressed", String(model.enabled));
				toggle.setAttribute("aria-label", `${model.name} enabled`);
				const toggleSignature = `${model.name}|${model.enabled}`;
				if (toggle.dataset.mcSignature !== toggleSignature) {
					toggle.dataset.mcSignature = toggleSignature;
					setProductHtml(
						toggle,
						`<span aria-hidden="true"><i></i></span><strong>${model.enabled ? "On" : "Off"}</strong>`,
					);
				}
			}

			const guidedProfile = serverOperationalProfile(model).next;
			let guided = $(".mc-row-guided-action", actionArea);
			if (!guided && actionArea) {
				guided = document.createElement("button");
				guided.type = "button";
				guided.className = "mc-row-guided-action mc-row-primary";
				toggle?.after(guided);
				guided.addEventListener("click", (event) => {
					event.preventDefault();
					event.stopPropagation();
					const name = row.dataset.serverName || model.name;
					const kind = guided.dataset.mcActionKind;
					const action = guided.dataset.mcAction;
					const tab = guided.dataset.mcActionTab || "overview";
					if (kind === "server") {
						const control =
							$$("[data-server-action]", row).find(
								(button) =>
									button !== guided && button.dataset.serverAction === action,
							) ||
							(action === "enable-test"
								? $$("[data-server-action]", row).find((button) =>
										["enable", "toggle"].includes(button.dataset.serverAction),
									)
								: null);
						control?.click();
					} else {
						openServer(name, tab);
					}
				});
			}
			if (guided) {
				const guidedLabel = guidedProfile.urgent
					? guidedProfile.label
					: "Open settings";
				guided.dataset.mcActionKind = guidedProfile.urgent
					? guidedProfile.kind
					: "tab";
				guided.dataset.mcAction = guidedProfile.urgent
					? guidedProfile.action
					: "overview";
				guided.dataset.mcActionTab = guidedProfile.urgent
					? guidedProfile.tab
					: "overview";
				guided.dataset.tone = guidedProfile.tone;
				guided.textContent = guidedLabel;
				guided.title = guidedProfile.reason;
				guided.setAttribute(
					"aria-label",
					`${guidedLabel} for ${model.name}. ${guidedProfile.reason}`,
				);
			}

			let peekToggle = $(".mc-row-peek-toggle", actionArea);
			if (!peekToggle && actionArea) {
				peekToggle = document.createElement("button");
				peekToggle.type = "button";
				peekToggle.className = "mc-row-peek-toggle";
				setProductHtml(peekToggle, `<span>Quick view</span>${ICON.chevron}`);
				peekToggle.addEventListener("click", (event) => {
					event.preventDefault();
					event.stopPropagation();
					toggleServerPeek(row.dataset.serverName || model.name);
				});
				actionArea.appendChild(peekToggle);
			}
			if (peekToggle) {
				peekToggle.setAttribute(
					"aria-label",
					`Toggle quick view for ${model.name}`,
				);
				peekToggle.setAttribute(
					"aria-expanded",
					String(state.expandedServer === model.name),
				);
			}

			let actionMenu = $(".mc-row-actions-menu", actionArea);
			if (!actionMenu && actionArea) {
				actionMenu = document.createElement("details");
				actionMenu.className = "mc-row-actions-menu";
				setProductHtml(
					actionMenu,
					`<summary aria-label="More actions for ${escapeHtml(model.name)}"><span aria-hidden="true">•••</span></summary><div><button type="button" data-mc-row-menu-tab="overview" aria-label="Open summary for ${escapeHtml(model.name)}"><strong>Summary</strong><small>Status and next step</small></button><button type="button" data-mc-row-menu-tab="tools" aria-label="Open tools for ${escapeHtml(model.name)}"><strong>Tools</strong><small>Available operations</small></button><button type="button" data-mc-row-menu-tab="routing" aria-label="Open isolation settings for ${escapeHtml(model.name)}"><strong>Isolation</strong><small>Routing and capacity</small></button><button type="button" data-mc-row-menu-tab="source" aria-label="Open setup for ${escapeHtml(model.name)}"><strong>Setup</strong><small>Command, URL, and source</small></button><button type="button" data-mc-row-menu-tab="usage" aria-label="Open activity for ${escapeHtml(model.name)}"><strong>Activity</strong><small>Calls and latency</small></button></div>`,
				);
				actionArea.appendChild(actionMenu);
				$$("[data-mc-row-menu-tab]", actionMenu).forEach((button) =>
					button.addEventListener("click", (event) => {
						event.preventDefault();
						event.stopPropagation();
						actionMenu.open = false;
						openServer(
							row.dataset.serverName || model.name,
							button.dataset.mcRowMenuTab,
						);
					}),
				);
			}

			let peek = $(".mc-row-peek", row);
			if (!peek) {
				peek = document.createElement("div");
				peek.className = "mc-row-peek";
				row.appendChild(peek);
			}
			const peekSignature = JSON.stringify([
				contextSignature,
				healthSignature,
				model.sourceLocation,
				serverOperationalProfile(model).tone,
				serverOperationalProfile(model).title,
				serverLifecycleProfile(model).tools.measured,
				model.toolCount,
				serverRuntimeProfile(model).pids,
			]);
			if (peek && peek.dataset.mcSignature !== peekSignature) {
				peek.dataset.mcSignature = peekSignature;
				setProductHtml(peek, serverPeekMarkup(model));
				$$("[data-mc-peek-tab]", peek).forEach((button) =>
					button.addEventListener("click", (event) => {
						event.preventDefault();
						event.stopPropagation();
						if (button.dataset.mcPeekKind === "server") {
							const control = $$("[data-server-action]", row).find(
								(item) =>
									item.dataset.serverAction === button.dataset.mcPeekAction,
							);
							control?.click();
						} else {
							openServer(model.name, button.dataset.mcPeekTab);
						}
					}),
				);
			}

			const controls = $$(
				".server-quick-controls > button[data-server-action]:not(.mc-inline-server-toggle)",
				row,
			);
			controls.forEach((button) => {
				if (
					button.dataset.serverAction === "settings" &&
					/open source|configure/i.test(text(button))
				)
					button.textContent = "Settings";
				if (
					button.dataset.serverAction === "routing" &&
					/routing/i.test(text(button))
				)
					button.textContent = "Isolation";
				button.classList.remove("mc-row-primary");
				button.classList.add("mc-row-secondary", "mc-native-row-action");
			});
		});
		syncServerPeeks();
	}

	function integrationMatches(model) {
		const filter = state.integrationFilter;
		const context = serverContextProfile(model);
		const filterMatch =
			filter === "all" ||
			(filter === "pinned" && state.pinnedServers.has(model.name)) ||
			(filter === "working" &&
				serverOperationalProfile(model).tone === "good") ||
			(filter === "attention" &&
				["warn", "bad"].includes(serverOperationalProfile(model).tone)) ||
			(filter === "disabled" &&
				serverOperationalProfile(model).tone === "off") ||
			(filter === "active" && context.live);
		const query = state.integrationQuery.trim().toLowerCase();
		const access = serverAccessProfile(model);
		const scope = state.integrationScope;
		const scopeMatch =
			scope === "all" ||
			(scope === "local" && !access.remote) ||
			(scope === "remote" && access.remote) ||
			(scope === "credentials" && access.credentialNames.length > 0) ||
			(scope === "risk" && (access.destructive > 0 || access.external > 0)) ||
			(scope === "unused" && model.usage.calls === 0);
		const clientMatch =
			state.integrationClient === "all" ||
			context.clients.includes(state.integrationClient);
		const projectMatch =
			state.integrationProject === "all" ||
			context.projects.includes(state.integrationProject);
		const contextSearch =
			state.contextLabels === "show"
				? `${context.clients.join(" ")} ${context.projects.join(" ")} ${context.latestOperation}`.toLowerCase()
				: "";
		const searchable = `${model.searchable} ${context.sourceShort.toLowerCase()} ${contextSearch}`;
		return (
			filterMatch &&
			scopeMatch &&
			clientMatch &&
			projectMatch &&
			(!query || searchable.includes(query))
		);
	}

	function sortServers(models) {
		const priority = { bad: 0, warn: 1, good: 2, off: 3, neutral: 4 };
		return [...models].sort((left, right) => {
			const leftContext = serverContextProfile(left);
			const rightContext = serverContextProfile(right);
			if (state.integrationSort === "name")
				return left.name.localeCompare(right.name);
			if (state.integrationSort === "tools")
				return (
					right.toolCount - left.toolCount ||
					left.name.localeCompare(right.name)
				);
			if (state.integrationSort === "activity")
				return (
					Number(rightContext.live) - Number(leftContext.live) ||
					right.usage.calls - left.usage.calls ||
					(rightContext.lastTimestamp || 0) -
						(leftContext.lastTimestamp || 0) ||
					left.name.localeCompare(right.name)
				);
			if (state.integrationSort === "latency")
				return (
					(right.usage.p95 ?? -1) - (left.usage.p95 ?? -1) ||
					right.usage.calls - left.usage.calls ||
					left.name.localeCompare(right.name)
				);
			const leftTone = serverOperationalProfile(left).tone;
			const rightTone = serverOperationalProfile(right).tone;
			return (
				(priority[leftTone] ?? 9) - (priority[rightTone] ?? 9) ||
				Number(rightContext.live) - Number(leftContext.live) ||
				Number(state.pinnedServers.has(right.name)) -
					Number(state.pinnedServers.has(left.name)) ||
				left.name.localeCompare(right.name)
			);
		});
	}

	function renderBulkSelection(host, visibleServers) {
		const existing = new Set(serverModels().map((server) => server.name));
		[...state.selectedServers].forEach((name) => {
			if (!existing.has(name)) state.selectedServers.delete(name);
		});
		const selected = state.selectedServers.size;
		const bar = $("[data-mc-bulk-bar]", host);
		if (bar) {
			bar.hidden = selected === 0;
			const count = $("[data-mc-bulk-count]", bar);
			if (count) count.textContent = `${selected} selected`;
		}
		const visibleNames = visibleServers.map((server) => server.name);
		const allVisibleSelected =
			visibleNames.length > 0 &&
			visibleNames.every((name) => state.selectedServers.has(name));
		const select = $("[data-mc-select-visible]", host);
		if (select) {
			select.setAttribute("aria-pressed", String(allVisibleSelected));
			const label = select.lastChild;
			if (label?.nodeType === Node.TEXT_NODE)
				label.textContent = allVisibleSelected
					? " Clear visible"
					: " Select visible";
		}
	}

	function toggleVisibleServerSelection() {
		const visible = sortServers(serverModels()).filter(integrationMatches);
		const allSelected =
			visible.length > 0 &&
			visible.every((server) => state.selectedServers.has(server.name));
		visible.forEach((server) => {
			if (allSelected) state.selectedServers.delete(server.name);
			else state.selectedServers.add(server.name);
		});
		renderIntegrations();
	}

	function waitForBackendControl(control, timeoutMs = 30000) {
		return new Promise((resolve) => {
			const started = Date.now();
			let sawBusy = control.disabled;
			const tick = () => {
				sawBusy ||= control.disabled;
				if ((sawBusy && !control.disabled) || Date.now() - started > timeoutMs)
					return resolve();
				setTimeout(tick, 120);
			};
			setTimeout(tick, 120);
		});
	}

	async function runBulkServerAction(action, trigger) {
		if (action === "clear") {
			state.selectedServers.clear();
			renderIntegrations();
			return;
		}
		const selected = serverModels().filter((server) =>
			state.selectedServers.has(server.name),
		);
		if (!selected.length || trigger?.disabled) return;
		const approved = await requestServerActionReview({
			action,
			names: selected.map((server) => server.name),
			bulk: true,
		});
		if (!approved) return;
		const original = trigger.textContent;
		const bar = $("[data-mc-bulk-bar]", state.hosts.integrations);
		trigger.disabled = true;
		bar?.setAttribute("data-busy", "true");
		let completed = 0;
		let skipped = 0;
		try {
			for (const server of selected) {
				let control = null;
				if (action === "test")
					control = $$("[data-server-action]", server.row).find(
						(button) => button.dataset.serverAction === "test",
					);
				else if (action === "enable" && !server.enabled)
					control =
						$$("[data-server-action]", server.row).find(
							(button) => button.dataset.serverAction === "enable",
						) || $(".mc-inline-server-toggle", server.row);
				else if (action === "disable" && server.enabled)
					control = $(".mc-inline-server-toggle", server.row);
				if (!control || control.disabled) {
					skipped += 1;
					continue;
				}
				trigger.textContent = `${action} ${completed + 1}/${selected.length}`;
				bar?.style.setProperty(
					"--mc-bulk-progress",
					`${((completed + 1) / selected.length) * 100}%`,
				);
				control.dataset.mcSkipProductConfirm = "true";
				control.dataset.mcBulkAction = "true";
				control.click();
				await waitForBackendControl(control);
				delete control.dataset.mcSkipProductConfirm;
				delete control.dataset.mcBulkAction;
				completed += 1;
			}
			toast(
				"Bulk action finished",
				`${completed} completed${skipped ? ` · ${skipped} skipped` : ""}. Runtime state will refresh as backend actions settle.`,
			);
			scheduleRender(300);
		} finally {
			trigger.disabled = false;
			trigger.textContent = original;
			bar?.removeAttribute("data-busy");
			bar?.style.removeProperty("--mc-bulk-progress");
		}
	}

	function renderIntegrations() {
		const host = state.hosts.integrations;
		if (!host) return;
		const model = metrics();
		if (state.contextLabels !== "show") {
			state.integrationClient = "all";
			state.integrationProject = "all";
			if (["client", "project"].includes(state.integrationGroup))
				state.integrationGroup = "none";
		}
		annotateServerRows(model.servers);
		const sorted = sortServers(model.servers);
		arrangeServerRows(sorted);
		let visibleCount = 0;
		sorted.forEach((server) => {
			const show = integrationMatches(server);
			server.row.hidden = !show || state.integrationLayout !== "list";
			if (show) visibleCount += 1;
		});

		$$(".mc-server-group-header", state.nodes.serverList).forEach((header) => {
			const rows = sorted.filter(
				(server) =>
					server.row.dataset.mcAtlasGroup === header.dataset.mcGroupKey &&
					!server.row.hidden,
			);
			header.hidden = state.integrationLayout !== "list" || rows.length === 0;
			const count = $("[data-mc-group-count]", header);
			if (count)
				count.textContent = `${rows.length} server${rows.length === 1 ? "" : "s"}`;
		});

		const search = $("[data-mc-integration-search]", host);
		if (search && search.value !== state.integrationQuery)
			search.value = state.integrationQuery;
		const sort = $("[data-mc-integration-sort]", host);
		if (sort) sort.value = state.integrationSort;
		const scope = $("[data-mc-integration-scope]", host);
		if (scope) scope.value = state.integrationScope;
		const group = $("[data-mc-integration-group]", host);
		if (group) group.value = state.integrationGroup;
		const contexts = model.servers.map(serverContextProfile);
		syncContextSelect(
			$("[data-mc-integration-client]", host),
			contexts.flatMap((context) => context.clients),
			state.integrationClient,
			"All observed clients",
		);
		syncContextSelect(
			$("[data-mc-integration-project]", host),
			contexts.flatMap((context) => context.projects),
			state.integrationProject,
			"All observed projects",
			projectDisplayName,
		);
		const contextDisabled = state.contextLabels !== "show";
		[
			$("[data-mc-integration-client]", host),
			$("[data-mc-integration-project]", host),
		].forEach((select) => {
			if (select) {
				select.disabled = contextDisabled;
				select.title = contextDisabled
					? "Client and project labels are hidden in Settings."
					: "";
			}
		});
		const clearContext = $("[data-mc-clear-context]", host);
		if (clearContext)
			clearContext.hidden =
				state.integrationClient === "all" && state.integrationProject === "all";

		$$("[data-mc-summary-filter]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcSummaryFilter === state.integrationFilter),
			),
		);
		$$("[data-mc-integration-filter]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcIntegrationFilter === state.integrationFilter),
			),
		);
		$$("[data-mc-integration-layout]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcIntegrationLayout === state.integrationLayout),
			),
		);

		const totalNode = $("[data-mc-summary-total]", host);
		if (totalNode) totalNode.textContent = model.servers.length;
		const readyNode = $("[data-mc-summary-ready]", host);
		if (readyNode) readyNode.textContent = model.ready;
		const reviewNode = $("[data-mc-summary-review]", host);
		if (reviewNode) reviewNode.textContent = model.review;
		const disabledNode = $("[data-mc-summary-disabled]", host);
		if (disabledNode) disabledNode.textContent = model.disabled;
		const routeCount = model.servers.filter(
			(server) => serverContextProfile(server).live,
		).length;
		const routesNode = $("[data-mc-summary-routes]", host);
		if (routesNode) routesNode.textContent = routeCount;
		const pinnedNode = $("[data-mc-pinned-count]", host);
		if (pinnedNode) pinnedNode.textContent = state.pinnedServers.size;

		renderRouteRibbon($("[data-mc-route-ribbon]", host), sorted);
		const results = $("[data-mc-integration-results]", host);
		if (results) {
			const narrow =
				typeof matchMedia === "function" &&
				matchMedia("(max-width: 679px)").matches;
			results.textContent = model.servers.length
				? narrow
					? `${visibleCount}/${model.servers.length} servers · ${model.tools} tools · ${routeCount} route${routeCount === 1 ? "" : "s"}${state.selectedServers.size ? ` · ${state.selectedServers.size} selected` : ""}`
					: `${visibleCount} of ${model.servers.length} servers visible · ${model.tools} tools retained · ${routeCount} route${routeCount === 1 ? "" : "s"} held now${state.selectedServers.size ? ` · ${state.selectedServers.size} selected` : ""}`
				: "No MCP server is configured yet.";
		}
		renderBulkSelection(host, sorted.filter(integrationMatches));

		const listShell = $("[data-mc-integration-list-shell]", host);
		const mapShell = $("[data-mc-route-map]", host);
		if (listShell) listShell.hidden = state.integrationLayout !== "list";
		if (mapShell) mapShell.hidden = state.integrationLayout !== "map";
		if (state.integrationLayout === "map")
			renderRouteMap(mapShell, sorted.filter(integrationMatches));

		const empty = $("[data-mc-integration-empty]", host);
		if (empty) {
			empty.hidden = state.integrationLayout !== "list" || visibleCount > 0;
			if (!empty.hidden) {
				const emptyTitle =
					state.integrationFilter === "pinned"
						? "No pinned servers"
						: "No server matches this view";
				const emptyDetail =
					state.integrationFilter === "pinned"
						? "Use the star beside a server name to keep frequent integrations one click away."
						: "Clear search, context, or status filters.";
				setProductHtml(
					empty,
					model.servers.length
						? `<strong>${emptyTitle}</strong><span>${emptyDetail}</span><button type="button" class="mc-secondary-button" data-mc-clear-integration-filter>Clear filters</button>`
						: `<strong>No MCP servers yet</strong><span>Add a catalog server, import a client config, or enter a command or URL.</span><button type="button" class="mc-primary-button" data-mc-add-empty>${ICON.plus} Add integration</button>`,
				);
				$("[data-mc-clear-integration-filter]", empty)?.addEventListener(
					"click",
					() => {
						state.integrationFilter = "all";
						state.integrationScope = "all";
						state.integrationClient = "all";
						state.integrationProject = "all";
						state.integrationQuery = "";
						renderIntegrations();
					},
				);
				$("[data-mc-add-empty]", empty)?.addEventListener("click", () =>
					openAddDialog(),
				);
			}
		}
	}

	function renderRouteMap(host, servers) {
		if (!host) return;
		const names = new Set(servers.map((server) => server.name));
		const serverByName = new Map(
			servers.map((server) => [server.name, server]),
		);
		const routes = observedClientRouteModels().filter((route) =>
			names.has(route.server),
		);
		const grouped = new Map();
		routes.forEach((route) => {
			const key = route.clientId || "Client not recorded";
			if (!grouped.has(key)) grouped.set(key, []);
			grouped.get(key).push(route);
		});
		const clients = [...grouped.entries()].sort(
			(left, right) =>
				Number(right[1].some((route) => route.live)) -
					Number(left[1].some((route) => route.live)) ||
				right[1].reduce((sum, route) => sum + route.calls, 0) -
					left[1].reduce((sum, route) => sum + route.calls, 0),
		);
		const observedServers = new Set(routes.map((route) => route.server));
		const unobserved = servers.filter(
			(server) => server.enabled && !observedServers.has(server.name),
		);
		const routeCount = routes.length;
		const liveCount = routes.filter((route) => route.live).length;
		const calls = routes.reduce((sum, route) => sum + route.calls, 0);
		setProductHtml(
			host,
			`<header class="mc-connections-header"><div><span>OBSERVED CONNECTIONS</span><h2>Who used which MCP server</h2><p>Built from retained tool calls and current route ownership. A held route can be idle; observed use is not the same as configured availability.</p></div><div class="mc-connections-facts"><span><strong>${clients.length}</strong><small>clients observed</small></span><span><strong>${liveCount}</strong><small>routes held now</small></span><span><strong>${formatNumber(calls)}</strong><small>retained calls</small></span><span><strong>${servers.length}</strong><small>servers visible</small></span></div></header>${
				clients.length
					? `<div class="mc-connections-list">${clients
							.map(([clientId, clientRoutes]) => {
								const clientLabel =
									state.contextLabels === "show"
										? clientId
										: "Client hidden locally";
								const clientCalls = clientRoutes.reduce(
									(sum, route) => sum + route.calls,
									0,
								);
								const projects = unique(
									clientRoutes.flatMap((route) => [...route.projects]),
								);
								const lastSeen =
									Math.max(
										...clientRoutes.map((route) => route.lastSeenAtMs || 0),
									) || null;
								return `<section class="mc-connection-group"><header><span>${initials(clientLabel)}</span><div><strong>${escapeHtml(clientLabel)}</strong><small>${clientCalls} calls · ${state.contextLabels === "show" ? `${projects.length} project${projects.length === 1 ? "" : "s"}` : projects.length ? "projects hidden locally" : "no project retained"} · ${escapeHtml(formatRelativeTimestamp(lastSeen))}</small></div><em>${clientRoutes.filter((route) => route.live).length} held</em></header><div>${clientRoutes
									.map((route) => {
										const server = serverByName.get(route.server);
										const contextProject =
											state.contextLabels === "show"
												? projectDisplayName([...route.projects][0])
												: route.projects.size
													? "Project hidden"
													: "No project";
										const capacity = server
											? serverCapacityProfile(server)
											: { label: "Capacity unavailable" };
										return `<button type="button" class="mc-connection-card" data-tone="${route.live ? "good" : route.failures ? "warn" : "neutral"}" data-mc-open-server="${escapeHtml(route.server)}"><span class="mc-connection-line" aria-hidden="true"><i></i></span><span class="mc-connection-server"><strong>${escapeHtml(route.server)}</strong><small>${route.live ? "Route ownership now; it may be idle" : "Observed before; not currently held"}</small></span><span class="mc-connection-operation"><strong>${escapeHtml(route.lastTool || `${route.calls} retained call${route.calls === 1 ? "" : "s"}`)}</strong><small>${escapeHtml(contextProject)}${route.failures ? ` · ${route.failures} failed` : ""}</small></span><span class="mc-connection-policy"><strong>${escapeHtml(server?.routeMode || "Policy unavailable")}</strong><small>${escapeHtml(capacity.label)}</small></span>${ICON.chevron}</button>`;
									})
									.join("")}</div></section>`;
							})
							.join("")}</div>`
					: `<div class="mc-large-empty">${ICON.activity}<strong>No observed client-to-server route</strong><span>Configured servers can still be available, but MCPace has no retained call or current route ownership for this filter.</span></div>`
			}${unobserved.length ? `<details class="mc-unobserved-servers"><summary>${unobserved.length} enabled server${unobserved.length === 1 ? "" : "s"} with no observed route</summary><div>${unobserved.map((server) => `<button type="button" data-mc-open-server="${escapeHtml(server.name)}"><strong>${escapeHtml(server.name)}</strong><small>${escapeHtml(serverReadinessHeadline(server).title)} · ${escapeHtml(serverCapacityLabel(server))}</small>${ICON.chevron}</button>`).join("")}</div></details>` : ""}<footer class="mc-connections-footer">${ICON.warning}<p><strong>Observed is not the same as available.</strong> Client configuration, trust, authorization, and route policy remain separate boundaries.</p></footer>`,
		);
		$$("[data-mc-open-server]", host).forEach((button) =>
			button.addEventListener("click", () =>
				openServer(button.dataset.mcOpenServer, "usage"),
			),
		);
	}

	function observedClientRouteModels() {
		const routes = new Map();
		const ensure = (clientId, server) => {
			const client = String(clientId || "unknown client");
			const integration = String(server || "unknown server");
			const key = `${client}\u0000${integration}`;
			if (!routes.has(key))
				routes.set(key, {
					key,
					clientId: client,
					server: integration,
					calls: 0,
					failures: 0,
					lastSeenAtMs: null,
					lastTool: "",
					lastToolTechnical: "",
					lastToolOk: null,
					lastCallAtMs: null,
					live: false,
					leases: 0,
					projects: new Set(),
					sessions: new Set(),
				});
			return routes.get(key);
		};
		rangedAuditRecords().forEach((record) => {
			const route = ensure(
				record.clientId || "client not recorded",
				record.server,
			);
			route.calls += record.callCount;
			route.failures += record.failedCount;
			if (
				!route.lastCallAtMs ||
				(record.timestamp || 0) >= route.lastCallAtMs
			) {
				const technical = record.tools?.[0] || "";
				const definition = technical
					? cachedToolDefinitionByName(technical, record.server)
					: null;
				route.lastTool = definition ? toolDisplayName(definition) : technical;
				route.lastToolTechnical = technical;
				route.lastToolOk = record.ok;
				route.lastCallAtMs = record.timestamp;
			}
			route.lastSeenAtMs =
				Math.max(route.lastSeenAtMs || 0, record.timestamp || 0) || null;
			if (record.projectRoot) route.projects.add(record.projectRoot);
			if (record.sessionId) route.sessions.add(record.sessionId);
		});
		liveSessionModels().forEach((session) =>
			session.servers.forEach((server) => {
				const route = ensure(session.clientId, server);
				route.live = true;
				route.leases += session.activeLeaseCount;
				route.lastSeenAtMs =
					Math.max(route.lastSeenAtMs || 0, session.lastSeenAtMs || 0) || null;
				if (session.projectRoot) route.projects.add(session.projectRoot);
				if (session.sessionId) route.sessions.add(session.sessionId);
			}),
		);
		activeLeaseModels().forEach((lease) => {
			const route = ensure(lease.clientId, lease.server);
			route.live = true;
			route.leases = Math.max(route.leases, 0) + 1;
			route.lastSeenAtMs =
				Math.max(
					route.lastSeenAtMs || 0,
					lease.renewedAtMs || lease.acquiredAtMs || 0,
				) || null;
			if (lease.projectRoot) route.projects.add(lease.projectRoot);
			if (lease.sessionId) route.sessions.add(lease.sessionId);
		});
		return [...routes.values()].sort(
			(left, right) =>
				Number(right.live) - Number(left.live) ||
				right.calls - left.calls ||
				(right.lastSeenAtMs || 0) - (left.lastSeenAtMs || 0),
		);
	}

	function clientExposureMarkup(clients, servers) {
		const enabled = servers.filter((server) => server.enabled);
		const observed = observedClientRouteModels();
		if (state.exposureMode === "observed") {
			const rows = observed.length
				? observed
						.map(
							(route) =>
								`<button type="button" class="mc-observed-route" data-mc-open-server="${escapeHtml(route.server)}" data-tone="${route.failures ? "warn" : route.live ? "good" : "neutral"}"><span class="mc-observed-route-client">${escapeHtml(initials(route.clientId))}</span><div><small>${route.live ? "LIVE LEASE" : "RETAINED AUDIT"}</small><strong>${escapeHtml(state.contextLabels === "show" ? route.clientId : "Client hidden locally")}</strong><p>${escapeHtml(route.server)}</p></div><span><strong>${formatNumber(route.calls)} calls</strong><small>${route.failures ? `${route.failures} failed` : route.calls ? "no retained failure" : "no completed call retained"}</small></span><span><strong>${route.live ? `${route.leases} active lease${route.leases === 1 ? "" : "s"}` : formatRelativeTimestamp(route.lastSeenAtMs)}</strong><small>${state.contextLabels === "show" ? `${route.projects.size} project${route.projects.size === 1 ? "" : "s"} · ${route.sessions.size} session${route.sessions.size === 1 ? "" : "s"}` : "context labels hidden"}</small></span>${ICON.chevron}</button>`,
						)
						.join("")
				: `<div class="mc-large-empty">${ICON.activity}<strong>No application has used an integration yet</strong><span>MCPace shows this only after a recorded tool call or a current reserved connection. Configuration alone appears under Available after setup. Observed routes require retained calls or active leases.</span></div>`;
			return `<section class="mc-observed-routes"><header><div><span>RECENT USE</span><h3>${humanCount(observed.length, "app-to-integration pair")}</h3></div><small>${escapeHtml(rangeLabel())} plus current leases</small></header><div>${rows}</div></section>`;
		}
		const visibleServers = enabled.slice(0, 8);
		const header = visibleServers
			.map(
				(server) =>
					`<button type="button" data-mc-open-server="${escapeHtml(server.name)}" title="Open ${escapeHtml(server.name)}"><span>${escapeHtml(server.initials)}</span><strong>${escapeHtml(server.name)}</strong></button>`,
			)
			.join("");
		const rows = clients
			.map((client) => {
				const canConfigure = ["patchable", "manual"].includes(client.category);
				const cells = visibleServers
					.map(
						(server) =>
							`<button type="button" data-mc-open-server="${escapeHtml(server.name)}" data-tone="${canConfigure ? "warn" : "neutral"}"><strong>${canConfigure ? "Potential" : "Unknown"}</strong><small>${canConfigure ? (serverAccessProfile(server).remote ? "remote route enabled" : "enabled in shared inventory") : "no local client route evidence"}</small></button>`,
					)
					.join("");
				return `<article><div><span>${escapeHtml(client.initials)}</span><strong>${escapeHtml(client.name)}</strong><small>${escapeHtml(client.status)}</small></div>${cells}</article>`;
			})
			.join("");
		return `<section class="mc-potential-matrix"><header><div><span>AVAILABLE AFTER SETUP</span><h3>Applications × enabled integrations</h3></div><small>${enabled.length > visibleServers.length ? `Showing 8 of ${enabled.length} enabled servers` : `${enabled.length} enabled servers`}</small></header>${visibleServers.length && clients.length ? `<div class="mc-potential-scroll"><div class="mc-potential-grid" style="--mc-exposure-columns:${visibleServers.length}"><div class="mc-potential-corner">Application</div>${header}${rows}</div></div>` : `<div class="mc-large-empty">${ICON.server}<strong>Potential access cannot be mapped yet</strong><span>Add an enabled integration and review at least one application target.</span></div>`}<footer>${ICON.warning}<p><strong>Available does not mean used.</strong> MCPace shows the enabled inventory, but does not invent per-client allowlists or a verified connection. Potential is not observed.</p></footer></section>`;
	}

	function renderApplications() {
		const host = state.hosts.applications;
		if (!host) return;
		const clients = clientModels();
		const patchable = clients.filter(
			(client) => client.category === "patchable",
		).length;
		const manual = clients.filter(
			(client) => client.category === "manual",
		).length;
		const cloud = clients.filter(
			(client) => client.category === "cloud",
		).length;
		const overview = $("[data-mc-client-overview]", host);
		if (overview)
			setProductHtml(
				overview,
				`<div class="mc-client-route-visual"><div class="mc-client-route-stack">${
					clients
						.slice(0, 4)
						.map(
							(client) =>
								`<div data-tone="${client.tone}"><span>${escapeHtml(client.initials)}</span><strong>${escapeHtml(client.name)}</strong><small>${escapeHtml(client.status)}</small></div>`,
						)
						.join("") ||
					"<div><span>?</span><strong>No clients listed</strong><small>Wait for client catalog</small></div>"
				}</div><div class="mc-client-route-line"><i></i></div><div class="mc-client-route-core"><span>${ICON.logo}</span><strong>MCPace</strong><small>one local broker endpoint</small></div></div><div class="mc-client-facts"><div><span>Patchable</span><strong>${patchable}</strong><small>managed preview/apply</small></div><div><span>Manual</span><strong>${manual}</strong><small>path shown, user edits</small></div><div><span>Cloud</span><strong>${cloud}</strong><small>diagnostic only</small></div></div>`,
			);

		const exposure = $("[data-mc-client-exposure]", host);
		if (exposure) {
			const servers = serverModels();
			const enabledServers = servers.filter((server) => server.enabled);
			const remoteServers = enabledServers.filter(
				(server) => serverAccessProfile(server).remote,
			);
			const credentialServers = enabledServers.filter(
				(server) => serverAccessProfile(server).credentialNames.length,
			);
			const observed = observedClientRouteModels();
			setProductHtml(
				exposure,
				`<span class="mc-sr-only">Separate observed use from potential access. credential-backed integrations are counted without exposing values.</span><header><div><span>APP ACCESS</span><h2>What your applications used — and what they could use</h2><p>Recent use comes from recorded calls or a current reserved connection. Available after setup comes only from enabled integrations; it is not proof that an app connected.</p></div><div><strong>${observed.length}</strong><span>recent app–server pairs</span></div></header><div class="mc-exposure-facts"><span>${ICON.server}<strong>${enabledServers.length}</strong><small>integrations available</small></span><span>${ICON.activity}<strong>${remoteServers.length}</strong><small>use the network</small></span><span>${ICON.shield}<strong>${credentialServers.length}</strong><small>refer to credentials</small></span></div><div class="mc-exposure-mode" role="tablist" aria-label="Application access evidence"><button id="mc-exposure-tab-observed" type="button" role="tab" data-mc-exposure-mode="observed" aria-controls="mc-exposure-panel" aria-selected="${state.exposureMode === "observed"}" tabindex="${state.exposureMode === "observed" ? "0" : "-1"}">Recently used<span class="mc-sr-only">Observed routes</span></button><button id="mc-exposure-tab-potential" type="button" role="tab" data-mc-exposure-mode="potential" aria-controls="mc-exposure-panel" aria-selected="${state.exposureMode === "potential"}" tabindex="${state.exposureMode === "potential" ? "0" : "-1"}">Available after setup<span class="mc-sr-only">Potential access</span></button></div><div id="mc-exposure-panel" role="tabpanel" aria-labelledby="mc-exposure-tab-${state.exposureMode}" data-mc-exposure-content>${clientExposureMarkup(clients, servers)}</div>`,
			);
			const exposureTabs = $$("[data-mc-exposure-mode]", exposure);
			const selectExposureTab = (button) => {
				state.exposureMode = button.dataset.mcExposureMode;
				writePreference("exposureMode", state.exposureMode);
				renderApplications();
				requestAnimationFrame(() =>
					document
						.getElementById(`mc-exposure-tab-${state.exposureMode}`)
						?.focus({ preventScroll: true }),
				);
			};
			exposureTabs.forEach((button) => {
				button.addEventListener("click", () => selectExposureTab(button));
				button.addEventListener("keydown", (event) => {
					if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key))
						return;
					event.preventDefault();
					const current = exposureTabs.indexOf(button);
					let next = current;
					if (event.key === "Home") next = 0;
					else if (event.key === "End") next = exposureTabs.length - 1;
					else {
						const direction = event.key === "ArrowRight" ? 1 : -1;
						next =
							(current + direction + exposureTabs.length) % exposureTabs.length;
					}
					selectExposureTab(exposureTabs[next]);
				});
			});
			$$("[data-mc-open-server]", exposure).forEach((button) =>
				button.addEventListener("click", () =>
					openServer(button.dataset.mcOpenServer, "access"),
				),
			);
		}

		const configMap = $("[data-mc-configuration-map]", host);
		if (configMap) {
			const clientRows = clients
				.map((client) => {
					const observed =
						overviewClients().find(
							(item) => String(item?.id || item?.name || "") === client.id,
						) || {};
					const path =
						client.path ||
						String(
							observed.path ||
								observed.configPath ||
								observed.settingsPath ||
								"",
						);
					const workflow =
						client.category === "patchable"
							? "Preview and reversible apply supported"
							: client.category === "manual"
								? "MCPace shows the target; edit remains manual"
								: "No local writable configuration target";
					const row = pathRow(client.name, path, workflow, client.tone);
					return (
						row ||
						`<article class="mc-path-row" data-tone="${client.tone}"><div><span>${escapeHtml(client.name)}</span><strong>No local path returned</strong><small>${escapeHtml(workflow)}</small></div><em>${escapeHtml(client.status)}</em></article>`
					);
				})
				.join("");
			const productRows = configurationPaths()
				.slice(0, 4)
				.map((path) => pathRow(path.label, path.value, path.note, path.tone))
				.join("");
			setProductHtml(
				configMap,
				`<div class="mc-config-map-columns"><section><h3>AI application targets</h3><div class="mc-path-list">${clientRows || '<div class="mc-inline-empty">No AI app returned by the backend.</div>'}</div></section><section><h3>MCPace-owned files</h3><div class="mc-path-list">${productRows || '<div class="mc-inline-empty">Runtime paths are not available yet.</div>'}</div></section></div><footer><span>${ICON.shield}</span><p><strong>Writing is explicit.</strong> Preview shows a proposed diff; apply writes it; restore uses the backend-owned backup path.</p></footer>`,
			);
		}

		$$(".client-setup-card[data-client-id]", state.nodes.clientList).forEach(
			(card) => {
				const chip = text($(".chip", card));
				card.dataset.mcTone = /patchable/i.test(chip)
					? "good"
					: /manual/i.test(chip)
						? "warn"
						: "neutral";
				const path = card.dataset.clientPath || "";
				let location = $(".mc-client-location", card);
				if (!location && path) {
					location = document.createElement("div");
					location.className = "mc-client-location";
					setProductHtml(
						location,
						`<span>Configuration target</span><code title="${escapeHtml(path)}">${escapeHtml(compactPath(path))}</code><button type="button" data-mc-copy-value="${escapeHtml(path)}">${ICON.copy}<span>Copy path</span></button>`,
					);
					$(".client-setup-actions", card)?.before(location);
				} else if (location && path) {
					const code = $("code", location);
					const compact = compactPath(path);
					if (code && code.textContent !== compact) code.textContent = compact;
					if (code && code.title !== path) code.title = path;
				}
				if (!$(".mc-client-guidance", card)) {
					const actionArea = $(".client-setup-actions", card);
					const guidance = document.createElement("div");
					guidance.className = "mc-client-guidance";
					setProductHtml(
						guidance,
						/patchable/i.test(chip)
							? `<span>1</span><strong>Preview</strong><i></i><span>2</span><strong>Apply</strong><i></i><span>3</span><strong>Verify</strong><i></i><span>4</span><strong>Restore</strong>`
							: /manual/i.test(chip)
								? `<span>1</span><strong>Copy path</strong><i></i><span>2</span><strong>Configure manually</strong><i></i><span>3</span><strong>Verify</strong>`
								: `<span>i</span><strong>Read-only client information</strong>`,
					);
					actionArea?.parentNode?.insertBefore(guidance, actionArea);
				}
			},
		);
	}

	function liveActivityMarkup() {
		const sessions = liveSessionModels();
		const leases = activeLeaseModels();
		const now = overviewLeaseEnvelope().nowMs || Date.now();
		const serverCount = unique(leases.map((lease) => lease.server)).length;
		const sessionCards = sessions.length
			? sessions
					.map((session) => {
						const client =
							state.contextLabels === "show"
								? session.clientId
								: session.clientId
									? "Client hidden locally"
									: "Unknown client";
						const project =
							state.contextLabels === "show"
								? compactPath(session.projectRoot)
								: session.projectRoot
									? "Project hidden locally"
									: "";
						const servers = session.servers.length
							? session.servers
									.map(
										(server) =>
											`<button type="button" data-mc-open-server="${escapeHtml(server)}">${escapeHtml(server)}</button>`,
									)
									.join("")
							: "<span>No server name retained</span>";
						return `<article class="mc-live-session-card"><header><span class="mc-live-pulse"><i></i></span><div><small>Client session</small><strong>${escapeHtml(client)}</strong><p>${project ? `${escapeHtml(project)} · ` : ""}${escapeHtml(formatRelativeTimestamp(session.lastSeenAtMs))}</p></div><em>${session.activeLeaseCount} lease${session.activeLeaseCount === 1 ? "" : "s"}</em></header><div class="mc-live-session-servers">${servers}</div><footer><span>Session ID</span><code>${escapeHtml(state.contextLabels === "show" ? session.sessionId || session.id : "hidden locally")}</code></footer></article>`;
					})
					.join("")
			: `<div class="mc-large-empty">${ICON.activity}<strong>No active session lease</strong><span>MCPace does not currently retain routing ownership for a client session. This does not imply that every client application is closed.</span></div>`;
		const leaseRows = leases.length
			? leases
					.map((lease) => {
						const remaining =
							lease.expiresAtMs === null
								? "expiry not retained"
								: lease.expiresAtMs <= now
									? "expired in snapshot"
									: `${formatDuration(lease.expiresAtMs - now)} remaining`;
						const context =
							state.contextLabels === "show"
								? [
										lease.clientId,
										lease.projectRoot ? compactPath(lease.projectRoot) : "",
										lease.sessionId,
									]
										.filter(Boolean)
										.join(" · ")
								: "Client, project, and session labels hidden locally";
						return `<article data-tone="${lease.expiresAtMs !== null && lease.expiresAtMs <= now ? "warn" : "good"}"><button type="button" data-mc-open-server="${escapeHtml(lease.server)}"><span>${escapeHtml(initials(lease.server))}</span><div><strong>${escapeHtml(lease.server)}</strong><small>${escapeHtml(context)}</small></div></button><span><strong>${escapeHtml(lease.strategy || "route strategy not retained")}</strong><small>${escapeHtml([lease.lane, lease.transport].filter(Boolean).join(" · ") || "scheduler metadata unavailable")}</small></span><span><strong>${escapeHtml(remaining)}</strong><small>${escapeHtml(formatRelativeTimestamp(lease.renewedAtMs || lease.acquiredAtMs))}</small></span><code title="${escapeHtml(lease.id)}">${escapeHtml(lease.id.length > 26 ? `${lease.id.slice(0, 12)}…${lease.id.slice(-8)}` : lease.id)}</code></article>`;
					})
					.join("")
			: '<div class="mc-inline-empty">No active lease records were returned by the current overview.</div>';
		return `<section class="mc-live-activity"><header><div><span>Current scheduler ownership</span><h2>${humanCount(sessions.length, "live session")} across ${humanCount(serverCount, "integration")}</h2><p>A lease means MCPace is preserving routing ownership or isolation for a session. It is not proof that a tool is executing at this exact moment.</p></div><button type="button" class="mc-secondary-button" data-mc-live-refresh>${ICON.refresh}<span>Refresh now</span></button></header><div class="mc-live-session-grid">${sessionCards}</div><section class="mc-live-lease-table"><header><div><span>Active leases</span><h3>${humanCount(leases.length, "lease")}</h3></div><small>Snapshot from the local lease store</small></header><div>${leaseRows}</div></section><footer>${ICON.shield}<p><strong>Privacy boundary:</strong> client, session, and project labels come from local routing metadata. Hide them in Observability settings before screen sharing.</p></footer></section>`;
	}

	function activityDayLabel(timestamp) {
		if (!timestamp) return "Time not recorded";
		const value = new Date(timestamp);
		if (Number.isNaN(value.getTime())) return "Time not recorded";
		const now = new Date();
		const startToday = new Date(
			now.getFullYear(),
			now.getMonth(),
			now.getDate(),
		).getTime();
		const startEvent = new Date(
			value.getFullYear(),
			value.getMonth(),
			value.getDate(),
		).getTime();
		const days = Math.round((startToday - startEvent) / 86400000);
		if (days === 0) return "Today";
		if (days === 1) return "Yesterday";
		return new Intl.DateTimeFormat(undefined, {
			month: "short",
			day: "numeric",
			year: value.getFullYear() === now.getFullYear() ? undefined : "numeric",
		}).format(value);
	}

	function activityFailureClassificationMarkup(audit) {
		if (!audit || audit.ok || !audit.errorKind || audit.errorKind === "none")
			return "";
		const label = String(audit.errorKind).replace(/[_-]+/g, " ").trim();
		const stage = String(audit.failureStage || "unknown")
			.replace(/[_-]+/g, " ")
			.trim();
		return `<div class="mc-event-classification" aria-label="Failure classification"><span>${escapeHtml(label)}</span><span>${escapeHtml(stage)} stage</span></div>`;
	}

	function activityEventMarkup(event) {
		return `<button type="button" class="mc-event-row" data-tone="${event.tone}" data-event-type="${event.type}" data-mc-open-event="${escapeHtml(event.id)}"><div class="mc-event-rail"><span></span><i></i></div><div class="mc-event-symbol">${event.type === "tool" ? ICON.terminal : event.type === "error" ? ICON.warning : ICON.activity}</div><div class="mc-event-body"><header><strong>${escapeHtml(event.title)}</strong><span>${escapeHtml(event.source)} · ${escapeHtml(formatRelativeTimestamp(event.timestamp))}</span></header><p>${escapeHtml(event.meta || "No additional metadata in the current backend entry.")}</p>${activityFailureClassificationMarkup(event.audit)}${event.audit ? `<div class="mc-event-mini-trace"><span>queue ${escapeHtml(formatDuration(event.audit.queueMs))}</span><i></i><span>upstream ${escapeHtml(formatDuration(event.audit.upstreamMs))}</span><i></i><span>total ${escapeHtml(formatDuration(event.audit.totalMs))}</span></div>` : ""}</div><em>${escapeHtml(event.chip)}</em>${ICON.chevron}</button>`;
	}

	function activityStreamMarkup(events, total) {
		if (!events.length) return "";
		const groups = [];
		events.forEach((event) => {
			const label = activityDayLabel(event.timestamp);
			let group = groups.at(-1);
			if (!group || group.label !== label) {
				group = { label, events: [] };
				groups.push(group);
			}
			group.events.push(event);
		});
		const rows = groups
			.map(
				(group) =>
					`<section class="mc-event-day"><header><strong>${escapeHtml(group.label)}</strong><span>${group.events.length} shown</span></header><div>${group.events.map(activityEventMarkup).join("")}</div></section>`,
			)
			.join("");
		const remaining = Math.max(0, total - events.length);
		return `${rows}${remaining ? `<div class="mc-event-load-more"><button type="button" class="mc-secondary-button" data-mc-load-more-events>Show ${Math.min(16, remaining)} more</button><span>${remaining} older matching event${remaining === 1 ? "" : "s"} remain</span></div>` : `<div class="mc-event-list-end"><span>${ICON.check}</span><p>End of the selected retained window</p></div>`}`;
	}

	function renderActivity() {
		const host = state.hosts.activity;
		if (!host) return;
		const records = rangedAuditRecords();
		const analytics = usageAnalytics(records);
		const model = metrics();
		const rangeStart = activityRangeStart();
		const allEvents = model.events.filter(
			(event) => !rangeStart || timestampInActivityRange(event.timestamp),
		);
		const query = state.activityQuery.trim().toLowerCase();
		const filtered = allEvents.filter(
			(event) =>
				(state.activityFilter === "all" ||
					event.type === state.activityFilter) &&
				(!query ||
					`${event.title} ${event.meta} ${event.chip} ${event.source} ${event.payload || ""}`
						.toLowerCase()
						.includes(query)),
		);

		$$("[data-mc-activity-view]", host).forEach((button) => {
			const active = button.dataset.mcActivityView === state.activityView;
			button.setAttribute("aria-selected", String(active));
			button.tabIndex = active ? 0 : -1;
		});
		$$("[data-mc-activity-panel]", host).forEach((panel) => {
			panel.hidden = panel.dataset.mcActivityPanel !== state.activityView;
		});
		const range = $("[data-mc-activity-range]", host);
		if (range) {
			range.value = state.activityRange;
			range.closest(".mc-range-select").hidden = state.activityView === "live";
		}

		const summary = $("[data-mc-activity-summary]", host);
		if (summary) {
			if (state.activityView === "live") {
				const live = liveSessionModels();
				const leases = activeLeaseModels();
				const liveServers = unique(leases.map((lease) => lease.server));
				const oldest =
					leases
						.map((lease) => lease.acquiredAtMs)
						.filter((value) => value !== null)
						.sort((a, b) => a - b)[0] || null;
				setProductHtml(
					summary,
					`<article><span>Live sessions</span><strong>${formatNumber(live.length)}</strong><small>derived from current lease ownership</small></article><article><span>Active leases</span><strong>${formatNumber(leases.length)}</strong><small>scheduler ownership records</small></article><article><span>Integrations held</span><strong>${formatNumber(liveServers.length)}</strong><small>${liveServers.slice(0, 3).map(escapeHtml).join(" · ") || "none"}</small></article><article><span>Oldest lease</span><strong>${escapeHtml(oldest ? formatRelativeTimestamp(oldest) : "none")}</strong><small>snapshot age, not call duration</small></article><article class="mc-summary-token"><span>Interpretation</span><strong>Ownership ≠ execution</strong><small>Open Events for completed tool calls.</small></article>`,
				);
			} else {
				setProductHtml(
					summary,
					`<article><span>Tool calls</span><strong>${formatNumber(analytics.calls)}</strong><small>${analytics.records.length} audit entr${analytics.records.length === 1 ? "y" : "ies"} · ${escapeHtml(rangeLabel().toLowerCase())}</small></article><article data-tone="${analytics.failures ? "warn" : "good"}"><span>Success</span><strong>${successLabel(analytics.successRate)}</strong><small>${formatNumber(analytics.failures)} failed call${analytics.failures === 1 ? "" : "s"}</small></article><article><span>Operation p95</span><strong>${escapeHtml(formatDuration(analytics.p95))}</strong><small>queue p95 ${escapeHtml(formatDuration(analytics.queueP95))}</small></article><article><span>MCP payload</span><strong>${escapeHtml(formatBytes(analytics.requestBytes + analytics.responseBytes))}</strong><small>${formatBytes(analytics.requestBytes)} in · ${formatBytes(analytics.responseBytes)} out</small></article><article class="mc-summary-token"><span>Token visibility</span>${usageTokenMarkup(analytics, true)}</article>`,
				);
			}
		}

		const liveMount = $("[data-mc-live-activity]", host);
		if (liveMount) {
			setProductHtml(liveMount, liveActivityMarkup());
			$("[data-mc-live-refresh]", liveMount)?.addEventListener(
				"click",
				refreshRuntime,
			);
			$$("[data-mc-open-server]", liveMount).forEach((button) =>
				button.addEventListener("click", () =>
					openServer(button.dataset.mcOpenServer, "overview"),
				),
			);
		}

		const overview = $("[data-mc-usage-overview]", host);
		if (overview) {
			setProductHtml(
				overview,
				`<div class="mc-observability-grid"><section class="mc-analytics-card mc-analytics-wide"><header><div><span>Throughput</span><h2>Calls over time</h2><p>Actual tool-call audit timestamps in the selected retained window.</p></div><strong>${formatNumber(analytics.calls)}</strong></header>${usageTimelineMarkup(records)}</section><section class="mc-analytics-card"><header><div><span>Data quality</span><h2>What MCPace can prove</h2></div></header><div class="mc-coverage-grid">${coverageItem("Duration", analytics.durationCoverage, `${Math.round(analytics.durationCoverage * analytics.records.length)} of ${analytics.records.length} audit entries measured`)}${coverageItem("Metrics schema", analytics.metricsCoverage, "New mcpace.toolAuditMetrics.v1 entries")}${coverageItem("Reported tokens", analytics.reportedCoverage, "Optional upstream/client metadata; often unavailable")}${coverageItem("Payload estimate", analytics.estimatedCoverage, "UTF-8 bytes ÷ 4; not model billing")}</div></section><section class="mc-analytics-card"><header><div><span>Token accounting</span><h2>Reported and estimated stay separate</h2></div></header>${usageTokenMarkup(analytics)}<p class="mc-analytics-note">MCP tool calls expose payloads, but model-context billing belongs to the AI client/provider. MCPace only labels exact values when metadata reports them.</p></section><section class="mc-analytics-card mc-analytics-wide"><header><div><span>Integrations</span><h2>Most active servers</h2><p>Call volume, reliability, latency, and payload in the current window.</p></div><button type="button" class="mc-text-button" data-mc-open-activity-view="servers">All servers ${ICON.chevron}</button></header>${usageGroupRows(analytics.servers, "server", 5)}</section><section class="mc-analytics-card mc-analytics-wide"><header><div><span>Tools</span><h2>Most used tools</h2><p>Batch durations are distributed as per-call averages for ranking only.</p></div><button type="button" class="mc-text-button" data-mc-open-activity-view="tools">All tools ${ICON.chevron}</button></header>${usageGroupRows(analytics.tools, "tool", 6)}</section><section class="mc-analytics-card"><header><div><span>Clients</span><h2>Who invoked tools</h2></div></header>${usageGroupRows(analytics.clients, "client", 5)}</section><section class="mc-analytics-card"><header><div><span>Projects</span><h2>Where calls originated</h2></div></header>${state.contextLabels === "show" ? usageGroupRows(analytics.projects, "project", 5) : '<div class="mc-inline-empty">Project and client labels are hidden by the local privacy preference.</div>'}</section></div>`,
			);
			$$("[data-mc-open-activity-view]", overview).forEach((button) =>
				button.addEventListener("click", () => {
					state.activityView = button.dataset.mcOpenActivityView;
					writePreference("activityView", state.activityView);
					renderActivity();
				}),
			);
		}

		const tools = $("[data-mc-usage-tools]", host);
		if (tools)
			setProductHtml(
				tools,
				`<section class="mc-analytics-card mc-directory-card"><header><div><span>Tool directory</span><h2>${humanCount(analytics.tools.length, "used tool")}</h2><p>Usage is derived from retained audit entries, not lifetime counters.</p></div></header>${usageGroupRows(analytics.tools, "tool")}</section>`,
			);
		const servers = $("[data-mc-usage-servers]", host);
		if (servers)
			setProductHtml(
				servers,
				`<section class="mc-analytics-card mc-directory-card"><header><div><span>Integration usage</span><h2>${humanCount(analytics.servers.length, "active integration")}</h2><p>Open a row to inspect server-specific tools, protection, configuration, and usage.</p></div></header>${usageGroupRows(analytics.servers, "server")}</section>`,
			);

		$$("[data-mc-activity-filter]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcActivityFilter === state.activityFilter),
			),
		);
		const search = $("[data-mc-activity-search]", host);
		if (search && search.value !== state.activityQuery)
			search.value = state.activityQuery;
		const results = $("[data-mc-activity-results]", host);
		if (results) {
			const retained = retainedWindow();
			const scope =
				retained.source === "api/operations"
					? `${formatNumber(retained.returned || 0)} retained operations${retained.truncated ? ` · latest ${formatNumber(retained.limit || retained.returned || 0)}` : ""}`
					: `${formatNumber(retained.returned || 0)} fallback log entries`;
			const shown = Math.min(filtered.length, state.activityLimit);
			const integrity = [
				analytics.excludedUnknownTimestamps
					? `${analytics.excludedUnknownTimestamps} undated audit entr${analytics.excludedUnknownTimestamps === 1 ? "y" : "ies"} excluded from this bounded range`
					: "",
				analytics.mixedBatchEntries
					? `${analytics.mixedBatchEntries} mixed batch entr${analytics.mixedBatchEntries === 1 ? "y uses" : "ies use"} proportional per-tool estimates`
					: "",
			]
				.filter(Boolean)
				.join(" · ");
			results.textContent = `Showing ${shown} of ${filtered.length} matching entries · ${allEvents.length} in this view · ${scope}${integrity ? ` · ${integrity}` : ""}`;
		}
		const stream = $("[data-mc-event-stream]", host);
		if (stream) {
			const visibleEvents = filtered.slice(0, state.activityLimit);
			setProductHtml(
				stream,
				filtered.length
					? activityStreamMarkup(visibleEvents, filtered.length)
					: `<div class="mc-large-empty">${ICON.activity}<strong>${allEvents.length ? "No event matches this filter" : "No activity in the selected window"}</strong><span>${allEvents.length ? "Clear the search or choose another event type." : "Tool calls and logs will appear after MCPace receives them."}</span></div>`,
			);
			$$("[data-mc-open-event]", stream).forEach((button) =>
				button.addEventListener("click", () =>
					openEventDetail(button.dataset.mcOpenEvent),
				),
			);
			$("[data-mc-load-more-events]", stream)?.addEventListener(
				"click",
				(event) => {
					const previousCount = visibleEvents.length;
					state.activityLimit += 16;
					renderActivity();
					requestAnimationFrame(() => {
						const rows = $$("[data-mc-open-event]", stream);
						rows[previousCount]?.focus({ preventScroll: true });
						rows[previousCount]?.scrollIntoView({
							block: "center",
							behavior:
								state.motion === "system" &&
								!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches
									? "smooth"
									: "auto",
						});
					});
				},
			);
		}

		const raw = $("[data-mc-raw-telemetry]", host);
		if (raw) {
			const rawSignature = `${rawLogs().length}|${rawLogs().at(-1)?.tsMs || ""}|${retainedWindow().totalParsed || ""}|${retainedWindow().parseErrors || ""}|${text(state.nodes.activityList)}`;
			if (state.signatures.rawTelemetry !== rawSignature) {
				state.signatures.rawTelemetry = rawSignature;
				const retained = retainedWindow();
				const files = Array.isArray(retained.files) ? retained.files : [];
				setProductHtml(
					raw,
					`<section class="mc-raw-section"><h3>Current runtime snapshot</h3></section><section class="mc-raw-section"><h3>Retention envelope</h3><pre>${escapeHtml(JSON.stringify({ schema: retained.schema, source: retained.source, returned: retained.returned, totalParsed: retained.totalParsed, limit: retained.limit, truncated: retained.truncated, parseErrors: retained.parseErrors, oldestTsMs: retained.oldestTsMs, newestTsMs: retained.newestTsMs, files }, null, 2))}</pre></section><section class="mc-raw-section"><h3>Retained backend operations (${rawLogs().length})</h3><pre>${escapeHtml(JSON.stringify(rawLogs(), null, 2))}</pre></section>`,
				);
				const snapshotSection = $(".mc-raw-section", raw);
				if (state.nodes.activityList) {
					const clone = state.nodes.activityList.cloneNode(true);
					clone.removeAttribute("id");
					snapshotSection.appendChild(clone);
				}
			}
		}
	}

	function setSettingsTab(tab) {
		if (
			![
				"general",
				"security",
				"discovery",
				"observability",
				"advanced",
			].includes(tab)
		)
			return;
		state.settingsTab = tab;
		const host = state.hosts.settings;
		$$("[data-mc-settings-tab]", host).forEach((button) => {
			const active = button.dataset.mcSettingsTab === tab;
			button.setAttribute("aria-selected", String(active));
			button.tabIndex = active ? 0 : -1;
		});
		$$("[data-mc-settings-panel]", host).forEach((panel) => {
			panel.hidden = panel.dataset.mcSettingsPanel !== tab;
		});
	}

	function settingsTabKeydown(event) {
		const valid = [
			"ArrowUp",
			"ArrowDown",
			"ArrowLeft",
			"ArrowRight",
			"Home",
			"End",
		];
		if (!valid.includes(event.key)) return;
		const tabs = $$(
			"[data-mc-settings-tab]",
			event.currentTarget.closest('[role="tablist"]'),
		).filter(visible);
		const current = tabs.indexOf(event.currentTarget);
		let next = current;
		if (event.key === "Home") next = 0;
		else if (event.key === "End") next = tabs.length - 1;
		else if (["ArrowDown", "ArrowRight"].includes(event.key))
			next = (current + 1) % tabs.length;
		else next = (current - 1 + tabs.length) % tabs.length;
		event.preventDefault();
		tabs[next]?.click();
		tabs[next]?.focus();
	}

	function setTheme(theme) {
		state.theme = theme;
		document.documentElement.dataset.mcTheme = theme;
		writePreference("theme", theme);
		updatePreferenceControls();
		requestAnimationFrame(syncThemeColor);
	}

	function setDetailLevel(level) {
		state.detailLevel = level === "full" ? "full" : "essential";
		if (
			state.detailLevel === "essential" &&
			["tools", "servers"].includes(state.activityView)
		) {
			state.activityView = "events";
			writePreference("activityView", state.activityView);
		}
		if (
			state.detailLevel === "essential" &&
			["observability", "advanced"].includes(state.settingsTab)
		)
			state.settingsTab = "general";
		document.documentElement.dataset.mcDetail = state.detailLevel;
		writePreference("detailLevel", state.detailLevel);
		updatePreferenceControls();
		renderAll();
		toast(
			state.detailLevel === "full"
				? "Full detail enabled"
				: "Essentials enabled",
			state.detailLevel === "full"
				? "Technical evidence and operator controls are visible."
				: "Daily tasks use plain language; technical detail remains available on demand.",
		);
	}

	function setDensity(density) {
		state.density = density;
		document.documentElement.dataset.mcDensity = density;
		writePreference("density", density);
		updatePreferenceControls();
	}

	function setTextSize(size) {
		state.textSize = size === "large" ? "large" : "normal";
		document.documentElement.dataset.mcTextSize = state.textSize;
		writePreference("textSize", state.textSize);
		updatePreferenceControls();
	}

	function setEffects(effects) {
		state.effects = effects === "minimal" ? "minimal" : "soft";
		document.documentElement.dataset.mcEffects = state.effects;
		writePreference("effects", state.effects);
		updatePreferenceControls();
	}

	function setMotion(motion) {
		state.motion = ["reduced", "off"].includes(motion) ? motion : "system";
		document.documentElement.dataset.mcMotion = state.motion;
		writePreference("motion", state.motion);
		updatePreferenceControls();
	}

	function updatePreferenceControls() {
		const host = state.hosts.settings;
		$$("[data-mc-theme]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcTheme === state.theme),
			),
		);
		$$("[data-mc-detail-level]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcDetailLevel === state.detailLevel),
			),
		);
		$$("[data-mc-density]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcDensity === state.density),
			),
		);
		$$("[data-mc-text-size]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcTextSize === state.textSize),
			),
		);
		$$("[data-mc-effects]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcEffects === state.effects),
			),
		);
		$$("[data-mc-motion]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcMotion === state.motion),
			),
		);
		$$("[data-mc-token-estimates]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcTokenEstimates === state.tokenEstimates),
			),
		);
		$$("[data-mc-path-visibility]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcPathVisibility === state.pathVisibility),
			),
		);
		$$("[data-mc-context-labels]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcContextLabels === state.contextLabels),
			),
		);
		$$("[data-mc-export-mode]", host).forEach((button) =>
			button.setAttribute(
				"aria-pressed",
				String(button.dataset.mcExportMode === state.exportMode),
			),
		);
	}

	function renderProtocolReadiness() {
		const mount = $("[data-mc-protocol-readiness]", state.hosts.settings);
		if (!mount) return;
		const servers = serverModels();
		const profiles = servers.map((server) => ({
			server,
			capability: serverCapabilityProfile(server),
			access: serverAccessProfile(server),
		}));
		const negotiated = profiles.filter(
			(item) => item.capability.protocolVersion,
		);
		const remote = profiles.filter((item) => item.access.remote);
		const remoteWithoutCredentialNames = remote.filter(
			(item) => !item.access.credentialNames.length,
		);
		const sessionVersions = unique(
			negotiated.map((item) => item.capability.protocolVersion),
		);
		setProductHtml(
			mount,
			`<header><div><span>Protocol readiness</span><h2>Stable today, migration-visible tomorrow</h2><p>This build targets the stable 2025-11-25 lifecycle. The 2026-07-28 revision is still treated as a future compatibility track, not silently enabled.</p></div><em>build target 2025-11-25</em></header><div class="mc-protocol-era-grid"><article data-tone="good"><span>Current era</span><strong>Session-based handshake</strong><small>initialize → capability negotiation → operation → shutdown</small></article><article data-tone="warn"><span>Next era</span><strong>2026-07-28 preview</strong><small>Stateless core, per-request metadata, extensions, and breaking transport changes require explicit implementation work</small></article><article data-tone="${remoteWithoutCredentialNames.length ? "warn" : remote.length ? "good" : "neutral"}"><span>Remote authorization</span><strong>${remote.length ? `${remote.length} HTTP server${remote.length === 1 ? "" : "s"}` : "No remote HTTP server"}</strong><small>${remoteWithoutCredentialNames.length ? `${remoteWithoutCredentialNames.length} lack retained credential-name evidence` : remote.length ? "Credential-name evidence exists; actual authorization still requires runtime verification" : "stdio credentials remain environment-based"}</small></article></div><section class="mc-protocol-negotiation"><header><strong>Retained negotiated versions</strong><span>${negotiated.length}/${servers.length} servers evidenced</span></header><div>${sessionVersions.length ? sessionVersions.map((version) => `<span>${ICON.check}<strong>${escapeHtml(version)}</strong><small>${profiles.filter((item) => item.capability.protocolVersion === version).length} server${profiles.filter((item) => item.capability.protocolVersion === version).length === 1 ? "" : "s"}</small></span>`).join("") : '<div class="mc-inline-empty">No upstream protocol version is retained in the current tool cache. Run Test and retain initialize evidence before drawing compatibility conclusions.</div>'}</div></section><section class="mc-migration-checklist"><header><strong>Migration checklist</strong><span>informational · no automatic upgrade</span></header><div><article><span>1</span><div><strong>Dual-era version routing</strong><small>Accept current initialize-based traffic while testing per-request modern metadata.</small></div><em>Not implemented</em></article><article><span>2</span><div><strong>Extension negotiation</strong><small>Treat Tasks and MCP Apps as separately negotiated extension surfaces.</small></div><em>Not measured</em></article><article><span>3</span><div><strong>Logging migration</strong><small>Keep local audit events and prepare OpenTelemetry-compatible export instead of relying on MCP logging.</small></div><em>Local audit ready</em></article><article><span>4</span><div><strong>Authorization hardening</strong><small>Keep remote HTTP authorization separate from stdio environment credentials and expose missing evidence.</small></div><em>${remoteWithoutCredentialNames.length ? "Review required" : "No known gap"}</em></article></div></section>`,
		);
	}

	function renderObservabilitySettings() {
		const host = state.hosts.settings;
		if (!host) return;
		const analytics = usageAnalytics();
		const quality = $("[data-mc-observability-quality]", host);
		if (quality) {
			const retained = retainedWindow();
			const fileCount = Array.isArray(retained.files)
				? retained.files.filter((file) => file.exists).length
				: 0;
			const retentionLabel =
				retained.source === "api/operations"
					? `${formatNumber(retained.returned || 0)}${retained.truncated ? ` / latest ${formatNumber(retained.limit || 0)}` : ""}`
					: `${formatNumber(retained.returned || 0)} fallback`;
			const fileRows = Array.isArray(retained.files)
				? retained.files
						.map(
							(file) =>
								`<article data-tone="${file.error || file.parseErrors ? "warn" : file.exists ? "good" : "off"}"><span>${escapeHtml(file.role || "log")}</span><code title="${escapeHtml(file.path || "")}">${escapeHtml(compactPath(file.path || "Path unavailable"))}</code><small>${file.exists ? `${formatBytes(file.bytes || 0)} · ${formatNumber(file.parsedLines || 0)} parsed${file.parseErrors ? ` · ${file.parseErrors} skipped` : ""}` : "not present"}${file.error ? ` · ${escapeHtml(file.error)}` : ""}</small></article>`,
						)
						.join("")
				: "";
			setProductHtml(
				quality,
				`<div class="mc-coverage-grid">${coverageItem("Duration", analytics.durationCoverage, `${analytics.records.filter((record) => record.totalMs !== null).length}/${analytics.records.length} audit entries`)}${coverageItem("Queue timing", analytics.records.length ? analytics.records.filter((record) => record.queueMs !== null).length / analytics.records.length : 0, "Audit entries with scheduler wait before upstream execution")}${coverageItem("Reported tokens", analytics.reportedCoverage, `${analytics.records.filter((record) => record.reportedTotalTokens !== null).length}/${analytics.records.length} entries include optional usage metadata`)}${coverageItem("Payload estimate", analytics.estimatedCoverage, "Approximation from serialized request/response size · audit-entry based")}</div><div class="mc-quality-summary"><div><span>Retained operations</span><strong>${retentionLabel}</strong></div><div><span>Log files</span><strong>${fileCount || (retained.source === "api/logs" ? 1 : 0)}</strong></div><div><span>Operation p95</span><strong>${escapeHtml(formatDuration(analytics.p95))}</strong></div><div><span>Failed calls</span><strong>${analytics.failures}</strong></div></div><section class="mc-retention-files"><header><strong>Retention sources</strong><span>${escapeHtml(retained.schema || "fallback log tail")}</span></header><div>${fileRows || `<div class="mc-inline-empty">Extended file provenance is unavailable. This backend is serving the bounded 500-entry fallback tail.</div>`}</div><footer><span>${retained.parseErrors || 0} parse errors</span><span>${retained.truncated ? "Window limited by API request" : "All parsed entries returned within the current bound"}</span></footer></section>`,
			);
		}
		const paths = $("[data-mc-data-paths]", host);
		if (paths)
			setProductHtml(
				paths,
				configurationPaths()
					.map((path) => pathRow(path.label, path.value, path.note, path.tone))
					.join("") ||
					'<div class="mc-inline-empty">Runtime paths are not available until the overview loads.</div>',
			);
		const jsonExport = $(
			'[data-mc-export-activity="json"]',
			state.hosts.activity,
		);
		const csvExport = $(
			'[data-mc-export-activity="csv"]',
			state.hosts.activity,
		);
		if (jsonExport)
			jsonExport.textContent =
				state.exportMode === "safe" ? "Export safe JSON" : "Export full JSON";
		if (csvExport)
			csvExport.textContent =
				state.exportMode === "safe" ? "Safe CSV" : "Full CSV";
		updatePreferenceControls();
	}

	function setupGuideModel(model = metrics()) {
		const tested = model.servers.filter((server) => {
			const lifecycle = serverLifecycleProfile(server);
			return (
				server.enabled &&
				lifecycle.protocolMeasured &&
				lifecycle.tools.measured &&
				serverOperationalProfile(server).tone === "good"
			);
		});
		const steps = [
			{
				id: "runtime",
				title: "Read the local runtime",
				detail: model.runtime.offline
					? "The backend overview is unavailable."
					: model.runtime.ready
						? "The local control plane is responding."
						: "MCPace is still checking runtime state.",
				done: model.runtime.ready && !model.runtime.offline,
				tone: model.runtime.offline
					? "bad"
					: model.runtime.ready
						? "good"
						: "warn",
				action: "refresh",
				label: model.runtime.offline ? "Retry" : "Refresh",
			},
			{
				id: "applications",
				title: model.clientConfigured
					? "AI application configured"
					: "Connect an AI application",
				detail: model.clientConfigured
					? "The backend reports at least one configured client route. Use Verify to confirm live client behavior."
					: model.clients.length
						? `${humanCount(model.clients.length, "AI app target")} listed. A discovered path is not treated as a verified connection.`
						: "No supported AI app target is listed yet.",
				done: model.clientConfigured,
				tone: model.clientConfigured ? "good" : "warn",
				action: "applications",
				label: model.clientConfigured
					? "Review applications"
					: "Connect application",
			},
			{
				id: "integration",
				title: "Add an MCP integration",
				detail: model.servers.length
					? `${humanCount(model.servers.length, "server")} configured.`
					: "Start from a catalog candidate, an existing config, a command, or a URL.",
				done: model.servers.length > 0,
				tone: model.servers.length ? "good" : "warn",
				action: model.servers.length ? "integrations" : "add",
				label: model.servers.length ? "Open integrations" : "Add integration",
			},
			{
				id: "evidence",
				title: "Verify tool evidence",
				detail: tested.length
					? `${humanCount(tested.length, "enabled server")} returned tool evidence.`
					: "Enable deliberately and run Test before relying on a server.",
				done: tested.length > 0,
				tone: tested.length ? "good" : "warn",
				action: "evidence",
				label: tested.length ? "Review evidence" : "Find server to test",
			},
			{
				id: "protection",
				title: "Resolve route review",
				detail: model.review
					? `${humanCount(model.review, "enabled route")} still needs testing, access review, or configuration.`
					: model.servers.length
						? "No enabled route currently needs review."
						: "Protection is evaluated after a server is added.",
				done: model.servers.length > 0 && model.review === 0,
				tone: model.review ? "warn" : model.servers.length ? "good" : "neutral",
				action: model.review ? "review" : "integrations",
				label: model.review ? "Resolve review" : "Inspect protection",
			},
		];
		return {
			steps,
			complete: steps.filter((step) => step.done).length,
			total: steps.length,
			finished: steps.every((step) => step.done),
		};
	}

	function createSetupGuideDialog() {
		const dialog = document.createElement("dialog");
		dialog.id = "mc-setup-dialog";
		dialog.className = "mc-setup-dialog";
		dialog.setAttribute("aria-labelledby", "mc-setup-title");
		setProductHtml(
			dialog,
			`<div class="mc-setup-shell"><header><div><span>Guided setup</span><h2 id="mc-setup-title">Make the first safe route</h2><p>MCPace separates configuration, enablement, verification, and client exposure so each decision stays reversible.</p></div><button type="button" class="mc-icon-button" data-mc-setup-close aria-label="Close setup guide">${ICON.close}</button></header><div class="mc-setup-progress" data-mc-setup-progress-detail></div><div class="mc-setup-steps" data-mc-setup-steps></div><footer><button type="button" class="mc-text-button" data-mc-setup-dismiss>Hide setup hint</button><button type="button" class="mc-primary-button" data-mc-setup-next>Continue setup</button></footer></div>`,
		);
		document.body.appendChild(dialog);
		state.setupDialog = dialog;
		dialog.addEventListener("cancel", (event) => {
			event.preventDefault();
			closeSetupGuide();
		});
		dialog.addEventListener("click", (event) => {
			if (event.target === dialog) closeSetupGuide();
		});
		dialog.addEventListener("keydown", (event) =>
			trapDialogFocus(event, dialog),
		);
		$("[data-mc-setup-close]", dialog)?.addEventListener(
			"click",
			closeSetupGuide,
		);
		$("[data-mc-setup-dismiss]", dialog)?.addEventListener("click", () => {
			state.setupDismissed = true;
			writePreference("setupDismissed", "true");
			closeSetupGuide();
			renderChrome();
		});
		$("[data-mc-setup-next]", dialog)?.addEventListener("click", () => {
			const next =
				setupGuideModel().steps.find((step) => !step.done) ||
				setupGuideModel().steps.at(-1);
			closeSetupGuide();
			runSetupStep(next?.action);
		});
	}

	function renderSetupGuide() {
		const dialog = state.setupDialog;
		if (!dialog) return;
		const guide = setupGuideModel();
		const progress = $("[data-mc-setup-progress-detail]", dialog);
		if (progress)
			setProductHtml(
				progress,
				`<div><span>Setup progress</span><strong>${guide.complete}/${guide.total}</strong></div><div role="progressbar" aria-label="Setup progress" aria-valuemin="0" aria-valuemax="${guide.total}" aria-valuenow="${guide.complete}"><i style="--setup-progress:${(guide.complete / guide.total) * 100}%"></i></div><small>${guide.finished ? "The basic path is ready. Continue to inspect actual activity and access as servers are used." : "Complete only the next unresolved step; advanced settings can wait."}</small>`,
			);
		const mount = $("[data-mc-setup-steps]", dialog);
		if (mount) {
			setProductHtml(
				mount,
				guide.steps
					.map(
						(step, index) =>
							`<article data-tone="${step.tone}" data-state="${step.done ? "done" : "open"}"><span>${step.done ? ICON.check : index + 1}</span><div><strong>${escapeHtml(step.title)}</strong><p>${escapeHtml(step.detail)}</p></div><button type="button" data-mc-setup-step="${escapeHtml(step.action)}">${escapeHtml(step.done ? "Review" : step.label)}${ICON.chevron}</button></article>`,
					)
					.join(""),
			);
			$$("[data-mc-setup-step]", mount).forEach((button) =>
				button.addEventListener("click", () => {
					closeSetupGuide();
					runSetupStep(button.dataset.mcSetupStep);
				}),
			);
		}
		const nextButton = $("[data-mc-setup-next]", dialog);
		if (nextButton)
			nextButton.textContent = guide.finished
				? "Review integrations"
				: "Continue setup";
	}

	function openSetupGuide() {
		if (!state.setupDialog) return;
		state.lastFocus = document.activeElement;
		renderSetupGuide();
		try {
			state.setupDialog.showModal();
		} catch (_) {
			state.setupDialog.setAttribute("open", "");
		}
		document.body.classList.add("mc-dialog-open");
		requestAnimationFrame(() =>
			$("[data-mc-setup-next]", state.setupDialog)?.focus(),
		);
	}

	function closeSetupGuide() {
		if (!state.setupDialog) return;
		try {
			state.setupDialog.close();
		} catch (_) {
			state.setupDialog.removeAttribute("open");
		}
		document.body.classList.remove("mc-dialog-open");
		state.lastFocus?.focus?.({ preventScroll: true });
	}

	function runSetupStep(action) {
		if (action === "refresh") refreshRuntime();
		else if (action === "add") openAddDialog();
		else if (action === "evidence") {
			const target =
				serverModels().find((server) => {
					const lifecycle = serverLifecycleProfile(server);
					return (
						server.enabled &&
						(!lifecycle.protocolMeasured ||
							!lifecycle.tools.measured ||
							serverOperationalProfile(server).tone !== "good")
					);
				}) || serverModels()[0];
			if (target) {
				const lifecycle = serverLifecycleProfile(target);
				openServer(
					target.name,
					lifecycle.tools.measured ? "tools" : "capabilities",
				);
			} else openAddDialog();
		} else if (action === "review") {
			state.integrationFilter = "attention";
			switchView("integrations");
		} else switchView(action || "home");
	}

	function createActionReviewDialog() {
		const dialog = document.createElement("dialog");
		dialog.id = "mc-action-review-dialog";
		dialog.className = "mc-action-review-dialog";
		dialog.setAttribute("aria-labelledby", "mc-action-review-title");
		setProductHtml(
			dialog,
			`<form method="dialog" class="mc-action-review-shell"><header><span class="mc-action-review-icon">${ICON.shield}</span><div><small>Review change</small><h2 id="mc-action-review-title">Confirm server action</h2><p data-mc-action-review-summary></p></div><button type="button" class="mc-icon-button" data-mc-action-review-cancel aria-label="Cancel action">${ICON.close}</button></header><div class="mc-action-impact" data-mc-action-impact></div><div class="mc-action-server-list" data-mc-action-server-list></div><section class="mc-action-consequence" data-mc-action-consequence></section><footer><button type="button" class="mc-secondary-button" data-mc-action-review-cancel>Cancel</button><button type="button" class="mc-primary-button" data-mc-action-review-confirm>Confirm</button></footer></form>`,
		);
		document.body.appendChild(dialog);
		state.actionReviewDialog = dialog;
		const finish = (result) => {
			const resolver = state.actionReviewResolver;
			state.actionReviewResolver = null;
			state.actionReviewContext = null;
			try {
				dialog.close();
			} catch (_) {
				dialog.removeAttribute("open");
			}
			document.body.classList.remove("mc-dialog-open");
			resolver?.(result);
		};
		$$("[data-mc-action-review-cancel]", dialog).forEach((button) =>
			button.addEventListener("click", () => finish(false)),
		);
		$("[data-mc-action-review-confirm]", dialog)?.addEventListener(
			"click",
			(event) => {
				if (event.currentTarget.disabled) return;
				finish(true);
			},
		);
		dialog.addEventListener("cancel", (event) => {
			event.preventDefault();
			finish(false);
		});
		dialog.addEventListener("click", (event) => {
			if (event.target === dialog) finish(false);
		});
		dialog.addEventListener("keydown", (event) =>
			trapDialogFocus(event, dialog),
		);
	}

	function actionReviewCopy(action, count) {
		const plural = count === 1 ? "server" : "servers";
		if (action === "remove")
			return {
				title: `Remove ${count} ${plural}?`,
				summary:
					"This deletes the saved server definition from its exact MCP settings source.",
				primary: "Remove definition",
				tone: "bad",
				consequence:
					"Removal is not the same as disabling. The definition is deleted from the source file and cannot be restored with the dashboard Undo action.",
			};
		if (action === "disable")
			return {
				title: `Disable ${count} ${plural}?`,
				summary:
					"Configured clients will stop reaching these integrations through MCPace.",
				primary: "Disable",
				tone: "bad",
				consequence:
					"Disabling changes route availability immediately. It does not delete source configuration, so the change can be reversed.",
			};
		if (action === "enable-test")
			return {
				title: `Enable and test ${count} ${plural}?`,
				summary:
					"MCPace may launch local commands or call remote endpoints, then request initialize and tools/list evidence.",
				primary: "Enable & test",
				tone: "warn",
				consequence:
					"Review source and credential names first. A successful connection does not make server-provided tool annotations trustworthy by itself.",
			};
		if (action === "test")
			return {
				title: `Test ${count} ${plural}?`,
				summary:
					"Testing may start a local process or contact a remote endpoint.",
				primary: "Run test",
				tone: "warn",
				consequence:
					"Test collects connection and capability evidence. It can still execute package startup code or send an authenticated network request.",
			};
		return {
			title: `Enable ${count} ${plural}?`,
			summary:
				"The integrations become routable through the MCPace endpoint after the backend applies the change.",
			primary: "Enable",
			tone: "warn",
			consequence:
				"Enablement changes routing state. Run Test separately when you need current tool evidence.",
		};
	}

	function requestServerActionReview(context = {}) {
		const names = unique(
			[...(Array.isArray(context.names) ? context.names : []), context.name]
				.map(String)
				.filter(Boolean),
		);
		const servers = serverModels().filter((server) =>
			names.includes(server.name),
		);
		const action = String(context.action || "enable");
		const copy = actionReviewCopy(
			action,
			Math.max(servers.length, names.length, 1),
		);
		const leases = activeLeaseModels().filter((lease) =>
			names.includes(lease.server),
		);
		const access = servers.map(serverAccessProfile);
		const remote = access.filter((item) => item.remote).length;
		const credentialBacked = access.filter(
			(item) => item.credentialNames.length,
		).length;
		const risky = servers.filter(
			(server) =>
				(server.riskCounts.bad || 0) + (server.riskCounts.warn || 0) > 0,
		).length;
		const dialog = state.actionReviewDialog;
		if (!dialog) return Promise.resolve(window.confirm?.(copy.title) !== false);
		state.lastFocus = document.activeElement;
		state.actionReviewContext = { ...context, action, names };
		$("#mc-action-review-title", dialog).textContent = copy.title;
		$("[data-mc-action-review-summary]", dialog).textContent = copy.summary;
		dialog.dataset.tone = copy.tone;
		const impact = $("[data-mc-action-impact]", dialog);
		if (impact)
			setProductHtml(
				impact,
				`<article data-tone="${remote ? "warn" : "neutral"}"><span>Remote</span><strong>${remote}</strong><small>network endpoints</small></article><article data-tone="${credentialBacked ? "warn" : "neutral"}"><span>Credentials</span><strong>${credentialBacked}</strong><small>name references</small></article><article data-tone="${risky ? "warn" : "neutral"}"><span>Risk hints</span><strong>${risky}</strong><small>servers to review</small></article><article data-tone="${leases.length ? "bad" : "good"}"><span>Active leases</span><strong>${leases.length}</strong><small>${leases.length ? "routing ownership exists" : "none observed"}</small></article>`,
			);
		const list = $("[data-mc-action-server-list]", dialog);
		if (list)
			setProductHtml(
				list,
				(servers.length
					? servers
					: names.map((name) => ({
							name,
							status: "State unavailable",
							sourceType: "",
							sourceLocation: "",
							routeMode: "",
						}))
				)
					.map(
						(server) =>
							`<article><span>${escapeHtml(initials(server.name))}</span><div><strong>${escapeHtml(server.name)}</strong><small>${escapeHtml([server.status, server.sourceType === "http" ? "HTTP" : server.sourceType || "", server.routeMode].filter(Boolean).join(" · "))}</small></div><em>${leases.filter((lease) => lease.server === server.name).length ? `${leases.filter((lease) => lease.server === server.name).length} active lease${leases.filter((lease) => lease.server === server.name).length === 1 ? "" : "s"}` : "no active lease"}</em></article>`,
					)
					.join(""),
			);
		const consequence = $("[data-mc-action-consequence]", dialog);
		const removalPlan =
			context.removalPlan && typeof context.removalPlan === "object"
				? context.removalPlan
				: {};
		const removalPath = String(
			removalPlan.path ||
				servers[0]?.sourceLocation ||
				"Source path unavailable",
		);
		const removalDetails =
			action === "remove"
				? `<div data-tone="bad">${ICON.warning}<p><strong>This is permanent from the dashboard.</strong> The saved definition will be removed from <code>${escapeHtml(removalPath)}</code>${Number.isFinite(Number(removalPlan.remainingServerCount)) ? `; ${Number(removalPlan.remainingServerCount)} other definition${Number(removalPlan.remainingServerCount) === 1 ? "" : "s"} will remain in that file` : ""}.</p></div>${leases.length ? `<div data-tone="bad">${ICON.warning}<p><strong>Active routing ownership exists.</strong> ${leases.length} current lease${leases.length === 1 ? "" : "s"} reference this integration. Disable it and let work finish before removal when possible.</p></div>` : ""}<label class="mc-destructive-confirm"><span>Type <strong>${escapeHtml(names[0] || "")}</strong> to confirm</span><input type="text" data-mc-remove-confirm autocomplete="off" spellcheck="false" aria-label="Type ${escapeHtml(names[0] || "")} to confirm removal"></label>`
				: `${leases.length && action === "disable" ? `<div data-tone="bad">${ICON.warning}<p><strong>Active work may be interrupted.</strong> ${leases.length} current lease${leases.length === 1 ? "" : "s"} reference the selected server set.</p></div>` : ""}`;
		if (consequence)
			setProductHtml(
				consequence,
				`${removalDetails}<div>${ICON.shield}<p><strong>What changes:</strong> ${escapeHtml(copy.consequence)}</p></div>`,
			);
		const confirm = $("[data-mc-action-review-confirm]", dialog);
		if (confirm) {
			confirm.textContent = copy.primary;
			confirm.className =
				copy.tone === "bad" ? "mc-danger-button" : "mc-primary-button";
			confirm.disabled = action === "remove";
		}
		const typedConfirm = $("[data-mc-remove-confirm]", dialog);
		if (typedConfirm && confirm)
			typedConfirm.addEventListener("input", () => {
				confirm.disabled = typedConfirm.value !== (names[0] || "");
			});
		try {
			dialog.showModal();
		} catch (_) {
			dialog.setAttribute("open", "");
		}
		document.body.classList.add("mc-dialog-open");
		requestAnimationFrame(() => (typedConfirm || confirm)?.focus());
		return new Promise((resolve) => {
			state.actionReviewResolver = resolve;
		});
	}

	function installServerActionHooks() {
		window.__MCPACE_PRODUCT_CONFIRM_SERVER_ACTION__ = (context) =>
			requestServerActionReview(context);
		window.__MCPACE_PRODUCT_SERVER_ACTION_RESULT__ = (context) => {
			if (!context?.ok || context.bulk || context.undo) return;
			if (context.action === "remove") {
				toast(
					`${context.name} removed`,
					"The saved definition was deleted from its MCP settings source. This action has no dashboard Undo.",
				);
				return;
			}
			if (!["enable", "enable-test", "disable"].includes(context.action))
				return;
			const inverse = context.action === "disable" ? "enable" : "disable";
			toastAction(
				context.action === "disable"
					? `${context.name} disabled`
					: `${context.name} enabled`,
				context.action === "disable"
					? "The source remains configured and can be enabled again."
					: context.action === "enable-test"
						? "The route was enabled and Test was requested."
						: "The route is available. Tool evidence still depends on Test.",
				"Undo",
				() => undoServerToggle(context.name, inverse),
				context.action === "disable" ? "warn" : "good",
			);
		};
	}

	function undoServerToggle(name, action) {
		const server = serverModels().find((item) => item.name === name);
		const control = server
			? $(".mc-inline-server-toggle", server.row) ||
				$$('[data-server-action="toggle"]', server.row)[0]
			: null;
		if (!control) {
			toast(
				"Undo unavailable",
				"The server control is no longer present. Refresh and change it from Integrations.",
			);
			return;
		}
		control.dataset.mcSkipProductConfirm = "true";
		control.dataset.mcUndoAction = "true";
		control.click();
		setTimeout(() => {
			delete control.dataset.mcSkipProductConfirm;
			delete control.dataset.mcUndoAction;
		}, 0);
	}

	function createAddDialog() {
		const dialog = document.createElement("dialog");
		dialog.id = "mc-add-dialog";
		dialog.className = "mc-add-dialog";
		dialog.setAttribute("aria-labelledby", "mc-add-title");
		setProductHtml(
			dialog,
			`<div class="mc-add-shell"><aside class="mc-add-rail"><span class="mc-add-brand">${ICON.logo}</span><div><small>ADD INTEGRATION</small><h2>One clear path.</h2><p>Choose a source, review it, then turn it on when you are ready.</p></div><ol data-mc-add-steps><li data-state="active"><i>1</i><span>Choose</span></li><li><i>2</i><span>Configure</span></li><li><i>3</i><span>Verify</span></li></ol><footer>${ICON.shield}<span>Nothing runs just because it was pasted.</span></footer></aside><section class="mc-add-content"><header><div><span>New integration</span><h2 id="mc-add-title">Add an MCP server</h2><p data-mc-add-description>Paste what you have or choose a setup path.</p></div><button type="button" class="mc-icon-button" data-mc-add-close aria-label="Close add integration">${ICON.close}</button></header><div class="mc-add-body" data-mc-add-body></div></section></div>`,
		);
		document.body.appendChild(dialog);
		state.addDialog = dialog;
		dialog.addEventListener("cancel", (event) => {
			event.preventDefault();
			closeAddDialog();
		});
		dialog.addEventListener("click", (event) => {
			if (event.target === dialog) closeAddDialog();
		});
		dialog.addEventListener("keydown", (event) =>
			trapDialogFocus(event, dialog),
		);
		$("[data-mc-add-close]", dialog)?.addEventListener("click", closeAddDialog);
	}

	function addChooserMarkup() {
		return `<section class="mc-add-chooser"><div class="mc-add-paste"><label for="mc-add-seed">${ICON.search}<input id="mc-add-seed" type="text" value="${escapeHtml(state.addSeed)}" placeholder="Paste a package, command, URL, or config file" autocomplete="off"><kbd>Enter</kbd></label><div class="mc-add-detection" role="status" aria-live="polite" data-mc-add-detection>Paste a value and MCPace will choose the right form.</div></div><div class="mc-add-methods"><button type="button" data-mc-add-method="catalog"><span>${ICON.compass}</span><div><small>FIND</small><strong>Browse servers</strong><p>Search available MCP servers and review one before adding it.</p></div>${ICON.chevron}</button><button type="button" data-mc-add-method="import"><span>${ICON.import}</span><div><small>IMPORT</small><strong>Use an existing config</strong><p>Bring in servers already configured in another AI app.</p></div>${ICON.chevron}</button><button type="button" data-mc-add-method="manual"><span>${ICON.terminal}</span><div><small>MANUAL</small><strong>Enter a command or URL</strong><p>Add a local process or a remote MCP endpoint.</p></div>${ICON.chevron}</button></div><div class="mc-safety-callout">${ICON.shield}<div><strong>You stay in control</strong><span>Adding, enabling, and testing are separate steps.<span class="mc-sr-only"> Saving a source, enabling it, and running tools/list are separate actions.</span></span></div></div></section>`;
	}

	function detectAddMethod(value) {
		const raw = String(value || "").trim();
		if (!raw)
			return {
				method: "",
				label:
					"Paste a value and MCPace will suggest a setup path. You can always choose another option below.",
				tone: "neutral",
			};
		if (
			/\.json(?:$|\s)|claude_desktop_config|mcp\.json|mcp_settings/i.test(
				raw,
			) &&
			!/^https?:\/\//i.test(raw)
		)
			return {
				method: "import",
				label:
					"Likely configuration file. Import preview will open; choose another option below if this is only a search phrase.",
				tone: "good",
			};
		if (/^https?:\/\//i.test(raw))
			return {
				method: "manual",
				label:
					"Likely remote MCP URL. Network access and credential names will be reviewed before enablement.",
				tone: "warn",
			};
		const launcher =
			/^(?:npx|pnpm\s+(?:dlx|exec)|yarn\s+dlx|bunx|uvx|python3?|node|deno|docker|podman|cargo\s+run|dotnet|java)(?:\s|$)/i.test(
				raw,
			);
		const pathLike = /^(?:\.\.?[/]|~[/]|[/]|[A-Za-z]:[\\/])/.test(raw);
		const shellSyntax = /(?:&&|\|\||[;<>]|\$\(|`)/.test(raw);
		const executableWithFlag = /^[^\s]+\s+--?[A-Za-z0-9][\w-]*/.test(raw);
		if (launcher || pathLike || shellSyntax || executableWithFlag)
			return {
				method: "manual",
				label:
					"Likely launch command or executable path. It will be copied into the manual form; choose Browse servers to search instead.",
				tone: "warn",
			};
		return {
			method: "catalog",
			label:
				"Likely package name or search phrase. Catalog search will open; choose Command or URL if this is executable input.",
			tone: "good",
		};
	}

	function openAddDialog(initialMethod = "") {
		if (!state.addDialog) return;
		state.lastFocus = document.activeElement;
		state.addMethod = "";
		renderAddChooser();
		try {
			state.addDialog.showModal();
		} catch (_) {
			state.addDialog.setAttribute("open", "");
		}
		document.body.classList.add("mc-dialog-open");
		if (initialMethod) showAddMethod(initialMethod, state.addSeed);
		else
			requestAnimationFrame(() => $("#mc-add-seed", state.addDialog)?.focus());
	}

	function renderAddChooser() {
		restoreWizardPanel();
		state.addMethod = "";
		const body = $("[data-mc-add-body]", state.addDialog);
		if (!body) return;
		setProductHtml(body, addChooserMarkup());
		$("[data-mc-add-description]", state.addDialog).textContent =
			"Paste what you have or choose a setup path.";
		updateAddSteps(1);
		const seed = $("#mc-add-seed", body);
		const detection = $("[data-mc-add-detection]", body);
		const update = () => {
			state.addSeed = seed.value;
			const result = detectAddMethod(seed.value);
			detection.dataset.tone = result.tone;
			detection.textContent = result.label;
		};
		seed.addEventListener("input", update);
		seed.addEventListener("keydown", (event) => {
			if (event.key !== "Enter") return;
			event.preventDefault();
			const result = detectAddMethod(seed.value);
			if (result.method) showAddMethod(result.method, seed.value);
		});
		$$("[data-mc-add-method]", body).forEach((button) =>
			button.addEventListener("click", () =>
				showAddMethod(button.dataset.mcAddMethod, seed.value),
			),
		);
		update();
	}

	function addMethodInfo(method, seed) {
		const detected = detectAddMethod(seed);
		if (method === "import")
			return {
				title: "Import existing configuration",
				description:
					"Preview a local JSON file and inspect the server diff before any write.",
				icon: ICON.import,
				review:
					"The imported file is read locally. Secret values are not displayed, and servers stay disabled by default.",
			};
		if (method === "manual" && /^https?:\/\//i.test(String(seed || "").trim()))
			return {
				title: "Connect a remote endpoint",
				description:
					"Save a Streamable HTTP URL, then review network access and authentication.",
				icon: ICON.terminal,
				review:
					"Remote servers can receive data sent by connected tools. Confirm origin, credentials, and intended access before enabling.",
			};
		if (method === "manual")
			return {
				title: "Add a launch command",
				description:
					"Save one stdio launcher or local executable, then test it deliberately.",
				icon: ICON.terminal,
				review:
					"A local server can execute with your user permissions. Review the command, working directory, environment names, and package source.",
			};
		return {
			title: "Explore catalog",
			description:
				"Search candidates first. Installation remains an explicit follow-up action.",
			icon: ICON.compass,
			review:
				detected.method === "catalog" && seed
					? `Search will start with “${seed.trim()}”. Review the resolved package and launch command before installation.`
					: "Catalog results are candidates, not trusted code. Review origin and resolved launch details before installation.",
		};
	}

	function addExecutionPlan(method, seed = "") {
		const serverInventory = configurationPaths().find(
			(path) => path.label === "Server inventory",
		);
		const target = serverInventory?.value || "mcp_settings.json";
		const value = String(seed || "").trim();
		const plans = {
			catalog: [
				[
					"Search",
					value
						? `Query “${value}” in configured catalogs`
						: "Search configured catalogs and registry sources",
				],
				[
					"Review",
					"Inspect resolved package, command, origin, and requested access",
				],
				["Save", `Write only after confirmation to ${compactPath(target)}`],
				["Activate", "Keep enablement and Test as explicit follow-up actions"],
			],
			import: [
				[
					"Read",
					value
						? `Read ${compactPath(value)} locally`
						: "Choose a local MCP client JSON file",
				],
				["Preview", "Show additions and replacements before writing"],
				[
					"Import",
					`Store reviewed server definitions in ${compactPath(target)}`,
				],
				["Activate", "Review every imported server before enable and Test"],
			],
			manual: [
				[
					"Define",
					/^https?:\/\//i.test(value)
						? "Validate the remote endpoint and transport"
						: "Validate the stdio command and arguments",
				],
				[
					"Secrets",
					"Store credential names only; values stay in environment or headers",
				],
				["Save", `Write the source definition to ${compactPath(target)}`],
				[
					"Verify",
					"Enable deliberately, initialize, then collect tools/list evidence",
				],
			],
		};
		return `<section class="mc-add-execution-plan"><header><span>What MCPace will do</span><strong>${escapeHtml(compactPath(target))}</strong></header><div>${(plans[method] || plans.catalog).map(([title, detail], index) => `<article><span>${index + 1}</span><div><strong>${escapeHtml(title)}</strong><small>${escapeHtml(detail)}</small></div>${index < 3 ? "<i></i>" : ""}</article>`).join("")}</div></section>`;
	}

	function wizardPanelFor(method) {
		return method === "import"
			? state.nodes.importPanel
			: method === "manual"
				? state.nodes.manualPanel
				: state.nodes.discoverPanel;
	}

	function wizardInputFor(method) {
		return method === "import"
			? state.nodes.importInput
			: method === "manual"
				? state.nodes.manualInput
				: state.nodes.discoverInput;
	}

	function wizardResultFor(method) {
		return method === "import"
			? state.nodes.importResult
			: method === "catalog"
				? state.nodes.discoverResult
				: state.nodes.manualResult;
	}

	function showAddMethod(method, seed = "") {
		restoreWizardPanel();
		state.addMethod = method;
		state.addSeed = seed || state.addSeed;
		const info = addMethodInfo(method, state.addSeed);
		const body = $("[data-mc-add-body]", state.addDialog);
		if (!body) return;
		setProductHtml(
			body,
			`<section class="mc-add-configure"><div class="mc-add-step-header"><button type="button" class="mc-icon-button" data-mc-add-back aria-label="Back to setup methods">${ICON.back}</button><span class="mc-add-step-icon">${info.icon}</span><div><small>STEP 2 · CONFIGURE</small><h3>${escapeHtml(info.title)}</h3><p>${escapeHtml(info.description)}</p></div></div><details class="mc-add-change-plan" ${state.detailLevel === "full" ? "open" : ""}><summary>${ICON.shield}<span><strong>What will change</strong><small>Review source, storage, access, and the next verification step</small></span>${ICON.chevron}</summary><div class="mc-add-change-plan-body"><div class="mc-add-review">${ICON.shield}<div><strong>Review boundary</strong><p>${escapeHtml(info.review)}</p></div></div>${addExecutionPlan(method, state.addSeed)}</div></details><div class="mc-add-panel-mount" data-mc-wizard-mount></div><div class="mc-add-next" data-mc-add-next hidden></div></section>`,
		);
		$("[data-mc-add-description]", state.addDialog).textContent =
			info.description;
		$("[data-mc-add-back]", body)?.addEventListener("click", () => {
			renderAddChooser();
			requestAnimationFrame(() => $("#mc-add-seed", state.addDialog)?.focus());
		});
		updateAddSteps(2);

		const panel = wizardPanelFor(method);
		const mount = $("[data-mc-wizard-mount]", body);
		if (!panel || !mount) {
			setProductHtml(
				mount,
				'<div class="mc-large-empty"><strong>This setup path is unavailable</strong><span>The current backend build does not expose the required form.</span></div>',
			);
			return;
		}
		const placeholder = document.createComment(`mcpace-${method}-placeholder`);
		panel.parentNode?.insertBefore(placeholder, panel);
		state.movedWizard = panel;
		state.movedWizardPlaceholder = placeholder;
		state.movedWizardSemantics = {
			role: panel.getAttribute("role"),
			labelledBy: panel.getAttribute("aria-labelledby"),
			label: panel.getAttribute("aria-label"),
		};
		panel.hidden = false;
		panel.removeAttribute("aria-hidden");
		panel.setAttribute("role", "region");
		panel.removeAttribute("aria-labelledby");
		panel.setAttribute("aria-label", info.title);
		mount.appendChild(panel);

		const input = wizardInputFor(method);
		if (input && state.addSeed.trim()) {
			input.value = state.addSeed.trim();
			input.dispatchEvent(new Event("input", { bubbles: true }));
		}
		monitorWizardResult(method);
		requestAnimationFrame(() =>
			(input || $("input,select,textarea,button", panel))?.focus(),
		);
	}

	function monitorWizardResult(method) {
		const result = wizardResultFor(method);
		if (!result) return;
		const update = () => {
			const value = text(result);
			const next = $("[data-mc-add-next]", state.addDialog);
			if (!next) return;
			const failed = /failed|error|rejected|invalid/i.test(value);
			const completed =
				!failed &&
				/will add|will replace|candidate|saved|installed|imported|preview|ready|success|added/i.test(
					value,
				) &&
				!/no .* yet|idle|waiting/i.test(value);
			if (!completed && !failed) return;
			next.hidden = false;
			if (failed) {
				next.dataset.tone = "bad";
				setProductHtml(
					next,
					`${ICON.warning}<div><strong>Review the error above</strong><span>No successful setup step is assumed.</span></div>`,
				);
			} else {
				updateAddSteps(3);
				next.dataset.tone = "good";
				setProductHtml(
					next,
					`${ICON.check}<div><strong>${method === "catalog" ? "Candidate data is ready" : "Configuration step completed"}</strong><span>Next: open Integrations, review the source, then enable and run Test when appropriate.</span></div><button type="button" class="mc-secondary-button" data-mc-open-integrations>Open integrations</button>`,
				);
				$("[data-mc-open-integrations]", next)?.addEventListener(
					"click",
					() => {
						closeAddDialog();
						switchView("integrations");
					},
				);
			}
		};
		update();
		const observer = new MutationObserver(update);
		observer.observe(result, {
			childList: true,
			subtree: true,
			characterData: true,
		});
		state.wizardObserver?.disconnect();
		state.wizardObserver = observer;
	}

	function updateAddSteps(activeStep) {
		$$("[data-mc-add-steps] li", state.addDialog).forEach((item, index) => {
			const step = index + 1;
			item.dataset.state =
				step < activeStep ? "done" : step === activeStep ? "active" : "pending";
		});
	}

	function restoreWizardPanel() {
		state.wizardObserver?.disconnect();
		state.wizardObserver = null;
		if (state.movedWizard && state.movedWizardPlaceholder?.parentNode) {
			const semantics = state.movedWizardSemantics;
			const restoreAttribute = (name, value) => {
				if (value === null) state.movedWizard.removeAttribute(name);
				else state.movedWizard.setAttribute(name, value);
			};
			restoreAttribute("role", semantics?.role ?? null);
			restoreAttribute("aria-labelledby", semantics?.labelledBy ?? null);
			restoreAttribute("aria-label", semantics?.label ?? null);
			state.movedWizardPlaceholder.parentNode.insertBefore(
				state.movedWizard,
				state.movedWizardPlaceholder,
			);
			state.movedWizardPlaceholder.remove();
		}
		state.movedWizard = null;
		state.movedWizardPlaceholder = null;
		state.movedWizardSemantics = null;
	}

	function closeAddDialog() {
		restoreWizardPanel();
		if (!state.addDialog) return;
		try {
			state.addDialog.close();
		} catch (_) {
			state.addDialog.removeAttribute("open");
		}
		document.body.classList.remove("mc-dialog-open");
		state.lastFocus?.focus?.({ preventScroll: true });
	}

	function createEventDetailDialog() {
		const dialog = document.createElement("dialog");
		dialog.id = "mc-event-detail-dialog";
		dialog.className = "mc-event-detail-dialog";
		dialog.setAttribute("aria-labelledby", "mc-event-detail-title");
		setProductHtml(
			dialog,
			`<div class="mc-event-detail-shell"><header><div><span>Operation trace</span><h2 id="mc-event-detail-title">Event detail</h2><p data-mc-event-detail-subtitle>Retained backend evidence</p></div><button type="button" class="mc-icon-button" data-mc-event-detail-close aria-label="Close event detail">${ICON.close}</button></header><div class="mc-event-detail-body" data-mc-event-detail-body></div><footer><button type="button" class="mc-text-button" data-mc-copy-event>Copy JSON</button><button type="button" class="mc-secondary-button" data-mc-event-server hidden>Open integration</button><button type="button" class="mc-primary-button" data-mc-event-detail-done>Done</button></footer></div>`,
		);
		document.body.appendChild(dialog);
		state.eventDetailDialog = dialog;
		dialog.addEventListener("cancel", (event) => {
			event.preventDefault();
			closeEventDetail();
		});
		dialog.addEventListener("click", (event) => {
			if (event.target === dialog) closeEventDetail();
		});
		dialog.addEventListener("keydown", (event) =>
			trapDialogFocus(event, dialog),
		);
		$("[data-mc-event-detail-close]", dialog)?.addEventListener(
			"click",
			closeEventDetail,
		);
		$("[data-mc-event-detail-done]", dialog)?.addEventListener(
			"click",
			closeEventDetail,
		);
		$("[data-mc-copy-event]", dialog)?.addEventListener("click", () => {
			const event = activityModels().find(
				(item) => item.id === state.eventDetailId,
			);
			if (event?.payload) copyText(event.payload, "Event JSON");
		});
		$("[data-mc-event-server]", dialog)?.addEventListener("click", (event) => {
			const name = event.currentTarget.dataset.mcEventServer;
			closeEventDetail();
			if (name) openServer(name, "events");
		});
	}

	function eventTimelineMarkup(audit) {
		const queue = audit.queueMs;
		const upstream = audit.upstreamMs;
		const total = audit.totalMs;
		const overhead =
			total !== null && upstream !== null
				? Math.max(0, total - upstream - (queue || 0))
				: null;
		const values = [queue, upstream, overhead].map((value) =>
			finiteNumber(value, 0),
		);
		const maximum = Math.max(1, ...values);
		return `<div class="mc-event-timeline-detail"><article><span>Queue</span><div><i style="--event-width:${clamp((values[0] / maximum) * 100, values[0] ? 4 : 0, 100)}%"></i></div><strong>${escapeHtml(formatDuration(queue))}</strong></article><article><span>Upstream</span><div><i style="--event-width:${clamp((values[1] / maximum) * 100, values[1] ? 4 : 0, 100)}%"></i></div><strong>${escapeHtml(formatDuration(upstream))}</strong></article><article><span>Bridge overhead</span><div><i style="--event-width:${clamp((values[2] / maximum) * 100, values[2] ? 4 : 0, 100)}%"></i></div><strong>${escapeHtml(formatDuration(overhead))}</strong></article></div>`;
	}

	function eventFact(label, value, note = "") {
		const display =
			value === null || value === undefined || value === ""
				? "not recorded"
				: String(value);
		return `<article><span>${escapeHtml(label)}</span><strong>${escapeHtml(display)}</strong>${note ? `<small>${escapeHtml(note)}</small>` : ""}</article>`;
	}

	function openEventDetail(id) {
		const dialog = state.eventDetailDialog;
		const event = activityModels().find((item) => item.id === id);
		if (!dialog || !event) return;
		state.lastFocus = document.activeElement;
		state.eventDetailId = id;
		$("#mc-event-detail-title", dialog).textContent = event.title;
		$("[data-mc-event-detail-subtitle]", dialog).textContent =
			`${event.source} · ${formatRelativeTimestamp(event.timestamp)}`;
		const serverButton = $("[data-mc-event-server]", dialog);
		if (serverButton) {
			serverButton.hidden = !event.audit?.server;
			serverButton.dataset.mcEventServer = event.audit?.server || "";
			serverButton.textContent = event.audit?.server
				? `Open ${event.audit.server}`
				: "Open integration";
		}
		const body = $("[data-mc-event-detail-body]", dialog);
		if (event.audit) {
			const audit = event.audit;
			const totalTokens =
				audit.reportedTotalTokens !== null
					? `${formatNumber(audit.reportedTotalTokens)} reported`
					: audit.estimatedTotalTokens !== null &&
							state.tokenEstimates === "show"
						? `≈ ${formatNumber(audit.estimatedTotalTokens)} payload estimate`
						: "not reported";
			setProductHtml(
				body,
				`<section class="mc-event-outcome" data-tone="${event.tone}"><span>${event.type === "error" ? ICON.warning : ICON.check}</span><div><small>${escapeHtml(audit.outcome.replace(/_/g, " "))}</small><h3>${escapeHtml(audit.server)} → ${escapeHtml(audit.toolLabel)}</h3><p>${escapeHtml(event.meta || "No additional detail recorded.")}</p></div><em>${escapeHtml(audit.failureStage)}</em></section><section class="mc-event-detail-section"><header><span>Latency path</span><strong>${escapeHtml(formatDuration(audit.totalMs))} total</strong></header>${eventTimelineMarkup(audit)}</section><section class="mc-event-facts">${eventFact("Call ID", audit.callId, audit.auditSchema)}${eventFact("Request kind", audit.requestKind)}${eventFact("Outcome", audit.outcome, audit.errorKind)}${eventFact("Transport", audit.transport || "not recorded")}${eventFact("Client", state.contextLabels === "show" ? audit.clientId : audit.clientId ? "hidden locally" : "")}${eventFact("Session", state.contextLabels === "show" ? audit.sessionId : audit.sessionId ? "hidden locally" : "")}${eventFact("Project", state.contextLabels === "show" ? compactPath(audit.projectRoot) : audit.projectRoot ? "hidden locally" : "")}${eventFact("Payload", `${formatBytes(audit.requestBytes)} in · ${formatBytes(audit.responseBytes)} out`)}${eventFact("Tokens", totalTokens, audit.tokenUsageSource || audit.estimateMethod)}${eventFact("Lease", audit.leaseId || "", audit.pooled ? "reused session" : "")}</section>${audit.error ? `<section class="mc-event-error"><strong>Error</strong><pre>${escapeHtml(audit.error)}</pre></section>` : ""}${audit.trace ? `<section class="mc-event-detail-section"><header><span>Trace</span></header><pre>${escapeHtml(audit.trace)}</pre></section>` : ""}<details class="mc-event-raw"><summary>Raw audit event</summary><pre>${escapeHtml(event.payload || "")}</pre></details>`,
			);
		} else {
			setProductHtml(
				body,
				`<section class="mc-event-outcome" data-tone="${event.tone}"><span>${event.type === "error" ? ICON.warning : ICON.activity}</span><div><small>${escapeHtml(event.chip)}</small><h3>${escapeHtml(event.title)}</h3><p>${escapeHtml(event.meta || "No additional metadata in this retained event.")}</p></div><em>${escapeHtml(formatRelativeTimestamp(event.timestamp))}</em></section><section class="mc-event-facts">${eventFact("Source", event.source)}${eventFact("Type", event.type)}${eventFact("Timestamp", event.timestamp ? new Date(event.timestamp).toISOString() : "")}</section><details class="mc-event-raw" open><summary>Raw backend event</summary><pre>${escapeHtml(event.payload || "No raw payload was retained.")}</pre></details>`,
			);
		}
		try {
			dialog.showModal();
		} catch (_) {
			dialog.setAttribute("open", "");
		}
		document.body.classList.add("mc-dialog-open");
		requestAnimationFrame(() =>
			$("[data-mc-event-detail-close]", dialog)?.focus(),
		);
	}

	function closeEventDetail() {
		if (!state.eventDetailDialog) return;
		try {
			state.eventDetailDialog.close();
		} catch (_) {
			state.eventDetailDialog.removeAttribute("open");
		}
		document.body.classList.remove("mc-dialog-open");
		state.eventDetailId = null;
		state.lastFocus?.focus?.({ preventScroll: true });
	}

	function csvCell(value) {
		const source = value === null || value === undefined ? "" : String(value);
		const formulaCapable =
			/^[\u0000-\u0020]*[=+\-@]/.test(source) || /^[\t\r\n]/.test(source);
		const neutralized = formulaCapable ? `'${source}` : source;
		return `"${neutralized.replace(/"/g, '""')}"`;
	}

	function downloadText(name, content, type) {
		const blob = new Blob([content], { type });
		const url = URL.createObjectURL(blob);
		const link = document.createElement("a");
		link.href = url;
		link.download = name;
		document.body.appendChild(link);
		link.click();
		link.remove();
		setTimeout(() => URL.revokeObjectURL(url), 0);
	}

	function exportAuditEnvelope(records, mode = state.exportMode) {
		if (mode === "full") return records.map((record) => record.raw);
		const aliases = {
			client: new Map(),
			project: new Map(),
			session: new Map(),
			lease: new Map(),
		};
		const alias = (kind, value) => {
			const source = String(value || "");
			if (!source) return "";
			const map = aliases[kind];
			if (!map.has(source)) map.set(source, `${kind}-${map.size + 1}`);
			return map.get(source);
		};
		return records.map((record) => ({
			auditSchema: record.auditSchema,
			callId: record.callId,
			timestampMs: record.timestamp,
			requestKind: record.requestKind,
			server: record.server,
			tools: [...record.tools],
			callCount: record.callCount,
			successCount: record.successCount,
			failedCount: record.failedCount,
			outcome: record.outcome,
			errorKind: record.errorKind,
			failureStage: record.failureStage,
			bridgeOk: record.bridgeOk,
			upstreamOk: record.upstreamOk,
			queueDurationMs: record.queueMs,
			upstreamDurationMs: record.upstreamMs,
			totalDurationMs: record.totalMs,
			requestBytes: record.requestBytes,
			responseBytes: record.responseBytes,
			reportedInputTokens: record.reportedInputTokens,
			reportedOutputTokens: record.reportedOutputTokens,
			reportedTotalTokens: record.reportedTotalTokens,
			estimatedInputTokens: record.estimatedInputTokens,
			estimatedOutputTokens: record.estimatedOutputTokens,
			estimatedTotalTokens: record.estimatedTotalTokens,
			tokenUsageSource: record.tokenUsageSource,
			tokenEstimateMethod: record.estimateMethod,
			clientAlias: alias("client", record.clientId),
			projectAlias: alias("project", record.projectRoot),
			sessionAlias: alias("session", record.sessionId),
			leaseAlias: alias("lease", record.leaseId),
			transport: record.transport,
			pooled: record.pooled,
			redactedFields: [
				"clientId",
				"projectRoot",
				"sessionId",
				"leaseId",
				"trace",
				"error",
				"raw",
			],
		}));
	}

	function exportRetentionEnvelope(retained, mode = state.exportMode) {
		return {
			schema: retained.schema,
			source: retained.source,
			returned: retained.returned,
			totalParsed: retained.totalParsed,
			limit: retained.limit,
			truncated: retained.truncated,
			parseErrors: retained.parseErrors,
			oldestTsMs: retained.oldestTsMs,
			newestTsMs: retained.newestTsMs,
			files: (retained.files || []).map((file) =>
				mode === "full"
					? file
					: {
							role: file.role,
							exists: file.exists,
							bytes: file.bytes,
							parsedLines: file.parsedLines,
							parseErrors: file.parseErrors,
							error: file.error ? "present" : "",
						},
			),
		};
	}

	function exportActivity(format = "json") {
		const records = rangedAuditRecords();
		const retained = retainedWindow();
		const generatedAt = new Date().toISOString();
		const exported = exportAuditEnvelope(records);
		if (format === "csv") {
			const headers =
				state.exportMode === "full"
					? [
							"callId",
							"timestamp",
							"server",
							"tool",
							"outcome",
							"errorKind",
							"failureStage",
							"clientId",
							"projectRoot",
							"sessionId",
							"leaseId",
							"queueMs",
							"upstreamMs",
							"totalMs",
							"requestBytes",
							"responseBytes",
							"reportedTotalTokens",
							"estimatedTotalTokens",
						]
					: [
							"callId",
							"timestampMs",
							"server",
							"tool",
							"outcome",
							"errorKind",
							"failureStage",
							"clientAlias",
							"projectAlias",
							"sessionAlias",
							"leaseAlias",
							"queueDurationMs",
							"upstreamDurationMs",
							"totalDurationMs",
							"requestBytes",
							"responseBytes",
							"reportedTotalTokens",
							"estimatedTotalTokens",
						];
			const rows =
				state.exportMode === "full"
					? records.flatMap((record) =>
							record.tools.map((tool) => ({ ...record, tool })),
						)
					: exported.flatMap((record) =>
							record.tools.map((tool) => ({ ...record, tool })),
						);
			const csv = [
				headers.join(","),
				...rows.map((record) =>
					headers.map((header) => csvCell(record[header])).join(","),
				),
			].join("\n");
			downloadText(
				`mcpace-operations-${state.exportMode}-${Date.now()}.csv`,
				csv,
				"text/csv;charset=utf-8",
			);
			toast(
				`${state.exportMode === "safe" ? "Privacy-safe " : ""}CSV exported`,
				`${rows.length} retained tool-operation rows.`,
			);
			return;
		}
		const payload = {
			schema:
				state.exportMode === "safe"
					? "mcpace.activityExport.safe.v2"
					: "mcpace.activityExport.full.v2",
			generatedAt,
			exportMode: state.exportMode,
			selectedRange: state.activityRange,
			privacy:
				state.exportMode === "safe"
					? {
							aliasesAreExportLocal: true,
							rawPayloadExcluded: true,
							exactContextLabelsExcluded: true,
							errorAndTraceTextExcluded: true,
						}
					: { containsRawLocalAuditValues: true },
			retention: exportRetentionEnvelope(retained),
			audits: exported,
		};
		downloadText(
			`mcpace-operations-${state.exportMode}-${Date.now()}.json`,
			JSON.stringify(payload, null, 2),
			"application/json;charset=utf-8",
		);
		toast(
			`${state.exportMode === "safe" ? "Privacy-safe " : "Full "}JSON exported`,
			`${records.length} retained audit entries with retention metadata.`,
		);
	}

	function createCommandDialog() {
		const dialog = document.createElement("dialog");
		dialog.id = "mc-command-dialog";
		dialog.className = "mc-command-dialog";
		dialog.setAttribute("aria-labelledby", "mc-command-title");
		setProductHtml(
			dialog,
			`<div class="mc-command-shell"><header><label for="mc-command-input">${ICON.search}<span class="mc-sr-only" id="mc-command-title">Command center</span><input id="mc-command-input" type="search" placeholder="Search integrations, tools, and actions" autocomplete="off"><kbd>Esc</kbd></label><button type="button" data-mc-command-close aria-label="Close command center">Close</button></header><div class="mc-command-results" data-mc-command-results></div><footer><span><kbd>↑</kbd><kbd>↓</kbd> Navigate</span><span><kbd>Enter</kbd> Run</span><strong>MCPace</strong></footer></div>`,
		);
		document.body.appendChild(dialog);
		state.commandDialog = dialog;
		dialog.addEventListener("cancel", (event) => {
			event.preventDefault();
			closeCommandDialog();
		});
		dialog.addEventListener("click", (event) => {
			if (event.target === dialog) closeCommandDialog();
		});
		dialog.addEventListener("keydown", (event) => {
			trapDialogFocus(event, dialog);
			if (!event.defaultPrevented) commandDialogKeydown(event);
		});
		$("#mc-command-input", dialog)?.addEventListener("input", (event) =>
			renderCommandResults(event.target.value),
		);
		$("[data-mc-command-close]", dialog)?.addEventListener(
			"click",
			closeCommandDialog,
		);
	}

	function commandItems() {
		const items = [
			{
				id: "add",
				group: "Actions",
				label: "Add integration",
				hint: "Catalog, import, command, or URL",
				icon: ICON.plus,
				run: () => {
					closeCommandDialog();
					openAddDialog();
				},
			},
			{
				id: "setup",
				group: "Actions",
				label: "Open setup guide",
				hint: "Follow the next unresolved step from runtime to verified tools",
				icon: ICON.shield,
				run: () => {
					closeCommandDialog();
					openSetupGuide();
				},
			},
			...(liveSessionModels().length || activeLeaseModels().length
				? [
						{
							id: "live",
							group: "Observe",
							label: "Open live sessions",
							hint: `${liveSessionModels().length} sessions · ${activeLeaseModels().length} leases`,
							icon: ICON.activity,
							run: () => {
								closeCommandDialog();
								state.activityView = "live";
								writePreference("activityView", "live");
								switchView("activity");
							},
						},
					]
				: []),
			{
				id: "refresh",
				group: "Actions",
				label: "Refresh runtime",
				hint: "Request current overview and logs",
				icon: ICON.refresh,
				run: () => {
					closeCommandDialog();
					refreshRuntime();
				},
			},
			...(state.nodes.updateCheckButton
				? [
						{
							id: "updates",
							group: "Actions",
							label: "Check for updates",
							hint: "Open maintenance and request the cached update check",
							icon: ICON.refresh,
							run: () => {
								closeCommandDialog();
								switchView("settings");
								setSettingsTab("general");
								state.nodes.updateCheckButton.click();
							},
						},
					]
				: []),
			...(state.nodes.accessReview
				? [
						{
							id: "security-review",
							group: "Navigate",
							label: "Open security review",
							hint: "Approval, secrets, remote access, and tool evidence",
							icon: ICON.shield,
							run: () => {
								closeCommandDialog();
								switchView("settings");
								setSettingsTab("security");
							},
						},
					]
				: []),
			...([
				state.nodes.repairButton,
				state.nodes.startButton,
				state.nodes.stopButton,
			].some(Boolean)
				? [
						{
							id: "runtime-maintenance",
							group: "Navigate",
							label: "Open runtime maintenance",
							hint: "Repair, start, or stop the optional local hub",
							icon: ICON.settings,
							run: () => {
								closeCommandDialog();
								switchView("settings");
								setSettingsTab("advanced");
							},
						},
					]
				: []),
			...Object.entries(VIEW_META).map(([id, [label, hint]]) => ({
				id: `view-${id}`,
				group: "Navigate",
				label: `Open ${label}`,
				hint,
				icon:
					id === "home"
						? ICON.home
						: id === "integrations"
							? ICON.server
							: id === "applications"
								? ICON.apps
								: id === "activity"
									? ICON.activity
									: ICON.settings,
				run: () => {
					closeCommandDialog();
					switchView(id);
				},
			})),
			{
				id: "shortcuts",
				group: "Help",
				label: "Keyboard shortcuts",
				hint: "⌘/Ctrl+K search · Alt+/ contextual search · Alt+1–5 navigate",
				icon: ICON.terminal,
				run: () => {
					closeCommandDialog();
					toast(
						"Keyboard shortcuts",
						"⌘/Ctrl+K opens search. Alt+/ focuses integration search there, or opens the command center from another section. Alt+1–5 switches sections.",
					);
				},
			},
		];
		serverModels().forEach((server) =>
			items.push({
				id: `server-${server.name}`,
				group: "Integrations",
				label: server.name,
				hint: `${server.status} · ${server.toolCount} tools · ${server.routeMode}`,
				icon: `<span>${escapeHtml(server.initials)}</span>`,
				searchable: server.searchable,
				run: () => {
					closeCommandDialog();
					openServer(server.name, "overview");
				},
			}),
		);
		return items;
	}

	function renderCommandResults(query = "") {
		const mount = $("[data-mc-command-results]", state.commandDialog);
		if (!mount) return;
		const needle = query.trim().toLowerCase();
		const items = commandItems().filter(
			(item) =>
				!needle ||
				`${item.label} ${item.hint} ${item.group} ${item.searchable || ""}`
					.toLowerCase()
					.includes(needle),
		);
		state.commandActions = items;
		const groups = unique(items.map((item) => item.group));
		setProductHtml(
			mount,
			groups.length
				? groups
						.map(
							(group) =>
								`<section><h2>${escapeHtml(group)}</h2>${items
									.filter((item) => item.group === group)
									.map(
										(item) =>
											`<button type="button" data-mc-command-id="${escapeHtml(item.id)}"><span class="mc-command-icon">${item.icon}</span><span><strong>${escapeHtml(item.label)}</strong><small>${escapeHtml(item.hint)}</small></span><kbd>↵</kbd></button>`,
									)
									.join("")}</section>`,
						)
						.join("")
				: `<div class="mc-large-empty">${ICON.search}<strong>No matching command</strong><span>Try a server name, tool name, or section.</span></div>`,
		);
		$$("[data-mc-command-id]", mount).forEach((button) =>
			button.addEventListener("click", () =>
				items.find((item) => item.id === button.dataset.mcCommandId)?.run(),
			),
		);
	}

	function openCommandDialog() {
		state.lastFocus = document.activeElement;
		renderCommandResults("");
		const input = $("#mc-command-input", state.commandDialog);
		if (input) input.value = "";
		try {
			state.commandDialog.showModal();
		} catch (_) {
			state.commandDialog.setAttribute("open", "");
		}
		document.body.classList.add("mc-dialog-open");
		requestAnimationFrame(() => input?.focus());
	}

	function closeCommandDialog() {
		if (!state.commandDialog) return;
		try {
			state.commandDialog.close();
		} catch (_) {
			state.commandDialog.removeAttribute("open");
		}
		document.body.classList.remove("mc-dialog-open");
		state.lastFocus?.focus?.({ preventScroll: true });
	}

	function commandDialogKeydown(event) {
		if (!["ArrowDown", "ArrowUp", "Home", "End", "Enter"].includes(event.key))
			return;
		const buttons = $$("[data-mc-command-id]", state.commandDialog);
		if (!buttons.length) return;
		const input = $("#mc-command-input", state.commandDialog);
		const current = buttons.indexOf(document.activeElement);
		if (event.key === "Enter" && document.activeElement === input) {
			event.preventDefault();
			buttons[0]?.click();
			return;
		}
		if (event.key === "Enter") return;
		event.preventDefault();
		let next = current;
		if (event.key === "Home") next = 0;
		else if (event.key === "End") next = buttons.length - 1;
		else if (event.key === "ArrowDown")
			next = current < 0 ? 0 : (current + 1) % buttons.length;
		else
			next =
				current < 0
					? buttons.length - 1
					: (current - 1 + buttons.length) % buttons.length;
		buttons[next]?.focus();
	}

	function trapDialogFocus(event, dialog) {
		if (event.key !== "Tab" || !dialog.open) return;
		const items = $$(
			'button:not([disabled]),a[href],input:not([disabled]),select:not([disabled]),textarea:not([disabled]),summary,[tabindex]:not([tabindex="-1"])',
			dialog,
		).filter(visible);
		if (!items.length) return;
		const first = items[0];
		const last = items[items.length - 1];
		if (event.shiftKey && document.activeElement === first) {
			event.preventDefault();
			last.focus();
		} else if (!event.shiftKey && document.activeElement === last) {
			event.preventDefault();
			first.focus();
		}
	}

	function openServer(name, tab = "overview") {
		state.serverDialogOpener = document.activeElement;
		state.serverDialogFocusToken = captureDashboardFocus();
		state.serverDialogReturnView = state.view;
		switchView("integrations", { focus: false });
		const custom = [
			"tools",
			"capabilities",
			"access",
			"usage",
			"events",
		].includes(tab);
		const api = dashboardApi();
		const baseTab = custom
			? "overview"
			: tab === "protection"
				? "routing"
				: tab === "configuration"
					? "source"
					: tab;
		if (api?.openServerDialog) api.openServerDialog(name, baseTab);
		else {
			const row = serverModels().find((server) => server.name === name)?.row;
			const button = $$("[data-server-action]", row).find(
				(control) => control.dataset.serverAction === "settings",
			);
			button?.click();
		}
		setTimeout(() => {
			enhanceServerDialog();
			if (custom) setCustomServerTab(tab);
		}, 0);
	}

	function serverDiagnosis(server) {
		if (!server) return null;
		const audits = rangedAuditRecords().filter(
			(record) => record.server === server.name,
		);
		const failures = auditFailureGroups(audits);
		const auth = failures.find((group) => group.key === "authorization");
		const policy = failures.find((group) => group.key === "policy_denied");
		const timeout = failures.find(
			(group) => group.key === "timeout" || group.key === "capacity",
		);
		const access = serverAccessProfile(server);
		const lifecycle = serverLifecycleProfile(server);
		const last = serverLastOperationProfile(server);
		if (!server.enabled)
			return {
				tone: "off",
				eyebrow: "Route disabled",
				title: `${server.name} is configured but unavailable to clients`,
				detail:
					"The source definition is preserved. Enable it first, then run Test to collect current tool evidence.",
				primary: ["Enable server", "enable"],
				secondary: ["Review access", "access"],
				steps: [
					"Enable routing state",
					"Run initialize and tools/list",
					"Verify the intended protection mode",
				],
			};
		if (lifecycle.sourceBlocked)
			return {
				tone: "bad",
				eyebrow: "Source blocked",
				title: `${server.name} cannot start from its current source`,
				detail: lifecycle.probe.detail,
				primary: ["Review configuration", "configuration"],
				secondary: ["Open events", "events"],
				steps: [
					"Verify command or endpoint",
					"Confirm platform support and source policy",
					"Run Test again",
				],
			};
		if (lifecycle.probe.failed)
			return {
				tone: "bad",
				eyebrow: "MCP verification failed",
				title: `${server.name} did not complete initialize or tool discovery`,
				detail: lifecycle.probe.detail,
				primary: ["Run Test again", "test"],
				secondary: ["Review configuration", "configuration"],
				steps: [
					"Verify source and transport",
					"Inspect initialize/tools/list evidence",
					"Retry after correcting the source",
				],
			};
		if (!lifecycle.protocolMeasured || !lifecycle.tools.measured)
			return {
				tone: "warn",
				eyebrow: "Evidence missing",
				title: `MCPace has no source-matched verification for ${server.name}`,
				detail: lifecycle.tools.measured
					? lifecycle.probe.detail
					: lifecycle.tools.detail,
				primary: ["Run Test", "test"],
				secondary: ["Review configuration", "configuration"],
				steps: [
					"Review command or endpoint",
					"Run initialize and tools/list",
					"Inspect returned capabilities and access",
				],
			};
		if (auth && last.failed && last.errorKind === "authorization")
			return {
				tone: "warn",
				eyebrow: "Latest call: authorization",
				title: `${server.name} is MCP-ready, but the latest tool call could not authenticate`,
				detail: `${auth.calls} retained failure${auth.calls === 1 ? "" : "s"} occurred during ${auth.stage}. Review credential names and the remote authorization boundary without exposing secret values.`,
				primary: ["Review access", "access"],
				secondary: ["Open failed events", "events"],
				steps: [
					"Confirm credential/header names",
					"Verify the endpoint origin",
					"Retry the affected tool",
				],
			};
		if (policy && last.failed && last.errorKind === "policy_denied")
			return {
				tone: "warn",
				eyebrow: "Latest call: policy denied",
				title: `${server.name} is MCP-ready, but policy denied a tool call`,
				detail: `${policy.calls} retained call${policy.calls === 1 ? "" : "s"} were stopped by policy. Do not weaken protection until the requested tool and data scope are understood.`,
				primary: ["Review protection", "protection"],
				secondary: ["Open failed events", "events"],
				steps: [
					"Inspect denied tool calls",
					"Confirm project/session scope",
					"Change policy only when justified",
				],
			};
		if (
			timeout &&
			last.failed &&
			(["timeout", "capacity"].includes(last.errorKind) ||
				last.stage === "queue")
		)
			return {
				tone: "warn",
				eyebrow: "Latest call: queue or timeout",
				title: `${server.name} is MCP-ready, but the latest operation exceeded a time or capacity boundary`,
				detail: `${timeout.calls} retained failure${timeout.calls === 1 ? "" : "s"} occurred at ${timeout.stage}. Check queue pressure before increasing concurrency.`,
				primary: ["Review protection", "protection"],
				secondary: ["Open failed events", "events"],
				steps: [
					"Inspect queue and upstream latency",
					"Check active sessions and leases",
					"Adjust capacity only with evidence",
				],
			};
		if (last.failed)
			return {
				tone: "warn",
				eyebrow: "Latest operation failed",
				title: `${server.name} remains MCP-ready`,
				detail: `${last.title}. ${last.detail}. This operation result does not invalidate retained initialize or tools/list evidence.`,
				primary: ["Open failed events", "events"],
				secondary: ["Run Test", "test"],
				steps: [
					"Inspect the exact failed tool and stage",
					"Correct operation-specific access or input",
					"Retest MCP only if source evidence may have changed",
				],
			};
		if (access.remote && !access.credentialNames.length)
			return {
				tone: "warn",
				eyebrow: "Remote access review",
				title: `${server.name} is remote and authorization evidence is incomplete`,
				detail:
					"The endpoint is outside loopback, but no retained credential or header name is available. This does not prove the endpoint is public or safe.",
				primary: ["Review access", "access"],
				secondary: ["Run Test", "test"],
				steps: [
					"Confirm endpoint origin",
					"Confirm authorization method",
					"Verify data sent by tools",
				],
			};
		if (server.lane === "blocked")
			return {
				tone: "bad",
				eyebrow: "Backend route blocked",
				title: `${server.name} passed MCP verification but remains blocked`,
				detail:
					server.evidenceBody ||
					"The structured backend plan marks this route blocked.",
				primary: ["Review protection", "protection"],
				secondary: ["Open events", "events"],
				steps: [
					"Inspect the backend block reason",
					"Review source and policy",
					"Change only the blocking condition",
				],
			};
		if (server.lane === "guarded" || access.approvalRequired)
			return {
				tone: "warn",
				eyebrow: "Review required",
				title: `${server.name} is verified but guarded`,
				detail:
					server.evidenceTitle ||
					server.evidenceBody ||
					"The backend requires an access or policy review.",
				primary: ["Review protection", "protection"],
				secondary: ["Review access", "access"],
				steps: [
					"Review requested access",
					"Confirm isolation scope",
					"Approve only the intended surface",
				],
			};
		return null;
	}

	function runServerResolution(server, action) {
		if (!server || !action) return;
		if (
			["tools", "capabilities", "access", "usage", "events"].includes(action)
		) {
			setCustomServerTab(action);
			return;
		}
		if (
			action === "protection" ||
			action === "configuration" ||
			action === "overview"
		) {
			const base =
				action === "protection"
					? "routing"
					: action === "configuration"
						? "source"
						: "overview";
			$(
				`[data-server-dialog-tab="${base}"]`,
				state.nodes.serverDialogTabs,
			)?.click();
			return;
		}
		if (action === "enable") {
			(
				$(".mc-sheet-switch", state.nodes.serverDialog) ||
				$(".mc-inline-server-toggle", server.row)
			)?.click();
			return;
		}
		if (action === "test") {
			const control = $$("[data-server-action]", server.row).find((button) =>
				["test", "enable-test"].includes(button.dataset.serverAction),
			);
			control?.click();
		}
	}

	function enhanceServerDialog() {
		const dialog = state.nodes.serverDialog;
		const body = state.nodes.serverDialogBody;
		const tabs = state.nodes.serverDialogTabs;
		if (!dialog || !body || !tabs || !dialog.open) return;
		dialog.classList.add("mc-server-sheet");
		const headingEyebrow = $(".server-dialog-heading .eyebrow", dialog);
		if (headingEyebrow) headingEyebrow.textContent = "Integration";
		const overviewTab = $('[data-server-dialog-tab="overview"]', tabs);
		const routingTab = $('[data-server-dialog-tab="routing"]', tabs);
		const sourceTab = $('[data-server-dialog-tab="source"]', tabs);
		if (overviewTab) overviewTab.textContent = "Summary";
		if (routingTab) routingTab.textContent = "Isolation";
		if (sourceTab) sourceTab.textContent = "Setup";

		const ensureCustomTab = (id, label, before) => {
			let tab = $(`[data-mc-server-tab="${id}"]`, tabs);
			if (!tab) {
				tab = document.createElement("button");
				tab.type = "button";
				tab.className = "task-tab";
				tab.id = `server-dialog-tab-${id}`;
				tab.setAttribute("role", "tab");
				tab.setAttribute("aria-selected", "false");
				tab.setAttribute("aria-controls", `server-dialog-panel-${id}`);
				tab.tabIndex = -1;
				tab.dataset.mcServerTab = id;
				tab.textContent = label;
				before?.before(tab) || tabs.appendChild(tab);
				tab.addEventListener("click", () => setCustomServerTab(id));
				tab.addEventListener("keydown", serverSheetTabKeydown);
			}
			return tab;
		};
		const toolsTab = ensureCustomTab("tools", "Tools", routingTab);
		const capabilitiesTab = ensureCustomTab(
			"capabilities",
			"Capabilities",
			routingTab,
		);
		const accessTab = ensureCustomTab("access", "Access", routingTab);
		const usageTab = ensureCustomTab("usage", "Usage", routingTab);
		const eventsTab = ensureCustomTab("events", "Events", routingTab);
		if (usageTab) usageTab.textContent = "Activity";
		if (capabilitiesTab) capabilitiesTab.textContent = "Protocol";
		if (eventsTab) eventsTab.textContent = "History";
		[capabilitiesTab, accessTab, eventsTab].filter(Boolean).forEach((tab) => {
			tab.dataset.mcTechnicalTab = "true";
		});
		if (overviewTab && toolsTab && routingTab && sourceTab && usageTab) {
			overviewTab.after(toolsTab);
			toolsTab.after(routingTab);
			routingTab.after(sourceTab);
			sourceTab.after(usageTab);
			if (accessTab) usageTab.after(accessTab);
			if (capabilitiesTab) (accessTab || usageTab).after(capabilitiesTab);
			if (eventsTab)
				(capabilitiesTab || accessTab || usageTab).after(eventsTab);
		}
		let moreMenu = $(".mc-server-more-menu", dialog);
		if (!moreMenu) {
			moreMenu = document.createElement("details");
			moreMenu.className = "mc-server-more-menu";
			setProductHtml(
				moreMenu,
				`<summary>${ICON.settings}<span>More details</span>${ICON.chevron}</summary><div><button type="button" data-mc-server-more-tab="access">${ICON.shield}<span><strong>Access</strong><small>Credentials, network, and source</small></span></button><button type="button" data-mc-server-more-tab="capabilities">${ICON.activity}<span><strong>Protocol evidence</strong><small>Negotiated capabilities and versions</small></span></button><button type="button" data-mc-server-more-tab="events">${ICON.terminal}<span><strong>Operation history</strong><small>Retained events and failure classes</small></span></button></div>`,
			);
			tabs.after(moreMenu);
			$$("[data-mc-server-more-tab]", moreMenu).forEach((button) =>
				button.addEventListener("click", () => {
					moreMenu.open = false;
					setCustomServerTab(button.dataset.mcServerMoreTab);
				}),
			);
		}
		$$("[data-server-dialog-tab]", tabs).forEach((tab) => {
			if (!tab.dataset.mcDeepBound) {
				tab.dataset.mcDeepBound = "true";
				tab.addEventListener("click", () =>
					setTimeout(() => syncServerSheetTabs(tab.dataset.serverDialogTab), 0),
				);
				tab.addEventListener("keydown", serverSheetTabKeydown);
			}
		});

		const selectedName = text(state.nodes.serverDialogTitle);
		const selected = serverModels().find(
			(server) => server.name === selectedName,
		);
		const bodyText = text(body);
		const tone =
			selected?.tone ||
			toneFrom(
				`${state.nodes.serverDialogSubtitle?.textContent || ""} ${bodyText}`,
			);
		const tools =
			selected?.toolCount ?? numberFrom(bodyText, /(\d+)\s+tools?/i, 0);
		const protection =
			selected?.routeMode ||
			(/project/i.test(bodyText)
				? "Per project"
				: /session|chat/i.test(bodyText)
					? "Per chat"
					: /serial/i.test(bodyText)
						? "Serialized"
						: "Automatic");
		const usage = selected ? usageForServer(selected.name) : usageAnalytics([]);

		let summary = $(".mc-server-summary", dialog);
		if (!summary) {
			summary = document.createElement("section");
			summary.className = "mc-server-summary";
			tabs.before(summary);
		}
		const technicalVisible = dialog.classList.contains("mc-show-technical");
		const summarySignature = JSON.stringify([
			tone,
			selected?.status || statusLabel(tone, bodyText),
			tools,
			usage.calls,
			usage.p95,
			protection,
			technicalVisible,
		]);
		if (summary.dataset.mcSignature !== summarySignature) {
			summary.dataset.mcSignature = summarySignature;
			setProductHtml(
				summary,
				`<div class="mc-server-summary-state"><span>Status</span><strong><i data-tone="${tone}" aria-hidden="true">${toneMark(tone)}</i>${escapeHtml(selected?.status || statusLabel(tone, bodyText))}</strong></div><div class="mc-server-summary-tools"><span>Tools</span><strong>${tools}</strong></div><div class="mc-server-summary-usage"><span>Recent calls</span><strong>${formatNumber(usage.calls)}${usage.p95 !== null ? `<small>p95 ${escapeHtml(formatDuration(usage.p95))}</small>` : ""}</strong></div><div class="mc-server-summary-isolation"><span>Isolation</span><strong>${escapeHtml(protection)}</strong></div><button type="button" class="mc-technical-toggle" aria-label="Technical details" aria-pressed="${technicalVisible}">${ICON.settings}<span>${technicalVisible ? "Hide advanced fields" : "Advanced fields"}</span></button>`,
			);
			$(".mc-technical-toggle", summary)?.addEventListener("click", (event) => {
				const active = dialog.classList.toggle("mc-show-technical");
				event.currentTarget.setAttribute("aria-pressed", String(active));
				$("span", event.currentTarget).textContent = active
					? "Hide advanced fields"
					: "Advanced fields";
			});
		}

		let sourceStrip = $(".mc-server-source-strip", dialog);
		if (!sourceStrip) {
			sourceStrip = document.createElement("section");
			sourceStrip.className = "mc-server-source-strip";
			summary.after(sourceStrip);
		}
		const sourceSignature = JSON.stringify([
			selected?.enabled,
			selected?.sourceType,
			selected?.sourceLocation,
			state.pathVisibility,
			selected?.sourceEnvNames,
			selected?.sourceHeaderNames,
		]);
		if (sourceStrip.dataset.mcSignature !== sourceSignature) {
			sourceStrip.dataset.mcSignature = sourceSignature;
			setProductHtml(
				sourceStrip,
				selected
					? `<div class="mc-server-source-copy"><span>${selected.sourceType === "http" ? "Remote endpoint" : selected.sourcePath ? "Configuration source" : "Launch command"}</span><code title="${escapeHtml(selected.sourceLocation || "Source not returned")}">${escapeHtml(compactPath(selected.sourceLocation || "Source not returned"))}</code><small>${selected.sourceEnvNames.length + selected.sourceHeaderNames.length ? `${selected.sourceEnvNames.length + selected.sourceHeaderNames.length} credential name${selected.sourceEnvNames.length + selected.sourceHeaderNames.length === 1 ? "" : "s"} referenced; value availability is not verified` : "No credential names reported"}</small></div><div class="mc-server-source-actions">${selected.sourceLocation ? `<button type="button" class="mc-secondary-button mc-server-copy-source" data-mc-copy-value="${escapeHtml(selected.sourceLocation)}">${ICON.copy}<span>Copy source</span></button>` : ""}<button type="button" class="mc-sheet-switch" data-server-name="${escapeHtml(selected.name)}" data-server-action="toggle" aria-pressed="${selected.enabled}"><span><i></i></span><strong>${selected.enabled ? "On" : "Off"}</strong></button></div>`
					: "<div><strong>Source metadata is not available.</strong></div>",
			);
		}

		let resolver = $(".mc-server-resolver", dialog);
		const diagnosis = selected ? serverDiagnosis(selected) : null;
		if (diagnosis) {
			if (!resolver) {
				resolver = document.createElement("section");
				resolver.className = "mc-server-resolver";
				sourceStrip.after(resolver);
			}
			const diagnosisSignature = JSON.stringify([
				selected?.name,
				diagnosis.tone,
				diagnosis.title,
				diagnosis.detail,
				diagnosis.primary,
				diagnosis.secondary,
				diagnosis.steps,
			]);
			if (resolver.dataset.mcSignature !== diagnosisSignature) {
				resolver.dataset.mcSignature = diagnosisSignature;
				resolver.dataset.tone = diagnosis.tone;
				setProductHtml(
					resolver,
					`<span class="mc-resolver-icon">${diagnosis.tone === "bad" ? ICON.warning : ICON.shield}</span><div><small>${escapeHtml(diagnosis.eyebrow)}</small><h3>${escapeHtml(diagnosis.title)}</h3><p>${escapeHtml(diagnosis.detail)}</p><ol>${diagnosis.steps.map((step) => `<li>${escapeHtml(step)}</li>`).join("")}</ol></div><div class="mc-resolver-actions"><button type="button" class="${diagnosis.tone === "bad" ? "mc-primary-button" : "mc-secondary-button"}" data-mc-resolve-action="${escapeHtml(diagnosis.primary[1])}">${escapeHtml(diagnosis.primary[0])}</button><button type="button" class="mc-text-button" data-mc-resolve-action="${escapeHtml(diagnosis.secondary[1])}">${escapeHtml(diagnosis.secondary[0])}</button></div>`,
				);
				$$("[data-mc-resolve-action]", resolver).forEach((button) =>
					button.addEventListener("click", () =>
						runServerResolution(selected, button.dataset.mcResolveAction),
					),
				);
			}
			resolver.hidden = false;
		} else if (resolver) {
			resolver.hidden = true;
		}

		const ensurePanel = (id, className) => {
			let panel = $(`#server-dialog-panel-${id}`, body);
			if (!panel) {
				panel = document.createElement("section");
				panel.id = `server-dialog-panel-${id}`;
				panel.className = `server-dialog-panel ${className}`;
				panel.setAttribute("role", "tabpanel");
				panel.setAttribute("aria-labelledby", `server-dialog-tab-${id}`);
				panel.dataset.mcServerPanel = id;
				panel.hidden = true;
				body.appendChild(panel);
			}
			return panel;
		};
		const overviewPanel = $("#server-dialog-panel-overview", body);
		const toolsPanel = ensurePanel("tools", "mc-tools-panel");
		const capabilitiesPanel = ensurePanel(
			"capabilities",
			"mc-server-capabilities-panel",
		);
		const accessPanel = ensurePanel("access", "mc-server-access-panel");
		const usagePanel = ensurePanel("usage", "mc-server-usage-panel");
		const eventsPanel = ensurePanel("events", "mc-server-events-panel");
		const sourcePanel = $("#server-dialog-panel-source", body);

		if (sourcePanel && selected) {
			let danger = $(".mc-server-danger-zone", sourcePanel);
			if (!danger) {
				danger = document.createElement("section");
				danger.className = "mc-server-danger-zone";
				sourcePanel.appendChild(danger);
			}
			const dangerSignature = JSON.stringify([
				selected.name,
				selected.sourceLocation,
				selected.enabled,
				selected.activeLeases.length,
				selected.usage.calls,
			]);
			if (danger.dataset.mcSignature !== dangerSignature) {
				danger.dataset.mcSignature = dangerSignature;
				setProductHtml(
					danger,
					`<div><small>Danger zone</small><h3>Remove saved definition</h3><p>Use Disable for a reversible change. Remove deletes <strong>${escapeHtml(selected.name)}</strong> from ${selected.sourceLocation ? `<code>${escapeHtml(compactPath(selected.sourceLocation))}</code>` : "the source file selected by the backend"}. Retained audit history may remain in local logs.</p>${selected.activeLeases.length ? `<em data-tone="bad">${selected.activeLeases.length} active lease${selected.activeLeases.length === 1 ? "" : "s"} observed — finish or disable work first.</em>` : "<em>No active lease is currently observed.</em>"}</div><button type="button" class="mc-danger-button" data-server-name="${escapeHtml(selected.name)}" data-server-action="remove">Remove integration</button>`,
				);
			}
		}

		if (overviewPanel && selected) {
			let dailySummary = $(".mc-server-daily-summary", overviewPanel);
			if (!dailySummary) {
				dailySummary = document.createElement("section");
				dailySummary.className = "mc-server-daily-summary";
				overviewPanel.prepend(dailySummary);
			}
			const dailyAccess = serverAccessProfile(selected);
			const dailyDiagnosis = serverDiagnosis(selected);
			const dailyOperational = serverOperationalProfile(selected);
			const dailyLifecycle = dailyOperational.lifecycle;
			const dailyToolsEvidence = dailyLifecycle.tools;
			const dailyRuntime = dailyLifecycle.runtime;
			const dailyConflicts = serverConflictProfile(selected);
			const dailyTools = (
				selected.toolDefinitions?.length
					? selected.toolDefinitions.map(toolDisplaySummary)
					: (selected.tools || []).map((name) => ({
							display: name,
							technical: name,
							differs: false,
						}))
			).slice(0, 5);
			const dailySignature = JSON.stringify([
				selected.name,
				dailyOperational.tone,
				dailyOperational.title,
				dailyOperational.detail,
				selected.enabled,
				selected.routeMode,
				selected.sourceType,
				selected.sourceLocation,
				dailyToolsEvidence.measured,
				dailyToolsEvidence.count,
				dailyRuntime.pids,
				dailyRuntime.rssBytes,
				dailyRuntime.shortestIdleMs,
				dailyConflicts.duplicateSources.map((item) => item.name),
				dailyConflicts.toolCollisions,
				dailyTools,
				usage.calls,
				usage.failures,
				usage.p95,
				usage.lastTimestamp,
				dailyAccess.dataScope,
				dailyAccess.credentialNames.length,
				dailyAccess.destructive,
				dailyAccess.external,
				dailyDiagnosis?.title,
			]);
			if (dailySummary.dataset.mcSignature !== dailySignature) {
				dailySummary.dataset.mcSignature = dailySignature;
				const recoveryVisible = Boolean(dailyDiagnosis);
				dailySummary.dataset.contextOnly = String(recoveryVisible);
				dailySummary.dataset.tone = recoveryVisible
					? "neutral"
					: dailyOperational.tone;
				const sourceKind =
					selected.sourceType === "http"
						? dailyAccess.remote
							? "Remote HTTP"
							: "Local HTTP"
						: "Local process";
				const lastUse = usage.calls
					? formatRelativeTimestamp(usage.lastTimestamp)
					: "No retained calls";
				const accessNote = dailyAccess.credentialNames.length
					? `${dailyAccess.credentialNames.length} credential name${dailyAccess.credentialNames.length === 1 ? "" : "s"} referenced; availability not verified`
					: dailyAccess.destructive || dailyAccess.external
						? `${dailyAccess.destructive + dailyAccess.external} risk hint${dailyAccess.destructive + dailyAccess.external === 1 ? "" : "s"} to review`
						: "No credential names reported";
				const headline = recoveryVisible
					? "Current integration context"
					: dailyOperational.title;
				const detail = recoveryVisible
					? "Source, live runtime, recent use, isolation, and access remain available here while the recovery action stays above."
					: dailyOperational.detail;
				const summaryAction = recoveryVisible
					? ""
					: `<button type="button" class="mc-secondary-button" data-mc-daily-tab="tools">Browse tools</button>`;
				const processLabel = dailyRuntime.processObserved
					? `${dailyRuntime.sessions.length || dailyRuntime.pids.length} process session${(dailyRuntime.sessions.length || dailyRuntime.pids.length) === 1 ? "" : "s"}`
					: dailyRuntime.routeOwners
						? `${dailyRuntime.routeOwners} route owner${dailyRuntime.routeOwners === 1 ? "" : "s"}`
						: "On demand";
				const processDetail = dailyRuntime.pids.length
					? `PID ${dailyRuntime.pids.slice(0, 3).join(", ")}${dailyRuntime.rssBytes ? ` · ${formatBytes(dailyRuntime.rssBytes)} RSS` : ""}`
					: dailyRuntime.shortestIdleMs !== null
						? `idle ${formatDuration(dailyRuntime.shortestIdleMs)}`
						: "No live process evidence";
				const collisionNote = dailyConflicts.hasDuplicateSource
					? `Same source as ${dailyConflicts.duplicateSources.map((item) => item.name).join(", ")}`
					: dailyConflicts.hasToolCollisions
						? `${dailyConflicts.toolCollisions.length} tool-name collision${dailyConflicts.toolCollisions.length === 1 ? "" : "s"}`
						: "No duplicate source detected";
				const toolEmpty = dailyToolsEvidence.measured
					? dailyToolsEvidence.count
						? "No retained tool names to preview."
						: "tools/list returned zero tools; inspect other capabilities."
					: "Run Test to collect source-matched tool evidence.";
				setProductHtml(
					dailySummary,
					`<header><span class="mc-tone-mark" aria-hidden="true">${toneMark(recoveryVisible ? "neutral" : dailyOperational.tone)}</span><div><small>At a glance</small><h3>${escapeHtml(headline)}</h3><p>${escapeHtml(detail)}</p></div>${summaryAction}</header><div class="mc-server-daily-facts"><button type="button" data-mc-daily-tab="source"><span>Source</span><strong>${escapeHtml(sourceKind)}</strong><small>${escapeHtml(compactPath(selected.sourceLocation || "Source not returned"))}</small></button><button type="button" data-mc-daily-tab="usage"><span>Recent use</span><strong>${formatNumber(usage.calls)} call${usage.calls === 1 ? "" : "s"}</strong><small>${escapeHtml(lastUse)}${usage.p95 !== null ? ` · p95 ${escapeHtml(formatDuration(usage.p95))}` : ""}</small></button><button type="button" data-mc-daily-tab="routing"><span>Runtime</span><strong>${escapeHtml(processLabel)}</strong><small>${escapeHtml(processDetail)}</small></button><button type="button" data-mc-daily-tab="access"><span>Access</span><strong>${escapeHtml(dailyAccess.dataScope)}</strong><small>${escapeHtml(accessNote)}</small></button></div><section class="mc-server-runtime-strip" data-tone="${dailyRuntime.current ? "good" : "neutral"}"><div><span>Isolation & capacity</span><strong>${escapeHtml(selected.routeMode || "Automatic")} · ${escapeHtml(serverCapacityLabel(selected))}</strong><small>${selected.activeLeases.length ? `${selected.activeLeases.length} current route lease${selected.activeLeases.length === 1 ? "" : "s"}; ownership may be idle` : "No current route lease"}</small></div><div><span>Evidence provenance</span><strong>${escapeHtml(serverEvidenceFreshness(selected).label)}</strong><small>${escapeHtml(serverEvidenceFreshness(selected).detail)}</small></div><div><span>Overlap check</span><strong>${escapeHtml(collisionNote)}</strong><small>Aliases and shared tool names can be intentional; verify before removing or renaming.</small></div></section><div class="mc-server-daily-tools"><span>Tools</span><div>${dailyTools.length ? dailyTools.map((tool) => `<button type="button" data-mc-daily-tab="tools" title="${escapeHtml(tool.differs ? tool.technical : tool.display)}">${escapeHtml(tool.display)}</button>`).join("") : `<small>${escapeHtml(toolEmpty)}</small>`}</div></div>`,
				);
				$("[data-mc-daily-action]", dailySummary)?.addEventListener(
					"click",
					(buttonEvent) =>
						runServerResolution(
							selected,
							buttonEvent.currentTarget.dataset.mcDailyAction,
						),
				);
				$$("[data-mc-daily-tab]", dailySummary).forEach((button) =>
					button.addEventListener("click", () => {
						const tab = button.dataset.mcDailyTab;
						if (["tools", "usage", "access"].includes(tab))
							setCustomServerTab(tab);
						else $(`[data-server-dialog-tab="${tab}"]`, tabs)?.click();
					}),
				);
			}
		}

		const definitions = selected?.toolDefinitions?.length
			? selected.toolDefinitions
			: (selected?.tools || []).map((name) => ({ name }));
		const toolSignature = JSON.stringify(
			definitions.map((tool) => [
				tool.name,
				tool.description,
				tool.annotations,
			]),
		);
		if (toolsPanel.dataset.mcSignature !== toolSignature) {
			toolsPanel.dataset.mcSignature = toolSignature;
			const toolRows = definitions
				.map((tool) => {
					const summary = toolDisplaySummary(tool);
					const description = String(
						tool?.description || "No description was returned by tools/list.",
					);
					const risks = toolRisk(tool);
					return `<article class="mc-tool-card" data-tool-search="${escapeHtml(`${summary.display} ${summary.technical} ${description}`.toLowerCase())}"><span class="mc-tool-icon">${ICON.terminal}</span><div><header><div class="mc-tool-title"><strong>${escapeHtml(summary.display)}</strong>${summary.differs ? `<code>${escapeHtml(summary.technical)}</code>` : ""}</div><div>${risks.map((risk) => `<em data-tone="${risk.tone}" title="${escapeHtml(risk.source === "annotation" ? "Server-provided annotation; not a trust decision." : risk.source === "heuristic" ? "Conservative MCPace heuristic from name and description." : "No retained risk evidence.")}">${escapeHtml(risk.label)}</em>`).join("")}</div></header><p>${escapeHtml(description)}</p><details><summary>Schema & server metadata</summary><pre>${escapeHtml(JSON.stringify({ name: summary.technical, title: tool.title || null, inputSchema: tool.inputSchema || null, outputSchema: tool.outputSchema || null, annotations: tool.annotations || null }, null, 2))}</pre></details></div></article>`;
				})
				.join("");
			setProductHtml(
				toolsPanel,
				`<header class="mc-tools-panel-head"><div><span>Reported tool surface</span><h3>${humanCount(definitions.length, "tool")}</h3><p>Risk badges combine server-provided hints with conservative name/description signals. Hints do not establish trust by themselves.</p></div><label>${ICON.search}<span class="mc-sr-only">Search tools</span><input type="search" data-mc-tool-search placeholder="Filter name or description"></label></header><div class="mc-tool-cards">${toolRows || `<div class="mc-large-empty">${ICON.terminal}<strong>No tool definitions are available</strong><span>Run Test to collect initialize and tools/list evidence.</span></div>`}</div>`,
			);
			$("[data-mc-tool-search]", toolsPanel)?.addEventListener(
				"input",
				(event) => {
					const query = event.target.value.trim().toLowerCase();
					$$("[data-tool-search]", toolsPanel).forEach((item) => {
						item.hidden =
							Boolean(query) && !item.dataset.toolSearch.includes(query);
					});
				},
			);
		}

		const capability = selected
			? serverCapabilityProfile(selected)
			: {
					protocolVersion: "",
					serverName: "",
					serverVersion: "",
					capabilities: [],
					measured: 0,
					total: 0,
					coverage: 0,
				};
		const capabilitySignature = JSON.stringify([
			selected?.name,
			capability.protocolVersion,
			capability.serverName,
			capability.serverVersion,
			capability.capabilities,
		]);
		if (capabilitiesPanel.dataset.mcSignature !== capabilitySignature) {
			capabilitiesPanel.dataset.mcSignature = capabilitySignature;
			const capabilityRows = capability.capabilities
				.map((item) => {
					const tone = ["measured", "reported"].includes(item.state)
						? "good"
						: item.state === "not-reported" || item.state === "disabled"
							? "off"
							: "warn";
					const stateLabel =
						item.state === "not-measured"
							? "Not measured"
							: item.state === "not-reported"
								? "Not reported"
								: item.state === "disabled"
									? "Disabled"
									: item.state === "measured"
										? "Measured"
										: "Reported";
					return `<article class="mc-capability-row" data-tone="${tone}"><span>${item.id === "tools" ? ICON.terminal : item.id === "resources" ? ICON.server : item.id === "prompts" ? ICON.apps : ICON.activity}</span><div><strong>${escapeHtml(item.label)}</strong><small>${escapeHtml(item.detail)}</small></div><em>${escapeHtml(stateLabel)}</em></article>`;
				})
				.join("");
			setProductHtml(
				capabilitiesPanel,
				`<header class="mc-capabilities-head"><div><span>Negotiated MCP surface</span><h3>${escapeHtml(capability.serverName || selected?.name || "Integration")}</h3><p>Only retained initialize or list evidence is marked measured. Missing evidence stays unknown.</p></div><div class="mc-capability-score"><strong>${capability.measured}/${capability.total}</strong><span>fields evidenced</span></div></header><section class="mc-protocol-facts"><article><span>Negotiated version</span><strong>${escapeHtml(capability.protocolVersion || "Not retained")}</strong><small>MCPace build target: 2025-11-25</small></article><article><span>Server identity</span><strong>${escapeHtml(capability.serverName || "Not retained")}</strong><small>${escapeHtml(capability.serverVersion || "Version not retained")}</small></article><article><span>Tool evidence</span><strong>${escapeHtml(selected ? serverToolsEvidenceProfile(selected).label : "Not measured")}</strong><small>${escapeHtml(selected ? serverToolsEvidenceProfile(selected).detail : "No retained evidence")}</small></article></section><div class="mc-capability-list">${capabilityRows || `<div class="mc-large-empty">${ICON.activity}<strong>No capability evidence is retained</strong><span>Enable and test the server to collect protocol and list evidence.</span></div>`}</div><section class="mc-capability-note">${ICON.shield}<div><strong>Capability is not authorization.</strong><p>A server reporting tools, resources, prompts, or tasks does not prove that its origin, annotations, or requested access are trustworthy. Definitions can change while a server runs; this panel only reflects retained evidence matching the current source, so re-test after a server or source update.</p></div></section>`,
			);
		}

		const access = selected ? serverAccessProfile(selected) : null;
		const accessSignature = JSON.stringify([
			selected?.name,
			selected?.enabled,
			selected?.sourceType,
			selected?.sourceLocation,
			selected?.sourceEnvNames,
			selected?.sourceHeaderNames,
			selected?.riskCounts,
			state.pathVisibility,
		]);
		if (accessPanel.dataset.mcSignature !== accessSignature) {
			accessPanel.dataset.mcSignature = accessSignature;
			const credentialRows = access?.credentialNames?.length
				? access.credentialNames
						.map(
							(name) =>
								`<span>${ICON.shield}<code>${escapeHtml(name)}</code><small>name only</small></span>`,
						)
						.join("")
				: '<div class="mc-inline-empty">No credential or header names were returned. This does not prove that the server requires no authorization.</div>';
			const riskSummary = access
				? [
						access.destructive
							? `${access.destructive} mutation-risk hint${access.destructive === 1 ? "" : "s"}`
							: "",
						access.external
							? `${access.external} external-access hint${access.external === 1 ? "" : "s"}`
							: "",
					]
						.filter(Boolean)
						.join(" · ") ||
					"No risk hint inferred from retained tool definitions"
				: "Not measured";
			setProductHtml(
				accessPanel,
				`<header class="mc-access-head" data-tone="${access?.tone || "neutral"}"><span>${ICON.shield}</span><div><small>Execution boundary</small><h3>${escapeHtml(access?.dataScope || "Not measured")}</h3><p>${escapeHtml(access?.exposure || "Exposure state is unavailable.")}</p></div></header><section class="mc-access-grid"><article><span>Source</span><strong>${escapeHtml(selected?.sourceType === "http" ? (access?.remote ? "Remote HTTP" : "Local HTTP") : "Local stdio process")}</strong><small>${escapeHtml(selected?.sourceLocation || "Source location not returned")}</small></article><article><span>Authorization evidence</span><strong>${escapeHtml(access?.auth || "Not measured")}</strong><small>Credential values are never rendered here</small></article><article><span>Tool risk surface</span><strong>${escapeHtml(riskSummary)}</strong><small>Annotations and name-based signals are advisory</small></article><article><span>Protection</span><strong>${escapeHtml(selected?.routeMode || "Automatic")}</strong><small>${selected?.activeInstances?.length || 0} active process instance${selected?.activeInstances?.length === 1 ? "" : "s"}</small></article></section><section class="mc-credential-names"><header><div><span>Credential references</span><h4>${humanCount(access?.credentialNames?.length || 0, "name")}</h4></div><p>Environment and HTTP header names may identify sensitive configuration; values remain in the runtime environment.</p></header><div>${credentialRows}</div></section><section class="mc-source-provenance"><div><span>Configuration provenance</span><code title="${escapeHtml(selected?.sourceLocation || "")}">${escapeHtml(compactPath(selected?.sourceLocation || "Source not returned"))}</code><small>${selected?.sourcePath ? "Loaded from a configuration file" : selected?.sourceType === "http" ? "Remote endpoint stored in MCPace server inventory" : "Launch command stored in MCPace server inventory"}</small></div>${selected?.sourceLocation ? `<button type="button" class="mc-secondary-button" data-mc-copy-value="${escapeHtml(selected.sourceLocation)}">${ICON.copy}<span>Copy source</span></button>` : ""}</section>`,
			);
		}

		const usageSignature = JSON.stringify([
			selected?.name,
			state.activityRange,
			state.tokenEstimates,
			usage.calls,
			usage.failures,
			usage.p95,
			usage.estimatedTokens,
			usage.reportedTokens,
			usage.records.at(0)?.timestamp,
		]);
		if (usagePanel.dataset.mcSignature !== usageSignature) {
			usagePanel.dataset.mcSignature = usageSignature;
			setProductHtml(
				usagePanel,
				`<header class="mc-server-usage-head"><div><span>${escapeHtml(rangeLabel())}</span><h3>${escapeHtml(selected?.name || "Integration")} usage</h3><p>Retained audit statistics, not lifetime billing.</p></div><button type="button" class="mc-secondary-button" data-mc-open-full-usage>Open full usage</button></header><div class="mc-server-usage-metrics"><article><span>Calls</span><strong>${formatNumber(usage.calls)}</strong><small>${usage.records.length} audit entries</small></article><article data-tone="${usage.failures ? "warn" : "good"}"><span>Success</span><strong>${successLabel(usage.successRate)}</strong><small>${usage.failures} failed</small></article><article><span>P50 / P95</span><strong>${escapeHtml(formatDuration(usage.p50))}</strong><small>${escapeHtml(formatDuration(usage.p95))} p95</small></article><article><span>Queue p95</span><strong>${escapeHtml(formatDuration(usage.queueP95))}</strong><small>before upstream execution</small></article><article><span>Payload</span><strong>${escapeHtml(formatBytes(usage.requestBytes + usage.responseBytes))}</strong><small>request + response</small></article></div><section class="mc-server-usage-chart"><header><strong>Call timeline</strong><span>${escapeHtml(rangeLabel())}</span></header>${usageTimelineMarkup(usage.records)}</section><div class="mc-server-usage-grid"><section><header><strong>Tools used</strong></header>${usageGroupRows(usage.tools, "tool", 8)}</section><section><header><strong>Token visibility</strong></header>${usageTokenMarkup(usage)}<div class="mc-server-context-list"><span>Clients: ${usage.clients.length ? usage.clients.map((item) => escapeHtml(item.label)).join(", ") : "not recorded"}</span><span>Projects: ${state.contextLabels === "show" && usage.projects.length ? usage.projects.map((item) => escapeHtml(compactPath(item.label))).join(", ") : state.contextLabels === "hide" ? "hidden locally" : "not recorded"}</span></div></section></div>`,
			);
			$("[data-mc-open-full-usage]", usagePanel)?.addEventListener(
				"click",
				() => {
					dialog.close();
					state.activityView = "servers";
					writePreference("activityView", state.activityView);
					switchView("activity");
				},
			);
		}

		const serverEvents = selected
			? activityModels()
					.filter(
						(event) =>
							event.audit?.server === selected.name ||
							(!event.audit &&
								String(event.meta || "").includes(selected.name)),
					)
					.slice(0, 80)
			: [];
		const serverAudits = selected
			? rangedAuditRecords().filter((record) => record.server === selected.name)
			: [];
		const failureGroups = auditFailureGroups(serverAudits);
		const eventSignature = JSON.stringify([
			selected?.name,
			state.activityRange,
			serverEvents.map((event) => [event.id, event.tone]),
			failureGroups.map((group) => [group.key, group.calls]),
		]);
		if (eventsPanel.dataset.mcSignature !== eventSignature) {
			eventsPanel.dataset.mcSignature = eventSignature;
			const failuresMarkup = failureGroups.length
				? failureGroups
						.map(
							(group) =>
								`<article data-tone="${group.key === "authorization" || group.key === "policy_denied" ? "warn" : "bad"}"><span>${escapeHtml(group.stage)}</span><strong>${formatNumber(group.calls)} ${escapeHtml(group.label)}</strong><small>${group.servers.size} integration${group.servers.size === 1 ? "" : "s"} · ${escapeHtml(formatRelativeTimestamp(group.latest))}</small></article>`,
						)
						.join("")
				: `<div class="mc-clear-state">${ICON.check}<div><strong>No failed retained calls</strong><span>No error classification appears for this server in the selected window.</span></div></div>`;
			const eventsMarkup = serverEvents.length
				? serverEvents
						.map(
							(event) =>
								`<button type="button" class="mc-server-event-row" data-tone="${event.tone}" data-mc-open-event="${escapeHtml(event.id)}"><span>${event.type === "error" ? ICON.warning : event.type === "tool" ? ICON.terminal : ICON.activity}</span><div><strong>${escapeHtml(event.title)}</strong><small>${escapeHtml(event.meta || event.source)}</small></div><em>${escapeHtml(formatRelativeTimestamp(event.timestamp))}</em>${ICON.chevron}</button>`,
						)
						.join("")
				: `<div class="mc-large-empty">${ICON.activity}<strong>No retained events for this integration</strong><span>Run a tool through MCPace or select another time range.</span></div>`;
			setProductHtml(
				eventsPanel,
				`<header class="mc-server-events-head"><div><span>${escapeHtml(rangeLabel())}</span><h3>${escapeHtml(selected?.name || "Integration")} operation history</h3><p>Each tool audit has a stable call ID, outcome, failure stage, latency split, payload size, and optional token evidence.</p></div><button type="button" class="mc-secondary-button" data-mc-export-server-events>${state.exportMode === "safe" ? "Export safe JSON" : "Export full JSON"}</button></header><section class="mc-server-failure-summary"><header><strong>Failure classification</strong><span>${failureGroups.reduce((sum, group) => sum + group.calls, 0)} failed calls</span></header><div>${failuresMarkup}</div></section><section class="mc-server-event-stream"><header><strong>Retained events</strong><span>${serverEvents.length} shown</span></header><div>${eventsMarkup}</div></section>`,
			);
			$$("[data-mc-open-event]", eventsPanel).forEach((button) =>
				button.addEventListener("click", () =>
					openEventDetail(button.dataset.mcOpenEvent),
				),
			);
			$("[data-mc-export-server-events]", eventsPanel)?.addEventListener(
				"click",
				() => {
					const retained = retainedWindow();
					const payload = {
						schema:
							state.exportMode === "safe"
								? "mcpace.serverActivityExport.safe.v2"
								: "mcpace.serverActivityExport.full.v2",
						generatedAt: new Date().toISOString(),
						exportMode: state.exportMode,
						server: selected?.name || "",
						selectedRange: state.activityRange,
						retention: exportRetentionEnvelope(retained),
						audits: exportAuditEnvelope(serverAudits),
					};
					downloadText(
						`mcpace-${String(selected?.name || "server").replace(/[^a-z0-9_-]+/gi, "-")}-${state.exportMode}-${Date.now()}.json`,
						JSON.stringify(payload, null, 2),
						"application/json;charset=utf-8",
					);
					toast(
						`${state.exportMode === "safe" ? "Privacy-safe " : "Full "}server events exported`,
						`${serverAudits.length} audit entries for ${selected?.name || "integration"}.`,
					);
				},
			);
		}

		const toolSection = $$(".server-explain-box", body).find((section) =>
			/available tools/i.test(text($(".label", section))),
		);
		if (toolSection) toolSection.classList.add("mc-overview-tools-preview");
		$$(".detail-box", body).forEach((box) => {
			const label = text($(".label", box));
			if (
				/kind|profile enabled|source enabled|effective enabled|scope|effect|state|binding|pool model|scheduler|strategy|conflict|locks|launcher|startup|transport status|evidence status/i.test(
					label,
				)
			)
				box.classList.add("mc-technical-field");
		});
		$$(".server-setting-box", body).forEach((box) => {
			const label = text($(".label", box));
			box.classList.toggle(
				"mc-advanced-routing-field",
				/worker|in-flight|queue|affinity|reuse|timeout|mutex|scheduler|strategy|pool|parallel/i.test(
					label,
				),
			);
		});
	}

	function revealServerSheetTab(tab) {
		const tabs = state.nodes.serverDialogTabs;
		if (!tabs || !tab) return;
		requestAnimationFrame(() => {
			const inset = 10;
			const left = tab.offsetLeft;
			const right = left + tab.offsetWidth;
			const visibleLeft = tabs.scrollLeft;
			const visibleRight = visibleLeft + tabs.clientWidth;
			if (left < visibleLeft + inset)
				tabs.scrollTo({ left: Math.max(0, left - inset), behavior: "auto" });
			else if (right > visibleRight - inset)
				tabs.scrollTo({
					left: Math.max(0, right - tabs.clientWidth + inset),
					behavior: "auto",
				});
		});
	}

	function setCustomServerTab(tabName) {
		const dialog = state.nodes.serverDialog;
		const body = state.nodes.serverDialogBody;
		const tabs = state.nodes.serverDialogTabs;
		if (
			!dialog ||
			!body ||
			!tabs ||
			!["tools", "capabilities", "access", "usage", "events"].includes(tabName)
		)
			return;
		$$(".server-dialog-panel", body).forEach((panel) => {
			panel.hidden = panel.id !== `server-dialog-panel-${tabName}`;
		});
		let activeTab = null;
		$$('[role="tab"]', tabs).forEach((tab) => {
			const active = tab.dataset.mcServerTab === tabName;
			tab.setAttribute("aria-selected", String(active));
			tab.classList.toggle("active", active);
			tab.tabIndex = active ? 0 : -1;
			if (active) activeTab = tab;
		});
		revealServerSheetTab(activeTab);
	}

	function syncServerSheetTabs(baseTab) {
		const tabs = state.nodes.serverDialogTabs;
		const body = state.nodes.serverDialogBody;
		if (!tabs) return;
		["tools", "capabilities", "access", "usage", "events"].forEach((name) => {
			const panel = $(`#server-dialog-panel-${name}`, body);
			if (panel) panel.hidden = true;
			const tab = $(`[data-mc-server-tab="${name}"]`, tabs);
			if (tab) {
				tab.setAttribute("aria-selected", "false");
				tab.classList.remove("active");
				tab.tabIndex = -1;
			}
		});
		const active = $(`[data-server-dialog-tab="${CSS.escape(baseTab)}"]`, tabs);
		active?.classList.add("active");
		active?.setAttribute("aria-selected", "true");
		active && (active.tabIndex = 0);
		revealServerSheetTab(active);
	}

	function serverSheetTabKeydown(event) {
		if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
		const tabs = $$('[role="tab"]', state.nodes.serverDialogTabs).filter(
			visible,
		);
		const current = tabs.indexOf(event.currentTarget);
		let next = current;
		if (event.key === "Home") next = 0;
		else if (event.key === "End") next = tabs.length - 1;
		else
			next =
				(current + (event.key === "ArrowRight" ? 1 : -1) + tabs.length) %
				tabs.length;
		event.preventDefault();
		tabs[next]?.click();
		tabs[next]?.focus();
	}

	function renderChrome() {
		const model = metrics();
		let tone = "neutral";
		let label = "Checking runtime";
		let detail = "Waiting for local state";
		if (model.runtime.offline) {
			tone = "bad";
			label = "Local service unavailable";
			detail = model.runtime.load || "Backend link failed";
		} else if (
			!model.runtime.ready &&
			/waiting|loading|initializing|checking|unknown|not loaded|—/.test(
				`${model.runtime.source} ${model.runtime.load} ${model.runtime.note}`.toLowerCase(),
			)
		) {
			tone = "neutral";
			label = "Checking runtime";
			detail = "Waiting for local service";
		} else if (!model.servers.length) {
			tone = "neutral";
			label = "Setup required";
			detail = "No integrations configured";
		} else if (model.review) {
			tone = "warn";
			label = `${model.review} server${model.review === 1 ? "" : "s"} need attention`;
			detail = `${model.ready} working · ${model.tools} tools`;
		} else {
			tone = "good";
			label = "Ready";
			detail = `${model.ready} working · ${model.tools} tools`;
		}

		const sidebar = $("[data-mc-sidebar-status]");
		if (sidebar) {
			sidebar.dataset.tone = tone;
			$("strong", sidebar).textContent = label;
			$("small", sidebar).textContent = detail;
		}
		const topbar = $("[data-mc-topbar-status]");
		if (topbar) {
			topbar.dataset.tone = tone;
			$("strong", topbar).textContent = label;
			$("small", topbar).textContent = detail;
		}
		$$("[data-mc-issue-count]").forEach((item) => {
			item.textContent = model.review;
			item.hidden = model.review === 0;
		});
		$$("[data-mc-issue-dot]").forEach((item) => {
			item.hidden = model.review === 0;
		});
		const guide = setupGuideModel(model);
		$$("[data-mc-setup]").forEach((button) => {
			button.hidden = guide.finished && state.setupDismissed;
			const progress = $("[data-mc-setup-progress]", button);
			if (progress) progress.textContent = `${guide.complete}/${guide.total}`;
			button.dataset.complete = String(guide.finished);
			button.setAttribute(
				"aria-label",
				guide.finished
					? "Open completed setup guide"
					: `Open setup guide, ${guide.complete} of ${guide.total} steps complete`,
			);
		});
		const liveButton = $("[data-mc-open-live]");
		if (liveButton) {
			const liveCount = model.liveSessions.length;
			const leaseCount = model.activeLeases.length;
			liveButton.hidden = liveCount === 0 && leaseCount === 0;
			const countNode = $("[data-mc-live-count]", liveButton);
			const detailNode = $("[data-mc-live-detail]", liveButton);
			if (countNode)
				countNode.textContent = `${liveCount} session${liveCount === 1 ? "" : "s"}`;
			if (detailNode)
				detailNode.textContent = `${leaseCount} lease${leaseCount === 1 ? "" : "s"}`;
			liveButton.setAttribute(
				"aria-label",
				`Open live sessions: ${liveCount} sessions and ${leaseCount} leases`,
			);
		}
	}

	function renderAll() {
		const focusToken = state.pendingFocusToken || captureDashboardFocus();
		state.pendingFocusToken = null;
		state.auditRecordCache = null;
		state.serverModelCache = null;
		state.serverConflictCache = null;
		renderChrome();
		renderHome();
		renderIntegrations();
		renderApplications();
		renderActivity();
		renderObservabilitySettings();
		renderProtocolReadiness();
		annotateRuntimePanels();
		enhanceServerDialog();
		restoreDashboardFocus(focusToken);
	}

	function annotateRuntimePanels() {
		$$(
			".mc-runtime-settings .item, .mc-runtime-settings .list > article",
		).forEach((item) => {
			item.dataset.mcTone = toneFrom(text(item));
		});
		if (state.nodes.accessReview) {
			const sourceTone =
				["good", "warn", "bad"].find((tone) =>
					state.nodes.accessReview.classList.contains(tone),
				) ||
				state.nodes.accessReview.dataset.tone ||
				toneFrom(text(state.nodes.accessReview));
			state.nodes.accessReview.dataset.mcTone = sourceTone;
			$$(".access-review-card", state.nodes.accessReview).forEach((card) => {
				card.dataset.mcTone =
					["good", "warn", "bad"].find((tone) =>
						card.classList.contains(tone),
					) || toneFrom(text(card));
			});
		}
	}

	function switchView(view, { updateHash = true, focus = true } = {}) {
		if (!state.hosts[view]) view = "home";
		state.view = view;
		document.documentElement.dataset.mcCurrentView = view;
		Object.entries(state.hosts).forEach(([id, host]) => {
			host.hidden = id !== view;
			if (id === view) {
				host.removeAttribute("data-mc-entering");
				requestAnimationFrame(() => {
					host.setAttribute("data-mc-entering", "true");
					window.setTimeout(
						() => host.removeAttribute("data-mc-entering"),
						state.motion === "off" ? 0 : 260,
					);
				});
			}
		});
		$$("[data-mc-view]").forEach((button) => {
			if (button.dataset.mcView === view)
				button.setAttribute("aria-current", "page");
			else button.removeAttribute("aria-current");
		});
		const [title] = VIEW_META[view];
		document.title = `${title} · MCPace`;
		const announcer = $("#mc-view-announcer");
		if (announcer) announcer.textContent = `Now showing ${title}.`;
		if (updateHash && location.hash !== `#${view}`) {
			try {
				history.pushState(null, "", `#${view}`);
			} catch (_) {
				location.hash = view;
			}
		}
		if (view === "home") renderHome();
		else if (view === "integrations") renderIntegrations();
		else if (view === "applications") renderApplications();
		else if (view === "activity") renderActivity();
		else if (view === "settings") {
			setSettingsTab(state.settingsTab);
			updatePreferenceControls();
			renderObservabilitySettings();
			renderProtocolReadiness();
		}
		const main = $("#mc-product-main");
		const activeHost = state.hosts[view];
		const heading = activeHost?.querySelector("h1, h2");
		if (heading) {
			if (!heading.id) heading.id = `mc-view-heading-${view}`;
			heading.tabIndex = -1;
			activeHost.setAttribute("aria-labelledby", heading.id);
			main?.setAttribute("aria-labelledby", heading.id);
		}
		window.scrollTo({ top: 0, behavior: "auto" });
		if (focus)
			requestAnimationFrame(() =>
				(heading || main)?.focus({ preventScroll: true }),
			);
	}

	function refreshRuntime() {
		const api = dashboardApi();
		if (api?.refreshDashboard)
			api.refreshDashboard({ force: true, reason: "product-ui" });
		else $("#refresh-button")?.click();
		toast(
			"Refreshing runtime",
			"MCPace requested the latest overview and log window.",
		);
		scheduleRender(450);
	}

	function removeToast(item, restoreFocus = true) {
		const shouldRestore = restoreFocus && item.contains(document.activeElement);
		const token = shouldRestore ? captureDashboardFocus() : null;
		item.remove();
		if (shouldRestore) {
			state.pendingFocusToken = null;
			restoreDashboardFocus(token);
		}
	}

	function toast(title, message, tone = "good") {
		const region = $(".mc-toast-region");
		if (!region) return;
		$$(".mc-toast:not(.mc-toast-action)", region).forEach((existing) =>
			removeToast(existing),
		);
		const item = document.createElement("div");
		item.className = "mc-toast";
		item.dataset.tone = tone;
		setProductHtml(
			item,
			`${tone === "warn" || tone === "bad" ? ICON.warning : ICON.check}<div><strong>${escapeHtml(title)}</strong><span>${escapeHtml(message)}</span></div><button type="button" data-mc-toast-dismiss aria-label="Dismiss notification">Dismiss</button>`,
		);
		$("[data-mc-toast-dismiss]", item)?.addEventListener("click", () =>
			removeToast(item),
		);
		region.appendChild(item);
		let timer = 0;
		const pause = () => clearTimeout(timer);
		const resume = () => {
			clearTimeout(timer);
			timer = setTimeout(() => {
				if (!item.matches(":hover") && !item.contains(document.activeElement))
					removeToast(item, false);
			}, 10_000);
		};
		item.addEventListener("mouseenter", pause);
		item.addEventListener("mouseleave", resume);
		item.addEventListener("focusin", pause);
		item.addEventListener("focusout", resume);
		resume();
	}

	function toastAction(title, message, actionLabel, action, tone = "good") {
		const region = $(".mc-toast-region");
		if (!region) return;
		$$(".mc-toast:not(.mc-toast-action)", region).forEach((existing) =>
			removeToast(existing),
		);
		$$(".mc-toast-action", region).forEach((existing) => removeToast(existing));
		const item = document.createElement("div");
		item.className = "mc-toast mc-toast-action";
		item.dataset.tone = tone;
		setProductHtml(
			item,
			`${tone === "warn" || tone === "bad" ? ICON.warning : ICON.check}<div><strong>${escapeHtml(title)}</strong><span>${escapeHtml(message)}</span></div><button type="button" data-mc-toast-action>${escapeHtml(actionLabel)}</button><button type="button" data-mc-toast-dismiss aria-label="Dismiss notification">Dismiss</button>`,
		);
		$("[data-mc-toast-action]", item)?.addEventListener("click", () => {
			removeToast(item, false);
			action?.();
		});
		$("[data-mc-toast-dismiss]", item)?.addEventListener("click", () =>
			removeToast(item),
		);
		region.appendChild(item);
	}

	function bindGlobalEvents() {
		window.addEventListener("mcpace:dashboard-rendered", () =>
			scheduleRender(0),
		);
		$$("[data-mc-view]").forEach((button) =>
			button.addEventListener("click", () => switchView(button.dataset.mcView)),
		);
		$("[data-mc-home-link]")?.addEventListener("click", (event) => {
			event.preventDefault();
			switchView("home");
		});
		$$("[data-mc-add]").forEach((button) =>
			button.addEventListener("click", () => openAddDialog()),
		);
		$$("[data-mc-setup]").forEach((button) =>
			button.addEventListener("click", openSetupGuide),
		);
		$$("[data-mc-command-open]").forEach((button) =>
			button.addEventListener("click", openCommandDialog),
		);
		$("[data-mc-open-live]")?.addEventListener("click", () => {
			state.activityView = "live";
			writePreference("activityView", "live");
			switchView("activity");
			renderActivity();
		});
		$("[data-mc-refresh]")?.addEventListener("click", refreshRuntime);

		document.addEventListener("click", (event) => {
			const copy = event.target.closest("[data-mc-copy-value]");
			if (copy) {
				event.preventDefault();
				event.stopPropagation();
				copyText(copy.dataset.mcCopyValue, "Path");
				return;
			}
			const usageRow = event.target.closest(
				".mc-usage-open[data-mc-open-server]",
			);
			if (usageRow) {
				event.preventDefault();
				openServer(usageRow.dataset.mcOpenServer, "usage");
			}
		});
		window.addEventListener("popstate", () => {
			const view = location.hash.replace(/^#/, "");
			switchView(state.hosts[view] ? view : "home", {
				updateHash: false,
				focus: false,
			});
		});

		document.addEventListener("keydown", (event) => {
			if (document.querySelector("dialog[open]")) return;
			const active = document.activeElement;
			const typing = active?.matches?.(
				'input,textarea,select,[contenteditable="true"]',
			);
			if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
				event.preventDefault();
				openCommandDialog();
				return;
			}
			if (
				event.altKey &&
				event.key === "/" &&
				!typing &&
				!state.addDialog?.open &&
				!state.commandDialog?.open
			) {
				event.preventDefault();
				if (state.view === "integrations")
					$("[data-mc-integration-search]", state.hosts.integrations)?.focus();
				else openCommandDialog();
				return;
			}
			if (
				!typing &&
				!event.ctrlKey &&
				!event.metaKey &&
				event.altKey &&
				/^[1-5]$/.test(event.key)
			) {
				event.preventDefault();
				switchView(
					["home", "integrations", "applications", "activity", "settings"][
						Number(event.key) - 1
					],
				);
			}
		});

		state.nodes.serverDialog?.addEventListener("keydown", (event) =>
			trapDialogFocus(event, state.nodes.serverDialog),
		);
		state.nodes.serverDialog?.addEventListener("close", () => {
			state.nodes.serverDialog.classList.remove("mc-show-technical");
			const opener = state.serverDialogOpener;
			const focusToken = state.serverDialogFocusToken;
			const returnView = state.serverDialogReturnView;
			state.serverDialogOpener = null;
			state.serverDialogFocusToken = null;
			state.serverDialogReturnView = null;
			if (returnView && returnView !== state.view)
				switchView(returnView, { focus: false });
			if (opener?.isConnected && visible(opener))
				opener.focus?.({ preventScroll: true });
			else if (focusToken) restoreDashboardFocus(focusToken);
			else {
				const heading = $("h1, h2", state.hosts[state.view]);
				if (heading) {
					heading.tabIndex = -1;
					heading.focus({ preventScroll: true });
				}
			}
		});
	}

	function scheduleRender(delay = 100) {
		clearTimeout(state.renderTimer);
		state.renderTimer = setTimeout(() => {
			discoverNodes();
			renderAll();
		}, delay);
	}

	function ensureAllServersVisible() {
		const toggle = state.nodes.originalEnabledToggle;
		if (!toggle || state.signatures.showAllRequested) return;
		if (
			toggle.getAttribute("aria-pressed") === "true" ||
			/show all/i.test(text(toggle))
		) {
			state.signatures.showAllRequested = true;
			toggle.click();
		}
	}

	function observeBackendDom() {
		const observer = new MutationObserver((mutations) => {
			const relevant = mutations.some((mutation) => {
				const target =
					mutation.target.nodeType === Node.ELEMENT_NODE
						? mutation.target
						: mutation.target.parentElement;
				if (!target || target.closest("#mc-product-shell")) return false;
				if (
					!target.closest(
						".mc-row-status,.mc-row-source,.mc-row-usage,.mc-row-route-meta,.mc-inline-server-toggle,.mc-client-location,.mc-client-guidance",
					)
				)
					return false;
				return true;
			});
			if (relevant) scheduleRender(80);
		});
		const roots = unique([
			state.nodes.serverList,
			state.nodes.clientList,
			state.nodes.clientResult,
			state.nodes.activityList,
			state.nodes.auditList,
			state.nodes.logList,
			state.nodes.systemState,
			state.nodes.loadState,
			state.nodes.loadNote,
			state.nodes.refreshChip,
		]);
		roots.forEach((root) =>
			observer.observe(root, {
				childList: true,
				subtree: true,
				characterData: true,
			}),
		);
		state.observer = observer;
	}

	function boot() {
		if (state.started || document.getElementById("mc-product-shell")) return;
		state.started = true;
		buildProductShell();
		ensureAllServersVisible();
		const initial = location.hash.replace(/^#/, "");
		switchView(state.hosts[initial] ? initial : "home", {
			updateHash: false,
			focus: false,
		});
		renderAll();
		observeBackendDom();
		setTimeout(() => {
			ensureAllServersVisible();
			scheduleRender(0);
		}, 700);
	}

	if (document.readyState === "loading")
		document.addEventListener("DOMContentLoaded", () => setTimeout(boot, 0), {
			once: true,
		});
	else setTimeout(boot, 0);
})();
/* MCPACE_ADAPTIVE_COMPOSITION_V2_START */
(() => {
	const VERSION = "adaptive-composition-v2";
	const ROOT_ID = "mc-product-shell";
	const ROLE = "adaptiveRole";
	const SLOT = "layoutSlot";
	const MODE_BREAKPOINTS = Object.freeze({
		wide: 1280,
		rail: 900,
		compact: 680,
	});
	let shell = null;
	let frame = 0;
	let stableTimer = 0;
	let shellObserver = null;
	let rootObserver = null;
	let resizeObserver = null;

	const all = (root, selector) =>
		Array.from(root?.querySelectorAll?.(selector) || []);
	const classText = (element) =>
		String(
			element?.className?.baseVal ?? element?.className ?? "",
		).toLowerCase();
	const idText = (element) => String(element?.id || "").toLowerCase();
	const compactText = (element) =>
		String(element?.textContent || "")
			.replace(/\s+/g, " ")
			.trim()
			.toLowerCase();
	const hasAny = (value, words) => words.some((word) => value.includes(word));
	const signature = (element) =>
		`${element?.tagName || ""} ${idText(element)} ${classText(element)}`;

	function modeForWidth(width) {
		if (width >= MODE_BREAKPOINTS.wide) return "wide";
		if (width >= MODE_BREAKPOINTS.rail) return "rail";
		if (width >= MODE_BREAKPOINTS.compact) return "compact";
		return "phone";
	}

	function viewportWidth() {
		return Math.round(
			window.visualViewport?.width ||
				window.innerWidth ||
				document.documentElement.clientWidth ||
				0,
		);
	}

	function viewportHeight() {
		return Math.round(
			window.visualViewport?.height ||
				window.innerHeight ||
				document.documentElement.clientHeight ||
				0,
		);
	}

	function setViewportState() {
		const width = viewportWidth();
		const height = viewportHeight();
		const mode = modeForWidth(width);
		const root = document.documentElement;
		if (root.dataset.mcViewport !== mode) root.dataset.mcViewport = mode;
		const widthValue = `${width}px`;
		const heightValue = `${height}px`;
		if (root.style.getPropertyValue("--mc-ac-visual-width") !== widthValue)
			root.style.setProperty("--mc-ac-visual-width", widthValue);
		if (root.style.getPropertyValue("--mc-ac-visual-height") !== heightValue)
			root.style.setProperty("--mc-ac-visual-height", heightValue);
		if (!shell) return;
		const modeChanged = shell.dataset.viewportMode !== mode;
		const heightChanged =
			shell.style.getPropertyValue("--mc-ac-viewport-height") !== heightValue;
		if (!modeChanged && !heightChanged) return;
		shell.dataset.viewportMode = mode;
		shell.style.setProperty("--mc-ac-viewport-height", heightValue);
		shell.dataset.viewportStable = "false";
		window.clearTimeout(stableTimer);
		stableTimer = window.setTimeout(
			() => {
				if (shell) shell.dataset.viewportStable = "true";
			},
			modeChanged ? 180 : 80,
		);
	}

	function setRole(element, role) {
		if (element instanceof Element && !element.dataset[ROLE])
			element.dataset[ROLE] = role;
		return element;
	}

	function setLayoutRole(element, role) {
		if (!(element instanceof Element)) return element;
		setRole(element, role);
		if (!element.dataset.layoutRole) element.dataset.layoutRole = role;
		return element;
	}

	function closestDirectChild(element, root) {
		let node = element;
		while (node?.parentElement && node.parentElement !== root)
			node = node.parentElement;
		return node?.parentElement === root ? node : null;
	}

	function findFirst(root, predicate) {
		if (!(root instanceof Element)) return null;
		const nodes = [root, ...all(root, "*")];
		return (
			nodes.find((node) => {
				try {
					return predicate(node);
				} catch {
					return false;
				}
			}) || null
		);
	}

	function findByKeywords(root, words, tags = []) {
		return findFirst(root, (element) => {
			if (tags.includes(element.tagName)) return true;
			return hasAny(signature(element), words);
		});
	}

	function assignLayoutSlot(element, role) {
		if (!(element instanceof Element) || !shell) return;
		setLayoutRole(element, role);
		const direct = closestDirectChild(element, shell);
		if (direct) {
			if (!direct.dataset[SLOT]) direct.dataset[SLOT] = role;
			if (!direct.dataset.layoutRole) direct.dataset.layoutRole = role;
		}
	}

	function markNavLabels(nav) {
		if (!(nav instanceof Element)) return;
		const items = all(nav, 'a, button, [role="link"], [role="button"]');
		items.forEach((item) => {
			const label = all(item, "span, strong, small, b, em").find((node) => {
				const text = compactText(node);
				return text && !node.querySelector("svg") && node.children.length <= 2;
			});
			if (label) label.dataset.adaptiveNavLabel = "true";
			const accessible =
				item.getAttribute("aria-label") || compactText(label || item);
			if (accessible && !item.getAttribute("aria-label"))
				item.setAttribute("aria-label", accessible);
			if (accessible && !item.getAttribute("title"))
				item.setAttribute("title", accessible);
		});
	}

	function ensureMobileNav(sidebar, existing) {
		if (existing instanceof Element || !(sidebar instanceof Element) || !shell)
			return existing;
		const sourceItems = all(
			sidebar,
			'nav a, nav button, [role="navigation"] a, [role="navigation"] button',
		)
			.filter((item) => !item.closest('[data-adaptive-owned="true"]'))
			.slice(0, 5);
		if (sourceItems.length < 3) return null;

		const nav = document.createElement("nav");
		nav.dataset.layoutRole = "mobile-nav";
		nav.dataset.adaptiveOwned = "true";
		nav.setAttribute("aria-label", "Primary navigation");
		nav.style.setProperty("--mc-ac-mobile-items", String(sourceItems.length));

		sourceItems.forEach((source) => {
			const button = document.createElement("button");
			button.type = "button";
			const labelText =
				source.getAttribute("aria-label") || compactText(source) || "Open";
			const sourceSvg = source.querySelector("svg");
			if (sourceSvg) {
				const icon = sourceSvg.cloneNode(true);
				all(icon, "[id]").forEach((node) => node.removeAttribute("id"));
				icon.setAttribute("aria-hidden", "true");
				button.append(icon);
			}
			const label = document.createElement("span");
			label.dataset.adaptiveNavLabel = "true";
			label.textContent = labelText;
			button.append(label);
			button.setAttribute("aria-label", labelText);
			button.addEventListener("click", () => source.click());
			nav.append(button);
		});

		document.body.append(nav);
		return nav;
	}

	function annotatePageStructure(root) {
		const pages = all(
			root,
			'[data-product-page], [data-page], [class*="product-page"], [class*="workspace-page"], main > section',
		);
		pages.forEach((page) => {
			setRole(page, "page");
			const children = Array.from(page.children);
			const header = children.find((child) => {
				const sig = signature(child);
				return (
					hasAny(sig, [
						"page-head",
						"page_header",
						"page-header",
						"section-head",
						"hero",
					]) || Boolean(child.querySelector(":scope > h1, :scope > h2"))
				);
			});
			if (header) {
				setRole(header, "page-header");
				const actionArea = Array.from(header.children).find(
					(child) =>
						child.querySelector('button, a, [role="button"]') &&
						!child.matches("h1, h2, p"),
				);
				if (actionArea) setRole(actionArea, "header-actions");
			}
		});
	}

	function annotateToolbars(root) {
		const candidates = all(
			root,
			'[role="search"], [class*="toolbar"], [class*="filter-bar"], [class*="controls-bar"], [class*="page-actions"]',
		);
		candidates.forEach((bar) => {
			if (bar.closest('[data-adaptive-role="server-row"]')) return;
			if (
				!bar.querySelector(
					'input, button, select, [role="searchbox"], [role="tab"]',
				)
			)
				return;
			setRole(bar, "toolbar");
			const direct = Array.from(bar.children);
			const search = direct.find(
				(child) =>
					child.matches('[role="search"]') ||
					child.querySelector('input[type="search"], [role="searchbox"]'),
			);
			if (search) setRole(search, "toolbar-search");
			const filters = direct.find((child) => {
				const sig = signature(child);
				return (
					hasAny(sig, ["filter", "segmented", "chips"]) ||
					child.querySelectorAll(
						'[role="tab"], input[type="radio"], [aria-pressed]',
					).length >= 2
				);
			});
			if (filters) {
				setRole(filters, "filter-strip");
				filters.dataset.adaptiveScrollStrip = "true";
			}
			const actions =
				direct.findLast?.(
					(child) =>
						child !== search &&
						child !== filters &&
						child.querySelector("button, a, select"),
				) ||
				[...direct]
					.reverse()
					.find(
						(child) =>
							child !== search &&
							child !== filters &&
							child.querySelector("button, a, select"),
					);
			if (actions) setRole(actions, "toolbar-actions");
		});
	}

	function rowZone(child, index, total) {
		const sig = `${signature(child)} ${compactText(child).slice(0, 260)}`;
		if (
			hasAny(sig, ["action", "control", "switch", "toggle", "menu"]) ||
			child.querySelector('[role="switch"], input[type="checkbox"], button')
		) {
			if (
				index === total - 1 ||
				hasAny(sig, ["action", "control", "switch", "toggle"])
			)
				return "actions";
		}
		if (
			hasAny(sig, ["identity", "server-name", "integration-name", "primary"]) ||
			child.querySelector("h2, h3, h4")
		)
			return "identity";
		if (hasAny(sig, ["p95", "success", "calls", "usage", "latency", "metric"]))
			return "metrics";
		if (hasAny(sig, ["protect", "isolat", "worker", "serialized", "shared"]))
			return "protection";
		if (
			hasAny(sig, [
				"tool",
				"source",
				"transport",
				"path",
				"endpoint",
				"detail",
				"scope",
			])
		)
			return "details";
		if (index === 0) return "identity";
		if (index === total - 1 && child.querySelector('button, [role="switch"]'))
			return "actions";
		return "details";
	}

	function annotateServerRows(root) {
		const selector = [
			"[data-server-id]",
			"[data-server-key]",
			"[data-integration-id]",
			'[class*="server-row"]',
			'[class*="integration-row"]',
			'[class*="server-card"]',
		].join(",");
		const candidates = all(root, selector);
		const candidateSet = new Set(candidates);
		const outer = candidates.filter((element) => {
			let parent = element.parentElement;
			while (parent && parent !== root) {
				if (candidateSet.has(parent)) return false;
				parent = parent.parentElement;
			}
			return true;
		});

		outer.forEach((row) => {
			if (row.matches('dialog, [role="dialog"]')) return;
			setRole(row, "server-row");
			const children = Array.from(row.children).filter(
				(child) => child instanceof HTMLElement && !child.hidden,
			);
			if (children.length === 1 && children[0].children.length >= 2) {
				const body = children[0];
				body.dataset.adaptiveRowBody = "true";
				delete body.dataset.adaptiveZone;
				return;
			}
			children.forEach((child, index) => {
				if (!child.dataset.adaptiveZone)
					child.dataset.adaptiveZone = rowZone(child, index, children.length);
			});
		});
	}

	function annotateGrids(root) {
		all(
			root,
			'[class*="metric-grid"], [class*="stats-grid"], [class*="summary-grid"], [class*="kpi-grid"]',
		).forEach((grid) => setRole(grid, "metric-grid"));
		all(
			root,
			'[class*="card-grid"], [class*="application-grid"], [class*="client-grid"], [class*="integration-grid"]',
		).forEach((grid) => setRole(grid, "card-grid"));
	}

	function annotateSettings(root) {
		const settings = findByKeywords(root, [
			"settings-page",
			"settings-layout",
			"settings-workspace",
		]);
		if (!settings) return;
		setRole(settings, "settings-layout");
		const tablist = settings.querySelector(
			'[role="tablist"], [class*="settings-nav"], [class*="settings-tabs"], nav',
		);
		if (tablist) {
			setRole(tablist, "settings-nav");
			tablist.dataset.adaptiveScrollStrip = "true";
			tablist.setAttribute(
				"aria-orientation",
				window.innerWidth < 1280 ? "horizontal" : "vertical",
			);
		}
	}

	function dialogKind(dialog) {
		const sig = `${signature(dialog)} ${dialog.getAttribute("aria-label") || ""} ${compactText(dialog.querySelector("h1, h2, h3") || dialog).slice(0, 160)}`;
		if (
			hasAny(sig, [
				"add integration",
				"add server",
				"add-dialog",
				"integration-plan",
			])
		)
			return "add";
		if (hasAny(sig, ["server-dialog", "inspector", "integration settings"]))
			return "inspector";
		if (hasAny(sig, ["operation", "trace", "event detail"])) return "trace";
		if (hasAny(sig, ["setup guide", "onboarding"])) return "setup";
		if (hasAny(sig, ["command center", "command palette"])) return "command";
		return "general";
	}

	function annotateDialogs(root) {
		all(document, 'dialog, [role="dialog"]').forEach((dialog) => {
			if (dialog.dataset.adaptiveDialog !== VERSION)
				dialog.dataset.adaptiveDialog = VERSION;
			if (!dialog.dataset.dialogKind)
				dialog.dataset.dialogKind = dialogKind(dialog);
			all(dialog, '[role="tablist"]').forEach((tablist) => {
				setRole(tablist, "tab-strip");
				if (tablist.dataset.adaptiveScrollStrip !== "true")
					tablist.dataset.adaptiveScrollStrip = "true";
			});
		});
	}

	function annotateRowsAndPaths(root) {
		all(
			root,
			'[class*="event-row"], [class*="activity-row"], [class*="audit-row"], [data-audit-event]',
		).forEach((row) => setRole(row, "event-row"));
		all(
			root,
			'[class*="path-row"], [class*="config-path"], [class*="location-row"]',
		).forEach((row) => setRole(row, "path-row"));
	}

	function annotateScrollStrips(root) {
		all(
			root,
			'[role="tablist"], [class*="chip-row"], [class*="filter-strip"], [class*="segmented"]',
		).forEach((strip) => {
			if (
				strip.scrollWidth > strip.clientWidth ||
				strip.querySelectorAll('button, [role="tab"]').length >= 4
			) {
				strip.dataset.adaptiveScrollStrip = "true";
			}
		});
	}

	function annotate() {
		shell = document.getElementById(ROOT_ID);
		if (!shell) return false;
		if (shell.dataset.layoutSystem !== VERSION)
			shell.dataset.layoutSystem = VERSION;

		const sidebar = findByKeywords(
			shell,
			["sidebar", "side-nav", "navigation-drawer"],
			["ASIDE"],
		);
		const main = findByKeywords(
			shell,
			["product-main", "app-main", "workspace-main", "main-content"],
			["MAIN"],
		);
		const topbar =
			Array.from(shell.children).find((element) => {
				if (!(element instanceof HTMLElement)) return false;
				if (element.matches('.mc-topbar, [data-layout-role="topbar"]'))
					return true;
				return (
					element.tagName === "HEADER" &&
					hasAny(signature(element), [
						"topbar",
						"top-bar",
						"app-header",
						"product-header",
					])
				);
			}) ||
			findByKeywords(shell, [
				"topbar",
				"top-bar",
				"app-header",
				"product-header",
			]);
		let mobileNav = findByKeywords(document.body, [
			"bottom-nav",
			"mobile-nav",
			"tab-bar",
		]);

		if (sidebar) assignLayoutSlot(sidebar, "sidebar");
		if (main) assignLayoutSlot(main, "main");
		if (topbar) setLayoutRole(topbar, "topbar");
		if (mobileNav) setLayoutRole(mobileNav, "mobile-nav");

		const sidebarSlot = sidebar ? closestDirectChild(sidebar, shell) : null;
		const mainSlot = main ? closestDirectChild(main, shell) : null;
		const adaptiveGrid = String(
			Boolean(sidebarSlot && mainSlot && sidebarSlot !== mainSlot),
		);
		if (shell.dataset.adaptiveGrid !== adaptiveGrid)
			shell.dataset.adaptiveGrid = adaptiveGrid;

		if (sidebar) markNavLabels(sidebar);
		mobileNav = ensureMobileNav(sidebar, mobileNav);
		if (mobileNav) {
			setLayoutRole(mobileNav, "mobile-nav");
			markNavLabels(mobileNav);
			const count = Math.max(
				1,
				mobileNav.querySelectorAll(":scope > a, :scope > button").length,
			);
			const mobileItems = String(Math.min(count, 5));
			if (
				mobileNav.style.getPropertyValue("--mc-ac-mobile-items") !== mobileItems
			)
				mobileNav.style.setProperty("--mc-ac-mobile-items", mobileItems);
		}

		annotatePageStructure(shell);
		annotateToolbars(shell);
		annotateServerRows(shell);
		annotateGrids(shell);
		annotateSettings(shell);
		annotateDialogs(shell);
		annotateRowsAndPaths(shell);
		annotateScrollStrips(shell);
		setViewportState();
		return true;
	}

	function schedule() {
		if (frame) return;
		frame = window.requestAnimationFrame(() => {
			frame = 0;
			annotate();
		});
	}

	function scrollActiveControlIntoView(target) {
		if (!(target instanceof Element)) return;
		const strip = target.closest(
			'[data-adaptive-scroll-strip="true"], [role="tablist"]',
		);
		if (!strip) return;
		const stripBox = strip.getBoundingClientRect();
		const targetBox = target.getBoundingClientRect();
		if (
			targetBox.left < stripBox.left + 8 ||
			targetBox.right > stripBox.right - 8
		) {
			const motion = document.documentElement.dataset.mcMotion;
			target.scrollIntoView({
				inline: "center",
				block: "nearest",
				behavior:
					motion === "reduced" ||
					motion === "off" ||
					matchMedia("(prefers-reduced-motion: reduce)").matches
						? "auto"
						: "smooth",
			});
		}
	}

	function bindGlobalBehavior() {
		window.addEventListener("resize", schedule, { passive: true });
		window.visualViewport?.addEventListener("resize", schedule, {
			passive: true,
		});
		window.visualViewport?.addEventListener("scroll", setViewportState, {
			passive: true,
		});

		document.addEventListener(
			"click",
			(event) => {
				schedule();
				const control =
					event.target instanceof Element
						? event.target.closest(
								'[role="tab"], [aria-current="page"], [data-page]',
							)
						: null;
				if (control)
					window.requestAnimationFrame(() =>
						scrollActiveControlIntoView(control),
					);
			},
			true,
		);

		document.addEventListener(
			"keydown",
			(event) => {
				if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key))
					return;
				const active =
					event.target instanceof Element
						? event.target.closest('[role="tab"]')
						: null;
				if (active)
					window.requestAnimationFrame(() =>
						scrollActiveControlIntoView(document.activeElement),
					);
			},
			true,
		);
	}

	function observe() {
		const connectShellObserver = () => {
			shell = document.getElementById(ROOT_ID);
			if (!shell || shellObserver) return Boolean(shell);
			shellObserver = new MutationObserver((mutations) => {
				const meaningful = mutations.some((mutation) => {
					const target =
						mutation.target instanceof Element
							? mutation.target
							: mutation.target?.parentElement;
					return !target?.closest?.('[data-adaptive-owned="true"]');
				});
				if (meaningful) schedule();
			});
			shellObserver.observe(shell, { childList: true, subtree: true });
			rootObserver?.disconnect();
			rootObserver = null;
			return true;
		};

		if (!connectShellObserver()) {
			rootObserver = new MutationObserver(() => {
				if (connectShellObserver()) schedule();
			});
			rootObserver.observe(document.body || document.documentElement, {
				childList: true,
				subtree: true,
			});
		}

		if (typeof ResizeObserver === "function") {
			resizeObserver = new ResizeObserver(() => schedule());
			resizeObserver.observe(document.documentElement);
		}
	}

	function start() {
		bindGlobalBehavior();
		observe();
		schedule();
	}

	if (document.readyState === "loading")
		document.addEventListener("DOMContentLoaded", start, { once: true });
	else start();
})();
/* MCPACE_ADAPTIVE_COMPOSITION_V2_END */

/* MCPACE_SERVER_ATLAS_COMPLETION_V4 */
(() => {
	const VERSION = "server-atlas-completion-v4";
	const STORAGE_VIEW = "mcpace.integrationSurface.v1";
	const STORAGE_DENSITY = "mcpace.integrationDensity.v1";
	const ROOT_SELECTOR = "#mc-product-shell";
	const readSetting = (key) => {
		try {
			return localStorage.getItem(key);
		} catch {
			return null;
		}
	};
	const writeSetting = (key, value) => {
		try {
			localStorage.setItem(key, value);
		} catch {
			/* private/opaque origins keep an in-memory default */
		}
	};
	const owned = (node) =>
		node?.nodeType === 1 &&
		(node.id === "mc-atlas-persistent-controls" ||
			node.closest?.(
				"#mc-atlas-persistent-controls, .mc-atlas-fact-guide, .mc-atlas-generated-connections, .mc-atlas-compact-summary",
			));
	let scheduled = false;
	let observer;

	const norm = (value) =>
		String(value ?? "")
			.replace(/\s+/g, " ")
			.trim();
	const lower = (value) => norm(value).toLowerCase();
	const visible = (node) =>
		!!node &&
		!node.hidden &&
		getComputedStyle(node).display !== "none" &&
		getComputedStyle(node).visibility !== "hidden";

	function integrationsPage(root = document.querySelector(ROOT_SELECTOR)) {
		if (!root) return null;
		const direct = root.querySelector(
			'[data-product-page="integrations"], [data-page="integrations"], #mc-product-integrations, .mc-product-page-integrations',
		);
		if (direct) return direct;
		return (
			[
				...root.querySelectorAll('main section, main > div, [role="tabpanel"]'),
			].find((node) => {
				const heading = node.querySelector("h1, h2");
				return (
					heading && /integrations|servers/i.test(norm(heading.textContent))
				);
			}) || null
		);
	}

	function toolbar(page) {
		return (
			page?.querySelector(
				".mc-integrations-toolbar, .mc-atlas-toolbar, [data-integrations-toolbar], .mc-page-toolbar, header + .mc-toolbar, .mc-page-heading",
			) ||
			page?.querySelector("header") ||
			null
		);
	}

	function rowCandidates(page) {
		const selectors = [
			"[data-atlas-server-row]",
			"[data-server-row]",
			".mc-server-atlas-row",
			".mc-integration-row",
			".server-row",
		];
		const result = [];
		const seen = new Set();
		for (const selector of selectors) {
			for (const node of page?.querySelectorAll(selector) || []) {
				if (node.closest(".mc-atlas-generated-connections")) continue;
				if (!seen.has(node)) {
					seen.add(node);
					result.push(node);
				}
			}
		}
		return result.filter(
			(node) =>
				node.querySelector('button, [role="switch"]') ||
				node.dataset.serverName ||
				node.dataset.serverId,
		);
	}

	function exactButton(scope, labels) {
		const wanted = labels.map(lower);
		return [
			...(scope?.querySelectorAll('button, [role="tab"], [role="radio"]') ||
				[]),
		].find(
			(button) =>
				!button.closest("#mc-atlas-persistent-controls") &&
				wanted.includes(lower(button.textContent)),
		);
	}

	function nativeViewButton(page, mode) {
		const selectors =
			mode === "connections"
				? [
						'[data-mc-integration-layout="map"]',
						'[data-atlas-view="connections"]',
						'[data-view="connections"]',
						'[data-mode="connections"]',
						"#mc-atlas-connections-button",
					]
				: [
						'[data-mc-integration-layout="list"]',
						'[data-atlas-view="servers"]',
						'[data-view="servers"]',
						'[data-mode="servers"]',
						'[data-atlas-view="list"]',
						'[data-view="list"]',
						"#mc-atlas-servers-button",
					];
		for (const selector of selectors) {
			const found = page?.querySelector(selector);
			if (
				found &&
				found.id !== "mc-atlas-view-servers" &&
				found.id !== "mc-atlas-view-connections"
			)
				return found;
		}
		return exactButton(
			page,
			mode === "connections" ? ["Connections", "Routes"] : ["Servers", "List"],
		);
	}

	function nativePanel(page, mode) {
		const selectors =
			mode === "connections"
				? [
						"[data-mc-route-map]",
						'[data-atlas-panel="connections"]',
						'[data-view-panel="connections"]',
						".mc-atlas-connections-view",
						".mc-connections-view",
					]
				: [
						"[data-mc-integration-list-shell]",
						'[data-atlas-panel="servers"]',
						'[data-view-panel="servers"]',
						'[data-view-panel="list"]',
						".mc-atlas-server-list",
						".mc-server-list",
					];
		return (
			selectors
				.map((selector) => page?.querySelector(selector))
				.find(Boolean) || null
		);
	}

	function currentMode(page) {
		const stored = readSetting(STORAGE_VIEW);
		const nativeConnections = nativeViewButton(page, "connections");
		const pressed =
			nativeConnections?.getAttribute("aria-pressed") === "true" ||
			nativeConnections?.getAttribute("aria-selected") === "true" ||
			nativeConnections?.classList.contains("is-active");
		if (pressed) return "connections";
		return stored === "connections" ? "connections" : "servers";
	}

	function setPressed(controls, mode) {
		controls
			?.querySelectorAll("[data-atlas-persistent-mode]")
			.forEach((button) => {
				const active = button.dataset.atlasPersistentMode === mode;
				button.removeAttribute("aria-selected");
				button.setAttribute("aria-pressed", String(active));
				button.tabIndex = active ? 0 : -1;
			});
	}

	function extractFact(row, selectors, fallback = "") {
		for (const selector of selectors) {
			const node = row.querySelector(selector);
			if (node && norm(node.textContent)) return norm(node.textContent);
		}
		return fallback;
	}

	function rowData(row) {
		const text = norm(row.textContent);
		const name =
			row.dataset.serverName ||
			row.dataset.serverId ||
			extractFact(
				row,
				[
					"[data-atlas-name]",
					".mc-atlas-name",
					".mc-server-name",
					"h3",
					"h4",
					"strong",
				],
				"MCP server",
			);
		const client =
			row.dataset.client ||
			row.dataset.clientName ||
			extractFact(
				row,
				["[data-atlas-client]", ".mc-atlas-client", "[data-route-client]"],
				"Not observed",
			);
		const project =
			row.dataset.project ||
			row.dataset.projectName ||
			extractFact(
				row,
				["[data-atlas-project]", ".mc-atlas-project", "[data-route-project]"],
				"No project evidence",
			);
		const operation =
			row.dataset.lastOperation ||
			extractFact(
				row,
				[
					"[data-atlas-last-operation]",
					".mc-atlas-last-operation",
					".mc-last-tool",
				],
				"No retained operation",
			);
		const source =
			row.dataset.source ||
			extractFact(
				row,
				["[data-atlas-source]", ".mc-atlas-source", ".mc-server-source"],
				"Source available in Setup",
			);
		const capacity =
			row.dataset.capacity ||
			extractFact(
				row,
				["[data-atlas-capacity]", ".mc-atlas-capacity", ".mc-capacity-summary"],
				/max\s+(?:\d+|auto)/i.exec(text)?.[0] || "Automatic capacity",
			);
		const lease =
			/\b(?:\d+\s+leases?|current route ownership|active route)\b/i.exec(
				text,
			)?.[0] || "No current route ownership";
		const attention =
			/failed|timeout|review|required|attention|unauthor|denied|error/i.test(
				text,
			);
		return {
			name,
			client,
			project,
			operation,
			source,
			capacity,
			lease,
			attention,
		};
	}

	function generatedConnections(page) {
		let panel = page.querySelector(".mc-atlas-generated-connections");
		if (!panel) {
			panel = document.createElement("section");
			panel.className = "mc-atlas-generated-connections";
			panel.hidden = true;
			panel.setAttribute("aria-label", "Observed MCP connections");
			const rows = rowCandidates(page);
			const list = rows[0]?.parentElement;
			(list?.parentElement || page).append(panel);
		}
		return panel;
	}

	function buildGeneratedConnections(page) {
		const panel = generatedConnections(page);
		const rows = rowCandidates(page);
		const facts = rows.map(rowData);
		const groups = new Map();
		for (const fact of facts) {
			const key =
				fact.client === "Not observed"
					? "Enabled but not observed"
					: fact.client;
			if (!groups.has(key)) groups.set(key, []);
			groups.get(key).push(fact);
		}
		const ordered = [...groups.entries()].sort(([a], [b]) => {
			if (a === "Enabled but not observed") return 1;
			if (b === "Enabled but not observed") return -1;
			return a.localeCompare(b);
		});
		setProductHtml(
			panel,
			`
      <div class="mc-atlas-connections-heading">
        <div>
          <p class="mc-atlas-eyebrow">Observed topology</p>
          <h2>Who reaches which MCP server</h2>
          <p>Observed use and current route ownership are facts. Enabled definitions without evidence remain separate.</p>
        </div>
        <div class="mc-atlas-connection-legend" aria-label="Connection evidence legend">
          <span><i class="is-current"></i> Current route ownership</span>
          <span><i class="is-observed"></i> Observed use</span>
          <span><i class="is-unobserved"></i> Enabled, not observed</span>
        </div>
      </div>
      <div class="mc-atlas-connection-groups">
        ${
					ordered
						.map(
							([client, entries]) => `
          <section class="mc-atlas-connection-group ${client === "Enabled but not observed" ? "is-unobserved" : ""}">
            <header>
              <span class="mc-atlas-connection-client-icon" aria-hidden="true">${client === "Enabled but not observed" ? "–" : "↗"}</span>
              <div><h3>${escapeHtml(client)}</h3><p>${entries.length} ${entries.length === 1 ? "server" : "servers"}</p></div>
            </header>
            <div class="mc-atlas-connection-routes">
              ${entries
								.map(
									(fact) => `
                <article class="mc-atlas-connection-route ${fact.attention ? "needs-attention" : ""}">
                  <div class="mc-atlas-route-line" aria-hidden="true"></div>
                  <div class="mc-atlas-route-server">
                    <strong>${escapeHtml(fact.name)}</strong>
                    <span>${escapeHtml(fact.project)}</span>
                  </div>
                  <div class="mc-atlas-route-operation">
                    <span class="mc-atlas-route-label">Last operation</span>
                    <strong>${escapeHtml(fact.operation)}</strong>
                  </div>
                  <div class="mc-atlas-route-state">
                    <span>${escapeHtml(fact.lease)}</span>
                    <span>${escapeHtml(fact.capacity)}</span>
                  </div>
                  <button class="mc-atlas-route-open" type="button" data-atlas-open-server="${escapeHtml(fact.name)}" aria-label="Open ${escapeHtml(fact.name)} settings">Open</button>
                </article>`,
								)
								.join("")}
            </div>
          </section>`,
						)
						.join("") ||
					'<div class="mc-empty-state"><strong>No connection evidence yet</strong><p>Use a configured AI application, then return here to see observed routes.</p></div>'
				}
      </div>`,
		);
		return panel;
	}

	function escapeHtml(value) {
		return String(value ?? "").replace(
			/[&<>'"]/g,
			(char) =>
				({
					"&": "&amp;",
					"<": "&lt;",
					">": "&gt;",
					"'": "&#39;",
					'"': "&quot;",
				})[char],
		);
	}

	function applyMode(page, mode, { user = false } = {}) {
		const safeMode = mode === "connections" ? "connections" : "servers";
		if (user) writeSetting(STORAGE_VIEW, safeMode);
		page.dataset.atlasSurface = safeMode;
		const nativeButton = nativeViewButton(page, safeMode);
		if (nativeButton) {
			const selected =
				nativeButton.getAttribute("aria-selected") === "true" ||
				nativeButton.getAttribute("aria-pressed") === "true";
			if (!selected) nativeButton.click();
		}
		const serversPanel = nativePanel(page, "servers");
		let connectionsPanel = nativePanel(page, "connections");
		if (!connectionsPanel && safeMode === "connections")
			connectionsPanel = buildGeneratedConnections(page);
		if (serversPanel) serversPanel.hidden = safeMode !== "servers";
		if (connectionsPanel) connectionsPanel.hidden = safeMode !== "connections";
		const generated = page.querySelector(".mc-atlas-generated-connections");
		if (generated)
			generated.hidden =
				safeMode !== "connections" ||
				(!!nativePanel(page, "connections") &&
					nativePanel(page, "connections") !== generated);
		setPressed(
			document.getElementById("mc-atlas-persistent-controls"),
			safeMode,
		);
		document.dispatchEvent(
			new CustomEvent("mcpace:atlas-surface-changed", {
				detail: { mode: safeMode },
			}),
		);
	}

	function factGuide() {
		const details = document.createElement("details");
		details.className = "mc-atlas-fact-guide";
		setProductHtml(
			details,
			`
      <summary>How to read server state</summary>
      <div class="mc-atlas-fact-guide-panel">
        <dl>
          <div><dt>On</dt><dd>The definition is enabled in MCPace.</dd></div>
          <div><dt>Runtime</dt><dd>A process or route owner is currently observed, or only historical runtime evidence exists.</dd></div>
          <div><dt>MCP</dt><dd>The protocol handshake or server test completed.</dd></div>
          <div><dt>Tools</dt><dd>MCPace retained a successful <code>tools/list</code> result.</dd></div>
          <div><dt>Last operation</dt><dd>A tool may fail or time out without invalidating protocol and tools readiness.</dd></div>
        </dl>
      </div>`,
		);
		return details;
	}

	function ensureControls(page) {
		let controls = document.getElementById("mc-atlas-persistent-controls");
		if (!controls) {
			controls = document.createElement("div");
			controls.id = "mc-atlas-persistent-controls";
			controls.className = "mc-atlas-persistent-controls";
			controls.dataset.productOwned = "true";
			setProductHtml(
				controls,
				`
        <div class="mc-atlas-surface-switch" role="group" aria-label="Integration presentation">
          <button id="mc-atlas-view-servers" type="button" data-atlas-persistent-mode="servers"><span aria-hidden="true">≡</span> Servers</button>
          <button id="mc-atlas-view-connections" type="button" data-atlas-persistent-mode="connections"><span aria-hidden="true">⌁</span> Connections</button>
        </div>
        <div class="mc-atlas-control-divider" aria-hidden="true"></div>
        <label class="mc-atlas-density-control"><span>Rows</span><select aria-label="Server row density"><option value="auto">Auto</option><option value="compact">Compact</option><option value="comfortable">Comfortable</option></select></label>`,
			);
			controls.append(factGuide());
			const anchor = toolbar(page);
			if (anchor?.parentElement)
				anchor.insertAdjacentElement("afterend", controls);
			else page.prepend(controls);
			controls.addEventListener("click", (event) => {
				const button = event.target.closest("[data-atlas-persistent-mode]");
				if (!button) return;
				applyMode(page, button.dataset.atlasPersistentMode, { user: true });
			});
			if (!page.dataset.atlasRouteOpenBound) {
				page.dataset.atlasRouteOpenBound = "true";
				page.addEventListener("click", (event) => {
					const open = event.target.closest("[data-atlas-open-server]");
					if (!open) return;
					const wanted = lower(open.dataset.atlasOpenServer);
					const row = rowCandidates(page).find(
						(candidate) => lower(rowData(candidate).name) === wanted,
					);
					if (!row) return;
					const action = [...row.querySelectorAll("button, a")].find((node) =>
						/open settings|settings|view|summary/i.test(norm(node.textContent)),
					);
					if (action) action.click();
					else {
						applyMode(page, "servers", { user: true });
						row.scrollIntoView({
							block: "center",
							behavior:
								["reduced", "off"].includes(
									document.documentElement.dataset.mcMotion,
								) || matchMedia("(prefers-reduced-motion: reduce)").matches
									? "auto"
									: "smooth",
						});
						row.focus({ preventScroll: true });
					}
				});
			}
			controls
				.querySelector(".mc-atlas-surface-switch")
				.addEventListener("keydown", (event) => {
					if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key))
						return;
					event.preventDefault();
					const buttons = [
						...controls.querySelectorAll("[data-atlas-persistent-mode]"),
					];
					const current = buttons.indexOf(document.activeElement);
					const next =
						event.key === "Home"
							? 0
							: event.key === "End"
								? buttons.length - 1
								: (current +
										(event.key === "ArrowRight" ? 1 : -1) +
										buttons.length) %
									buttons.length;
					buttons[next].focus();
					buttons[next].click();
				});
			const density = controls.querySelector("select");
			density.value = readSetting(STORAGE_DENSITY) || "auto";
			density.addEventListener("change", () => {
				writeSetting(STORAGE_DENSITY, density.value);
				applyDensity(page, density.value);
			});
		}
		controls.hidden = false;
		return controls;
	}

	function deDuplicateNativeViewControls(page) {
		for (const mode of ["servers", "connections"]) {
			const button = nativeViewButton(page, mode);
			if (!button || button.closest("#mc-atlas-persistent-controls")) continue;
			button.dataset.atlasSuperseded = "true";
			button.tabIndex = -1;
			button.setAttribute("aria-hidden", "true");
		}
	}

	function applyDensity(page, mode = readSetting(STORAGE_DENSITY) || "auto") {
		const rowCount = rowCandidates(page).length;
		const effective =
			mode === "auto" ? (rowCount >= 10 ? "compact" : "comfortable") : mode;
		page.dataset.atlasDensity = effective;
		const select = document.querySelector(
			"#mc-atlas-persistent-controls select",
		);
		if (select && select.value !== mode) select.value = mode;
	}

	function enhanceRows(page) {
		for (const row of rowCandidates(page)) {
			row.dataset.atlasEnhanced = "true";
			row.removeAttribute("tabindex");
			row.removeAttribute("aria-label");
		}
	}

	function ensureResultsSummary(page) {
		const rows = rowCandidates(page);
		let summary = page.querySelector(
			'.mc-atlas-compact-summary[data-atlas-owned="true"]',
		);
		if (!rows.length) {
			summary?.remove();
			return;
		}
		if (!summary) {
			summary = document.createElement("div");
			summary.className = "mc-atlas-compact-summary";
			summary.dataset.atlasOwned = "true";
			summary.setAttribute("aria-live", "polite");
			const controls = document.getElementById("mc-atlas-persistent-controls");
			controls?.insertAdjacentElement("afterend", summary);
		}
		const facts = rows.map(rowData);
		const attention = facts.filter((fact) => fact.attention).length;
		const observed = facts.filter(
			(fact) => fact.client !== "Not observed",
		).length;
		const leases = facts.filter((fact) =>
			/\b(?:[1-9]\d*\s+leases?|current route ownership|active route)\b/i.test(
				fact.lease,
			),
		).length;
		const signature = [rows.length, observed, leases, attention].join(":");
		if (summary.dataset.atlasSignature === signature) return;
		summary.dataset.atlasSignature = signature;
		setProductHtml(
			summary,
			`<span><strong>${rows.length}</strong> servers</span><span><strong>${observed}</strong> observed routes</span><span><strong>${leases}</strong> current owners</span><span class="${attention ? "needs-attention" : ""}"><strong>${attention}</strong> need attention</span>`,
		);
	}

	function refresh() {
		scheduled = false;
		const root = document.querySelector(ROOT_SELECTOR);
		const page = integrationsPage(root);
		const existing = document.getElementById("mc-atlas-persistent-controls");
		if (!page || !visible(page)) {
			if (existing) existing.hidden = true;
			return;
		}
		const controls = ensureControls(page);
		deDuplicateNativeViewControls(page);
		enhanceRows(page);
		applyDensity(page);
		ensureResultsSummary(page);
		const mode = currentMode(page);
		setPressed(controls, mode);
		applyMode(page, mode);
		document.documentElement.dataset.mcpaceServerAtlas = VERSION;
	}

	function schedule() {
		if (scheduled) return;
		scheduled = true;
		requestAnimationFrame(refresh);
	}

	function mutationIsOwned(record) {
		if (record.type === "attributes") return owned(record.target);
		const changedNodes = [...record.addedNodes, ...record.removedNodes];
		return (
			owned(record.target) ||
			(changedNodes.length > 0 && changedNodes.every(owned))
		);
	}

	function observe() {
		observer?.disconnect();
		const root = document.querySelector(ROOT_SELECTOR) || document.body;
		observer = new MutationObserver((records) => {
			if (records.every(mutationIsOwned)) return;
			schedule();
		});
		observer.observe(root, {
			childList: true,
			subtree: true,
			attributes: true,
			attributeFilter: ["hidden", "class", "aria-selected", "aria-pressed"],
		});
	}

	document.addEventListener("mcpace:dashboard-rendered", schedule);
	document.addEventListener("mcpace:product-rendered", schedule);
	window.addEventListener("resize", schedule, { passive: true });
	if (document.readyState === "loading")
		document.addEventListener(
			"DOMContentLoaded",
			() => {
				observe();
				schedule();
			},
			{ once: true },
		);
	else {
		observe();
		schedule();
	}
})();
