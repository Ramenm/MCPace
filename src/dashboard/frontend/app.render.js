// MCPace dashboard render/action chunk. Loaded after /dashboard.js with defer so shared state is ready.
function render() {
	window.mcpaceCaptureDashboardFocus?.();
	const overview = state.overview;
	if (!overview) return;
	const doctor = overview.doctor || {};
	const project = doctor.project || {};
	const hub = overview.hub || {};
	const readiness = overview.readiness || {};
	const servers = normalizeServers(overview.servers);
	const instances = normalizeInstances(overview.instances);
	const clients = normalizeClients(overview.clients);
	const leases = normalizeLeases(overview.leases);
	const runtime = overview.runtime || {};
	const http = runtime.http || {};
	const pool = runtime.upstreamSessionPool || {};
	const sessionStore = runtime.httpSessionStore || {};
	const cache = overview.cache || {};
	const instanceSummary = overview.instances?.summary || {};
	const groups = groupByServer(instances);
	const policyRows = buildPolicyRows(servers, instances);
	const attentionItems = buildAttentionItems(
		hub,
		readiness,
		policyRows,
		leases,
	);
	const activePolicyRows = policyRows.filter(
		(row) => row.server?.effectiveEnabled || row.risk.rank <= 1,
	);
	const active = num(http.activeConnections);
	const max = num(http.maxConnections);
	const failed = num(http.failedConnections);
	const completed =
		num(http.completedConnections) || num(http.acceptedConnections);
	const saturation = max ? Math.round((active / max) * 100) : 0;
	const enabledCount = servers.filter(
		(server) => server.effectiveEnabled,
	).length;
	const statefulCount = servers.filter((server) =>
		["stateful", "external", "interactive", "side-effecting"].includes(
			server.runtimeType,
		),
	).length;
	const needsTrustCount = activePolicyRows.filter(
		(row) => row.risk.rank === 2,
	).length;
	const badPolicies = activePolicyRows.filter(
		(row) => row.risk.tone === "bad",
	).length;
	const warnPolicies = activePolicyRows.filter(
		(row) => row.risk.tone === "warn",
	).length;
	const attentionTotal = attentionItems.length;
	const evidenceAttentionCount = activePolicyRows.filter((row) => {
		const evidence = serverToolEvidence(row.server);
		return row.server?.effectiveEnabled && (!evidence.checked || !evidence.ok);
	}).length;
	const attentionCount = Math.max(
		attentionTotal,
		badPolicies,
		evidenceAttentionCount,
	);

	const runtimeReady = Boolean(
		readiness.runtimePrerequisitesReady ?? hub.readyForRuntimeOps,
	);
	const systemTone = !runtimeReady
		? "bad"
		: attentionItems.some((item) => item.tone === "bad")
			? "bad"
			: attentionItems.length
				? "warn"
				: "good";
	document.body.dataset.systemTone = systemTone;
	setSignalTones([
		[els.systemState, systemTone],
		[els.attentionCount, attentionCount ? systemTone : "good"],
		[
			els.serverCount,
			servers.length ? (enabledCount ? "good" : "warn") : "warn",
		],
		[
			els.loadState,
			state.backend.overview?.ok ? msTone(state.backend.overview.ms) : "bad",
		],
	]);
	els.systemState.textContent = !runtimeReady
		? "Blocked"
		: attentionItems.length
			? "Needs action"
			: "Ready";
	setSafeHtml(
		els.systemNote,
		`${escapeHtml(hub.status || hub.health || "unknown")} · updated ${escapeHtml(fmtDate(overview.generatedAtMs))}`,
	);
	els.attentionCount.textContent = String(attentionCount);
	els.attentionNote.textContent = attentionItems.length
		? attentionItems[0]?.title ||
			`${badPolicies} blocker(s) · ${warnPolicies} guarded`
		: "No active action.";
	els.serverCount.textContent = `${enabledCount}/${servers.length}`;
	els.serverNote.textContent = `${needsTrustCount} guarded · ${statefulCount} stateful/effectful`;
	const overviewMs = state.backend.overview?.ok
		? fmtMs(state.backend.overview.ms)
		: "offline";
	els.loadState.textContent = state.backend.overview?.ok ? overviewMs : "Check";
	els.loadNote.textContent = `API ${state.backend.overview?.ok ? "connected" : "not connected"} · ${active}/${max || "?"} active HTTP`;

	document.body.dataset.density = state.density;
	const bucketLabel =
		state.bucket === "all"
			? state.enabledOnly
				? "enabled only"
				: "all servers"
			: `${state.bucket} servers`;
	setChip(
		els.serverFilterChip,
		bucketLabel,
		state.bucket === "blocked"
			? "bad"
			: state.bucket === "protected" || state.bucket === "off"
				? "warn"
				: "good",
	);
	els.toggleEnabled.textContent = state.enabledOnly
		? "Show All"
		: "Show Enabled";
	els.toggleEnabled.setAttribute("aria-pressed", String(state.enabledOnly));
	els.clearSearch.disabled = !state.query;
	updateRefreshChip();

	renderOperations({
		overview,
		hub,
		readiness,
		servers,
		instances,
		attentionItems,
		attentionTotal,
		systemTone,
		runtimeReady,
		enabledCount,
		active,
		max,
		completed,
		failed,
		saturation,
		badPolicies,
		warnPolicies,
	});
	renderDecisionRunway({
		overview,
		hub,
		servers,
		clients,
		attentionItems,
		attentionTotal,
		runtimeReady,
		enabledCount,
		active,
		max,
		failed,
		saturation,
		badPolicies,
		warnPolicies,
	});
	renderBaseSetup({
		overview,
		hub,
		servers,
		clients,
		runtimeReady,
	});
	renderAccessReview(overview.accessReview, servers);
	renderNextAction({
		overview,
		hub,
		servers,
		clients,
		attentionItems,
		attentionTotal,
		runtimeReady,
		enabledCount,
		active,
		max,
		failed,
		saturation,
		badPolicies,
		warnPolicies,
	});
	renderConnectionMap(overview, servers, clients);
	renderUserReadiness(overview.userReadiness, servers, clients);
	renderAttention(attentionItems, systemTone);
	renderServerImportPanel();
	renderDiscoveryPanel();
	renderClientSetup(clients, overview.clients || {});
	renderAutomation(overview, servers, instances);
	renderAutoSetup(servers, instances);
	renderOperatorPlan(overview.operatorPlan, servers, instances);
	renderFleetBoard(servers, instances);
	renderSetupQueue({
		overview,
		hub,
		servers,
		clients,
		instances,
		attentionItems,
		attentionTotal,
		runtimeReady,
	});
	renderServers(servers, instances, groups);
	renderContext(overview, readiness, project, hub, cache, runtime);
	renderInstances(instances, instanceSummary);
	renderRuntime(runtime, hub, readiness, project);
	renderPolicies(policyRows);
	renderCapacity(runtime, cache, active, max, pool, sessionStore);
	renderTelemetry(servers, instances, http, state.logs);
	renderActivity(leases, http, pool, sessionStore);
	renderClients(clients, overview.clients || {});
	renderProtocolCompatibility(overview, servers, clients, instances);
	renderLogs(state.logs);
	renderSummaryChips(enabledCount, servers.length);
	if (els.serverDialog?.open && state.selectedServer)
		renderServerDialogByName(state.selectedServer);
	window.dispatchEvent(
		new CustomEvent("mcpace:dashboard-rendered", {
			detail: { generatedAtMs: overview.generatedAtMs || null },
		}),
	);
}

function renderAttention(items, systemTone) {
	if (!items.length) {
		setSafeHtml(
			els.attentionList,
			`<article class="item good"><div class="item-head"><div class="name">Nothing needs attention</div>${chip("ready", "good")}</div><div class="meta">Runtime, routing, and server inventory have no visible blockers. Advanced diagnostics can stay closed.</div></article>`,
		);
		return;
	}
	setSafeHtml(
		els.attentionList,
		items
			.slice(0, 5)
			.map(
				(item) => `
          <article class="${itemClass(item.tone || systemTone)}">
            <div class="item-head"><div class="name">${escapeHtml(item.title)}</div>${chip(item.tag || "attention", item.tone || systemTone)}</div>
            <div class="meta">${escapeHtml(item.meta)}</div>
          </article>
        `,
			)
			.join("") +
			(items.length > 5
				? `<div class="note">${items.length - 5} more grouped item(s) in Advanced diagnostics.</div>`
				: ""),
	);
}

function renderSummaryChips(enabledCount, total) {
	const offCount = Math.max(total - enabledCount, 0);
	setSafeHtml(
		els.serverChips,
		[
			chip(`${enabledCount} enabled`, enabledCount ? "good" : "warn"),
			chip(`${offCount} parked`, offCount ? "warn" : "good"),
		].join(""),
	);
}

function readout(label, value, meta, tone = "warn") {
	return `<div class="backend-readout ${escapeHtml(tone)}"><strong>${escapeHtml(value)}</strong><span>${escapeHtml(label)} · ${escapeHtml(meta)}</span></div>`;
}

function stepCard(title, meta, tone = "warn") {
	return `<div class="ops-step ${escapeHtml(tone)}"><strong>${escapeHtml(title)}</strong><span>${escapeHtml(meta)}</span></div>`;
}

function msTone(ms) {
	const value = num(ms, 0);
	if (!value) return "warn";
	if (value < 250) return "good";
	if (value < 1500) return "warn";
	return "bad";
}

function plural(value, singular, pluralForm = `${singular}s`) {
	return `${value} ${value === 1 ? singular : pluralForm}`;
}

function decisionCard(
	label,
	value,
	meta,
	tone = "warn",
	progress = 0,
	icon = "•",
) {
	const bounded = Math.max(0, Math.min(100, Math.round(progress)));
	return `<article class="decision-card ${escapeHtml(tone)}">
          <span class="decision-icon">${escapeHtml(icon)}</span>
          <div>${chip(label, tone)}<strong>${escapeHtml(value)}</strong><p>${escapeHtml(meta)}</p></div>
          <div class="decision-meter" aria-hidden="true"><span style="width: ${bounded}%"></span></div>
        </article>`;
}

function baseStepCard(index, step = {}, currentKey = "") {
	const safeTone = ["good", "warn", "bad"].includes(String(step.tone))
		? String(step.tone)
		: "warn";
	const key =
		String(step.key || step.label || index)
			.replace(/[^a-z0-9-]/gi, "")
			.toLowerCase() || `step-${index}`;
	const isCurrent = Boolean(currentKey && key === currentKey);
	const currentClass = isCurrent ? " active" : "";
	const currentAttr = isCurrent ? ' aria-current="step"' : "";
	const label = text(step.label, `Step ${index}`);
	const action = String(step.action || "").trim();
	const actionLabel = text(
		step.actionLabel,
		labelForBaseStepAction(action, key),
	);
	const actionButton =
		action && isCurrent
			? `<button type="button" data-global-action="${escapeHtml(action)}" aria-label="${escapeHtml(actionLabel)} for ${escapeHtml(label)}">${escapeHtml(actionLabel)}</button>`
			: "";
	return `<article class="base-step ${safeTone}${currentClass}"${currentAttr} data-base-step="${escapeHtml(key)}"><span>${String(index).padStart(2, "0")}</span><strong>${escapeHtml(step.title || label)}</strong><p>${escapeHtml(step.body || "")}</p>${actionButton}</article>`;
}

function labelForBaseStepAction(action, key = "") {
	if (action === "repair") return "Repair";
	if (action === "check-link") return "Check link";
	if (action === "refresh") return "Refresh";
	if (action === "clients" || action === "client")
		return key === "client" ? "Connect" : "Open";
	if (action === "import-server") return "Import";
	if (action === "add-server") return "Add";
	if (action === "servers") {
		if (key === "tools") return "Open servers";
		if (key === "routing") return "Review";
		if (key === "source") return "Open sources";
		return "Open servers";
	}
	if (action === "discover") return "Discover";
	return action ? "Open" : "";
}

function normalizeFoundationAction(action, fallback = "refresh") {
	const safeAction = String(action?.action || fallback || "refresh").replace(
		/[^a-z-]/g,
		"",
	);
	return {
		label: text(
			action?.label,
			humanizeKey(safeAction || fallback || "refresh"),
		),
		action: safeAction || fallback || "refresh",
	};
}

function normalizeFoundationStep(step, index) {
	const tone = ["good", "warn", "bad"].includes(
		String(step?.status || step?.tone),
	)
		? String(step.status || step.tone)
		: "warn";
	const key = String(step?.key || step?.label || `step-${index + 1}`)
		.replace(/[^a-z0-9-]/gi, "")
		.toLowerCase();
	const action = String(step?.action || "").trim();
	return {
		tone,
		key,
		label: text(step?.label || step?.key, `Step ${index + 1}`),
		title: text(step?.title, step?.label || `Step ${index + 1}`),
		body: text(step?.body, "No detail reported."),
		action,
		actionLabel: text(step?.actionLabel, labelForBaseStepAction(action, key)),
	};
}

function buildFoundationModelFromOverview(foundation) {
	if (
		!foundation ||
		!Array.isArray(foundation.steps) ||
		!foundation.steps.length
	)
		return null;
	const steps = foundation.steps.slice(0, 5).map(normalizeFoundationStep);
	const done = Math.min(
		num(
			foundation.complete,
			steps.filter((step) => step.tone === "good").length,
		),
		steps.length,
	);
	const pct = Math.max(
		0,
		Math.min(
			100,
			num(foundation.progressPct, Math.round((done / steps.length) * 100)),
		),
	);
	const tone = ["good", "warn", "bad"].includes(String(foundation.status))
		? String(foundation.status)
		: steps.some((step) => step.tone === "bad")
			? "bad"
			: done === steps.length
				? "good"
				: "warn";
	const rawActions =
		Array.isArray(foundation.actions) && foundation.actions.length
			? foundation.actions.map((action, index) =>
					normalizeFoundationAction(action, index ? "servers" : "refresh"),
				)
			: [
					{ label: "Refresh", action: "refresh" },
					{ label: "Import", action: "import-server" },
					{ label: "Client", action: "clients" },
					{ label: "Servers", action: "servers" },
				];
	const actions = [];
	const seenActions = new Set();
	for (const action of rawActions) {
		if (!action.action || seenActions.has(action.action)) continue;
		seenActions.add(action.action);
		actions.push(action);
		if (actions.length >= 4) break;
	}
	const nextStep = foundation.nextStep
		? normalizeFoundationStep(foundation.nextStep, done)
		: steps.find((step) => step.tone !== "good") || {
				label: "Ready",
				title: "Base setup is ready",
				body: "Normal use can stay on the server rows.",
				tone: "good",
				action: "servers",
			};
	return {
		steps,
		done,
		pct,
		tone,
		blocked: tone === "bad",
		stateKey: text(foundation.stateKey, nextStep.key || "unknown"),
		nextStepKey: text(
			foundation.nextStepKey || foundation.nextStep?.key,
			nextStep.key || "ready",
		),
		title: text(
			foundation.title,
			done === steps.length ? "Base setup is ready" : "Finish base setup",
		),
		body: text(
			foundation.body,
			"Start with backend, client, source, tools, and routing before opening advanced controls.",
		),
		actions,
		nextStep,
		primaryAction: actions[0] || { label: "Refresh", action: "refresh" },
		secondaryAction: actions[3] ||
			actions[1] || { label: "Servers", action: "servers" },
		safety: normalizeFoundationSafety(foundation.safety || {}),
	};
}

function buildBaseSetupModel(context = {}) {
	return setupFoundationModel(context);
}

function isLoopbackUrl(value) {
	return /^https?:\/\/(localhost|127\.0\.0\.1|\[::1\])(?::\d+)?(?:[/?#]|$)/i.test(
		String(value || ""),
	);
}

function isRemoteServer(server = {}) {
	const url = String(server.sourceUrl || server.url || "").trim();
	return /^https?:\/\//i.test(url) && !isLoopbackUrl(url);
}

function hasSecretBoundary(server = {}) {
	const credential = String(server.credentialBinding || "").toLowerCase();
	const envNames = Array.isArray(server.sourceEnvNames)
		? server.sourceEnvNames
		: [];
	const headerNames = Array.isArray(server.sourceHeaderNames)
		? server.sourceHeaderNames
		: [];
	return (
		envNames.length > 0 ||
		headerNames.length > 0 ||
		/credential|secret|token|api[-_ ]?key|oauth|auth|header|env/.test(
			credential,
		)
	);
}

function normalizeFoundationSafety(safety = {}) {
	const counts =
		safety && typeof safety.counts === "object" ? safety.counts : {};
	const tone = ["good", "warn", "bad"].includes(String(safety.status))
		? String(safety.status)
		: "warn";
	return {
		tone,
		title: text(safety.title, "Review source, evidence, and secrets."),
		body: text(
			safety.body,
			"Keep new imports parked. Review first. Enable deliberately, then run Test.",
		),
		counts: {
			unchecked: num(counts.enabledWithoutEvidence ?? counts.unchecked, 0),
			remote: num(counts.remoteSources ?? counts.remote, 0),
			secretBearing: num(
				counts.secretBearingSources ?? counts.secretBearing,
				0,
			),
		},
	};
}

function renderBaseSafety(safety = {}) {
	if (!els.baseSafety) return;
	const model = normalizeFoundationSafety(safety);
	els.baseSafety.dataset.tone = model.tone;
	if (els.baseSafetyTitle) els.baseSafetyTitle.textContent = model.title;
	if (els.baseSafetyBody) els.baseSafetyBody.textContent = model.body;
	if (els.baseSafetyGrid) {
		setSafeHtml(
			els.baseSafetyGrid,
			[
				{
					label: `${model.counts.unchecked} unchecked`,
					tone: model.counts.unchecked ? "warn" : "good",
				},
				{
					label: `${model.counts.remote} remote`,
					tone: model.counts.remote ? "warn" : "good",
				},
				{
					label: `${model.counts.secretBearing} secret-bearing`,
					tone: model.counts.secretBearing ? "warn" : "good",
				},
			]
				.map((item) => chip(item.label, item.tone))
				.join(""),
		);
	}
}

function setupFoundationModel(context = {}) {
	const overview = context.overview || {};
	const backendOwned = buildFoundationModelFromOverview(
		overview.dashboardFoundation,
	);
	if (backendOwned) return backendOwned;
	const hub = context.hub || {};
	const servers = Array.isArray(context.servers) ? context.servers : [];
	const clients = Array.isArray(context.clients) ? context.clients : [];
	const clientCatalog = overview.clients || {};
	const backendOk = Boolean(state.backend.overview?.ok);
	const runtimeReady = Boolean(context.runtimeReady);
	const enabled = servers.filter((server) => server?.effectiveEnabled).length;
	const parked = Math.max(servers.length - enabled, 0);
	const tested = servers.filter(
		(server) => serverToolEvidence(server).checked,
	).length;
	const usable = servers.some(
		(server) =>
			server?.effectiveEnabled &&
			serverToolEvidence(server).checked &&
			serverToolEvidence(server).ok !== false,
	);
	const policyPlan = autoPolicyPlan(servers, currentInstances());
	const riskyEnabled = servers.filter((server) => {
		const evidence = serverToolEvidence(server);
		const risk = riskForServer(server, []);
		return (
			server?.effectiveEnabled &&
			risk.rank <= 3 &&
			(!evidence.checked || evidence.ok === false)
		);
	}).length;
	const routingSafe = Boolean(
		runtimeReady &&
			enabled > 0 &&
			usable &&
			!policyPlan.changes.length &&
			!riskyEnabled,
	);
	const routingIssue = !runtimeReady
		? "Runtime prerequisites are not ready, so routing should stay conservative."
		: !servers.length
			? "Routing becomes meaningful after at least one source is saved."
			: !enabled
				? "Saved sources are still parked. Review one, enable deliberately, then run Test before use."
				: !usable
					? "Keep routing conservative until Test creates tools/list evidence."
					: `${policyPlan.changes.length} policy fix${policyPlan.changes.length === 1 ? "" : "es"} · ${riskyEnabled} risky enabled.`;
	const endpoint =
		overview.userReadiness?.endpoint ||
		hub.endpoint ||
		overview.publicMcpUrl ||
		"/mcp";
	const localTargets = clients.filter(
		(client) =>
			String(
				client?.surfaceClass || client?.clientTargetSurfaceClass || "",
			).toLowerCase() === "local",
	);
	const patchableClients = localTargets.filter(
		(client) =>
			client?.installSupported ||
			client?.clientInstallImplemented ||
			client?.installSupport,
	);
	const configuredClientKey = String(
		clientCatalog?.configuredClientKeyName || "",
	).trim();
	const clientReady = Boolean(configuredClientKey);
	const clientTargetCount = localTargets.length || clients.length;
	const clientTone = clientReady ? "good" : clientTargetCount ? "warn" : "warn";
	const clientTitle = clientReady
		? `Client key ${configuredClientKey}`
		: patchableClients.length
			? `${patchableClients.length} patch target${patchableClients.length === 1 ? "" : "s"}`
			: clientTargetCount
				? `${clientTargetCount} client target${clientTargetCount === 1 ? "" : "s"}`
				: "Connect a client";
	const clientBody = clientReady
		? `Use the local endpoint ${endpoint}; patches remain reversible.`
		: patchableClients.length
			? "Preview a client patch first. A target catalog is not the same as a wired client."
			: "Copy the endpoint or open Clients before editing app config.";
	const steps = [
		{
			tone: backendOk ? "good" : "bad",
			key: "backend",
			label: "Backend",
			title: backendOk ? "Backend online" : "Connect backend",
			body: backendOk
				? `${fmtMs(state.backend.overview?.ms)} · /api/overview responded. Runtime is checked before use.`
				: "Start hub or check /api/overview before changing config.",
			action: backendOk ? "refresh" : "check-link",
			actionLabel: backendOk ? "Refresh" : "Check link",
		},
		{
			tone: clientTone,
			key: "client",
			label: "Client",
			title: clientTitle,
			body: clientBody,
			action: "clients",
			actionLabel: clientReady ? "Open client" : "Connect",
		},
		{
			tone: servers.length ? "good" : "warn",
			key: "source",
			label: "Source",
			title: servers.length
				? `${servers.length} source${servers.length === 1 ? "" : "s"} saved`
				: "Add one source",
			body: servers.length
				? `${enabled} on · ${parked} parked.`
				: "Import existing config first; otherwise discover or add manually.",
			action: servers.length ? "servers" : "import-server",
			actionLabel: servers.length ? "Open sources" : "Import",
		},
		{
			tone: usable ? "good" : servers.length ? "warn" : "warn",
			key: "tools",
			label: "Tools",
			title: usable
				? "One tools path tested"
				: servers.length
					? "Test enabled sources"
					: "Test after adding",
			body: usable
				? `${tested}/${servers.length} source${servers.length === 1 ? "" : "s"} have initialize/tools evidence.`
				: "Open Servers, then run Test on the source you intend to use.",
			action: "servers",
			actionLabel: "Open servers",
		},
		{
			tone: routingSafe ? "good" : "warn",
			key: "routing",
			label: "Routing",
			title: routingSafe
				? "Routing conservative"
				: !runtimeReady
					? "Repair runtime"
					: !enabled
						? "Enable one source"
						: "Review routing",
			body: routingSafe
				? "Tools evidence exists and no obvious safe-policy fix is waiting."
				: routingIssue,
			action: runtimeReady ? "servers" : "repair",
			actionLabel: runtimeReady
				? routingSafe
					? "Open routing"
					: !enabled
						? "Enable"
						: "Review"
				: "Repair",
		},
	];
	const done = steps.filter((step) => step.tone === "good").length;
	const blocked = steps.some((step) => step.tone === "bad");
	const pct = Math.round((done / steps.length) * 100);
	const tone = blocked ? "bad" : done === steps.length ? "good" : "warn";
	const nextStep = steps.find((step) => step.tone !== "good") || {
		key: "ready",
		label: "Ready",
		title: "Base setup is ready",
		body: "Normal use can stay on the server rows.",
		tone: "good",
		action: "servers",
		actionLabel: "Open servers",
	};
	const titleByStep = {
		backend: "Start with the local backend",
		client: "Connect a local client",
		source: "Bring in one MCP server source",
		tools: "Test tools before trust",
		routing: !runtimeReady
			? "Repair runtime before use"
			: !enabled
				? "Enable one reviewed source"
				: "Review conservative routing",
		ready: "The base is ready",
	};
	const bodyByStep = {
		backend:
			"The safest base path is: start hub, check the backend link, then refresh. Do not read server state as final while the backend is offline.",
		client:
			"Choose a supported local client, preview its patch, then apply only after the diff looks right.",
		source:
			"Use an existing MCP config when you have one. Imported sources stay parked by default; enable intentionally, then test before normal use.",
		tools:
			"A saved server is not the same as a usable server. Run Test to collect initialize and tools/list evidence first.",
		routing: !runtimeReady
			? "Runtime prerequisites are a use-boundary problem. Repair them after client, source, and tool setup are clear."
			: !enabled
				? "Saved sources are parked. Review one, enable it deliberately, then run Test before normal routing."
				: "Keep normal users on the safe path: apply conservative policy fixes before changing worker counts or trusting guarded sources.",
		ready:
			"Normal use can stay simple: use the configured client, and open the source panel only for routing or source metadata.",
	};
	const title = titleByStep[nextStep.key] || "Finish base setup";
	const body =
		bodyByStep[nextStep.key] ||
		nextStep.body ||
		"Finish the next basic step before opening advanced controls.";
	const primaryAction = {
		label: text(nextStep.actionLabel, "Open"),
		action: text(nextStep.action, "refresh"),
	};
	const secondaryAction = !servers.length
		? { label: "Add manually", action: "add-server" }
		: { label: "Servers", action: "servers" };
	const foundationSafety = normalizeFoundationSafety({
		status:
			riskyEnabled || servers.some((server) => isRemoteServer(server))
				? "warn"
				: "good",
		counts: {
			enabledWithoutEvidence: riskyEnabled,
			remoteSources: servers.filter((server) => isRemoteServer(server)).length,
			secretBearingSources: servers.filter((server) =>
				hasSecretBoundary(server),
			).length,
		},
	});
	const actions = [
		primaryAction,
		{ label: "Import config", action: "import-server" },
		{ label: "Connect client", action: "clients" },
		secondaryAction,
	];
	return {
		steps,
		done,
		blocked,
		pct,
		tone,
		stateKey: nextStep.key,
		nextStepKey: nextStep.key,
		title,
		body,
		actions,
		nextStep,
		primaryAction,
		secondaryAction,
		safety: foundationSafety,
	};
}

function renderBaseSetup(context = {}) {
	if (!els.baseStepGrid) return;
	const model = buildBaseSetupModel(context);
	setChip(
		els.baseStateChip,
		model.done === model.steps.length
			? "ready"
			: model.blocked
				? "blocked"
				: `${model.done}/5 basics`,
		model.tone,
	);
	setCardTone(els.baseSetup, model.tone);
	if (els.baseSetup) {
		els.baseSetup.dataset.foundationState = model.stateKey || "unknown";
		els.baseSetup.dataset.nextStep =
			model.nextStepKey || model.nextStep?.key || "ready";
	}
	if (els.baseTitle) els.baseTitle.textContent = model.title;
	if (els.baseBody) els.baseBody.textContent = model.body;
	if (els.baseProgressFill) els.baseProgressFill.style.width = `${model.pct}%`;
	if (els.baseProgressLabel)
		els.baseProgressLabel.textContent = `${model.done} of 5 basics complete. Next: ${text(model.nextStep?.title, model.title)}.`;
	const currentBaseStepKey = String(
		model.nextStepKey || model.nextStep?.key || "",
	)
		.replace(/[^a-z0-9-]/gi, "")
		.toLowerCase();
	setSafeHtml(
		els.baseStepGrid,
		model.steps
			.map((step, index) => baseStepCard(index + 1, step, currentBaseStepKey))
			.join(""),
	);
	renderBaseSafety(model.safety || {});
	if (els.baseActionRow) {
		const item = model.primaryAction;
		setSafeHtml(
			els.baseActionRow,
			item?.action
				? `<button class="primary" type="button" data-global-action="${escapeHtml(item.action)}">${escapeHtml(item.label)}</button>`
				: "",
		);
	}
}

function normalizeAccessReviewItem(item, fallback = {}) {
	const tone = ["good", "warn", "bad"].includes(
		String(item?.status || item?.tone),
	)
		? String(item?.status || item?.tone)
		: fallback.tone || "warn";
	return {
		label: text(item?.label, fallback.label || "Review"),
		count: num(item?.count, fallback.count || 0),
		tone,
		body: text(item?.body, fallback.body || "Review before enabling."),
	};
}

function fallbackAccessReview(servers = []) {
	const enabled = servers.filter((server) => server?.effectiveEnabled);
	let approval = 0;
	let remote = 0;
	let secrets = 0;
	let evidenceMissing = 0;
	let sensitiveWithoutEvidence = 0;
	for (const server of servers) {
		const risk = riskForServer(server, []);
		const evidence = serverToolEvidence(server);
		const remoteSource = isRemoteServer(server);
		const secretSource = hasSecretBoundary(server);
		const approvalNeeded =
			server?.approvalRequired === true ||
			risk.rank <= 3 ||
			/write|mutation|credential|remote|external|unknown/i.test(
				`${server?.effectClass || ""} ${server?.credentialBinding || ""} ${server?.runtimeType || ""}`,
			);
		if (approvalNeeded) approval += 1;
		if (remoteSource) remote += 1;
		if (secretSource) secrets += 1;
		if (server?.effectiveEnabled && !evidence.checked) evidenceMissing += 1;
		if (
			server?.effectiveEnabled &&
			!evidence.checked &&
			(approvalNeeded || remoteSource || secretSource)
		)
			sensitiveWithoutEvidence += 1;
	}
	const status = sensitiveWithoutEvidence
		? "bad"
		: !servers.length || approval || remote || secrets || evidenceMissing
			? "warn"
			: "good";
	const title = !servers.length
		? "Access review waits for one source"
		: sensitiveWithoutEvidence
			? "Review access before enabling"
			: approval || remote || secrets
				? "Access needs explicit review"
				: "Access boundary looks quiet";
	const body = !servers.length
		? "Add or import one source first. MCPace should not describe permissions for tools that do not exist yet."
		: sensitiveWithoutEvidence
			? "Some enabled sources look sensitive but have no tools/list evidence. Test them or park them before normal use."
			: "Review approval, secret names, remote origins, and tools/list evidence before widening routing.";
	return {
		schema: "mcpace.dashboardAccessReview.fallback",
		status,
		title,
		body,
		counts: {
			servers: servers.length,
			enabled: enabled.length,
			approvalRequired: approval,
			hiddenSecretNames: secrets,
			remoteHttp: remote,
			enabledWithoutEvidence: evidenceMissing,
			sensitiveWithoutEvidence,
		},
		items: [
			{
				label: "Approval",
				count: approval,
				status: approval ? "warn" : "good",
				body: "Write, destructive, open-world, credential, and unknown tools should ask before use.",
			},
			{
				label: "Secrets",
				count: secrets,
				status: secrets ? "warn" : "good",
				body: "Show env/header names only. Never render secret values in the dashboard.",
			},
			{
				label: "Remote/Auth",
				count: remote,
				status: remote ? "warn" : "good",
				body: "Remote HTTP and auth-backed sources need explicit origin and scope review.",
			},
			{
				label: "Evidence",
				count: evidenceMissing,
				status: evidenceMissing ? "bad" : "good",
				body: "Enabled sources need initialize/tools-list evidence before normal routing.",
			},
		],
	};
}

function renderAccessReview(review = {}, servers = []) {
	if (!els.accessReview || !els.accessReviewList) return;
	const model =
		review && typeof review === "object" && review.schema
			? review
			: fallbackAccessReview(servers);
	const tone = ["good", "warn", "bad"].includes(String(model.status))
		? String(model.status)
		: "warn";
	setCardTone(els.accessReview, tone);
	if (els.accessReviewTitle)
		els.accessReviewTitle.textContent = text(model.title, "Trust boundary");
	if (els.accessReviewBody)
		els.accessReviewBody.textContent = text(
			model.body,
			"Review approval, secrets, remote access, and evidence before enabling sources.",
		);
	const counts = model.counts || {};
	const serverCount = num(counts.servers, servers.length);
	const enabledCount = num(
		counts.enabled,
		servers.filter((server) => server?.effectiveEnabled).length,
	);
	setChip(
		els.accessReviewChip,
		serverCount ? `${enabledCount}/${serverCount} enabled` : "no sources",
		tone,
	);
	const fallbackItems = fallbackAccessReview(servers).items;
	const items = (
		Array.isArray(model.items) && model.items.length
			? model.items
			: fallbackItems
	)
		.slice(0, 4)
		.map((item, index) =>
			normalizeAccessReviewItem(item, fallbackItems[index] || {}),
		);
	setSafeHtml(
		els.accessReviewList,
		items
			.map(
				(item) =>
					`<article class="access-review-card ${escapeHtml(item.tone)}"><span>${escapeHtml(item.label)}</span><strong>${escapeHtml(String(item.count))}</strong><p>${escapeHtml(item.body)}</p></article>`,
			)
			.join(""),
	);
}

function renderNextAction(context) {
	if (!els.nextActionBoard) return;
	const servers = Array.isArray(context?.servers) ? context.servers : [];
	const backendOk = Boolean(state.backend.overview?.ok);
	const plan = autoPolicyPlan(servers, currentInstances());
	const unchecked = servers.filter((server) => {
		const evidence = serverToolEvidence(server);
		return server.effectiveEnabled && (!evidence.checked || !evidence.ok);
	}).length;
	const confidence = Math.max(
		0,
		Math.min(
			100,
			Math.round(num(context?.overview?.userReadiness?.confidence, 0) * 100),
		),
	);
	const firstAttention = Array.isArray(context?.attentionItems)
		? context.attentionItems[0]
		: null;
	let tone = "good";
	let eyebrow = "Normal route";
	let title = "Ready for normal use";
	let body =
		"Diagnostics are optional. Use Sources when adding, testing, or changing a source.";
	let primary = { label: "Refresh", action: "refresh" };
	let secondary = { label: "Servers", action: "servers" };
	let tertiary = { label: "Diagnostics", action: "diagnostics" };
	const map = [
		["Connect", backendOk ? "good" : "bad"],
		["Prove", unchecked ? "warn" : servers.length ? "good" : "warn"],
		[
			"Enable",
			context?.enabledCount ? "good" : servers.length ? "warn" : "bad",
		],
		[
			"Use",
			confidence >= 80 && !context?.attentionTotal
				? "good"
				: context?.attentionTotal
					? "warn"
					: "good",
		],
	];

	if (!backendOk) {
		tone = "bad";
		eyebrow = "Connection first";
		title = "Reconnect the local backend";
		body =
			"Do not interpret inventory or policy while /api/overview is offline. Start the hub, check the link, then refresh evidence.";
		primary = { label: "Check link", action: "check-link" };
		secondary = { label: "Start hub", action: "start-hub" };
		tertiary = { label: "Refresh", action: "refresh" };
	} else if (!context?.runtimeReady) {
		tone = "bad";
		eyebrow = "Runtime blocker";
		title = "Repair runtime before using servers";
		body =
			"The backend is reachable, but runtime prerequisites are not ready. Repair first so later server state is meaningful.";
		primary = { label: "Repair runtime", action: "repair" };
		secondary = { label: "Check link", action: "check-link" };
		tertiary = { label: "Diagnostics", action: "diagnostics" };
	} else if (!servers.length) {
		tone = "warn";
		eyebrow = "Inventory empty";
		title = "Import or add one server disabled";
		body =
			"Start from an existing MCP config when one exists; otherwise discover a trusted candidate or paste one command. Keep the new source parked until review; after enabling, run Test to collect tools/list evidence.";
		primary = { label: "Import config", action: "import-server" };
		secondary = { label: "Discover", action: "servers" };
		tertiary = { label: "Add manually", action: "add-server" };
	} else if (context?.badPolicies || firstAttention?.tone === "bad") {
		tone = "bad";
		eyebrow = "Blocker route";
		title =
			firstAttention?.title ||
			`${context.badPolicies} server blocker${context.badPolicies === 1 ? "" : "s"}`;
		body =
			firstAttention?.meta ||
			"Resolve source/profile mismatch or runtime setup before widening policy or trusting tools.";
		primary = { label: "Open servers", action: "servers" };
		secondary = { label: "Refresh evidence", action: "refresh" };
		tertiary = { label: "Diagnostics", action: "diagnostics" };
	} else if (plan.changes.length) {
		tone = "warn";
		eyebrow = "Safe policy plan";
		title = `${plural(plan.changes.length, "safe policy fix", "safe policy fixes")} ready`;
		body =
			"Apply the backend-backed low-resource route before changing worker counts manually. This keeps active sources conservative by default.";
		primary = { label: `Apply ${plan.changes.length}`, action: "auto-tune" };
		secondary = { label: "Review servers", action: "servers" };
		tertiary = { label: "Refresh", action: "refresh" };
	} else if (unchecked) {
		tone = "warn";
		eyebrow = "Evidence route";
		title = `${plural(unchecked, "enabled server")} need evidence`;
		body =
			"Run Test on guarded rows before relying on capabilities. Keep the server enabled only if the workflow actually needs it.";
		primary = { label: "Open servers", action: "servers" };
		secondary = { label: "Refresh evidence", action: "refresh" };
		tertiary = { label: "Add server", action: "add-server" };
	} else if (context?.attentionTotal) {
		tone = "warn";
		eyebrow = "Watchlist";
		title =
			firstAttention?.title ||
			`${context.attentionTotal} watch item${context.attentionTotal === 1 ? "" : "s"}`;
		body =
			firstAttention?.meta ||
			"Review the attention panel, then return to normal mode.";
		primary = { label: "Review attention", action: "attention" };
		secondary = { label: "Servers", action: "servers" };
		tertiary = { label: "Refresh", action: "refresh" };
	}

	const route = map
		.map(
			([label, nodeTone], index) =>
				`<span class="next-action-node ${escapeHtml(nodeTone)}"><strong>${String(index + 1).padStart(2, "0")}</strong>${escapeHtml(label)}</span>`,
		)
		.join("");
	setSafeHtml(
		els.nextActionBoard,
		`
          <article class="next-action-card ${escapeHtml(tone)}">
            <div class="next-action-copy">
              <div class="label">${escapeHtml(eyebrow)}</div>
              <h2 id="next-action-title">${escapeHtml(title)}</h2>
              <p>${escapeHtml(body)}</p>
              <div class="next-action-metadata" aria-label="Decision inputs">
                ${chip(`${context?.enabledCount || 0}/${servers.length} enabled`, context?.enabledCount ? "good" : "warn")}
                ${chip(readiness.label, readiness.tone)}
                ${chip(`${plan.changes.length} policy fixes`, plan.changes.length ? "warn" : "good")}
                ${chip(`${unchecked} unproved`, unchecked ? "warn" : "good")}
              </div>
            </div>
            <div class="next-action-tools" aria-label="Quick action choices">
              <button class="primary" type="button" data-global-action="${escapeHtml(primary.action)}">${escapeHtml(primary.label)}</button>
              <button type="button" data-global-action="${escapeHtml(secondary.action)}">${escapeHtml(secondary.label)}</button>
              <button class="quiet" type="button" data-global-action="${escapeHtml(tertiary.action)}">${escapeHtml(tertiary.label)}</button>
            </div>
          </article>
          <aside class="next-action-map" aria-label="Operating sequence">${route}</aside>
        `,
	);
}

function serverTransportKind(server = {}) {
	const sourceType = String(
		server.sourceType ||
			server.type ||
			server.transport ||
			server.runtimeType ||
			"unknown",
	).toLowerCase();
	const url = String(server.sourceUrl || server.url || server.launch || "");
	const command = String(server.sourceCommand || server.command || "");
	if (/sse|legacy/.test(`${sourceType} ${url}`))
		return {
			tone: "bad",
			kind: "legacy",
			label: "legacy SSE",
			detail: "replace or keep out of automatic routing",
		};
	if (/http|url|streamable/.test(sourceType) || /^https?:\/\//i.test(url)) {
		const local = /^https?:\/\/(127\.0\.0\.1|localhost|\[::1\])/i.test(url);
		return {
			tone: local ? "good" : "warn",
			kind: "http",
			label: local ? "local Streamable HTTP" : "remote Streamable HTTP",
			detail: local ? "local HTTP boundary" : "auth and origin review needed",
		};
	}
	if (/stdio|command|process|npm|pypi|oci/.test(sourceType) || command)
		return {
			tone: server.effectiveEnabled ? "good" : "warn",
			kind: "stdio",
			label: "Local command",
			detail: server.effectiveEnabled
				? "local child process"
				: "parked until enabled and tested",
		};
	return {
		tone: "warn",
		kind: "unknown",
		label: "unknown",
		detail: "keep parked until Test succeeds",
	};
}

function localClientTargets(clients = []) {
	const list = Array.isArray(clients) ? clients : normalizeClients(clients);
	return list.filter(
		(client) =>
			client?.surfaceClass === "local" ||
			client?.installSupported ||
			client?.installSupport,
	);
}

function connectionStep(label, title, body, tone = "warn") {
	return `<article class="connection-step ${itemClass(tone)}"><span>${escapeHtml(label)}</span><strong>${escapeHtml(title)}</strong><p>${escapeHtml(body)}</p></article>`;
}

function renderConnectionMap(overview = {}, servers = [], clients = []) {
	if (!els.connectionGrid) return;
	const user = normalizeUserReadiness(overview.userReadiness || {});
	const localClients = localClientTargets(clients);
	const enabled = servers.filter((server) => server.effectiveEnabled).length;
	const tested = servers.filter((server) => {
		const evidence = serverToolEvidence(server);
		return evidence.checked && evidence.ok;
	}).length;
	const firstTransport = servers[0] ? serverTransportKind(servers[0]) : null;
	const endpoint = user.endpoint || overview.hub?.endpoint || "/mcp";
	const title = servers.length
		? `${enabled}/${servers.length} source${servers.length === 1 ? "" : "s"} active`
		: "No upstreams yet";
	if (els.connectionMapTitle)
		els.connectionMapTitle.textContent = "Client → MCPace → Server → Tools";
	if (els.connectionMapBody)
		els.connectionMapBody.textContent = servers.length
			? "Read this left to right: patch a local client, route through MCPace, test each upstream, then expose only evidenced tools."
			: "Start by importing an existing client config or patching one local client; MCPace stays between the client and upstream tools.";
	setSafeHtml(
		els.connectionGrid,
		[
			connectionStep(
				"Client",
				localClients.length
					? countLabel(localClients.length, "local target")
					: "No client target",
				localClients.length
					? "Preview/apply patches from Clients; every write has restore."
					: "Use Clients or import an existing config path first.",
				localClients.length ? "good" : "warn",
			),
			connectionStep(
				"MCPace",
				endpoint,
				state.backend.overview?.ok
					? "Local broker overview is reachable."
					: "Start hub or check /api/overview first.",
				state.backend.overview?.ok ? "good" : "bad",
			),
			connectionStep(
				"Server",
				title,
				firstTransport
					? `${firstTransport.label}: ${firstTransport.detail}.`
					: "Import, discover, or add one source disabled.",
				servers.length ? (enabled ? "good" : "warn") : "warn",
			),
			connectionStep(
				"Tools",
				tested ? countLabel(tested, "tested server") : "Not tested",
				tested
					? "tools/list evidence exists for at least one source."
					: "Run Test before relying on capabilities.",
				tested ? "good" : "warn",
			),
		].join(""),
	);
	setSurfaceTone(
		els.connectionMap,
		servers.length && tested ? "good" : servers.length ? "warn" : "warn",
	);
}

function setupQueueItems(context = {}) {
	const servers = context.servers || [];
	const clients = context.clients || [];
	const hub = context.hub || {};
	const runtimeReady = Boolean(context.runtimeReady);
	const plan = context.overview?.operatorPlan || { changes: [] };
	const unchecked = servers.filter(
		(server) => server.effectiveEnabled && !serverToolEvidence(server).checked,
	).length;
	const localClients = localClientTargets(clients).filter(
		(client) => client.installSupported,
	);
	const items = [];
	if (!state.backend.overview?.ok)
		items.push({
			label: "1",
			title: "Connect backend",
			body: "Start hub, check link, then refresh overview.",
			tone: "bad",
			action: "check-link",
		});
	else if (!runtimeReady)
		items.push({
			label: "1",
			title: "Repair runtime",
			body: `${hub.status || "runtime"} is not ready for routing.`,
			tone: "bad",
			action: "repair",
		});
	if (!servers.length)
		items.push({
			label: items.length + 1,
			title: "Import existing config",
			body: "Use what the user already has before discovery or manual add.",
			tone: "warn",
			action: "import-server",
		});
	else if (unchecked)
		items.push({
			label: items.length + 1,
			title: "Test enabled sources",
			body: `${unchecked} enabled source${unchecked === 1 ? "" : "s"} need tools/list evidence.`,
			tone: "warn",
			action: "servers",
		});
	if (localClients.length)
		items.push({
			label: items.length + 1,
			title: "Preview client patch",
			body: `${localClients.length} local client target${localClients.length === 1 ? "" : "s"} can be patched and restored.`,
			tone: "good",
			action: "client",
		});
	if (Array.isArray(plan.changes) && plan.changes.length)
		items.push({
			label: items.length + 1,
			title: "Apply safe policy",
			body: `${plan.changes.length} conservative route change${plan.changes.length === 1 ? "" : "s"} available.`,
			tone: "warn",
			action: "auto-tune",
		});
	if (!items.length)
		items.push({
			label: "OK",
			title: "No queued setup",
			body: "Routine use can stay on the server rows; diagnostics are optional.",
			tone: "good",
			action: "servers",
		});
	return items.slice(0, 1);
}

function renderSetupQueue(context = {}) {
	if (!els.setupQueueList) return;
	const items = setupQueueItems(context);
	if (els.setupQueueBody)
		els.setupQueueBody.textContent =
			"One safe next step is shown here; setup tasks are grouped in the Setup workspace.";
	setSafeHtml(
		els.setupQueueList,
		items
			.map(
				(item) =>
					`<article class="setup-queue-item ${itemClass(item.tone)}"><span>${escapeHtml(item.label)}</span><strong>${escapeHtml(item.title)}</strong><p>${escapeHtml(item.body)}</p>${item.action ? `<button type="button" data-global-action="${escapeHtml(item.action)}">Open</button>` : ""}</article>`,
			)
			.join(""),
	);
	setSurfaceTone(
		els.setupQueue,
		items.some((item) => item.tone === "bad")
			? "bad"
			: items.some((item) => item.tone === "warn")
				? "warn"
				: "good",
	);
}

function protocolDescriptor(server = {}, instances = []) {
	const base = serverTransportKind(server);
	const matchedInstances = instances.filter(
		(instance) =>
			(instance.server || instance.serverName || instance.name) === server.name,
	);
	const modes = [
		...new Set(
			matchedInstances
				.map(
					(instance) =>
						instance.mode || instance.schedulerLane || instance.routingMode,
				)
				.filter(Boolean),
		),
	];
	return { ...base, modes };
}

function renderProtocolCompatibility(
	overview,
	servers,
	clients,
	instances = [],
) {
	overview = overview || {};
	servers = Array.isArray(servers) ? servers : [];
	clients = Array.isArray(clients)
		? clients
		: normalizeClients(overview.clients || []);
	instances = Array.isArray(instances)
		? instances
		: normalizeInstances(overview.instances);
	if (!els.protocolCompatGrid) return;
	const descriptors = servers.map((server) => ({
		server,
		info: protocolDescriptor(server, instances),
	}));
	const counts = descriptors.reduce(
		(acc, item) => {
			acc[item.info.kind] = (acc[item.info.kind] || 0) + 1;
			return acc;
		},
		{ stdio: 0, http: 0, legacy: 0, unknown: 0 },
	);
	const remote = descriptors.filter((item) =>
		/remote/i.test(item.info.label),
	).length;
	const authHints = servers.filter(
		(server) =>
			listValues(server.sourceHeaderNames).length ||
			/oauth|credential|token|auth/i.test(
				`${server.credentialBinding || ""} ${server.sourceHeaderNames || ""}`,
			),
	).length;
	const clientIngresses = [
		...new Set(
			clients.flatMap((client) => listValues(client?.supportedIngresses)),
		),
	].sort();
	const cache = overview.cachedToolEvidence || {};
	const cacheTone = num(cache.failedCount)
		? "bad"
		: num(cache.serverCount)
			? "good"
			: "warn";
	const tone = counts.legacy
		? "bad"
		: remote || authHints || counts.unknown
			? "warn"
			: servers.length
				? "good"
				: "warn";
	setChip(
		els.protocolCompatChip,
		counts.legacy
			? `${counts.legacy} legacy`
			: remote
				? `${remote} remote`
				: servers.length
					? "compatible"
					: "pending",
		tone,
	);
	const summaryCards = [
		`<article class="protocol-compat-card ${clientIngresses.length ? "good" : "warn"}"><span>Client ingress</span><strong>${escapeHtml(clientIngresses.length ? clientIngresses.join(" + ") : "not loaded")}</strong><p>${clients.length ? "Patch/restore clients from Clients; keep config changes reversible." : "Client surfaces appear after backend catalog loads."}</p></article>`,
		`<article class="protocol-compat-card ${counts.stdio ? "good" : "warn"}"><span>stdio</span><strong>${counts.stdio}</strong><p>Local process servers. Keep parked until Test collects initialize + tools/list evidence.</p></article>`,
		`<article class="protocol-compat-card ${counts.http ? (remote || authHints ? "warn" : "good") : "warn"}"><span>Streamable HTTP</span><strong>${counts.http}</strong><p>${remote || authHints ? "HTTP upstreams need explicit origin/auth review; secret values stay hidden." : "HTTP upstreams use the broker boundary when configured."}</p></article>`,
		`<article class="protocol-compat-card ${counts.legacy ? "bad" : "good"}"><span>Legacy / blocked</span><strong>${counts.legacy}</strong><p>Legacy SSE or unsupported transports stay out of automatic routing.</p></article>`,
		`<article class="protocol-compat-card ${cacheTone}"><span>Tool evidence</span><strong>${num(cache.serverCount) ? `${num(cache.okCount)}/${num(cache.serverCount)} ok` : "not cached"}</strong><p>${num(cache.failedCount) ? `${num(cache.failedCount)} failed cache entr${num(cache.failedCount) === 1 ? "y" : "ies"}.` : "Run Test before treating tools as usable."}</p></article>`,
	];
	const serverCards = descriptors
		.slice(0, 5)
		.map(
			({ server, info }) =>
				`<article class="protocol-compat-card ${itemClass(info.tone)}"><span>${escapeHtml(server.name || "server")}</span><strong>${escapeHtml(info.label)}</strong><p>${escapeHtml(info.detail)}${info.modes.length ? ` · route ${escapeHtml(info.modes.join(", "))}` : ""}</p></article>`,
		);
	if (!serverCards.length)
		serverCards.push(
			`<article class="protocol-compat-card warn"><span>No upstreams</span><strong>Import first</strong><p>No protocol surface is configured yet. Import, discover, or add one server, then run Test.</p></article>`,
		);
	setSafeHtml(
		els.protocolCompatGrid,
		[...summaryCards, ...serverCards].join(""),
	);
}

function renderDecisionRunway(context) {
	if (!els.decisionGrid) return;
	const {
		overview = {},
		hub = {},
		servers = [],
		clients = [],
		attentionItems = [],
		attentionTotal = 0,
		runtimeReady = false,
		enabledCount = 0,
		active = 0,
		max = 0,
		badPolicies = 0,
		warnPolicies = 0,
	} = context || {};
	const backend = state.backend.overview;
	const backendOk = Boolean(backend?.ok);
	const user = normalizeUserReadiness(overview.userReadiness);
	const userBand = readinessBand(user.confidence);
	const serverTotal = servers.length || 0;
	const enabledPct = serverTotal ? (enabledCount / serverTotal) * 100 : 0;
	const topAction = !backendOk
		? "Connect backend first"
		: !runtimeReady
			? "Complete runtime setup"
			: attentionTotal
				? attentionItems[0]?.title || "Resolve visible watchlist"
				: user.primaryAction || "Use normally";
	const body = !backendOk
		? "Recovery path is intentionally short: start hub, verify API, then refresh overview. Server controls remain visible but should not be trusted without live state."
		: attentionTotal
			? "The runway compresses backend, runtime, server safety, and normal-user readiness into one decision path."
			: "All high-level gates are green; diagnostics are there for audit, not routine operation.";
	if (els.decisionRunwayTitle) els.decisionRunwayTitle.textContent = topAction;
	if (els.decisionRunwayBody) els.decisionRunwayBody.textContent = body;
	setSafeHtml(
		els.decisionGrid,
		[
			decisionCard(
				"Connection",
				backendOk ? "API online" : "API offline",
				backendOk
					? `/api/overview ${fmtMs(backend.ms)}`
					: text(
							backend?.error?.message || backend?.error,
							"start/check local hub",
						),
				backendOk ? msTone(backend.ms) : "bad",
				backendOk ? 100 : 12,
				"01",
			),
			decisionCard(
				"Runtime",
				runtimeReady ? "Ready to route" : "Prerequisites blocked",
				`${hub.status || hub.health || "unknown"} · ${active}/${max || "?"} active HTTP`,
				runtimeReady ? "good" : "bad",
				runtimeReady ? 100 : 24,
				"02",
			),
			decisionCard(
				"Fleet",
				serverTotal
					? `${enabledCount}/${serverTotal} enabled`
					: "Inventory missing",
				`${badPolicies} blocked · ${warnPolicies} guarded`,
				badPolicies
					? "bad"
					: warnPolicies
						? "warn"
						: serverTotal
							? "good"
							: "warn",
				serverTotal ? enabledPct : 12,
				"03",
			),
			decisionCard(
				"User",
				userBand.label,
				`${clients.length} client surface${clients.length === 1 ? "" : "s"} · ${user.endpoint || "/mcp"}`,
				userBand.tone,
				userBand.pct,
				"04",
			),
		].join(""),
	);
}

function handleGlobalAction(control) {
	const action = control?.dataset?.globalAction || "";
	if (action === "start-hub")
		runAction("/api/actions/hub-up", control, "", "Starting…");
	else if (action === "repair")
		runAction(
			"/api/actions/repair",
			control,
			"Run MCPace repair now? This may update local runtime wiring and client config files.",
			"Repairing…",
		);
	else if (action === "check-link") checkBackendLink(control);
	else if (action === "refresh")
		refreshDashboard({ force: true, reason: "hero" });
	else if (action === "servers") setDashboardView("sources", { focus: true });
	else if (action === "diagnostics") {
		setDashboardView("diagnostics", { focus: true });
		setDiagnosticTab("runtime");
	} else if (action === "help") {
		setDashboardView("diagnostics", { focus: true });
		setDiagnosticTab("preferences");
		revealElementById("help-page", "center");
	} else if (action === "attention") {
		setDashboardView("overview", { focus: true });
		revealElementById("attention-title", "center");
	} else if (action === "import-server") focusImportPath();
	else if (action === "add-server") focusInstallCommand();
	else if (action === "client" || action === "clients") {
		updateSetupToolsState("client");
		revealElementById("client-setup-panel", "center");
		window.setTimeout(() => els.clientPreviewAll?.focus?.(), 120);
	} else if (action === "discover") {
		updateSetupToolsState("discover");
		revealElementById("server-discovery-panel", "center");
	} else if (action === "auto-tune") {
		setDashboardView("diagnostics", { scroll: false, focus: false });
		setDiagnosticTab("policy");
		autoTuneVisibleServers(control);
	}
}

function focusImportPath() {
	updateSetupToolsState("import");
	revealElementById("server-import-panel", "center");
	window.setTimeout(() => els.serverImportPath?.focus?.(), 120);
}

function focusInstallCommand() {
	updateSetupToolsState("add");
	revealElementById("server-install-panel", "center");
	window.setTimeout(() => els.serverInstallCommand?.focus?.(), 120);
}
