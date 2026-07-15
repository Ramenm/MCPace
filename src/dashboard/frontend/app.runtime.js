// MCPace dashboard transport, refresh, update, and runtime actions.
function apiErrorMessage(error) {
	if (!error) return "Request failed";
	if (error.name === "TimeoutError") return "Request timed out";
	if (error.name === "AbortError") return "Request aborted";
	return error.message || String(error);
}

async function fetchJson(url, options = {}) {
	const {
		timeoutMs = REQUEST_TIMEOUT_MS,
		headers = {},
		...fetchOptions
	} = options;
	const signal = combineSignals([
		fetchOptions.signal,
		timeoutMs > 0 ? timeoutSignal(timeoutMs) : null,
	]);
	const response = await fetch(url, {
		...fetchOptions,
		signal: signal || fetchOptions.signal,
		cache: fetchOptions.cache || "no-store",
		headers: { accept: "application/json", ...headers },
	});
	const raw = await response.text();
	let payload = null;
	if (raw) {
		try {
			payload = JSON.parse(raw);
		} catch (_) {
			payload = { error: raw };
		}
	}
	if (!response.ok) {
		const message =
			payload?.error?.message ||
			payload?.error ||
			`${response.status} ${response.statusText}`;
		const error = new Error(message);
		error.status = response.status;
		error.url = url;
		throw error;
	}
	return payload;
}

async function timedFetchJson(url, options = {}) {
	const started = performance.now();
	try {
		const value = await fetchJson(url, options);
		return {
			ok: true,
			value,
			ms: Math.round(performance.now() - started),
			url,
			at: Date.now(),
		};
	} catch (error) {
		return {
			ok: false,
			error,
			ms: Math.round(performance.now() - started),
			url,
			at: Date.now(),
		};
	}
}

function updateResultPayload(value) {
	return value?.result && typeof value.result === "object"
		? value.result
		: value || null;
}

function updateCommand(report = {}) {
	const commands = Array.isArray(report.recommendedCommands)
		? report.recommendedCommands
		: [];
	return (
		commands.find((command) =>
			/^npm\s+(?:install|update)\s+-g\s+/i.test(String(command)),
		) || "npm install -g @mcpace/cli@latest"
	);
}

function renderUpdateNotice() {
	const notice = els.updateNotice;
	if (!notice) return;
	const update = state.update || {};
	const report = update.result;
	const shouldShow =
		update.loading ||
		Boolean(report && (report.updateAvailable || update.manual));
	notice.hidden = !shouldShow;
	if (!shouldShow) return;
	if (update.loading) {
		setSafeHtml(
			notice,
			`<div><div class="label">Updates</div><strong>Checking for a newer MCPace release…</strong><p>This checks npm once; MCPace never replaces its running binary.</p></div>`,
		);
		return;
	}
	if (!report) {
		setSafeHtml(
			notice,
			`<div><div class="label">Updates</div><strong>Update check unavailable</strong><p>${escapeHtml(update.error || "Try again when npm registry access is available.")}</p></div><button type="button" data-update-action="check">Try again</button>`,
		);
		return;
	}
	if (!report.updateAvailable) {
		const detail =
			report.status === "current"
				? `MCPace ${text(report.currentVersion, "")} is current.`
				: text(report.reason, "No update information is available right now.");
		setSafeHtml(
			notice,
			`<div><div class="label">Updates</div><strong>${escapeHtml(report.status === "current" ? "You are up to date" : "Update check unavailable")}</strong><p>${escapeHtml(detail)}</p></div><button type="button" data-update-action="check">Check again</button>`,
		);
		return;
	}
	const command = updateCommand(report);
	setSafeHtml(
		notice,
		`<div><div class="label">Update available</div><strong>MCPace ${escapeHtml(text(report.latestVersion, "new version"))} is ready</strong><p>Installed: ${escapeHtml(text(report.currentVersion, "unknown"))}. Update from a terminal; MCPace will not silently replace a running binary.</p></div><div class="update-command"><code>${escapeHtml(command)}</code><button class="primary" type="button" data-update-action="copy" data-update-command="${escapeHtml(command)}">Copy command</button><button type="button" data-update-action="check">Check again</button></div>`,
	);
}

async function copyUpdateCommand(control) {
	const command = String(control?.dataset?.updateCommand || "").trim();
	if (!command) return;
	const original = control.textContent;
	try {
		if (navigator.clipboard?.writeText)
			await navigator.clipboard.writeText(command);
		else {
			const area = document.createElement("textarea");
			area.value = command;
			area.style.position = "fixed";
			area.style.opacity = "0";
			document.body.append(area);
			area.select();
			document.execCommand("copy");
			area.remove();
		}
		control.textContent = "Copied";
	} catch (_) {
		control.textContent = "Copy manually";
	}
	window.setTimeout(() => {
		control.textContent = original;
	}, 1800);
}

async function checkForUpdates(button, options = {}) {
	if (state.update.loading) return;
	state.update.loading = true;
	state.update.requested = true;
	state.update.manual = Boolean(options.manual);
	state.update.error = null;
	if (button) button.disabled = true;
	renderUpdateNotice();
	const response = await timedFetchJson("/api/actions/update-check", {
		method: "POST",
		timeoutMs: ACTION_TIMEOUT_MS,
	});
	state.update.loading = false;
	if (response.ok) {
		state.update.result = updateResultPayload(response.value);
		state.update.error = null;
	} else {
		state.update.result = null;
		state.update.error = apiErrorMessage(response.error);
	}
	if (button) button.disabled = false;
	renderUpdateNotice();
}

async function refreshDashboard(options = {}) {
	if (
		(document.visibilityState === "hidden" || state.lifecycle.frozen) &&
		!options.force &&
		!options.allowHidden
	) {
		scheduleRefresh();
		return;
	}
	if (state.refreshing && !options.forceAbort) {
		scheduleRefresh();
		return;
	}
	const now = Date.now();
	if (
		options.reason === "visible" &&
		state.lastRefreshFinishedAt &&
		now - state.lastRefreshFinishedAt < VISIBLE_REFRESH_MIN_INTERVAL_MS
	) {
		scheduleRefresh();
		return;
	}
	const seq = state.seq + 1;
	state.seq = seq;
	if (state.controller) {
		if (options.forceAbort) state.controller.abort();
		else {
			scheduleRefresh();
			return;
		}
	}
	const controller =
		typeof AbortController !== "undefined" ? new AbortController() : null;
	state.controller = controller;
	state.lastRefreshStartedAt = now;
	setBusy(true);
	try {
		const request = controller ? { signal: controller.signal } : {};
		const overviewUrl = options.force
			? "/api/overview?refresh=1"
			: "/api/overview";
		const [overviewResult, logsResult, operationsResult, resourcesResult] =
			await Promise.allSettled([
				timedFetchJson(overviewUrl, request),
				timedFetchJson("/api/logs?tail=500", request),
				timedFetchJson("/api/operations?limit=5000", request),
				timedFetchJson("/api/resources", request),
			]);
		if (seq !== state.seq) return;
		const overviewCheck =
			overviewResult.status === "fulfilled"
				? overviewResult.value
				: {
						ok: false,
						error: overviewResult.reason,
						ms: 0,
						url: overviewUrl,
						at: Date.now(),
					};
		const logsCheck =
			logsResult.status === "fulfilled"
				? logsResult.value
				: {
						ok: false,
						error: logsResult.reason,
						ms: 0,
						url: "/api/logs?tail=500",
						at: Date.now(),
					};
		const operationsCheck =
			operationsResult.status === "fulfilled"
				? operationsResult.value
				: {
						ok: false,
						error: operationsResult.reason,
						ms: 0,
						url: "/api/operations?limit=5000",
						at: Date.now(),
					};
		const resourcesCheck =
			resourcesResult.status === "fulfilled"
				? resourcesResult.value
				: {
						ok: false,
						error: resourcesResult.reason,
						ms: 0,
						url: "/api/resources",
						at: Date.now(),
					};
		state.backend.overview = overviewCheck;
		state.backend.logs = logsCheck;
		state.backend.operations = operationsCheck;
		state.backend.resources = resourcesCheck;
		state.backend.checkedAt = Date.now();
		if (!overviewCheck.ok) throw overviewCheck.error;
		state.overview = overviewCheck.value;
		state.lastSuccessAt = Date.now();
		state.failureCount = 0;
		if (logsCheck.ok)
			state.logs = Array.isArray(logsCheck.value) ? logsCheck.value : [];
		if (
			operationsCheck.ok &&
			operationsCheck.value &&
			Array.isArray(operationsCheck.value.events)
		)
			state.operations = operationsCheck.value;
		else if (!state.operations) state.operations = null;
		state.lastError =
			logsCheck.ok && operationsCheck.ok && resourcesCheck.ok
				? null
				: [
						logsCheck.ok ? "" : `Logs: ${apiErrorMessage(logsCheck.error)}`,
						operationsCheck.ok
							? ""
							: `Retained operations: ${apiErrorMessage(operationsCheck.error)}`,
						resourcesCheck.ok
							? ""
							: `Resources: ${apiErrorMessage(resourcesCheck.error)}`,
					]
						.filter(Boolean)
						.join(" · ");
		render();
	} catch (error) {
		if (error?.name !== "AbortError") {
			state.lastError = apiErrorMessage(error);
			state.failureCount = (state.failureCount || 0) + 1;
			renderError(state.lastError);
		}
	} finally {
		if (seq === state.seq) {
			state.controller = null;
			state.lastRefreshFinishedAt = Date.now();
			setBusy(false);
			scheduleRefresh();
		}
	}
}

function renderError(message) {
	document.body.dataset.systemTone = "bad";
	setSignalTones([
		[els.systemState, "bad"],
		[els.attentionCount, "bad"],
		[els.serverCount, "warn"],
		[els.loadState, "bad"],
	]);
	els.systemState.textContent = "Degraded";
	els.systemNote.textContent = `Dashboard refresh failed: ${message}`;
	if (els.opsDot) els.opsDot.className = "dot bad";
	if (els.opsTitle)
		els.opsTitle.textContent = "Dashboard backend is not connected";
	if (els.opsBody) els.opsBody.textContent = `Last refresh failed: ${message}`;
	if (els.opsCommandRow) {
		setSafeHtml(
			els.opsCommandRow,
			[
				`<button class="primary" type="button" data-global-action="start-hub">Start hub</button>`,
				`<button type="button" data-global-action="check-link">Check link</button>`,
				`<button class="quiet" type="button" data-global-action="refresh">Refresh overview</button>`,
			].join(""),
		);
	}
	if (els.opsSteps)
		setSafeHtml(
			els.opsSteps,
			[
				stepCard("Backend offline", message, "bad"),
				stepCard("Runtime unknown", "No fresh overview", "warn"),
				stepCard(
					"Actions not verified",
					"Use Check link after backend is reachable",
					"warn",
				),
			].join(""),
		);
	renderDecisionRunway({
		overview: {
			userReadiness: {
				confidence: 0,
				primaryAction: "Connect backend",
				endpoint: "/mcp",
			},
		},
		hub: { status: "offline" },
		servers: [],
		clients: [],
		attentionItems: [{ title: "Backend offline" }],
		attentionTotal: 1,
		runtimeReady: false,
		enabledCount: 0,
		active: 0,
		max: 0,
		badPolicies: 1,
		warnPolicies: 0,
	});
	renderBaseSetup({
		overview: { userReadiness: { endpoint: "/mcp" } },
		hub: { status: "offline" },
		servers: [],
		clients: [],
		runtimeReady: false,
	});
	renderAccessReview(
		{
			status: "bad",
			title: "Access review paused",
			body: "Reconnect /api/overview before trusting tool permissions, secrets, or remote origins.",
			counts: { servers: 0, enabled: 0 },
			items: [
				{
					label: "Approval",
					count: 0,
					status: "warn",
					body: "Backend offline; approval state is unknown.",
				},
				{
					label: "Secrets",
					count: 0,
					status: "warn",
					body: "Secret values remain hidden while offline.",
				},
				{
					label: "Remote/Auth",
					count: 0,
					status: "warn",
					body: "Remote origins require live overview.",
				},
				{
					label: "Evidence",
					count: 0,
					status: "bad",
					body: "No tools/list evidence while backend is offline.",
				},
			],
		},
		[],
	);
	renderNextAction({
		overview: {
			userReadiness: {
				confidence: 0,
				primaryAction: "Connect backend",
				endpoint: "/mcp",
			},
		},
		hub: { status: "offline" },
		servers: [],
		clients: [],
		attentionItems: [{ title: "Backend offline", tone: "bad", meta: message }],
		attentionTotal: 1,
		runtimeReady: false,
		enabledCount: 0,
		active: 0,
		max: 0,
		badPolicies: 1,
		warnPolicies: 0,
	});
	renderConnectionMap(
		{
			userReadiness: { confidence: 0, endpoint: "/mcp" },
			hub: { status: "offline" },
			readiness: { runtimePrerequisitesReady: false },
		},
		[],
		[],
	);
	renderProtocolCompatibility(
		{
			userReadiness: { confidence: 0, endpoint: "/mcp" },
			hub: { status: "offline" },
			readiness: { runtimePrerequisitesReady: false },
		},
		[],
		[],
	);
	renderSetupQueue({
		overview: {},
		hub: { status: "offline" },
		servers: [],
		clients: [],
		instances: [],
		attentionItems: [{ title: "Backend offline" }],
		attentionTotal: 1,
		runtimeReady: false,
	});
	if (els.backendState) els.backendState.textContent = "Backend not connected";
	if (els.backendGrid)
		setSafeHtml(
			els.backendGrid,
			[
				readout("/api/overview", "failed", message, "bad"),
				readout("/api/logs", "unknown", "overview failed first", "warn"),
				readout(
					"/api/resources",
					state.backend.resources?.ok ? "ok" : "unknown",
					state.backend.resources
						? `${fmtMs(state.backend.resources.ms)} · ${fmtDate(state.backend.resources.at)}`
						: "waiting",
					state.backend.resources?.ok ? "good" : "warn",
				),
				readout(
					"action ping",
					state.backend.action?.ok ? "ok" : "not checked",
					state.backend.action
						? `${fmtMs(state.backend.action.ms)} · ${fmtDate(state.backend.action.at)}`
						: "waiting",
					state.backend.action?.ok ? "good" : "warn",
				),
			].join(""),
		);
	els.loadState.textContent = "Failed";
	els.loadNote.textContent = "Backend overview request failed.";
	els.attentionCount.textContent = "1";
	els.attentionNote.textContent = "Refresh failed.";
	setSafeHtml(
		els.attentionList,
		`<article class="item bad"><div class="item-head"><div class="name">Dashboard refresh failed</div>${chip("error", "bad")}</div><div class="meta">${escapeHtml(message)}</div></article>`,
	);
	if (els.serverCommandCenter) {
		setCardTone(els.serverCommandCenter, "bad");
		if (els.serverCommandTitle)
			els.serverCommandTitle.textContent =
				"Server fleet is not trustworthy without live backend state.";
		if (els.serverCommandBody)
			els.serverCommandBody.textContent =
				"Reconnect /api/overview before applying policy, testing servers, or interpreting inventory counts.";
		if (els.serverMetricRow)
			setSafeHtml(
				els.serverMetricRow,
				[
					fleetMetric("Visible", "—", "backend offline", "bad"),
					fleetMetric("Evidence", "—", "not loaded", "warn"),
					fleetMetric("Policy", "—", "not loaded", "warn"),
					fleetMetric("Capacity", "—", "not loaded", "warn"),
				].join(""),
			);
	}
	if (els.serverWorkbench)
		setSafeHtml(
			els.serverWorkbench,
			`<div class="workbench-summary"><span class="workbench-index">!</span><div><strong>Reconnect before tuning.</strong><p>Server actions stay available, but the safest path is Start hub → Check link → Refresh overview.</p></div></div>`,
		);
	if (els.serverList)
		setSafeHtml(
			els.serverList,
			`<div class="empty-state bad"><strong>Server list is paused.</strong><p>${escapeHtml(message)}</p><div class="empty-actions"><button class="primary" type="button" data-empty-action="check-link">Check link</button><button type="button" data-empty-action="refresh">Refresh</button></div></div>`,
		);
	updateRefreshChip();
}

async function runAction(path, button, confirmMessage, busyLabel) {
	if (confirmMessage && !window.confirm(confirmMessage)) return;
	const original = button?.textContent || "";
	try {
		if (button) {
			button.disabled = true;
			button.textContent = busyLabel || "Working…";
		}
		const result = await timedFetchJson(path, {
			method: "POST",
			timeoutMs: ACTION_TIMEOUT_MS,
		});
		state.backend.action = {
			ok: result.ok,
			ms: result.ms,
			at: result.at,
			endpoint: path.replace("/api/actions/", ""),
			error: result.ok ? "" : apiErrorMessage(result.error),
		};
		if (!result.ok) throw result.error;
		await refreshDashboard({ force: true, reason: "action" });
	} catch (error) {
		state.lastError = apiErrorMessage(error);
		renderError(state.lastError);
	} finally {
		if (button) {
			button.disabled = false;
			button.textContent = original;
		}
	}
}
