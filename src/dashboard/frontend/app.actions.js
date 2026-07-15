// MCPace dashboard dialog/action chunk. Loaded after /dashboard.render.js and before /dashboard.boot.js.
function openServerDialog(name, initialTab = "overview") {
	const nextTab = SERVER_DIALOG_TABS.includes(initialTab)
		? initialTab
		: "overview";
	state.selectedServer = name;
	state.serverDialogTab = nextTab;
	renderServerDialogByName(name);
	if (els.serverDialog && !els.serverDialog.open) {
		if (typeof els.serverDialog.showModal === "function")
			els.serverDialog.showModal();
		else els.serverDialog.setAttribute("open", "");
	}
	setServerDialogTab(nextTab);
	window.setTimeout(
		() => els.serverDialogTitle?.focus?.({ preventScroll: true }),
		0,
	);
}

function closeServerDialog() {
	state.selectedServer = null;
	if (!els.serverDialog) return;
	if (typeof els.serverDialog.close === "function") els.serverDialog.close();
	else els.serverDialog.removeAttribute("open");
}

function renderServerDialogByName(name) {
	const server = findServer(name);
	if (!server) {
		if (els.serverDialog?.open) closeServerDialog();
		return;
	}
	const related = relatedInstances(name);
	const model = serverViewModel(server, related);
	const risk = model.risk;
	const workers = model.workers;
	const inFlight = model.inFlight;
	const mode = model.routeMode;
	const routing = routingPlain(server, risk, related);
	const evidence = evidenceLine(server);
	const toolEvidence = serverToolEvidence(server);
	const recommendation = model.recommendation;
	const verdict = model.verdict;
	const decision = model.decision;
	const settings = model.settings;
	const checklist = serverChecklist(server, risk)
		.map((item) => `<li>${escapeHtml(item)}</li>`)
		.join("");
	const operatorRunbook = renderServerRunbook(model.operatorPlan);
	const runtimeControl = runtimeControlForServer(server.name);
	const lockDomains = Array.isArray(server.lockDomains)
		? server.lockDomains.join(", ")
		: text(
				server.hostLock || server.hostLockKey || server.conflictDomain,
				"none",
			);
	const requiredCommands = Array.isArray(server.requiredCommands)
		? server.requiredCommands.join(", ")
		: "none";
	const launch = launchCommand(server);
	const sourceArgs = compactList(server.sourceArgs);
	const sourceEnvNames = compactList(server.sourceEnvNames);
	const sourceHeaderNames = compactList(server.sourceHeaderNames);
	const sourcePath = text(server.sourcePath, "default MCP settings");
	const idName = domId(server.name || "server");
	const nameEsc = escapeHtml(server.name || "server");
	const tools = Array.isArray(toolEvidence.toolNames)
		? toolEvidence.toolNames
		: [];
	const toolTags = tools.length
		? `<div class="tags">${tools
				.slice(0, 10)
				.map((tool) => `<span class="tag">${escapeHtml(tool)}</span>`)
				.join(
					"",
				)}${tools.length > 10 ? `<span class="tag">+${tools.length - 10} more</span>` : ""}</div>`
		: `<p>No tool names are available yet. Run Test to collect initialize + tools/list evidence.</p>`;

	els.serverDialogTitle.textContent = server.name || "source";
	els.serverDialogSubtitle.textContent = `${verdict.label} · ${model.category} · ${server.effectiveEnabled ? "enabled" : "disabled"}`;
	setSafeHtml(
		els.serverDialogBody,
		`
          <section class="server-dialog-panel" id="server-dialog-panel-overview" role="tabpanel" aria-labelledby="server-dialog-tab-overview" data-server-dialog-panel="overview">
            <div class="server-dialog-lead ${escapeHtml(verdict.tone || "warn")}">
              <div class="label">Next action</div>
              <strong>${escapeHtml(decision.title)}</strong>
              <p>${escapeHtml(decision.body)}</p>
              <div class="server-dialog-actions" aria-label="Primary actions for ${nameEsc}">
                ${serverControls(server, related, "dialog")}
              </div>
            </div>
            <div class="server-setting-brief" aria-label="Source summary for ${nameEsc}">
              ${settingCard("Current state", settings.stateTitle, settings.stateBody)}
              ${settingCard("Tool evidence", settings.useTitle, settings.useBody)}
              ${settingCard("Current route", settings.current, model.needsTuning ? `Recommended: ${recommendation.label}.` : "Current route matches the conservative recommendation.")}
            </div>
            <section class="server-explain-box">
              <div class="label">Available tools</div>
              ${toolTags}
            </section>
            <section class="server-explain-box">
              <div class="label">Evidence interpretation</div>
              <p>${escapeHtml(evidence)}</p>
            </section>
            <section class="server-explain-box">
              <div class="label">Safety notes</div>
              <ul class="server-checklist">${checklist}</ul>
            </section>
            ${operatorRunbook}
          </section>

          <section class="server-dialog-panel" id="server-dialog-panel-routing" role="tabpanel" aria-labelledby="server-dialog-tab-routing" data-server-dialog-panel="routing" hidden>
            <div class="server-dialog-lead">
              <div class="label">Routing decision</div>
              <strong>${escapeHtml(model.needsTuning ? `${recommendation.label} recommended` : "Current route is conservative")}</strong>
              <p>${escapeHtml(routing)}</p>
            </div>
            <section class="server-settings" aria-label="Editable routing settings for ${nameEsc}">
              <div class="server-setting-box">
                <div><div class="label">Routing mode</div><p class="server-setting-brief">Choose how requests share or isolate the upstream process.</p></div>
                <label class="sr-only" for="dialog-mode-${idName}">Routing mode for ${nameEsc}</label>
                <select class="server-mode-select" id="dialog-mode-${idName}" data-server-input="mode">${modeOptions(mode)}</select>
              </div>
              <div class="server-setting-box">
                <div><div class="label">Worker count</div><p class="server-setting-brief">Maximum upstream processes available to this source.</p></div>
                <label class="sr-only" for="dialog-workers-${idName}">Worker count</label>
                <input id="dialog-workers-${idName}" type="number" min="1" step="1" value="${escapeHtml(workers)}" data-server-input="workers">
              </div>
              <div class="server-setting-box">
                <div><div class="label">In-flight per worker</div><p class="server-setting-brief">Maximum simultaneous requests admitted to each worker.</p></div>
                <label class="sr-only" for="dialog-inflight-${idName}">In-flight requests per worker</label>
                <input id="dialog-inflight-${idName}" type="number" min="1" step="1" value="${escapeHtml(inFlight)}" data-server-input="inFlight">
              </div>
              <button class="primary" type="button" data-server-name="${nameEsc}" data-server-action="apply-policy">Apply routing changes</button>
            </section>
            <div class="dialog-plan-note"><strong>Recommendation:</strong> ${escapeHtml(recommendation.label)} · ${escapeHtml(recommendation.reason || model.nextStep)}</div>
            ${renderRuntimeControl(runtimeControl)}
          </section>

          <section class="server-dialog-panel" id="server-dialog-panel-source" role="tabpanel" aria-labelledby="server-dialog-tab-source" data-server-dialog-panel="source" hidden>
            <div class="server-dialog-lead">
              <div class="label">Launch boundary</div>
              <strong>${escapeHtml(launch || server.sourceUrl || "No launch command reported")}</strong>
              <p>Configuration names are visible for review. Secret values remain hidden.</p>
            </div>
            <section class="server-explain-box">
              <div class="label">Source configuration</div>
              <div class="detail-grid" style="margin-top: 10px;">
                ${detail("Source file", sourcePath)}
                ${detail("Launch command", launch || "none")}
                ${detail("Source URL", server.sourceUrl || "none")}
                ${detail("Arguments", sourceArgs)}
                ${detail("Environment names", sourceEnvNames)}
                ${detail("Header names", sourceHeaderNames)}
                ${detail("Required commands", requiredCommands)}
                ${detail("Transport", server.transportPreference || server.sourceType || "stdio")}
              </div>
            </section>
            <section class="server-explain-box">
              <div class="label">Technical metadata</div>
              <div class="detail-grid" style="margin-top: 10px;">
                ${detail("Kind", server.kind)}
                ${detail("Profile enabled", server.profileEnabled ? "yes" : "no")}
                ${detail("Source enabled", server.sourceEnabled ? "yes" : "no")}
                ${detail("Effective enabled", server.effectiveEnabled ? "yes" : "no")}
                ${detail("Scope", server.scopeClass)}
                ${detail("Effect", server.effectClass)}
                ${detail("State", server.stateClass)}
                ${detail("State binding", server.stateBinding)}
                ${detail("Credential binding", server.credentialBinding)}
                ${detail("Reuse model", server.defaultPoolModel)}
                ${detail("Scheduler lane", server.schedulerLane)}
                ${detail("Request strategy", server.requestStrategy)}
                ${detail("Conflict domain", server.conflictDomain)}
                ${detail("Conflict domains", lockDomains)}
                ${detail("Launcher", server.launcherKind)}
                ${detail("Startup", server.startupStrategy)}
                ${detail("Transport status", server.transportStatus)}
                ${detail("Evidence status", toolEvidence.status)}
              </div>
            </section>
          </section>
        `,
	);
	setServerDialogTab(state.serverDialogTab);
}

function actionPayloadForPolicy(server, related, overrides = {}) {
	return {
		server: server.name,
		mode: overrides.mode ?? serverMode(server, related),
		maxWorkers: overrides.maxWorkers ?? maxWorkers(server, related),
		maxInFlightPerWorker:
			overrides.maxInFlightPerWorker ?? maxInFlight(server, related),
	};
}

function positiveInputValue(selector, fallback) {
	const value = Number(els.serverDialogBody?.querySelector(selector)?.value);
	return Number.isFinite(value) && value > 0 ? Math.round(value) : fallback;
}

async function postServerAction(endpoint, payload) {
	const result = await timedFetchJson(`/api/actions/${endpoint}`, {
		method: "POST",
		timeoutMs: ACTION_TIMEOUT_MS,
		headers: { "content-type": "application/json" },
		body: JSON.stringify(payload),
	});
	state.backend.action = {
		ok: result.ok,
		ms: result.ms,
		at: result.at,
		endpoint,
		error: result.ok ? "" : apiErrorMessage(result.error),
	};
	if (!result.ok) throw result.error;
	return result.value;
}

async function submitServerImportConfig(event) {
	event.preventDefault();
	const sourcePath = String(els.serverImportPath?.value || "").trim();
	const settingsPath = String(els.serverImportSettings?.value || "").trim();
	const intent = importPathIntent(sourcePath);
	if (intent.tone === "bad" || !sourcePath) {
		state.importer.error = !sourcePath
			? "Enter a local MCP settings JSON path to preview."
			: intent.body;
		setFieldError(
			els.serverImportError,
			els.serverImportPath,
			state.importer.error,
		);
		renderServerImportPanel();
		els.serverImportPath?.focus?.();
		return;
	}
	setFieldError(els.serverImportError, els.serverImportPath, "");
	const payload = {
		sourcePath,
		dryRun: els.serverImportDryRun?.checked !== false,
		disabled: els.serverImportDisabled?.checked !== false,
	};
	if (settingsPath) payload.settingsPath = settingsPath;
	if (els.serverImportForce?.checked) payload.force = true;
	const button = els.serverImportButton;
	const original = button?.textContent || "Preview import";
	try {
		state.importer.loading = true;
		state.importer.error = null;
		setFieldError(els.serverImportError, els.serverImportPath, "");
		state.importer.last = payload;
		renderServerImportPanel();
		if (button) {
			button.disabled = true;
			button.textContent = payload.dryRun ? "Previewing…" : "Importing…";
		}
		const response = await postServerAction("server-import-config", payload);
		state.importer.result = response;
		state.importer.error = null;
		if (!payload.dryRun)
			await refreshDashboard({ force: true, reason: "server-import-config" });
	} catch (error) {
		state.importer.error = apiErrorMessage(error);
		state.lastError = state.importer.error;
		renderError(state.importer.error);
	} finally {
		state.importer.loading = false;
		if (button) {
			button.disabled = false;
			button.textContent = original;
		}
		renderServerImportPanel();
	}
}

async function submitServerDiscovery(event) {
	event.preventDefault();
	const query = String(els.serverDiscoverQuery?.value || "").trim();
	const mode = String(els.serverDiscoverMode?.value || "preview");
	if (mode !== "preview" && !query) {
		state.discovery.error =
			"Install mode needs a search term so MCPace does not run a broad automatic sweep from the dashboard.";
		setFieldError(
			els.serverDiscoverError,
			els.serverDiscoverQuery,
			state.discovery.error,
		);
		renderDiscoveryPanel();
		els.serverDiscoverQuery?.focus?.();
		return;
	}
	setFieldError(els.serverDiscoverError, els.serverDiscoverQuery, "");
	const payload = {
		mode: mode === "install" ? "apply" : "preview",
		dryRun: mode === "preview",
		disabled: true,
	};
	if (query) payload.query = query;
	if (els.serverDiscoverRefresh?.checked) payload.refresh = true;
	if (els.serverDiscoverReview?.checked) payload.allowReviewInstall = true;
	if (mode === "install") payload.autoInstall = true;
	const button = els.serverDiscoverButton;
	const original = button?.textContent || "Find candidates";
	try {
		state.discovery.loading = true;
		state.discovery.error = null;
		setFieldError(els.serverDiscoverError, els.serverDiscoverQuery, "");
		state.discovery.lastMode = mode;
		renderDiscoveryPanel();
		if (button) {
			button.disabled = true;
			button.textContent = mode === "preview" ? "Previewing…" : "Installing…";
		}
		const response = await postServerAction("server-discover", payload);
		state.discovery.result = response;
		state.discovery.error = null;
		if (mode === "install")
			await refreshDashboard({ force: true, reason: "server-discover" });
	} catch (error) {
		state.discovery.error = apiErrorMessage(error);
		state.lastError = state.discovery.error;
		renderError(state.discovery.error);
	} finally {
		state.discovery.loading = false;
		if (button) {
			button.disabled = false;
			button.textContent = original;
		}
		renderDiscoveryPanel();
	}
}

async function submitServerInstallCommand(event) {
	event.preventDefault();
	const commandLine = String(els.serverInstallCommand?.value || "").trim();
	const server = String(els.serverInstallName?.value || "").trim();
	if (!commandLine) {
		const message =
			"Paste a command, package, local path, or Streamable HTTP URL first.";
		setInstallNote(message, "bad");
		setFieldError(els.serverInstallError, els.serverInstallCommand, message);
		els.serverInstallCommand?.focus();
		return;
	}
	const intent = installCommandIntent(commandLine);
	if (intent.tone === "bad") {
		setInstallNote(`${intent.label}: ${intent.body}`, "bad");
		setFieldError(
			els.serverInstallError,
			els.serverInstallCommand,
			intent.body,
		);
		els.serverInstallCommand?.focus();
		return;
	}
	setFieldError(els.serverInstallError, els.serverInstallCommand, "");
	const button = els.serverInstallButton;
	const original = button?.textContent || "Save server";
	const payload = {
		commandLine,
		disabled: Boolean(els.serverInstallDisabled?.checked),
		force: Boolean(els.serverInstallForce?.checked),
		dryRun: Boolean(els.serverInstallDryRun?.checked),
	};
	if (server) payload.server = server;
	try {
		if (button) {
			button.disabled = true;
			button.textContent = payload.dryRun ? "Previewing…" : "Saving…";
		}
		setFieldError(els.serverInstallError, els.serverInstallCommand, "");
		setInstallNote(
			payload.dryRun ? "Previewing install plan…" : "Saving server source…",
			"warn",
		);
		const response = await postServerAction("server-install-command", payload);
		const planned =
			response?.result?.plan?.name ||
			response?.result?.write?.name ||
			server ||
			"server";
		setInstallNote(
			payload.dryRun
				? `Preview ready for ${planned}; nothing was written.`
				: `Saved ${planned}. Run Test on its row before relying on its tools.`,
			"good",
		);
		if (!payload.dryRun) {
			if (els.serverInstallCommand) els.serverInstallCommand.value = "";
			if (els.serverInstallName) els.serverInstallName.value = "";
		}
		await refreshDashboard({ force: true, reason: "server-install-command" });
	} catch (error) {
		state.lastError = apiErrorMessage(error);
		setInstallNote(state.lastError, "bad");
		renderError(state.lastError);
	} finally {
		if (button) {
			button.disabled = false;
			button.textContent = original;
		}
	}
}

function applyOptimisticServerAction(endpoint, payload, response) {
	const server = findServer(payload?.server);
	if (!server) return;
	if (endpoint === "server-enable" || endpoint === "server-disable") {
		const enabled = endpoint === "server-enable";
		server.sourceEnabled = enabled;
		server.profileEnabled = enabled;
		server.effectiveEnabled = enabled && server.platformSupported !== false;
		delete state.serverTests?.[payload?.server];
		return;
	}
	if (endpoint === "server-test") {
		state.serverTests[payload.server] = normalizeProbeEvidence(
			payload.server,
			response,
		);
		return;
	}
	if (endpoint === "server-policy") {
		const result = response?.result || {};
		const policy = result.policy || {};
		const execution = result.execution || {};
		const firstDefined = (...values) =>
			values.find(
				(value) => value !== undefined && value !== null && value !== "",
			);
		server.maxWorkers = Number(
			firstDefined(
				result.maxWorkers,
				execution.maxWorkers,
				payload.maxWorkers,
				server.maxWorkers,
				1,
			),
		);
		server.maxInFlightPerWorker = Number(
			firstDefined(
				result.maxInFlightPerWorker,
				execution.maxInFlightPerWorker,
				payload.maxInFlightPerWorker,
				server.maxInFlightPerWorker,
				1,
			),
		);
		for (const key of [
			"scopeClass",
			"concurrencyPolicy",
			"stateBinding",
			"credentialBinding",
			"parallelismLimit",
			"conflictDomain",
			"projectRootMode",
			"worktreeBinding",
			"stateProfileMode",
			"hostLock",
			"startupStrategy",
			"routingGroup",
			"discoveryRequiresLease",
		]) {
			if (policy[key] !== undefined) server[key] = policy[key];
		}
	}
}

function syncAfterServerAction(delay = 250) {
	window.setTimeout(
		() => refreshDashboard({ force: true, reason: "server-action-sync" }),
		delay,
	);
}

async function runServerAction(
	endpoint,
	payload,
	control,
	busyLabel = "Working…",
	options = {},
) {
	const originalText =
		control && "textContent" in control ? control.textContent : "";
	try {
		if (control) {
			control.disabled = true;
			if (control.tagName === "BUTTON") control.textContent = busyLabel;
		}
		const response = await postServerAction(endpoint, payload);
		applyOptimisticServerAction(endpoint, payload, response);
		state.lastError = null;
		if (options.render !== false) render();
		if (options.sync !== false) syncAfterServerAction();
		return response;
	} catch (error) {
		state.lastError = apiErrorMessage(error);
		renderError(state.lastError);
		return null;
	} finally {
		if (control) {
			control.disabled = false;
			if (control.tagName === "BUTTON") control.textContent = originalText;
		}
	}
}

async function runServerTest(serverName, control, options = {}) {
	return runServerAction(
		"server-test",
		{ server: serverName, timeoutMs: 10000 },
		control,
		"Testing…",
		options,
	);
}

async function confirmServerMutation(
	action,
	serverName,
	server,
	related,
	control,
	extra = {},
) {
	if (control?.dataset?.mcSkipProductConfirm === "true") return true;
	const hook = window.__MCPACE_PRODUCT_CONFIRM_SERVER_ACTION__;
	if (typeof hook === "function") {
		try {
			return Boolean(
				await hook({
					action,
					name: serverName,
					enabled: Boolean(server?.effectiveEnabled),
					sourceType: server?.sourceType || "",
					activeInstances: Array.isArray(related) ? related.length : 0,
					...extra,
				}),
			);
		} catch (_) {
			return false;
		}
	}
	const messages = {
		"enable-test": `Enable ${serverName} and run Test now? This can launch the upstream command or call a remote endpoint, so review source and secrets first.`,
		enable: `Turn on ${serverName}? This changes routing state but does not launch the upstream command. Run Test next if you need tools evidence.`,
		disable: `Turn off ${serverName}? Configured clients will stop reaching this integration through MCPace.`,
		test: `Run Test for ${serverName}? This can launch a local command or contact a remote endpoint.`,
		remove: `Remove ${serverName} from its MCP settings source? This deletes the saved definition and cannot be undone from the dashboard.`,
	};
	if (typeof window.confirm === "function")
		return window.confirm(
			messages[action] || `Continue with ${action} for ${serverName}?`,
		);
	return true;
}

function notifyServerMutation(action, serverName, control, response) {
	const hook = window.__MCPACE_PRODUCT_SERVER_ACTION_RESULT__;
	if (typeof hook !== "function") return;
	try {
		hook({
			action,
			name: serverName,
			ok: Boolean(response),
			bulk: control?.dataset?.mcBulkAction === "true",
			undo: control?.dataset?.mcUndoAction === "true",
		});
	} catch (_) {}
}

async function enableAndTestServer(serverName, control) {
	const server = findServer(serverName);
	const related = relatedInstances(serverName);
	if (
		!(await confirmServerMutation(
			"enable-test",
			serverName,
			server,
			related,
			control,
		))
	)
		return null;
	const enabled = await runServerAction(
		"server-enable",
		{ server: serverName },
		control,
		"Enabling…",
		{ sync: false },
	);
	if (!enabled) return null;
	notifyServerMutation("enable-test", serverName, control, enabled);
	return runServerTest(serverName, control);
}

async function handleServerControl(control) {
	const action = control?.dataset?.serverAction;
	const name =
		control?.dataset?.serverName ||
		control?.closest("[data-server-name]")?.dataset?.serverName;
	if (!action || !name) return;
	const server = findServer(name);
	const related = relatedInstances(name);
	if (!server && !["settings", "routing"].includes(action)) return;

	if (action === "settings" || action === "routing") {
		openServerDialog(name, action === "routing" ? "routing" : "overview");
		return;
	}
	if (action === "enable-test") {
		if (server.effectiveEnabled) {
			if (
				!(await confirmServerMutation("test", name, server, related, control))
			)
				return;
			await runServerTest(name, control);
		} else {
			await enableAndTestServer(name, control, server, related);
		}
		return;
	}
	if (action === "toggle") {
		const mutation = server.effectiveEnabled ? "disable" : "enable";
		if (
			!(await confirmServerMutation(mutation, name, server, related, control))
		)
			return;
		const response =
			mutation === "enable"
				? await runServerAction(
						"server-enable",
						{ server: name },
						control,
						"Turning on…",
					)
				: await runServerAction(
						"server-disable",
						{ server: name },
						control,
						"Turning off…",
					);
		notifyServerMutation(mutation, name, control, response);
		return;
	}
	if (action === "test") {
		if (!(await confirmServerMutation("test", name, server, related, control)))
			return;
		await runServerTest(name, control);
		return;
	}
	if (action === "remove") {
		const payload = { server: name, dryRun: true };
		if (server.sourcePath) payload.settingsPath = server.sourcePath;
		const preview = await runServerAction(
			"server-remove",
			payload,
			control,
			"Reviewing…",
			{ sync: false, render: false },
		);
		if (!preview) return;
		const plan = preview?.result || {};
		if (plan.name && String(plan.name) !== String(name)) {
			reportError("Removal preview did not match the selected server.");
			return;
		}
		const approved = await confirmServerMutation(
			"remove",
			name,
			server,
			related,
			control,
			{ removalPlan: plan },
		);
		if (!approved) return;
		const removePayload = { server: name, dryRun: false };
		const previewPath = String(plan.path || payload.settingsPath || "").trim();
		if (previewPath) removePayload.settingsPath = previewPath;
		const removed = await runServerAction(
			"server-remove",
			removePayload,
			control,
			"Removing…",
			{ sync: false, render: false },
		);
		notifyServerMutation("remove", name, control, removed);
		if (removed) {
			closeServerDialog();
			await refreshDashboard({ force: true, reason: "server-remove" });
		}
		return;
	}
	if (action === "workers-dec" || action === "workers-inc") {
		const delta = action === "workers-inc" ? 1 : -1;
		const nextWorkers = Math.max(1, maxWorkers(server, related) + delta);
		await runServerAction(
			"server-policy",
			actionPayloadForPolicy(server, related, { maxWorkers: nextWorkers }),
			control,
			"Saving…",
		);
		return;
	}
	if (action === "auto") {
		const recommendation = recommendedPolicy(server, related);
		await runServerAction(
			"server-policy",
			actionPayloadForPolicy(server, related, recommendation),
			control,
			"Auto…",
		);
		return;
	}
	if (action === "mode") {
		await runServerAction(
			"server-policy",
			actionPayloadForPolicy(server, related, { mode: control.value }),
			control,
			"Saving…",
		);
		return;
	}
	if (action === "apply-policy") {
		const nextWorkers = positiveInputValue(
			'[data-server-input="workers"]',
			maxWorkers(server, related),
		);
		const nextInFlight = positiveInputValue(
			'[data-server-input="inFlight"]',
			maxInFlight(server, related),
		);
		const mode =
			els.serverDialogBody?.querySelector('[data-server-input="mode"]')
				?.value || serverMode(server, related);
		await runServerAction(
			"server-policy",
			actionPayloadForPolicy(server, related, {
				mode,
				maxWorkers: nextWorkers,
				maxInFlightPerWorker: nextInFlight,
			}),
			control,
			"Applying…",
		);
	}
}

async function autoTuneVisibleServers(button) {
	const plan = autoPolicyPlan();
	if (!plan.changes.length) return;
	const original = button?.textContent || "";
	try {
		if (button) {
			button.disabled = true;
			button.textContent = "Applying safe plan…";
		}
		const response = await postServerAction("server-autotune", {
			changes: plan.changes,
		});
		const results = response?.result?.results || [];
		for (let index = 0; index < plan.changes.length; index += 1) {
			applyOptimisticServerAction("server-policy", plan.changes[index], {
				result: results[index] || {},
			});
		}
		state.lastError = null;
		render();
		syncAfterServerAction(100);
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

async function checkBackendLink(button) {
	const original = button?.textContent || "";
	try {
		if (button) {
			button.disabled = true;
			button.textContent = "Checking…";
		}
		const [overview, logs, resources, ping] = await Promise.all([
			timedFetchJson("/api/overview"),
			timedFetchJson("/api/logs?tail=1"),
			timedFetchJson("/api/resources"),
			timedFetchJson("/api/actions/ping", {
				method: "POST",
				timeoutMs: ACTION_TIMEOUT_MS,
			}),
		]);
		state.backend.overview = overview;
		state.backend.logs = logs;
		state.backend.resources = resources;
		state.backend.action = {
			ok: ping.ok,
			ms: ping.ms,
			at: ping.at,
			endpoint: "ping",
			error: ping.ok ? "" : apiErrorMessage(ping.error),
		};
		state.backend.checkedAt = Date.now();
		if (overview.ok) state.overview = overview.value;
		if (logs.ok)
			state.logs = Array.isArray(logs.value) ? logs.value : state.logs;
		state.lastError =
			[
				overview.ok ? "" : `Overview: ${apiErrorMessage(overview.error)}`,
				logs.ok ? "" : `Logs: ${apiErrorMessage(logs.error)}`,
				resources.ok ? "" : `Resources: ${apiErrorMessage(resources.error)}`,
				ping.ok ? "" : `Action ping: ${apiErrorMessage(ping.error)}`,
			]
				.filter(Boolean)
				.join(" · ") || null;
		if (state.overview) render();
		else renderError(state.lastError || "Backend check failed");
	} catch (error) {
		state.lastError = apiErrorMessage(error);
		if (state.overview) render();
		else renderError(state.lastError);
	} finally {
		if (button) {
			button.disabled = false;
			button.textContent = original;
		}
	}
}
