// MCPace dashboard server model/policy helpers. Loaded after /dashboard.js and before render/action chunks.
function profileSourceMismatch(server) {
	return Boolean(
		(server?.profileEnabled || server?.defaultEnabled || server?.required) &&
			server?.sourceEnabled === false,
	);
}

function riskForServer(server, instances = []) {
	const requiredDisabled = Boolean(
		server?.required && !server?.effectiveEnabled,
	);
	if (requiredDisabled || profileSourceMismatch(server))
		return { tone: "bad", rank: 1, label: "needs setup" };
	if (!server?.effectiveEnabled) return { tone: "warn", rank: 4, label: "off" };
	const effect = String(server.effectClass || "").toLowerCase();
	const stateClass = String(server.stateClass || "").toLowerCase();
	const credential = String(server.credentialBinding || "").toLowerCase();
	const scope = String(server.scopeClass || "").toLowerCase();
	const routing = String(server.routingGroup || "").toLowerCase();
	const concurrency = String(server.concurrencyPolicy || "").toLowerCase();
	const runtime = String(server.runtimeType || "").toLowerCase();
	const lock = String(
		server.hostLock || server.hostLockKey || "none",
	).toLowerCase();
	const locks = Array.isArray(server.lockDomains)
		? server.lockDomains.filter(Boolean)
		: [];
	const hasLiveTools = serverToolEvidence(server).checked;
	const unknown =
		!scope ||
		scope === "configured-source" ||
		runtime === "unknown" ||
		stateClass === "unknown-conservative" ||
		effect === "external-unknown";
	const exclusive =
		["single-session", "single-writer", "isolated-per-project"].includes(
			concurrency,
		) ||
		lock !== "none" ||
		locks.length > 0 ||
		instances.some((instance) => instance.mode === "serialized");
	const sensitive =
		/write|mutation|external|host|remote|credential|stateful|session/.test(
			`${effect} ${stateClass} ${credential} ${scope} ${routing}`,
		);
	if (sensitive || exclusive || server.discoveryRequiresLease)
		return {
			tone: "warn",
			rank: hasLiveTools ? 3 : 2,
			label: hasLiveTools ? "guarded" : "unchecked",
		};
	if (unknown) return { tone: "warn", rank: 2, label: "unchecked" };
	return { tone: "good", rank: 5, label: "ready" };
}

function compactField(value, fallback = "unknown") {
	const raw = String(value || "").trim();
	return raw || fallback;
}

function humanizeKey(value) {
	const words = String(value || "")
		.replace(/([a-z0-9])([A-Z])/g, "$1 $2")
		.replace(/[_./:-]+/g, " ")
		.replace(/\s+/g, " ")
		.trim();
	return words || "tools";
}

function normalizeToolNames(value) {
	const items = Array.isArray(value) ? value : [];
	return items
		.map((item) => {
			if (typeof item === "string") return item;
			return item?.name || item?.title || item?.qualifiedName || "";
		})
		.map((item) => String(item || "").trim())
		.filter(Boolean);
}

function normalizeTools(value) {
	const items = Array.isArray(value) ? value : [];
	return items
		.map((item) => (typeof item === "string" ? { name: item } : item))
		.filter((item) => item && (item.name || item.title || item.qualifiedName));
}

function topToolNames(names, limit = 4) {
	const unique = [...new Set(normalizeToolNames(names))];
	const visible = unique.slice(0, limit);
	const suffix =
		unique.length > limit ? `, +${unique.length - limit} more` : "";
	return `${visible.join(", ")}${suffix}`;
}

function commonToolNamespace(names) {
	const prefixes = normalizeToolNames(names)
		.map((name) => String(name).split(/[_.:-]/)[0])
		.map((prefix) => prefix.trim())
		.filter((prefix) => prefix.length > 1);
	if (!prefixes.length) return "";
	const counts = prefixes.reduce(
		(map, prefix) => map.set(prefix, (map.get(prefix) || 0) + 1),
		new Map(),
	);
	const [prefix, count] = [...counts.entries()].sort(
		(a, b) => b[1] - a[1] || a[0].localeCompare(b[0]),
	)[0] || ["", 0];
	return count >= 2 ? prefix : "";
}

function resultPayload(value) {
	if (value?.action && value?.result) return value.result;
	return value?.result || value || {};
}

function probeResultForServer(name, value) {
	const payload = resultPayload(value);
	const rows = Array.isArray(payload.results) ? payload.results : [];
	if (!rows.length) return payload?.name ? payload : null;
	return (
		rows.find((row) => String(row.name || "") === String(name || "")) || rows[0]
	);
}

function normalizeProbeEvidence(name, value) {
	const payload = resultPayload(value);
	const row = probeResultForServer(name, value) || {};
	const tools = normalizeTools(row.tools || payload.tools || []);
	const toolNames = normalizeToolNames(
		row.toolNames || row.tools || payload.toolNames || payload.tools || [],
	);
	const toolCount = num(
		row.toolCount ??
			row.returnedToolCount ??
			payload.toolCount ??
			payload.returnedToolCount,
		toolNames.length || tools.length || 0,
	);
	const ok = row.ok === true || (payload.ok === true && row.ok !== false);
	const error = row.error || payload.error || "";
	return {
		checked: true,
		checkedAtMs: Date.now(),
		ok,
		status: row.status || payload.status || (ok ? "listed-tools" : "failed"),
		toolCount,
		toolNames: toolNames.length ? toolNames : normalizeToolNames(tools),
		tools,
		error: String(error || ""),
		elapsedMs: row.elapsedMs ?? payload.elapsedMs,
		sourceType: row.sourceType || payload.sourceType || "",
		runtimeCallable: row.runtimeCallable ?? payload.runtimeCallable,
	};
}

function serverToolEvidence(server) {
	const name = String(server?.name || "");
	const cached = state.serverTests?.[name];
	if (cached) return cached;
	const tools = normalizeTools(server?.tools || server?.topTools || []);
	const toolNames = normalizeToolNames(
		server?.toolNames || server?.tools || server?.topTools || [],
	);
	const directCount = num(server?.toolCount ?? server?.returnedToolCount, 0);
	if (directCount || toolNames.length || tools.length) {
		return {
			checked: true,
			ok: server?.toolsOk !== false,
			status: server?.toolsStatus || "listed-tools",
			toolCount: directCount || toolNames.length || tools.length,
			toolNames: toolNames.length ? toolNames : normalizeToolNames(tools),
			tools,
			error: String(server?.toolsError || ""),
			checkedAtMs: server?.toolsListedAtMs || server?.toolsCheckedAtMs || 0,
		};
	}
	if (server?.sourceEnabled === false) {
		return {
			checked: false,
			ok: false,
			status: "source-disabled",
			toolCount: 0,
			toolNames: [],
			tools: [],
			error: "source disabled",
			checkedAtMs: 0,
		};
	}
	return {
		checked: false,
		ok: false,
		status: "not-checked",
		toolCount: 0,
		toolNames: [],
		tools: [],
		error: "",
		checkedAtMs: 0,
	};
}

function serverCategory(server) {
	const source = String(
		server.sourceType || server.transportPreference || "",
	).toLowerCase();
	if (/http|url|streamable/.test(source)) return "HTTP source";
	if (/stdio|command|process|npm|pypi|oci/.test(source) || server.sourceCommand)
		return "Local command";
	return "Configured source";
}

function serverEvidenceSummary(server) {
	const evidence = serverToolEvidence(server);
	if (evidence.checked && evidence.ok && evidence.toolCount > 0) {
		const namespace = commonToolNamespace(evidence.toolNames);
		const title = namespace
			? `${humanizeKey(namespace)} tools available`
			: `${evidence.toolCount} tool${evidence.toolCount === 1 ? "" : "s"} available`;
		const body = evidence.toolNames.length
			? `Live tools/list: ${topToolNames(evidence.toolNames)}.`
			: `Live tools/list reported ${evidence.toolCount} tool${evidence.toolCount === 1 ? "" : "s"}.`;
		return { title, body, evidence };
	}
	if (evidence.checked && !evidence.ok) {
		return {
			title: "Test failed",
			body: evidence.error
				? `Last Test could not list tools: ${evidence.error}`
				: "Last Test could not list tools.",
			evidence,
		};
	}
	if (profileSourceMismatch(server)) {
		return {
			title: "Selected but source off",
			body: "Profile selects this server, but the MCP settings source has it disabled. No tools/list evidence is available while it is off.",
			evidence,
		};
	}
	if (!server.effectiveEnabled) {
		return {
			title: "No live tools evidence",
			body: "This server is not effectively enabled, so MCPace has not listed its tools in this dashboard view.",
			evidence,
		};
	}
	return {
		title: "Not tested",
		body: "Run Test to list this source's tools before relying on it.",
		evidence,
	};
}

function serverVerdict(server, risk, related = []) {
	const recommendation = recommendedPolicy(server, related);
	const evidence = serverToolEvidence(server);
	if (risk.rank === 1)
		return {
			tone: "bad",
			label: "Needs setup",
			summary: "Selected or required, but not usable.",
		};
	if (!server.effectiveEnabled)
		return {
			tone: "warn",
			label: "Off",
			summary: "Not active. No live tools listed.",
		};
	if (evidence.checked && !evidence.ok)
		return {
			tone: "bad",
			label: "Test failed",
			summary: "Latest tools/list check failed.",
		};
	if (policyNeedsTuning(server, related, recommendation))
		return {
			tone: "warn",
			label: "Review setting",
			summary: "A recommended setting is available.",
		};
	if (!evidence.checked)
		return {
			tone: "warn",
			label: "Needs Test",
			summary: "Run Test to list tools.",
		};
	if (risk.rank <= 3)
		return {
			tone: "warn",
			label: "Guarded",
			summary: "Usable with conservative policy.",
		};
	return { tone: "good", label: "Ready", summary: "Live tools are listed." };
}

function serverBucket(server, risk) {
	if (risk.rank <= 1) return "blocked";
	if (!server.effectiveEnabled) return "off";
	if (risk.rank <= 3) return "protected";
	return "ready";
}

function serverViewModel(server, related = []) {
	const risk = riskForServer(server, related);
	const recommendation = recommendedPolicy(server, related);
	return {
		server,
		related,
		name: server.name || "server",
		risk,
		evidence: serverToolEvidence(server),
		evidenceSummary: serverEvidenceSummary(server),
		verdict: serverVerdict(server, risk, related),
		bucket: serverBucket(server, risk),
		decision: serverDecision(server, related),
		settings: serverSettingProfile(server, related),
		nextStep: serverNextStep(server, risk, related),
		category: serverCategory(server),
		routeMode: serverMode(server, related),
		workers: maxWorkers(server, related),
		inFlight: maxInFlight(server, related),
		recommendation,
		needsTuning: policyNeedsTuning(server, related, recommendation),
		operatorPlan: operatorPlanForServer(server.name),
	};
}

function serverDecision(server, related = []) {
	const recommendation = recommendedPolicy(server, related);
	const guidance = serverEvidenceSummary(server);
	const evidence = guidance.evidence || serverToolEvidence(server);
	if (profileSourceMismatch(server)) {
		return {
			title: "Fix source enablement",
			body: "The active profile selects this server, but its source is disabled. Turn it on or remove it from the profile.",
		};
	}
	if (!server.effectiveEnabled) {
		return {
			title: "Off; no live tools listed",
			body: "Turn on only if the current workflow needs this MCP source, then run Test to list tools.",
		};
	}
	if (evidence.checked && !evidence.ok) {
		return {
			title: "Retry tools/list",
			body:
				evidence.error ||
				"The last check failed; run Test again to collect fresh evidence.",
		};
	}
	if (!evidence.checked) {
		return {
			title: "Run test",
			body: `No live tool evidence yet: ${firstSentence(guidance.body)}`,
		};
	}
	if (policyNeedsTuning(server, related, recommendation)) {
		return {
			title: "Apply recommended policy",
			body: "MCPace has a lower-resource setting ready. Tool evidence stays unchanged.",
		};
	}
	return {
		title: "Evidence available",
		body: `${guidance.title}: ${firstSentence(guidance.body)}`,
	};
}

function serverSettingProfile(server, related = []) {
	const recommendation = recommendedPolicy(server, related);
	const guidance = serverEvidenceSummary(server);
	const current = `${routeLabel(serverMode(server, related))} / ${maxWorkers(server, related)}×${maxInFlight(server, related)}`;
	const recommended = `${recommendation.label}`;
	if (!server.effectiveEnabled) {
		const stateTitle = profileSourceMismatch(server)
			? "Profile/source mismatch"
			: "Off";
		const stateBody = profileSourceMismatch(server)
			? "The profile selects this server, but the settings source is disabled. No live tools can be listed until it is enabled."
			: "The server is not active, so the dashboard will not invent capabilities.";
		return {
			stateTitle,
			stateBody,
			routeTitle: recommended,
			routeBody: `${recommendation.reason} Current is ${current}.`,
			useTitle: guidance.title,
			useBody: guidance.body,
			current,
		};
	}
	if (policyNeedsTuning(server, related, recommendation)) {
		return {
			stateTitle: "On, policy differs",
			stateBody:
				"The server is usable, but the current policy differs from MCPace's inferred low-resource recommendation.",
			routeTitle: recommended,
			routeBody: `Current is ${current}. Apply only if you want the recommended policy.`,
			useTitle: guidance.title,
			useBody: guidance.body,
			current,
		};
	}
	return {
		stateTitle: "On",
		stateBody:
			"The visible capability text is based only on live/cached evidence or source state.",
		routeTitle: recommended,
		routeBody: `${recommendation.reason} Current is ${current}.`,
		useTitle: guidance.title,
		useBody: guidance.body,
		current,
	};
}

function settingCard(label, title, body) {
	return `<section class="server-setting-card"><div class="label">${escapeHtml(label)}</div><strong>${escapeHtml(title)}</strong><p>${escapeHtml(body)}</p></section>`;
}

function firstSentence(textValue) {
	const value = String(textValue || "").trim();
	const match = value.match(/^.*?[.!?](?:\s|$)/);
	return (match ? match[0] : value).trim();
}

function profileEvidence(server) {
	return Array.isArray(server.profileEvidence) && server.profileEvidence.length
		? server.profileEvidence[0]
		: null;
}

function evidenceLine(server) {
	const evidence = profileEvidence(server);
	if (!evidence)
		return "No profile evidence was reported yet. Recommended policy keeps this route conservative.";
	const confidence =
		typeof evidence.confidence === "number"
			? `${Math.round(evidence.confidence * 100)}%`
			: text(evidence.evidenceLevel, "unknown");
	const confidenceValue =
		typeof evidence.confidence === "number" ? evidence.confidence : 0;
	if (
		confidenceValue < 0.7 ||
		/low|weak|unknown/i.test(String(evidence.evidenceLevel || ""))
	) {
		return `${text(evidence.evidenceLevel, "evidence")} evidence · ${confidence} readiness. Recommended policy keeps this route conservative.`;
	}
	return `${text(evidence.evidenceLevel, "evidence")} evidence · ${confidence} readiness. ${text(evidence.summary, "Recommended policy keeps this route conservative.")}`;
}

function serverNextStep(server, risk, instances = []) {
	if (risk.rank === 1)
		return "Fix the source/profile mismatch, then run Test to collect tools/list evidence.";
	if (risk.rank === 2)
		return "Run Test to collect initialize + tools/list evidence before assuming capabilities.";
	if (risk.rank === 3)
		return instances.length
			? "Evidence exists, but routing remains conservative because runtime state, credentials, or locks are present."
			: "Evidence exists, but MCPace still keeps conservative routing from inferred policy fields.";
	if (!server.effectiveEnabled)
		return "Disabled. Run Test when ready, then turn on only when a workflow asks for this source.";
	return "Ready from current evidence. Re-test if the source config changes.";
}

function routingPlain(server, risk, instances = []) {
	if (risk.rank === 2)
		return "MCPace has no live tools/list evidence in the dashboard yet, so it keeps conservative routing.";
	if (risk.rank === 3)
		return "Requests are serialized or isolated because this server has state/credentials/locks that should not be shared freely.";
	if (server.runtimeType === "stateless")
		return "The server looks stateless, so MCPace can share it more safely across requests.";
	if (instances.some((instance) => instance.mode === "pool"))
		return "MCPace plans a pool, so multiple workers can serve traffic while respecting the configured limits.";
	return "Routing follows the server policy fields and any planned instance/mutex hints returned by the runtime.";
}

function serverChecklist(server, risk) {
	const checks = [];
	if (risk.rank <= 2) {
		checks.push(
			"No capability is assumed without source state or tools/list evidence.",
		);
		checks.push("Use Test to collect initialize + tools/list evidence.");
	} else if (risk.rank === 3) {
		checks.push(
			"This server is intentionally one-at-a-time because it has state, credentials, or locks.",
		);
		checks.push(
			"Manual worker changes stay in the Routing task because they are explicit overrides.",
		);
	} else if (!server.effectiveEnabled) {
		checks.push("No live tools are listed while the server is off.");
		checks.push("After enabling, run Test before relying on capabilities.");
	} else {
		checks.push("Visible capability text is evidence-backed.");
		checks.push("Re-test after changing source configuration.");
	}
	return checks;
}

function serverMetric(label, value) {
	return `<div class="server-metric"><div class="label">${escapeHtml(label)}</div><div class="value">${escapeHtml(text(value))}</div></div>`;
}

function currentServers() {
	return normalizeServers(state.overview?.servers);
}

function currentInstances() {
	return normalizeInstances(state.overview?.instances);
}

function findServer(name) {
	return currentServers().find(
		(server) => String(server.name || "") === String(name || ""),
	);
}

function relatedInstances(name) {
	return currentInstances().filter(
		(instance) =>
			String(instance.server || instance.serverName || "") ===
			String(name || ""),
	);
}

function serverMode(server, instances = []) {
	const instanceMode = instances.find((instance) => instance.mode)?.mode;
	const routingGroup = String(server.routingGroup || "");
	const concurrency = String(server.concurrencyPolicy || "");
	const startup = String(server.startupStrategy || "");
	if (startup === "disabled" || routingGroup === "disabled") return "disabled";
	if (instanceMode === "pool" || /pool/.test(routingGroup)) return "pool";
	if (
		instanceMode === "shared" ||
		/shared|parallel/.test(routingGroup) ||
		concurrency === "multi-reader"
	)
		return "shared";
	if (/project/.test(routingGroup) || concurrency === "isolated-per-project")
		return "project-isolated";
	if (/session/.test(routingGroup) || concurrency === "single-session")
		return "session-isolated";
	return "serialized";
}

function modeOptions(selected) {
	return ROUTING_MODES.map(
		([value, label]) =>
			`<option value="${value}"${value === selected ? " selected" : ""}>${escapeHtml(label)}</option>`,
	).join("");
}

function routeLabel(mode) {
	return (
		ROUTING_MODES.find(([value]) => value === mode)?.[1] || mode || "Safe queue"
	);
}

function domId(value) {
	return (
		String(value || "server")
			.replace(/[^a-zA-Z0-9_-]+/g, "-")
			.slice(0, 80) || "server"
	);
}

function profileConfidence(server) {
	const evidence = profileEvidence(server);
	return typeof evidence?.confidence === "number" ? evidence.confidence : 0;
}

function recommendedPolicy(server, related = []) {
	const risk = riskForServer(server, related);
	const confidence = profileConfidence(server);
	const effect = String(server.effectClass || "");
	const stateClass = String(server.stateClass || "");
	const credential = String(server.credentialBinding || "");
	const currentWorkers = maxWorkers(server, related);
	const isStateless =
		server.runtimeType === "stateless" ||
		/stateless|read-only/.test(`${stateClass} ${effect}`);
	const canShare =
		risk.rank >= 5 &&
		confidence >= 0.7 &&
		isStateless &&
		!/credential|stateful|external-unknown|write|host/.test(
			`${credential} ${stateClass} ${effect}`,
		);
	if (canShare) {
		return {
			mode: "shared",
			maxWorkers: 1,
			maxInFlightPerWorker: 1,
			label: "Shared / 1 worker",
			reason:
				"High-readiness stateless server; one shared worker is the lowest-resource safe default.",
		};
	}
	if (
		serverMode(server, related) === "pool" &&
		currentWorkers > 1 &&
		risk.rank <= 3
	) {
		return {
			mode: "serialized",
			maxWorkers: 1,
			maxInFlightPerWorker: 1,
			label: "Safe queue / 1 worker",
			reason:
				"Recommended policy reduces resource use for this stateful or low-evidence server.",
		};
	}
	return {
		mode: "serialized",
		maxWorkers: 1,
		maxInFlightPerWorker: 1,
		label: "Safe queue / 1 worker",
		reason:
			risk.rank <= 2
				? "Recommended policy keeps this server low-resource and one-at-a-time."
				: "Conservative one-worker routing avoids extra idle upstream processes.",
	};
}

function policyNeedsTuning(
	server,
	related = [],
	recommendation = recommendedPolicy(server, related),
) {
	if (!server?.effectiveEnabled) return false;
	return (
		serverMode(server, related) !== recommendation.mode ||
		maxWorkers(server, related) !== recommendation.maxWorkers ||
		maxInFlight(server, related) !== recommendation.maxInFlightPerWorker
	);
}

function autoPolicyPlan(
	servers = currentServers(),
	instances = currentInstances(),
) {
	const groups = groupByServer(instances);
	const plan = {
		enabled: 0,
		disabled: 0,
		already: 0,
		protected: 0,
		ready: 0,
		changes: [],
	};
	for (const server of servers) {
		if (!server.effectiveEnabled) {
			plan.disabled += 1;
			continue;
		}
		plan.enabled += 1;
		const related = groups.get(server.name) || [];
		const risk = riskForServer(server, related);
		const recommendation = recommendedPolicy(server, related);
		if (risk.rank >= 5) plan.ready += 1;
		if (risk.rank >= 2 && risk.rank <= 3) plan.protected += 1;
		if (policyNeedsTuning(server, related, recommendation)) {
			plan.changes.push({
				...actionPayloadForPolicy(server, related, recommendation),
				label: recommendation.label,
				reason: recommendation.reason,
			});
		} else {
			plan.already += 1;
		}
	}
	return plan;
}
