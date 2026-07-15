// MCPace dashboard detailed operations, fleet, runtime, and log renderers.
function renderOperations(context) {
	const {
		hub,
		readiness,
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
	} = context;
	const overview = state.backend.overview;
	const logs = state.backend.logs;
	const resources = state.backend.resources;
	const action = state.backend.action;
	const backendOk = Boolean(overview?.ok);
	const logsOk = !logs || logs.ok;
	const resourcesOk = !resources || resources.ok;
	const actionLabel = action
		? action.ok
			? `${action.endpoint} ok`
			: `${action.endpoint} failed`
		: "action not checked";
	const actionTone = !action ? "warn" : action.ok ? "good" : "bad";
	const logsMeta = logs?.ok
		? `updated ${fmtDate(logs.at)}`
		: logs
			? apiErrorMessage(logs.error)
			: "waiting";
	const resourcesMeta = resources?.ok
		? `${active}/${max || "?"} active HTTP`
		: resources
			? apiErrorMessage(resources.error)
			: "waiting";
	setSurfaceTone(els.opsTitle, systemTone);
	setSurfaceTone(
		els.backendState,
		backendOk && logsOk && resourcesOk ? "good" : backendOk ? "warn" : "bad",
	);

	if (els.opsDot) els.opsDot.className = `dot ${systemTone}`;
	if (els.opsTitle) {
		els.opsTitle.textContent = !backendOk
			? "Dashboard backend is not connected"
			: !runtimeReady
				? "Runtime needs setup"
				: attentionTotal
					? `${attentionTotal} blocker${attentionTotal === 1 ? "" : "s"} need attention`
					: "Ready for normal use";
	}
	if (els.opsBody) {
		els.opsBody.textContent = !backendOk
			? "The UI is loaded, but live overview data did not return. Use Check link or Refresh to verify the local backend."
			: attentionTotal
				? "Fix the watchlist first. Fleet groups and named diagnostic tasks hold the remaining context."
				: "No server-by-server tuning is required. Watch backend link, auto plan, and fleet groups; open Settings only for manual overrides.";
	}
	if (els.opsCommandRow) {
		setSafeHtml(
			els.opsCommandRow,
			!backendOk
				? [
						`<button class="primary" type="button" data-global-action="start-hub">Start hub</button>`,
						`<button type="button" data-global-action="check-link">Check link</button>`,
						`<button class="quiet" type="button" data-global-action="refresh">Refresh overview</button>`,
					].join("")
				: !runtimeReady
					? [
							`<button class="primary" type="button" data-global-action="repair">Repair runtime</button>`,
							`<button type="button" data-global-action="check-link">Check link</button>`,
							`<button class="quiet" type="button" data-global-action="refresh">Refresh overview</button>`,
						].join("")
					: attentionTotal
						? [
								`<button class="primary" type="button" data-global-action="refresh">Refresh evidence</button>`,
								`<button type="button" data-global-action="check-link">Check link</button>`,
								`<button class="quiet" type="button" data-global-action="repair">Repair</button>`,
							].join("")
						: [
								`<button class="primary" type="button" data-global-action="refresh">Refresh</button>`,
								`<button type="button" data-global-action="check-link">Verify link</button>`,
							].join(""),
		);
	}
	if (els.opsSteps) {
		setSafeHtml(
			els.opsSteps,
			[
				stepCard(
					backendOk ? "Backend connected" : "Backend offline",
					backendOk
						? `/api/overview ${fmtMs(overview.ms)}`
						: text(overview?.error?.message || overview?.error, "waiting"),
					backendOk ? msTone(overview.ms) : "bad",
				),
				stepCard(
					runtimeReady ? "Runtime usable" : "Runtime setup needed",
					`${hub.status || hub.health || "unknown"} · ${readiness.profileSelectionSource || "profile"}`,
					runtimeReady ? "good" : "bad",
				),
				stepCard(
					attentionTotal ? "Watchlist has work" : "No visible blockers",
					attentionItems[0]?.title ||
						`${enabledCount} enabled server(s) under auto plan`,
					attentionTotal ? "warn" : "good",
				),
			].join(""),
		);
	}
	if (els.backendState) {
		els.backendState.textContent =
			backendOk && logsOk && resourcesOk
				? "Live backend connected"
				: backendOk
					? "Partial backend link"
					: "Backend not connected";
	}
	if (els.backendGrid) {
		setSafeHtml(
			els.backendGrid,
			[
				readout(
					"/api/overview",
					backendOk ? fmtMs(overview.ms) : "failed",
					backendOk
						? `updated ${fmtDate(overview.at)}`
						: text(overview?.error?.message || overview?.error, "waiting"),
					backendOk ? msTone(overview.ms) : "bad",
				),
				readout(
					"/api/logs",
					logs?.ok ? fmtMs(logs.ms) : logs ? "failed" : "pending",
					logsMeta,
					logs?.ok ? msTone(logs.ms) : "warn",
				),
				readout(
					"/api/resources",
					resources?.ok
						? fmtMs(resources.ms)
						: resources
							? "failed"
							: "pending",
					resourcesMeta,
					resources?.ok ? msTone(resources.ms) : "warn",
				),
				readout(
					"action ping",
					actionLabel,
					action
						? `${fmtMs(action.ms)} · ${fmtDate(action.at)}`
						: "use Check link or any action",
					actionTone,
				),
			].join(""),
		);
	}
}

function normalizeUserReadiness(value) {
	const item = value && typeof value === "object" ? value : {};
	return {
		schema: item.schema || "mcpace.userReadiness.v0",
		headline: item.headline || "User readiness unknown",
		body: item.body || "Backend did not return a user-readiness summary yet.",
		confidence: Number.isFinite(Number(item.confidence))
			? Number(item.confidence)
			: 0,
		primaryAction: item.primaryAction || "Refresh overview",
		primaryReason:
			item.primaryReason ||
			"Live backend state is required before trusting the UI.",
		shouldSee: Array.isArray(item.shouldSee) ? item.shouldSee : [],
		shouldHide: Array.isArray(item.shouldHide) ? item.shouldHide : [],
		missing: Array.isArray(item.missing) ? item.missing : [],
		endpoint: item.endpoint || "/mcp",
	};
}

function listSentence(items, fallback) {
	const values = Array.isArray(items)
		? items.filter(Boolean).map((value) => String(value))
		: [];
	if (!values.length) return fallback;
	return (
		values.slice(0, 4).join(" · ") +
		(values.length > 4 ? ` · +${values.length - 4}` : "")
	);
}

function renderUserReadiness(rawReadiness, servers = [], clients = []) {
	const readiness = normalizeUserReadiness(rawReadiness);
	const band = readinessBand(readiness.confidence);
	const tone = band.tone;
	setSurfaceTone(els.userReadinessTitle, tone);
	if (els.userReadinessTitle)
		els.userReadinessTitle.textContent = readiness.headline;
	if (els.userReadinessBody) els.userReadinessBody.textContent = readiness.body;
	if (els.userConfidenceChip) setChip(els.userConfidenceChip, band.label, tone);
	if (!els.userReadinessGrid) return;
	const visible = listSentence(
		readiness.shouldSee,
		"status, endpoint, server launch commands, live tool evidence",
	);
	const hidden = listSentence(
		readiness.shouldHide,
		"secret values, raw JSON, manual worker settings, advanced logs",
	);
	const missing = listSentence(
		readiness.missing,
		"nothing critical from the current user view",
	);
	setSafeHtml(
		els.userReadinessGrid,
		[
			readout(
				"Can I use it?",
				readiness.primaryAction,
				readiness.primaryReason,
				tone,
			),
			readout(
				"Visible now",
				`${servers.length} server${servers.length === 1 ? "" : "s"}`,
				visible,
				"good",
			),
			readout("Hidden by default", "safe defaults", hidden, "warn"),
			readout(
				"Missing",
				readiness.missing.length
					? `${readiness.missing.length} gap${readiness.missing.length === 1 ? "" : "s"}`
					: "clean",
				missing,
				readiness.missing.length ? "warn" : "good",
			),
		].join(""),
	);
}

function renderFleetBoard(servers, instances) {
	const groups = groupByServer(instances);
	const buckets = [
		{ key: "all", label: "All", tone: "good", rows: [...servers] },
		{ key: "blocked", label: "Blocked", tone: "bad", rows: [] },
		{ key: "protected", label: "Guarded", tone: "warn", rows: [] },
		{ key: "ready", label: "Ready", tone: "good", rows: [] },
		{ key: "off", label: "Off", tone: "warn", rows: [] },
	];
	for (const server of servers) {
		const risk = riskForServer(server, groups.get(server.name) || []);
		const key = serverBucket(server, risk);
		buckets.find((bucket) => bucket.key === key)?.rows.push(server);
	}
	if (!els.serverFleetBoard) return;
	const metaForBucket = (bucket) => {
		if (bucket.key === "all") return "reset group filter";
		if (bucket.key === "blocked")
			return bucket.rows.length ? "fix these first" : "no blockers";
		if (bucket.key === "protected")
			return bucket.rows.length ? "on · conservative policy" : "none";
		if (bucket.key === "ready")
			return bucket.rows.length ? "evidence listed" : "none";
		if (bucket.key === "off")
			return bucket.rows.length ? "parked · enable when needed" : "none";
		return "";
	};
	setSafeHtml(
		els.serverFleetBoard,
		buckets
			.map((bucket) => {
				const meta = metaForBucket(bucket);
				const pressed = state.bucket === bucket.key;
				return `<button class="fleet-card ${bucket.tone}" type="button" data-server-bucket="${bucket.key}" aria-pressed="${pressed}"><strong>${bucket.rows.length} ${bucket.label}</strong><span>${escapeHtml(meta)}</span></button>`;
			})
			.join(""),
	);
	if (els.serverGuide) {
		const counts = Object.fromEntries(
			buckets.map((bucket) => [bucket.key, bucket.rows.length]),
		);
		const allPrefix = counts.blocked
			? `${counts.blocked} blocked server${counts.blocked === 1 ? "" : "s"} need setup before use.`
			: "No blocked servers right now.";
		const guidance = {
			all: [
				"Fleet brief",
				`${allPrefix} Read each row as: live evidence, current state, then buttons. Worker overrides stay in the focused source panel.`,
			],
			blocked: [
				"Blocked view",
				"Fix source/profile setup first; then run Test to collect tools/list evidence.",
			],
			protected: [
				"Guarded view",
				"These are on with conservative policy or incomplete evidence. Run Test if tools are not listed.",
			],
			ready: [
				"Ready view",
				"These have live or cached tools/list evidence. Re-test after source changes.",
			],
			off: [
				"Off view",
				"These are parked. Turn one on only when a workflow explicitly needs that source, then run Test.",
			],
		}[state.bucket] || ["Fleet brief", allPrefix];
		setSafeHtml(
			els.serverGuide,
			`<strong>${escapeHtml(guidance[0])}.</strong> ${escapeHtml(guidance[1])}`,
		);
	}
}

function fleetMetric(label, value, meta, tone = "warn") {
	return `<div class="server-metric ${escapeHtml(tone)}"><span>${escapeHtml(label)}</span><strong>${escapeHtml(value)}</strong><em>${escapeHtml(meta)}</em></div>`;
}

function renderServerCommandCenter(servers, rows, groups) {
	if (!els.serverCommandCenter) return;
	const all = Array.isArray(servers) ? servers : [];
	const visible = Array.isArray(rows) ? rows : [];
	const visibleModels = visible.map((server) =>
		serverViewModel(server, groups.get(server.name) || []),
	);
	const enabled = all.filter((server) => server.effectiveEnabled).length;
	const blocked = visibleModels.filter(
		(model) => model.risk.tone === "bad",
	).length;
	const guarded = visibleModels.filter(
		(model) => model.risk.tone === "warn" && model.risk.rank <= 3,
	).length;
	const ready = visibleModels.filter(
		(model) => model.risk.tone === "good",
	).length;
	const off = visibleModels.filter(
		(model) => !model.server?.effectiveEnabled,
	).length;
	const evidenceChecked = visible.filter(
		(server) => serverToolEvidence(server).checked,
	).length;
	const evidenceFailed = visible.filter((server) => {
		const evidence = serverToolEvidence(server);
		return evidence.checked && !evidence.ok;
	}).length;
	const policyFixes = visibleModels.filter(
		(model) => model.needsTuning && model.server?.effectiveEnabled,
	).length;
	const workerTotal = visibleModels.reduce(
		(sum, model) => sum + Math.max(1, num(model.workers, 1)),
		0,
	);
	const tone = blocked
		? "bad"
		: guarded || policyFixes || off
			? "warn"
			: visible.length
				? "good"
				: "warn";
	setCardTone(els.serverCommandCenter, tone);
	const title = !all.length
		? "Build server inventory before tuning policy."
		: !visible.length
			? "No rows in this lens."
			: blocked
				? `${plural(blocked, "server")} need setup before normal use.`
				: policyFixes
					? `${plural(policyFixes, "safe policy fix", "safe policy fixes")} ready to apply.`
					: guarded
						? `${plural(guarded, "guarded server")} should be tested before trust.`
						: "Fleet can stay in normal operating mode.";
	const body = !all.length
		? "Paste one command or URL, save it disabled, review it, run Test, then enable deliberately before normal use."
		: !visible.length
			? "Clear search or show all servers to restore the working set."
			: blocked
				? "Start with setup blockers, then collect live evidence. Avoid batch tuning until blocked rows are resolved."
				: policyFixes
					? "The visible rows include backend-backed recommendations. Apply safe policy fixes before increasing worker counts manually."
					: guarded
						? "Guarded rows are not wrong; they simply need lease, tool evidence, or serialized routing before they become low-risk."
						: "Routine work should happen from Sources and its task-specific row actions.";
	if (els.serverCommandTitle) els.serverCommandTitle.textContent = title;
	if (els.serverCommandBody) els.serverCommandBody.textContent = body;
	if (els.serverMetricRow) {
		setSafeHtml(
			els.serverMetricRow,
			[
				fleetMetric(
					"Visible",
					String(visible.length),
					`${enabled}/${all.length || 0} enabled total`,
					visible.length ? "good" : "warn",
				),
				fleetMetric(
					"Evidence",
					`${evidenceChecked}/${visible.length || 0}`,
					evidenceFailed ? `${evidenceFailed} failed` : "tools/list coverage",
					evidenceFailed
						? "bad"
						: evidenceChecked === visible.length && visible.length
							? "good"
							: "warn",
				),
				fleetMetric(
					"Policy",
					policyFixes ? String(policyFixes) : "clean",
					policyFixes
						? "backend fixes ready"
						: `${guarded} guarded · ${ready} ready`,
					policyFixes ? "warn" : blocked ? "bad" : "good",
				),
				fleetMetric(
					"Capacity",
					String(workerTotal),
					`${off} off in lens`,
					workerTotal ? "good" : "warn",
				),
			].join(""),
		);
	}
	if (els.serverWorkbench) {
		const focus =
			visibleModels.find((model) => model.risk.tone === "bad") ||
			visibleModels.find((model) => model.needsTuning) ||
			visibleModels.find((model) => model.risk.rank <= 3) ||
			visibleModels[0];
		setSafeHtml(
			els.serverWorkbench,
			focus
				? `<div class="workbench-summary ${escapeHtml(focus.verdict.tone)}">
                <span class="workbench-index">${escapeHtml(String(visible.findIndex((row) => row.name === focus.server?.name) + 1 || 1).padStart(2, "0"))}</span>
                <div><strong>${escapeHtml(focus.server?.name || "server")}</strong><p>${escapeHtml(focus.decision?.body || focus.nextStep || "Open the source panel for the next safe action.")}</p></div>
                <button type="button" data-server-name="${escapeHtml(focus.server?.name || "")}" data-server-action="settings">Open source</button>
              </div>`
				: `<div class="workbench-summary"><span class="workbench-index">00</span><div><strong>No current server lens.</strong><p>Clear filters or add a server to create an actionable row.</p></div></div>`,
		);
	}
}

function handleEmptyStateAction(control) {
	const action = control?.dataset?.emptyAction || "";
	if (action === "refresh") refreshDashboard({ force: true, reason: "empty" });
	else if (action === "check-link") checkBackendLink(control);
	else if (action === "clear-search") {
		state.query = "";
		if (els.serverSearch) els.serverSearch.value = "";
		render();
		els.serverSearch?.focus?.();
	} else if (action === "show-all") {
		state.enabledOnly = false;
		state.bucket = "all";
		writePref("enabledOnly", "false");
		writePref("bucket", "all");
		render();
	} else if (action === "import-config") {
		focusImportPath();
	} else if (action === "discover") {
		updateSetupToolsState("empty");
		revealElementById("server-discovery-panel", "center");
		window.setTimeout(() => els.serverDiscoverQuery?.focus?.(), 120);
	} else if (action === "add-server") {
		focusInstallCommand();
	}
}

function renderServers(servers, instances, groups) {
	const query = state.query.trim().toLowerCase();
	let rows = servers.filter((server) => {
		const risk = riskForServer(server, groups.get(server.name) || []);
		if (state.enabledOnly && !server.effectiveEnabled && risk.rank > 1)
			return false;
		if (state.bucket !== "all" && serverBucket(server, risk) !== state.bucket)
			return false;
		if (state.scope === "attention" && risk.rank > 3) return false;
		if (!query) return true;
		const evidence = serverToolEvidence(server);
		return [
			server.name,
			server.kind,
			server.runtimeType,
			server.stateClass,
			server.effectClass,
			server.scopeClass,
			server.concurrencyPolicy,
			server.routingGroup,
			server.transportPreference,
			server.launcherKind,
			server.startupStrategy,
			server.sourceType,
			server.sourceCommand,
			server.sourceUrl,
			server.sourcePath,
			...(server.sourceArgs || []),
			...(server.sourceEnvNames || []),
			...(server.sourceHeaderNames || []),
			evidence.status,
			...(evidence.toolNames || []),
		].some((value) =>
			String(value || "")
				.toLowerCase()
				.includes(query),
		);
	});
	const sourcePriority = (server, related) => {
		const evidence = serverToolEvidence(server);
		const risk = riskForServer(server, related);
		if (server.effectiveEnabled && evidence.checked && !evidence.ok) return 0;
		if (risk.rank === 1) return 1;
		if (server.effectiveEnabled && !evidence.checked) return 2;
		if (risk.rank <= 3) return 3;
		if (server.effectiveEnabled) return 4;
		return 5;
	};
	rows = rows.sort((left, right) => {
		const leftInstances = groups.get(left.name) || [];
		const rightInstances = groups.get(right.name) || [];
		if (state.sort === "name")
			return String(left.name || "").localeCompare(String(right.name || ""));
		if (state.sort === "instances")
			return (
				rightInstances.length - leftInstances.length ||
				String(left.name || "").localeCompare(String(right.name || ""))
			);
		return (
			sourcePriority(left, leftInstances) -
				sourcePriority(right, rightInstances) ||
			riskForServer(left, leftInstances).rank -
				riskForServer(right, rightInstances).rank ||
			String(left.name || "").localeCompare(String(right.name || ""))
		);
	});
	renderServerCommandCenter(servers, rows, groups);
	if (!rows.length) {
		const hasServers = servers.length > 0;
		const reason = state.query
			? `No source matches “${state.query}”.`
			: hasServers
				? "Current filters hide every source."
				: "No sources are configured yet.";
		setSafeHtml(
			els.serverList,
			`<div class="empty-state">
            <strong>${escapeHtml(reason)}</strong>
            <p>${hasServers ? "Restore the lens before changing policy." : "Start with an existing client config when possible, then preview discovery or add one source manually."}</p>
            <div class="empty-actions">
              ${state.query ? `<button class="primary" type="button" data-empty-action="clear-search">Clear search</button>` : ""}
              ${hasServers ? `<button type="button" data-empty-action="show-all">Show all sources</button>` : `<button class="primary" type="button" data-empty-action="import-config">Import config</button><button type="button" data-empty-action="discover">Discover</button><button class="quiet" type="button" data-empty-action="add-server">Add manually</button>`}
              <button class="quiet" type="button" data-empty-action="refresh">Refresh</button>
            </div>
          </div>`,
		);
		els.serverOverflowNote.textContent = "";
		return;
	}
	const visible = rows.slice(0, MAX_SERVER_ROWS);
	setSafeHtml(
		els.serverList,
		visible
			.map((server) => {
				const related = groups.get(server.name) || [];
				const model = serverViewModel(server, related);
				const transport = serverTransportKind(server);
				const evidence = serverToolEvidence(server);
				const evidenceTone = evidence.checked
					? evidence.ok
						? "good"
						: "bad"
					: "warn";
				const toolCount = evidence.toolCount || evidence.toolNames?.length || 0;
				const summary = !server.effectiveEnabled
					? {
							title: "Off",
							body: "Enable only when the workflow needs this source, then collect tool evidence.",
						}
					: !evidence.checked
						? {
								title: "Not tested",
								body: "Run Test to see which tools this source provides.",
							}
						: !evidence.ok
							? {
									title: "Test failed",
									body:
										evidence.error ||
										"Try Test again, or open the source panel to inspect the failure.",
								}
							: model.needsTuning
								? {
										title: `${toolCount} tool${toolCount === 1 ? "" : "s"} available`,
										body: "Tools are listed. A recommended route is available in the source panel.",
									}
								: {
										title: `${toolCount} tool${toolCount === 1 ? "" : "s"} available`,
										body: evidence.toolNames?.length
											? `Includes ${topToolNames(evidence.toolNames, 3)}.`
											: "Test completed successfully.",
									};
				const name = escapeHtml(server.name || "server");
				const routeTitle = `${routeLabel(model.routeMode)} · ${model.workers} worker${model.workers === 1 ? "" : "s"}`;
				const routeBody = model.needsTuning
					? `Recommended: ${model.recommendation.label}. Current in-flight limit ${model.inFlight}.`
					: `${model.category} · in-flight limit ${model.inFlight}.`;
				const toolPreview = evidence.toolNames?.length
					? `<div class="server-tool-preview">${evidence.toolNames
							.slice(0, 4)
							.map((tool) => `<span class="tag">${escapeHtml(tool)}</span>`)
							.join(
								"",
							)}${evidence.toolNames.length > 4 ? `<span class="tag">+${evidence.toolNames.length - 4}</span>` : ""}</div>`
					: "";
				return `
            <article class="server-row ${itemClass(evidence.checked && !evidence.ok ? "bad" : evidenceTone)}" data-server-name="${name}" data-server-bucket="${model.bucket}" data-enabled="${server.effectiveEnabled ? "true" : "false"}">
              <div class="server-row-layout">
                <div class="server-cell server-source-cell">
                  <div class="server-cell-label">Source</div>
                  <div class="server-title-row"><div class="name">${name}</div>${chip(server.effectiveEnabled ? "on" : "off", server.effectiveEnabled ? "good" : "warn")}</div>
                  <div class="server-cell-secondary">${escapeHtml(transport.label)} · ${escapeHtml(model.verdict.label)}</div>
                </div>
                <div class="server-cell server-evidence-cell">
                  <div class="server-cell-label">Evidence & tools</div>
                  <div class="server-cell-primary">${escapeHtml(summary.title)}</div>
                  <div class="server-cell-secondary">${escapeHtml(summary.body)}</div>
                  ${toolPreview}
                </div>
                <div class="server-cell server-routing-cell">
                  <div class="server-cell-label">Routing</div>
                  <div class="server-cell-primary">${escapeHtml(routeTitle)}</div>
                  <div class="server-cell-secondary">${escapeHtml(routeBody)}</div>
                </div>
                <div class="server-quick-controls" aria-label="Actions for ${name}">
                  ${serverControls(server, related, "row")}
                </div>
              </div>
            </article>
          `;
			})
			.join(""),
	);
	els.serverOverflowNote.textContent =
		rows.length > visible.length
			? `${rows.length - visible.length} more source(s) hidden by the compact list. Search or change filters to narrow it.`
			: "";
}

function detail(label, value) {
	return `<div class="detail-box"><div class="label">${escapeHtml(label)}</div><div class="detail-value">${escapeHtml(text(value))}</div></div>`;
}

function renderContext(overview, readiness, project, hub, cache, runtime) {
	const rows = [
		["Workspace", overview.rootPath || "—"],
		["Hub", hub.status || hub.health || "unknown"],
		["Profile", hub.activeProfile || readiness.activeProfile || "—"],
		[
			"Cache",
			cache.hit
				? `hit · ttl ${fmtMs(cache.ttlMs)}`
				: cache.bypassed
					? "bypass"
					: "fresh",
		],
		["Surface", runtime.surface || "dashboard-http"],
		[
			"Prereqs",
			`Rust ${project.rustSourceReady ? "ok" : "missing"} · npm ${project.npmSurfaceReady ? "ok" : "missing"} · Docker ${project.containerToolingReady ? "ok" : "missing"}`,
		],
	];
	setSafeHtml(
		els.contextList,
		rows
			.map(
				([name, value]) => `
          <article class="item"><div class="item-head"><div class="name">${escapeHtml(name)}</div></div><div class="meta">${escapeHtml(value)}</div></article>
        `,
			)
			.join(""),
	);
}

function renderInstances(instances, summary) {
	setChip(
		els.instanceChip,
		`${instances.length || summary.serverCount || 0} planned`,
		instances.length ? "good" : "warn",
	);
	if (!instances.length) {
		setSafeHtml(
			els.instanceList,
			`<div class="empty">No planned instances returned for this context.</div>`,
		);
		return;
	}
	setSafeHtml(
		els.instanceList,
		instances
			.slice(0, 10)
			.map((instance) => {
				const reusable = instance.mode === "shared" || instance.mode === "pool";
				const modeLabel =
					instance.mode === "pool"
						? "reused session"
						: instance.mode || "planned";
				const routeKey = instance.requestMutexKey
					? `route key ${instance.requestMutexKey}`
					: "no route key";
				return `
          <article class="${itemClass(reusable ? "good" : "warn")}">
            <div class="item-head"><div class="name">${escapeHtml(instance.server || instance.serverName || "server")}</div>${chip(modeLabel, reusable ? "good" : "warn")}</div>
            <div class="meta">${escapeHtml(instance.trace || instance.instanceId || "no trace")}</div>
            <div class="tags">${tags([`workers ${text(instance.maxWorkers, 1)}`, `in-flight ${text(instance.maxInFlightPerWorker, 1)}`, instance.schedulerLane, instance.requestStrategy, routeKey])}</div>
          </article>`;
			})
			.join("") +
			(instances.length > 10
				? `<div class="note">${instances.length - 10} more planned lane(s).</div>`
				: ""),
	);
}

function renderRuntime(runtime, hub, readiness, project) {
	const http = runtime.http || {};
	const pool = runtime.upstreamSessionPool || {};
	const control = state.overview?.runtimeControlPlane?.summary || {};
	const monitor = runtime.serverResourceMonitoring || {};
	const rows = [
		[
			"Runtime actions",
			hub.readyForRuntimeOps ? "ready" : "blocked",
			hub.readyForRuntimeOps ? "good" : "bad",
		],
		[
			"Read-only actions",
			hub.readyForReadOnlyOps ? "ready" : "blocked",
			hub.readyForReadOnlyOps ? "good" : "bad",
		],
		[
			"HTTP workers",
			`${num(http.activeConnections)}/${num(http.maxConnections) || "?"} active`,
			"warn",
		],
		[
			"Reusable sessions",
			`${num(pool.size)}/${num(pool.maxSize) || "?"} retained · internally synchronized`,
			num(pool.size) ? "good" : "warn",
		],
		[
			"Server resources",
			`${num(monitor.sessionCount, 0)} live session(s) · ${text(monitor.status, "waiting")}`,
			num(monitor.sessionCount, 0) ? "good" : "warn",
		],
		[
			"Runtime control",
			`${num(control.noLiveEvidence, 0)} need probe · ${num(control.approvalRequired, 0)} need approval · ${num(control.containerRequired, 0)} need container`,
			num(control.containerRequired, 0)
				? "bad"
				: num(control.noLiveEvidence, 0)
					? "warn"
					: "good",
		],
		[
			"Parallelism",
			`${runtime.availableParallelism ?? "?"} reported by OS`,
			num(runtime.availableParallelism) > 1 ? "good" : "warn",
		],
		[
			"Project",
			`Rust ${project.rustSourceReady ? "ok" : "missing"} · npm ${project.npmSurfaceReady ? "ok" : "missing"}`,
			project.rustSourceReady && project.npmSurfaceReady ? "good" : "warn",
		],
	];
	setChip(
		els.runtimeChip,
		readiness.runtimePrerequisitesReady ? "ready" : "blocked",
		readiness.runtimePrerequisitesReady ? "good" : "bad",
	);
	setSafeHtml(
		els.runtimeList,
		rows
			.map(
				([name, meta, tone]) =>
					`<article class="${itemClass(tone)}"><div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(tone === "good" ? "ok" : "watch", tone)}</div><div class="meta">${escapeHtml(meta)}</div></article>`,
			)
			.join(""),
	);
}

function renderPolicies(rows) {
	const bad = rows.filter((row) => row.risk.tone === "bad").length;
	setChip(
		els.policyChip,
		bad ? `${bad} need decisions` : "clean enough",
		bad ? "bad" : "good",
	);
	if (!rows.length) {
		setSafeHtml(
			els.policyList,
			`<div class="empty">No configured servers to route.</div>`,
		);
		return;
	}
	setSafeHtml(
		els.policyList,
		rows
			.slice(0, 10)
			.map(
				(row) => `
          <article class="${itemClass(row.risk.tone)}">
            <div class="item-head"><div class="name">${escapeHtml(row.server.name || "server")}</div>${chip(row.risk.label, row.risk.tone)}</div>
            <div class="meta">${escapeHtml(row.policy)} · group ${escapeHtml(text(row.server.routingGroup))}</div>
          </article>
        `,
			)
			.join(""),
	);
}

function renderCapacity(runtime, cache, active, max, pool, sessions) {
	const http = runtime.http || {};
	const caches = runtime.caches || {};
	const saturated = max && active >= max;
	setChip(
		els.capacityChip,
		saturated ? "saturated" : "within limits",
		saturated ? "bad" : "good",
	);
	const rows = [
		[
			"HTTP capacity",
			`${active}/${max || "?"} active · max observed ${num(http.maxObservedActiveConnections)} · timeout ${fmtMs(http.ioTimeoutMs)}`,
			saturated ? "bad" : "good",
			[
				`body ${fmtBytes(http.maxBodyBytes)}`,
				`${http.maxHeaderCount ?? "?"} headers`,
			],
		],
		[
			"Dashboard cache",
			`${cache.hit ? "hit" : cache.bypassed ? "bypass" : "fresh"} · age ${fmtMs(cache.ageMs)} · ttl ${fmtMs(cache.ttlMs)}`,
			cache.stale || cache.refreshError ? "warn" : "good",
			[
				`overview ${fmtMs(caches.overviewTtlMs)}`,
				`health ${fmtMs(caches.healthTtlMs)}`,
			],
		],
		[
			"HTTP sessions",
			`${num(sessions.size)}/${num(sessions.maxSize) || "?"} sessions · ${num(sessions.prunedExpiredSessions)} pruned`,
			num(sessions.size) >= num(sessions.maxSize, 1) ? "warn" : "good",
			[`ttl ${fmtMs(sessions.ttlMs)}`],
		],
		[
			"Reusable sessions",
			`${num(pool.size)}/${num(pool.maxSize) || "?"} retained · manager ready`,
			num(pool.size) >= num(pool.maxSize, 1) ? "warn" : "good",
			[`idle ${fmtMs(pool.idleTtlMs)}`],
		],
	];
	setSafeHtml(
		els.capacityList,
		rows
			.map(
				([name, meta, tone, tagList]) =>
					`<article class="${itemClass(tone)}"><div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(tone === "good" ? "ok" : "watch", tone)}</div><div class="meta">${escapeHtml(meta)}</div><div class="tags">${tags(tagList)}</div></article>`,
			)
			.join(""),
	);
}

function renderTelemetry(servers, instances, http, logs) {
	const rows = [
		[
			"Server inventory",
			servers.length
				? `${servers.length} servers with runtime policy fields`
				: "No server inventory returned",
			servers.length ? "good" : "warn",
			["runtimeType", "stateClass", "effectClass"],
		],
		[
			"Instance plan",
			instances.length
				? `${instances.length} lanes with worker and route-isolation hints`
				: "No server instances payload returned",
			instances.length ? "good" : "warn",
			["mode", "trace", "workers"],
		],
		[
			"Traffic / errors",
			Object.keys(http).length
				? `${num(http.acceptedConnections)} accepted · ${num(http.failedConnections)} failed`
				: "HTTP counters unavailable",
			Object.keys(http).length ? "good" : "warn",
			["accepted", "completed", "failed"],
		],
		[
			"Request duration",
			Object.keys(http).length
				? `avg ${fmtMs(http.requestDurationAverageMs)} · max ${fmtMs(http.requestDurationMaxMs)}`
				: "HTTP duration counters unavailable",
			Object.keys(http).length ? msTone(http.requestDurationAverageMs) : "warn",
			["avg", "max", "request duration"],
		],
		[
			"Per-server CPU/RAM",
			"Not collected yet: current payload has limits/sessions, not OS process usage per upstream worker.",
			"warn",
			["missing", "process telemetry"],
		],
		[
			"Logs and safe audit",
			Array.isArray(logs)
				? `${logs.length} recent log entries loaded`
				: "Logs endpoint missing",
			Array.isArray(logs) ? "good" : "warn",
			["lifecycle", "safe fingerprints"],
		],
	];
	setSafeHtml(
		els.telemetryList,
		rows
			.map(
				([name, meta, tone, tagList]) =>
					`<article class="${itemClass(tone)}"><div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(tone === "good" ? "available" : "gap", tone)}</div><div class="meta">${escapeHtml(meta)}</div><div class="tags">${tags(tagList)}</div></article>`,
			)
			.join(""),
	);
}

function renderActivity(leases, http, pool, sessions) {
	setChip(
		els.activityChip,
		leases.length ? `${leases.length} route leases` : "no route leases",
		leases.length ? "warn" : "good",
	);
	const rows = [
		[
			"HTTP workers",
			`${num(http.activeConnections)}/${num(http.maxConnections) || "?"} active · ${num(http.failedConnections)} failed`,
			num(http.failedConnections) ? "warn" : "good",
		],
		[
			"HTTP sessions",
			`${num(sessions.size)}/${num(sessions.maxSize) || "?"} sessions · ttl ${fmtMs(sessions.ttlMs)}`,
			"good",
		],
		[
			"Reusable sessions",
			`${num(pool.size)}/${num(pool.maxSize) || "?"} retained · idle ${fmtMs(pool.idleTtlMs)}`,
			num(pool.size) ? "good" : "warn",
		],
		...(leases.length
			? leases
					.slice(0, 6)
					.map((lease) => [
						lease.server || lease.serverName || lease.id || "route lease",
						`${lease.status || "active"} · ${lease.requestMutexKey || lease.mutexKey || lease.conflictDomain || "held by scheduler"}`,
						"warn",
					])
			: [["Route leases", "No active route leases returned by hub.", "good"]]),
	];
	setSafeHtml(
		els.activityList,
		rows
			.map(
				([name, meta, tone]) =>
					`<article class="${itemClass(tone)}"><div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(tone === "good" ? "ok" : "active", tone)}</div><div class="meta">${escapeHtml(meta)}</div></article>`,
			)
			.join(""),
	);
}

function renderClients(clients, catalog) {
	setChip(
		els.clientChip,
		`${clients.length} clients`,
		clients.length ? "good" : "warn",
	);
	if (!clients.length) {
		setSafeHtml(
			els.clientList,
			`<div class="empty">No client surfaces returned.</div>`,
		);
		return;
	}
	setSafeHtml(
		els.clientList,
		clients
			.slice(0, 8)
			.map(
				(client) =>
					`<article class="item"><div class="item-head"><div class="name">${escapeHtml(client.displayName || client.id || "client")}</div>${chip(client.surfaceClass || "surface", client.surfaceClass === "local" ? "good" : "warn")}</div><div class="meta">${escapeHtml(client.id || "client")} · ${escapeHtml(client.surfaceKind || "surface")} · ingress ${escapeHtml((client.supportedIngresses || []).join(", ") || "—")}</div></article>`,
			)
			.join(""),
	);
}

function renderLogs(logs) {
	const audits = Array.isArray(logs)
		? logs
				.filter(
					(entry) =>
						entry.event === "tool_call_audit" ||
						entry.event === "tool_batch_audit",
				)
				.slice(-6)
				.reverse()
		: [];
	setChip(els.logChip, `${logs.length} logs`, logs.length ? "good" : "warn");
	setSafeHtml(
		els.auditList,
		audits.length
			? audits
					.map(
						(entry) =>
							`<article class="item"><div class="item-head"><div class="name">${escapeHtml(entry.server || "server")} · ${escapeHtml(entry.tool || entry.event || "tool")}</div>${chip(entry.bridgeOk && entry.upstreamOk ? "ok" : "watch", entry.bridgeOk && entry.upstreamOk ? "good" : "warn")}</div><div class="meta">${escapeHtml(entry.trace || "no trace")} · ${fmtDate(entry.tsMs)}</div></article>`,
					)
					.join("")
			: `<div class="empty">No tool-call audit entries yet.</div>`,
	);
	setSafeHtml(
		els.logList,
		Array.isArray(logs) && logs.length
			? logs
					.slice(-8)
					.reverse()
					.map(
						(entry) =>
							`<article class="item"><div class="item-head"><div class="name">${escapeHtml(entry.event || "event")}</div>${chip(entry.level || "info", entry.level === "error" ? "bad" : entry.level === "warn" ? "warn" : "good")}</div><div class="meta">${fmtDate(entry.tsMs)}</div><details class="server-settings"><summary>Raw payload</summary><pre>${escapeHtml(JSON.stringify(entry, null, 2))}</pre></details></article>`,
					)
					.join("")
			: `<div class="empty">No recent log entries.</div>`,
	);
}
