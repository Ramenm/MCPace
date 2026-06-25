// MCPace dashboard frontend. Backend owns product state; this file renders, validates, and dispatches explicit actions.
const PREF = "mcpace.dashboard.slim.";
      const REFRESH_MS = { "15": 15000, "30": 30000, "60": 60000, paused: 0 };
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
        ["project-isolated", "Per project"]
      ];

      const state = {
        overview: null,
        logs: [],
        query: "",
        enabledOnly: readPref("enabledOnly", "true", ["true", "false"]) === "true",
        refreshMode: readPref("refreshMode", "30", Object.keys(REFRESH_MS)),
        sort: readPref("sort", "risk", ["risk", "name", "instances"]),
        scope: readPref("scope", "normal", ["normal", "attention"]),
        bucket: readPref("bucket", "all", ["all", "blocked", "protected", "ready", "off"]),
        density: readPref("density", "comfortable", ["comfortable", "compact"]),
        selectedServer: null,
        backend: {
          overview: null,
          logs: null,
          resources: null,
          action: null,
          checkedAt: 0
        },
        timer: null,
        seq: 0,
        controller: null,
        refreshing: false,
        failureCount: 0,
        lastRefreshStartedAt: 0,
        lastRefreshFinishedAt: 0,
        lastSuccessAt: 0,
        lifecycle: { frozen: false, freezeCount: 0, resumeCount: 0, lastResumeAt: 0, wasDiscarded: Boolean(document.wasDiscarded) },
        serverTests: {},
        discovery: {
          loading: false,
          result: null,
          error: null,
          lastMode: "preview"
        },
        importer: {
          loading: false,
          result: null,
          error: null,
          last: null
        },
        clientSetup: {
          loading: false,
          result: null,
          error: null,
          last: null
        },
        lastError: null
      };

      const $ = id => document.getElementById(id);
      const els = {
        shell: document.querySelector(".shell"),
        refreshButton: $("refresh-button"),
        startButton: $("hub-up-button"),
        stopButton: $("hub-down-button"),
        repairButton: $("repair-button"),
        systemState: $("system-state"),
        systemNote: $("system-note"),
        focusAddServer: $("focus-add-server"),
        baseSetup: $("base-setup"),
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
        mobileActionDock: $("mobile-action-dock"),
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
        logList: $("log-list")
      };

      function readPref(key, fallback, allowed) {
        try {
          const value = window.localStorage?.getItem(PREF + key);
          if (value && (!allowed || allowed.includes(value))) return value;
        } catch (_) {}
        return fallback;
      }

      function writePref(key, value) {
        try { window.localStorage?.setItem(PREF + key, value); } catch (_) {}
      }

      function escapeHtml(value) {
        return String(value ?? "")
          .replaceAll("&", "&amp;")
          .replaceAll("<", "&lt;")
          .replaceAll(">", "&gt;")
          .replaceAll('"', "&quot;");
      }

      function num(value, fallback = 0) {
        const parsed = Number(value);
        return Number.isFinite(parsed) ? parsed : fallback;
      }

      function text(value, fallback = "—") {
        return value === null || value === undefined || value === "" ? fallback : String(value);
      }

      function listValues(value) {
        return Array.isArray(value) ? value.map(item => String(item || "")).filter(Boolean) : [];
      }
      function revealElementById(id, block = "nearest") {
        const node = document.getElementById(id);
        if (!node) return false;
        for (let parent = node.parentElement; parent; parent = parent.parentElement) {
          if (parent.tagName === "DETAILS") parent.open = true;
        }
        try { node.scrollIntoView({ behavior: "smooth", block }); }
        catch (_) { node.scrollIntoView(); }
        return true;
      }

      function updateSetupToolsState(reason = "") {
        if (!els.setupTools) return;
        const needsSetup = ["empty", "import", "client", "discover", "add"].includes(reason);
        if (needsSetup) els.setupTools.open = true;
      }


      function shellWord(value) {
        const raw = String(value || "").trim();
        if (!raw) return "''";
        if (/^[A-Za-z0-9_./:@=,+-]+$/.test(raw)) return raw;
        return `'${raw.replaceAll("'", "'\\''")}'`;
      }

      function launchCommand(server) {
        const command = String(server?.sourceCommand || "").trim();
        if (command) return [command, ...listValues(server?.sourceArgs)].map(shellWord).join(" ");
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
        els.serverInstallNote.innerHTML = `${dot(tone)}${escapeHtml(message)}`;
      }

      function setImportNote(message, tone = "warn") {
        if (!els.serverImportNote) return;
        els.serverImportNote.innerHTML = `${dot(tone)}${escapeHtml(message)}`;
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
        try { return new Date(Number(ms)).toLocaleString(); } catch (_) { return String(ms); }
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
        while (size >= 1024 && unit < units.length - 1) { size /= 1024; unit += 1; }
        return `${size.toFixed(size >= 10 || unit === 0 ? 0 : 1)} ${units[unit]}`;
      }

      function dot(tone) { return `<span class="dot ${escapeHtml(tone || "warn")}"></span>`; }
      function chip(label, tone = "warn") { return `<span class="chip">${dot(tone)}${escapeHtml(label)}</span>`; }
      function tags(items) { return items.filter(Boolean).map(item => `<span class="tag">${escapeHtml(item)}</span>`).join(""); }
      function itemClass(tone) { return tone === "bad" ? "item bad" : tone === "warn" ? "item warn" : tone === "good" ? "item good" : "item"; }
      function setChip(element, label, tone) { if (element) element.innerHTML = `${dot(tone)}${escapeHtml(label)}`; }
      function setSurfaceTone(element, tone) {
        const surface = element?.closest?.(".signal, .ops-card, .panel, .mini-panel, .connection-map, .setup-queue");
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
        const fromInstances = instances.reduce((max, instance) => Math.max(max, num(instance.maxWorkers)), 0);
        return num(server.maxWorkers, 0) || num(server.parallelismLimit, 0) || fromInstances || 1;
      }

      function maxInFlight(server, instances) {
        if (serverMode(server, instances) === "disabled") return 0;
        const fromInstances = instances.reduce((max, instance) => Math.max(max, num(instance.maxInFlightPerWorker)), 0);
        return num(server.maxInFlightPerWorker, 0) || fromInstances || 1;
      }

      function profileSourceMismatch(server) {
        return Boolean((server?.profileEnabled || server?.defaultEnabled || server?.required) && server?.sourceEnabled === false);
      }

      function riskForServer(server, instances = []) {
        const requiredDisabled = Boolean(server?.required && !server?.effectiveEnabled);
        if (requiredDisabled || profileSourceMismatch(server)) return { tone: "bad", rank: 1, label: "needs setup" };
        if (!server?.effectiveEnabled) return { tone: "warn", rank: 4, label: "off" };
        const effect = String(server.effectClass || "").toLowerCase();
        const stateClass = String(server.stateClass || "").toLowerCase();
        const credential = String(server.credentialBinding || "").toLowerCase();
        const scope = String(server.scopeClass || "").toLowerCase();
        const routing = String(server.routingGroup || "").toLowerCase();
        const concurrency = String(server.concurrencyPolicy || "").toLowerCase();
        const runtime = String(server.runtimeType || "").toLowerCase();
        const lock = String(server.hostLock || server.hostLockKey || "none").toLowerCase();
        const locks = Array.isArray(server.lockDomains) ? server.lockDomains.filter(Boolean) : [];
        const hasLiveTools = serverToolEvidence(server).checked;
        const unknown = !scope || scope === "configured-source" || runtime === "unknown" || stateClass === "unknown-conservative" || effect === "external-unknown";
        const exclusive = ["single-session", "single-writer", "isolated-per-project"].includes(concurrency)
          || lock !== "none"
          || locks.length > 0
          || instances.some(instance => instance.mode === "serialized");
        const sensitive = /write|mutation|external|host|remote|credential|stateful|session/.test(`${effect} ${stateClass} ${credential} ${scope} ${routing}`);
        if (sensitive || exclusive || server.discoveryRequiresLease) return { tone: "warn", rank: hasLiveTools ? 3 : 2, label: hasLiveTools ? "guarded" : "unchecked" };
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
        return items.map(item => {
          if (typeof item === "string") return item;
          return item?.name || item?.title || item?.qualifiedName || "";
        }).map(item => String(item || "").trim()).filter(Boolean);
      }

      function normalizeTools(value) {
        const items = Array.isArray(value) ? value : [];
        return items.map(item => typeof item === "string" ? { name: item } : item).filter(item => item && (item.name || item.title || item.qualifiedName));
      }

      function topToolNames(names, limit = 4) {
        const unique = [...new Set(normalizeToolNames(names))];
        const visible = unique.slice(0, limit);
        const suffix = unique.length > limit ? `, +${unique.length - limit} more` : "";
        return `${visible.join(", ")}${suffix}`;
      }

      function commonToolNamespace(names) {
        const prefixes = normalizeToolNames(names)
          .map(name => String(name).split(/[_.:-]/)[0])
          .map(prefix => prefix.trim())
          .filter(prefix => prefix.length > 1);
        if (!prefixes.length) return "";
        const counts = prefixes.reduce((map, prefix) => map.set(prefix, (map.get(prefix) || 0) + 1), new Map());
        const [prefix, count] = [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))[0] || ["", 0];
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
        return rows.find(row => String(row.name || "") === String(name || "")) || rows[0];
      }

      function normalizeProbeEvidence(name, value) {
        const payload = resultPayload(value);
        const row = probeResultForServer(name, value) || {};
        const tools = normalizeTools(row.tools || payload.tools || []);
        const toolNames = normalizeToolNames(row.toolNames || row.tools || payload.toolNames || payload.tools || []);
        const toolCount = num(row.toolCount ?? row.returnedToolCount ?? payload.toolCount ?? payload.returnedToolCount, toolNames.length || tools.length || 0);
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
          runtimeCallable: row.runtimeCallable ?? payload.runtimeCallable
        };
      }

      function serverToolEvidence(server) {
        const name = String(server?.name || "");
        const cached = state.serverTests?.[name];
        if (cached) return cached;
        const tools = normalizeTools(server?.tools || server?.topTools || []);
        const toolNames = normalizeToolNames(server?.toolNames || server?.tools || server?.topTools || []);
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
            checkedAtMs: server?.toolsListedAtMs || server?.toolsCheckedAtMs || 0
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
            checkedAtMs: 0
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
          checkedAtMs: 0
        };
      }

      function serverCategory(server) {
        const source = compactField(server.sourceType || server.transportPreference, "transport unknown");
        const kind = compactField(server.kind, "configured server");
        const runtime = compactField(server.runtimeType, "runtime unknown");
        return `${kind} · ${source} · ${runtime}`;
      }

      function serverImpact(server) {
        const effect = compactField(server.effectClass, "effect unknown");
        const stateClass = compactField(server.stateClass, "state unknown");
        const credential = compactField(server.credentialBinding, "credential unknown");
        return `${effect} · ${stateClass} · ${credential}`;
      }

      function serverEvidenceSummary(server) {
        const evidence = serverToolEvidence(server);
        if (evidence.checked && evidence.ok && evidence.toolCount > 0) {
          const namespace = commonToolNamespace(evidence.toolNames);
          const title = namespace ? `${humanizeKey(namespace)} tools` : `${evidence.toolCount} tool${evidence.toolCount === 1 ? "" : "s"} listed`;
          const body = evidence.toolNames.length
            ? `Live tools/list: ${topToolNames(evidence.toolNames)}.`
            : `Live tools/list reported ${evidence.toolCount} tool${evidence.toolCount === 1 ? "" : "s"}.`;
          return { title, body, evidence };
        }
        if (evidence.checked && !evidence.ok) {
          return {
            title: "Probe failed",
            body: evidence.error ? `Last tools/list check failed: ${evidence.error}` : "Last tools/list check failed.",
            evidence
          };
        }
        if (profileSourceMismatch(server)) {
          return {
            title: "Selected but source off",
            body: "Profile selects this server, but the MCP settings source has it disabled. No tools/list evidence is available while it is off.",
            evidence
          };
        }
        if (!server.effectiveEnabled) {
          return {
            title: "No live tools evidence",
            body: "This server is not effectively enabled, so MCPace has not listed its tools in this dashboard view.",
            evidence
          };
        }
        return {
          title: "Tools not checked",
          body: "Click Test to run initialize and tools/list before trusting what this server exposes.",
          evidence
        };
      }

      function serverVerdict(server, risk, related = []) {
        const recommendation = recommendedPolicy(server, related);
        const evidence = serverToolEvidence(server);
        if (risk.rank === 1) return { tone: "bad", label: "Needs setup", summary: "Selected or required, but not usable." };
        if (!server.effectiveEnabled) return { tone: "warn", label: "Off", summary: "Not active. No live tools listed." };
        if (evidence.checked && !evidence.ok) return { tone: "bad", label: "Test failed", summary: "Latest tools/list check failed." };
        if (policyNeedsTuning(server, related, recommendation)) return { tone: "warn", label: "Fix policy", summary: "A lower-resource policy is available." };
        if (!evidence.checked) return { tone: "warn", label: "Unchecked", summary: "Run Test to list tools." };
        if (risk.rank <= 3) return { tone: "warn", label: "Guarded", summary: "Usable with conservative policy." };
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
          decision: serverDecision(server, risk, related),
          settings: serverSettingProfile(server, risk, related),
          human: serverHumanSummary(server, risk, related),
          nextStep: serverNextStep(server, risk, related),
          category: serverCategory(server),
          impact: serverImpact(server),
          routeMode: serverMode(server, related),
          workers: maxWorkers(server, related),
          inFlight: maxInFlight(server, related),
          recommendation,
          needsTuning: policyNeedsTuning(server, related, recommendation),
          operatorPlan: operatorPlanForServer(server.name),
          facts: serverFacts(server, risk, related)
        };
      }

      function serverDecision(server, risk, related = []) {
        const recommendation = recommendedPolicy(server, related);
        const guidance = serverEvidenceSummary(server);
        const evidence = guidance.evidence || serverToolEvidence(server);
        if (profileSourceMismatch(server)) {
          return {
            title: "Fix source enablement",
            body: "The active profile selects this server, but its source is disabled. Turn it on or remove it from the profile."
          };
        }
        if (!server.effectiveEnabled) {
          return {
            title: "Off; no live tools listed",
            body: "Turn on only if the current workflow needs this MCP source, then run Test to list tools."
          };
        }
        if (evidence.checked && !evidence.ok) {
          return {
            title: "Retry tools/list",
            body: evidence.error || "The last check failed; run Test again to collect fresh evidence."
          };
        }
        if (!evidence.checked) {
          return {
            title: "Run test",
            body: `No live tool evidence yet: ${firstSentence(guidance.body)}`
          };
        }
        if (policyNeedsTuning(server, related, recommendation)) {
          return {
            title: "Apply recommended policy",
            body: "MCPace has a lower-resource setting ready. Tool evidence stays unchanged."
          };
        }
        return {
          title: "Evidence available",
          body: `${guidance.title}: ${firstSentence(guidance.body)}`
        };
      }

      function serverSettingProfile(server, risk, related = []) {
        const recommendation = recommendedPolicy(server, related);
        const guidance = serverEvidenceSummary(server);
        const current = `${routeLabel(serverMode(server, related))} / ${maxWorkers(server, related)}×${maxInFlight(server, related)}`;
        const recommended = `${recommendation.label}`;
        if (!server.effectiveEnabled) {
          const stateTitle = profileSourceMismatch(server) ? "Profile/source mismatch" : "Off";
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
            current
          };
        }
        if (policyNeedsTuning(server, related, recommendation)) {
          return {
            stateTitle: "On, policy differs",
            stateBody: "The server is usable, but the current policy differs from MCPace's inferred low-resource recommendation.",
            routeTitle: recommended,
            routeBody: `Current is ${current}. Apply only if you want the recommended policy.`,
            useTitle: guidance.title,
            useBody: guidance.body,
            current
          };
        }
        return {
          stateTitle: "On",
          stateBody: "The visible capability text is based only on live/cached evidence or source state.",
          routeTitle: recommended,
            routeBody: `${recommendation.reason} Current is ${current}.`,
          useTitle: guidance.title,
          useBody: guidance.body,
          current
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

      function serverHumanSummary(server, risk, related = []) {
        const guidance = serverEvidenceSummary(server);
        const evidence = guidance.evidence || serverToolEvidence(server);
        if (!server.effectiveEnabled) {
          return {
            capabilityTitle: guidance.title,
            capabilityBody: firstSentence(guidance.body),
            nowTitle: profileSourceMismatch(server) ? "Needs setup" : "Off",
            nowBody: profileSourceMismatch(server) ? "Source is disabled while the profile selects this server." : "No live tools are listed while it is off."
          };
        }
        if (evidence.checked && !evidence.ok) {
          return {
            capabilityTitle: guidance.title,
            capabilityBody: firstSentence(guidance.body),
            nowTitle: "Test failed",
            nowBody: "Retry Test to collect fresh tools/list evidence."
          };
        }
        if (!evidence.checked) {
          return {
            capabilityTitle: guidance.title,
            capabilityBody: firstSentence(guidance.body),
            nowTitle: "Unchecked",
            nowBody: "Run Test before assuming capabilities."
          };
        }
        if (policyNeedsTuning(server, related)) {
          return {
            capabilityTitle: guidance.title,
            capabilityBody: firstSentence(guidance.body),
            nowTitle: "Policy fix available",
            nowBody: "Apply the recommended policy only if you want MCPace's inferred low-resource setting."
          };
        }
        return {
          capabilityTitle: guidance.title,
          capabilityBody: firstSentence(guidance.body),
          nowTitle: "Evidence listed",
          nowBody: evidence.checkedAtMs ? `Checked ${fmtDate(evidence.checkedAtMs)}.` : "Tools/list evidence is available."
        };
      }

      function profileEvidence(server) {
        return Array.isArray(server.profileEvidence) && server.profileEvidence.length ? server.profileEvidence[0] : null;
      }

      function evidenceLine(server) {
        const evidence = profileEvidence(server);
        if (!evidence) return "No profile evidence was reported yet. Recommended policy keeps this route conservative.";
        const confidence = typeof evidence.confidence === "number" ? `${Math.round(evidence.confidence * 100)}%` : text(evidence.evidenceLevel, "unknown");
        const confidenceValue = typeof evidence.confidence === "number" ? evidence.confidence : 0;
        if (confidenceValue < 0.7 || /low|weak|unknown/i.test(String(evidence.evidenceLevel || ""))) {
          return `${text(evidence.evidenceLevel, "evidence")} evidence · ${confidence} readiness. Recommended policy keeps this route conservative.`;
        }
        return `${text(evidence.evidenceLevel, "evidence")} evidence · ${confidence} readiness. ${text(evidence.summary, "Recommended policy keeps this route conservative.")}`;
      }

      function serverNextStep(server, risk, instances = []) {
        if (risk.rank === 1) return "Fix the source/profile mismatch, then run Test to collect tools/list evidence.";
        if (risk.rank === 2) return "Run Test to collect initialize + tools/list evidence before assuming capabilities.";
        if (risk.rank === 3) return instances.length
          ? "Evidence exists, but routing remains conservative because runtime state, credentials, or locks are present."
          : "Evidence exists, but MCPace still keeps conservative routing from inferred policy fields.";
        if (!server.effectiveEnabled) return "Disabled. Run Test when ready, then turn on only when a workflow asks for this source.";
        return "Ready from current evidence. Re-test if the source config changes.";
      }

      function routingPlain(server, risk, instances = []) {
        if (risk.rank === 2) return "MCPace has no live tools/list evidence in the dashboard yet, so it keeps conservative routing.";
        if (risk.rank === 3) return "Requests are serialized or isolated because this server has state/credentials/locks that should not be shared freely.";
        if (server.runtimeType === "stateless") return "The server looks stateless, so MCPace can share it more safely across requests.";
        if (instances.some(instance => instance.mode === "pool")) return "MCPace plans a pool, so multiple workers can serve traffic while respecting the configured limits.";
        return "Routing follows the server policy fields and any planned instance/mutex hints returned by the runtime.";
      }

      function serverChecklist(server, risk) {
        const checks = [];
        if (risk.rank <= 2) {
          checks.push("No capability is assumed without source state or tools/list evidence.");
          checks.push("Use Test to collect initialize + tools/list evidence.");
        } else if (risk.rank === 3) {
          checks.push("This server is intentionally one-at-a-time because it has state, credentials, or locks.");
          checks.push("Manual worker changes stay collapsed because they are advanced overrides.");
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
        return currentServers().find(server => String(server.name || "") === String(name || ""));
      }

      function relatedInstances(name) {
        return currentInstances().filter(instance => String(instance.server || instance.serverName || "") === String(name || ""));
      }

      function serverMode(server, instances = []) {
        const instanceMode = instances.find(instance => instance.mode)?.mode;
        const routingGroup = String(server.routingGroup || "");
        const concurrency = String(server.concurrencyPolicy || "");
        const startup = String(server.startupStrategy || "");
        if (startup === "disabled" || routingGroup === "disabled") return "disabled";
        if (instanceMode === "pool" || /pool/.test(routingGroup)) return "pool";
        if (instanceMode === "shared" || /shared|parallel/.test(routingGroup) || concurrency === "multi-reader") return "shared";
        if (/project/.test(routingGroup) || concurrency === "isolated-per-project") return "project-isolated";
        if (/session/.test(routingGroup) || concurrency === "single-session") return "session-isolated";
        return "serialized";
      }

      function modeOptions(selected) {
        return ROUTING_MODES.map(([value, label]) => `<option value="${value}"${value === selected ? " selected" : ""}>${escapeHtml(label)}</option>`).join("");
      }

      function routeLabel(mode) {
        return ROUTING_MODES.find(([value]) => value === mode)?.[1] || mode || "Safe queue";
      }

      function domId(value) {
        return String(value || "server").replace(/[^a-zA-Z0-9_-]+/g, "-").slice(0, 80) || "server";
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
        const isStateless = server.runtimeType === "stateless" || /stateless|read-only/.test(`${stateClass} ${effect}`);
        const canShare = risk.rank >= 5 && confidence >= 0.7 && isStateless && !/credential|stateful|external-unknown|write|host/.test(`${credential} ${stateClass} ${effect}`);
        if (canShare) {
          return {
            mode: "shared",
            maxWorkers: 1,
            maxInFlightPerWorker: 1,
            label: "Shared / 1 worker",
            reason: "High-readiness stateless server; one shared worker is the lowest-resource safe default."
          };
        }
        if (serverMode(server, related) === "pool" && currentWorkers > 1 && risk.rank <= 3) {
          return {
            mode: "serialized",
            maxWorkers: 1,
            maxInFlightPerWorker: 1,
            label: "Safe queue / 1 worker",
            reason: "Recommended policy reduces resource use for this stateful or low-evidence server."
          };
        }
        return {
          mode: "serialized",
          maxWorkers: 1,
          maxInFlightPerWorker: 1,
          label: "Safe queue / 1 worker",
          reason: risk.rank <= 2
            ? "Recommended policy keeps this server low-resource and one-at-a-time."
            : "Conservative one-worker routing avoids extra idle upstream processes."
        };
      }

      function policyNeedsTuning(server, related = [], recommendation = recommendedPolicy(server, related)) {
        if (!server?.effectiveEnabled) return false;
        return serverMode(server, related) !== recommendation.mode
          || maxWorkers(server, related) !== recommendation.maxWorkers
          || maxInFlight(server, related) !== recommendation.maxInFlightPerWorker;
      }

      function autoPolicyPlan(servers = currentServers(), instances = currentInstances()) {
        const groups = groupByServer(instances);
        const plan = {
          enabled: 0,
          disabled: 0,
          already: 0,
          protected: 0,
          ready: 0,
          changes: []
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
              reason: recommendation.reason
            });
          } else {
            plan.already += 1;
          }
        }
        return plan;
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
            : "Enabled servers already have automatic safety limits. You do not need to understand workers unless you open Details on purpose.";
        }
        if (els.serverAutoStats) {
          els.serverAutoStats.innerHTML = [
            chip(`${plan.enabled} on`, plan.enabled ? "good" : "warn"),
            chip(`${plan.protected} guarded`, plan.protected ? "warn" : "good"),
            chip(`${plan.ready} ready`, plan.ready ? "good" : "warn"),
            chip(`${changeCount} fixes`, changeCount ? "warn" : "good")
          ].join("");
        }
        if (els.autoTuneVisible) {
          els.autoTuneVisible.disabled = changeCount === 0;
          els.autoTuneVisible.textContent = changeCount
            ? `Apply ${changeCount} safe fix${changeCount === 1 ? "" : "es"}`
            : "Safe plan active";
        }
      }



      function importResultPayload(value) {
        const result = value && typeof value === "object" ? value.result || value : {};
        return result && typeof result === "object" ? result : {};
      }

      function renderServerImportPanel() {
        if (!els.serverImportResult) return;
        updateServerImportPreflight();
        if (state.importer.loading) {
          els.serverImportResult.innerHTML = `<article class="item warn"><div class="item-head"><div class="name">Reading config…</div>${chip("running", "warn")}</div><div class="meta">MCPace is validating the local file and preparing a preview. No secret values are rendered here.</div></article>`;
          return;
        }
        if (state.importer.error) {
          els.serverImportResult.innerHTML = `<article class="item bad"><div class="item-head"><div class="name">Import failed</div>${chip("error", "bad")}</div><div class="meta">${escapeHtml(state.importer.error)}</div></article>`;
          return;
        }
        if (!state.importer.result) {
          els.serverImportResult.innerHTML = `<article class="item"><div class="item-head"><div class="name">No import run yet</div>${chip("idle", "warn")}</div><div class="meta">Recommended first move: import what you already use, then test one server at a time.</div></article>`;
          return;
        }
        const payload = importResultPayload(state.importer.result);
        const entries = Array.isArray(payload.entries) ? payload.entries : [];
        const copied = num(payload.copiedCount ?? payload.importedCount ?? payload.updatedCount, entries.filter(entry => entry.action !== "skipped").length);
        const skipped = num(payload.skippedCount, entries.filter(entry => entry.action === "skipped").length);
        const dryRun = Boolean(payload.dryRun ?? state.importer.last?.dryRun);
        const disabled = Boolean(payload.disabled ?? state.importer.last?.disabled ?? true);
        const force = Boolean(payload.force ?? state.importer.last?.force);
        const addCount = num(payload.addedCount ?? payload.wouldAddCount ?? payload.importedCount ?? payload.copiedCount, entries.filter(entry => /add|copy|import|new|would/i.test(String(entry.action || entry.status || ""))).length || copied);
        const replaceCount = num(payload.replacedCount ?? payload.wouldReplaceCount ?? payload.updatedCount, entries.filter(entry => /replace|update|overwrite/i.test(String(entry.action || entry.status || ""))).length);
        const title = dryRun ? `${entries.length || copied || 0} server${(entries.length || copied) === 1 ? "" : "s"} in preview` : `${copied} source${copied === 1 ? "" : "s"} copied ${disabled ? "disabled" : "as saved"}`;
        const tone = copied || entries.length ? "good" : "warn";
        const diff = `<div class="import-diff-grid" aria-label="Import change summary">
          <article class="import-diff-card"><span>Will add</span><strong>${escapeHtml(addCount)}</strong><p>New sources from the selected config.</p></article>
          <article class="import-diff-card"><span>Will replace</span><strong>${escapeHtml(replaceCount)}</strong><p>${escapeHtml(force ? "Force is on; duplicates may be overwritten." : "Duplicates stay protected unless force is on.")}</p></article>
          <article class="import-diff-card"><span>Will skip</span><strong>${escapeHtml(skipped)}</strong><p>Self entries, duplicates, or unsupported shapes.</p></article>
          <article class="import-diff-card"><span>Saved state</span><strong>${escapeHtml(disabled ? "Off" : "Source")}</strong><p>${escapeHtml(disabled ? "Imported sources stay parked until Review → Enable → Test." : "Imported enabled flags are preserved.")}</p></article>
        </div>`;
        const summary = `<article class="item ${tone}"><div class="item-head"><div class="name">${escapeHtml(title)}</div>${chip(dryRun ? "preview" : "saved", tone)}</div><div class="meta">${escapeHtml(skipped ? `${skipped} skipped. Review duplicates or MCPace self entries before forcing.` : "Next: review the imported row, enable deliberately, then run Test.")}</div></article>`;
        const entryHtml = entries.slice(0, 5).map(entry => {
          const name = entry.name || entry.server || entry.id || "server";
          const action = entry.action || entry.status || (dryRun ? "would copy" : "copied");
          const source = entry.sourcePath || entry.source || entry.type || entry.command || entry.url || "source hidden";
          const entryTone = /skip|error|fail/i.test(action) ? "bad" : /would|preview|copy|import|update/i.test(action) ? "good" : "warn";
          return `<article class="item ${itemClass(entryTone)}"><div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(action, entryTone)}</div><div class="meta">${escapeHtml(source)}</div></article>`;
        }).join("");
        els.serverImportResult.innerHTML = `${diff}${summary}${entryHtml ? `<div class="discovery-candidate-list">${entryHtml}</div>` : ""}`;
      }

      function clientPreferredConfigPath(client) {
        const supportPath = client?.installSupport?.preferredConfigPath;
        const paths = Array.isArray(client?.configPaths) ? client.configPaths : [];
        const candidates = [supportPath, ...paths].filter(Boolean).map(value => String(value).trim()).filter(Boolean);
        return candidates.find(path => /\.json(?:$|\b)/i.test(path)) || candidates[0] || "";
      }

      function clientImportPathAllowed(client) {
        const path = clientPreferredConfigPath(client);
        return Boolean(path && (/\.json(?:$|\b)/i.test(path) || String(client?.configFormat || "").toLowerCase().includes("json")));
      }

      function clientSetupTargets(clients) {
        const targets = Array.isArray(clients) ? clients : normalizeClients(clients);
        const weight = client => {
          let score = 0;
          if (client?.surfaceClass === "local") score -= 10;
          if (client?.installSupported) score -= 8;
          if (clientImportPathAllowed(client)) score -= 4;
          if (/claude|cursor|vscode|vs code|codex/i.test(`${client?.displayName || ""} ${client?.id || ""}`)) score -= 3;
          if (String(client?.surfaceClass || "") === "cloud") score += 12;
          return score;
        };
        return [...targets].sort((left, right) => weight(left) - weight(right) || String(left?.displayName || left?.id || "").localeCompare(String(right?.displayName || right?.id || "")));
      }

      function renderClientSetup(clients = [], catalog = {}) {
        if (!els.clientSetupList) return;
        const targets = clientSetupTargets(clients);
        const local = targets.filter(client => client?.surfaceClass === "local");
        const writable = local.filter(client => client?.installSupported);
        if (!targets.length) {
          els.clientSetupList.innerHTML = `<article class="client-setup-card item warn"><div class="item-head"><div class="name">No client catalog returned</div>${chip("waiting", "warn")}</div><div class="meta">Import and manual server setup still work. Client patch actions appear after client list data loads.</div></article>`;
          renderClientSetupResult();
          return;
        }
        const shown = targets.slice(0, 6);
        els.clientSetupList.innerHTML = shown.map(client => {
          const id = client.id || client.clientTargetId || "client";
          const name = client.displayName || id;
          const localSurface = client.surfaceClass === "local";
          const supported = Boolean(client.installSupported);
          const path = clientPreferredConfigPath(client);
          const canImport = clientImportPathAllowed(client);
          const tone = supported ? "good" : localSurface ? "warn" : "bad";
          const meta = [client.surfaceKind || client.surfaceClass || "surface", client.configFormat ? `format ${client.configFormat}` : "format unknown", path || "no local config path"].filter(Boolean).join(" · ");
          const ingress = Array.isArray(client.supportedIngresses) ? client.supportedIngresses.slice(0, 3).join(", ") : "—";
          return `<article class="client-setup-card item ${itemClass(tone)}" data-client-id="${escapeHtml(id)}" data-client-path="${escapeHtml(path)}">
            <div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(supported ? "patchable" : localSurface ? "manual" : "cloud", tone)}</div>
            <div class="meta">${escapeHtml(meta)}</div>
            <div class="tags">${tags([`ingress ${ingress}`, client.installSupport?.preferredScope ? `scope ${client.installSupport.preferredScope}` : "scope manual", canImport ? "importable JSON" : "not import-first"])}</div>
            <div class="client-setup-actions" aria-label="Client actions for ${escapeHtml(name)}">
              ${canImport ? `<button type="button" data-client-setup-action="use-import-path" data-client-path="${escapeHtml(path)}">Use import path</button>` : ""}
              ${supported ? `<button type="button" data-client-setup-action="preview-client" data-client-id="${escapeHtml(id)}">Preview patch</button><button class="primary" type="button" data-client-setup-action="install-client" data-client-id="${escapeHtml(id)}">Apply patch</button><button type="button" data-client-setup-action="restore-client" data-client-id="${escapeHtml(id)}">Restore</button>` : `<button type="button" data-client-setup-action="show-client" data-client-id="${escapeHtml(id)}">Show details</button>`}
            </div>
          </article>`;
        }).join("") + (targets.length > shown.length ? `<div class="note">${targets.length - shown.length} more client target(s) stay in Diagnostics.</div>` : "");
        if (els.clientPreviewAll) els.clientPreviewAll.disabled = !writable.length || state.clientSetup.loading;
        if (els.clientApplyAll) els.clientApplyAll.disabled = !writable.length || state.clientSetup.loading;
        if (els.clientRestoreAll) els.clientRestoreAll.disabled = !writable.length || state.clientSetup.loading;
        renderClientSetupResult();
      }

      function clientActionPayload(value) {
        return value?.result || value || {};
      }

      function renderClientSetupResult() {
        if (!els.clientSetupResult) return;
        if (state.clientSetup.loading) {
          els.clientSetupResult.innerHTML = `<article class="item warn"><div class="item-head"><div class="name">Running client action…</div>${chip("working", "warn")}</div><div class="meta">MCPace is using the CLI action and will show changed/would-change status here.</div></article>`;
          return;
        }
        if (state.clientSetup.error) {
          els.clientSetupResult.innerHTML = `<article class="item bad"><div class="item-head"><div class="name">Client action failed</div>${chip("error", "bad")}</div><div class="meta">${escapeHtml(state.clientSetup.error)}</div></article>`;
          return;
        }
        if (!state.clientSetup.result) {
          els.clientSetupResult.innerHTML = `<article class="item"><div class="item-head"><div class="name">No client patch run yet</div>${chip("idle", "warn")}</div><div class="meta">Use Preview first. Apply writes are explicit and restorable.</div></article>`;
          return;
        }
        const payload = clientActionPayload(state.clientSetup.result);
        const installed = Array.isArray(payload.installed) ? payload.installed : payload.clientTargetId ? [payload] : [];
        const failed = Array.isArray(payload.failed) ? payload.failed : [];
        const skipped = Array.isArray(payload.skipped) ? payload.skipped : [];
        const changed = installed.filter(item => item.changed || item.wouldChange || item.persisted).length;
        const dryRun = Boolean(payload.dryRun ?? installed.some(item => item.dryRun));
        const restoreMode = /restore/i.test(String(payload.mode || state.clientSetup.last?.action || ""));
        const title = restoreMode
          ? `Restore ${payload.clientTargetId || state.clientSetup.last?.clientId || "client"}`
          : installed.length
            ? `${installed.length} client patch${installed.length === 1 ? "" : "es"} ${dryRun ? "previewed" : "processed"}`
            : payload.clientTargetId
              ? `${payload.clientTargetId} processed`
              : "Client action complete";
        const tone = failed.length ? "bad" : changed || restoreMode ? "good" : "warn";
        const cards = installed.slice(0, 4).map(item => {
          const name = item.displayName || item.clientTargetId || "client";
          const meta = [item.configPath, item.dryRun ? "preview only" : item.persisted ? "written" : "no write", item.backupId ? `backup ${item.backupId}` : "backup pending"].filter(Boolean).join(" · ");
          const itemTone = item.persisted || item.changed || item.wouldChange ? "good" : "warn";
          return `<article class="item ${itemClass(itemTone)}"><div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(item.dryRun ? "preview" : item.persisted ? "written" : "checked", itemTone)}</div><div class="meta">${escapeHtml(meta)}</div>${item.diff ? `<pre class="client-result-diff">${escapeHtml(shortText(item.diff, 2000))}</pre>` : ""}</article>`;
        }).join("");
        const restoreCard = restoreMode ? `<article class="item good"><div class="item-head"><div class="name">${escapeHtml(payload.clientTargetId || "client")}</div>${chip("restored", "good")}</div><div class="meta">${escapeHtml([payload.configPath, payload.backupId].filter(Boolean).join(" · ") || "Latest backup restored.")}</div></article>` : "";
        const failedCard = failed.length ? `<article class="item bad"><div class="item-head"><div class="name">${failed.length} failed</div>${chip("check", "bad")}</div><div class="meta">${escapeHtml(failed.slice(0, 3).map(item => `${item.clientTargetId || "client"}: ${item.error || "failed"}`).join(" · "))}</div></article>` : "";
        const skippedNote = skipped.length ? `<div class="note">${skipped.length} skipped: ${escapeHtml(skipped.slice(0, 3).join(" · "))}</div>` : "";
        els.clientSetupResult.innerHTML = `<article class="item ${tone}"><div class="item-head"><div class="name">${escapeHtml(title)}</div>${chip(dryRun ? "preview" : restoreMode ? "restored" : "done", tone)}</div><div class="meta">${escapeHtml(dryRun ? "Nothing was written. Apply only after the patch looks right." : restoreMode ? "Rollback completed from the selected backup." : "Client config action completed; refresh shows updated catalog state.")}</div></article>${cards}${restoreCard}${failedCard}${skippedNote}`;
      }

      function fillClientImportPath(path) {
        if (!path || !els.serverImportPath) return;
        if (els.setupTools) els.setupTools.open = true;
        els.serverImportPath.value = path;
        state.importer.error = null;
        renderServerImportPanel();
        revealElementById("server-import-panel", "center");
        window.setTimeout(() => els.serverImportPath?.focus?.(), 120);
      }

      async function runClientSetupAction(action, payload, control, busyLabel = "Working…") {
        const originalText = control && "textContent" in control ? control.textContent : "";
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
          if (action === "client-install" && payload?.dryRun !== true) await refreshDashboard({ force: true, reason: action });
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
          renderClientSetup(normalizeClients(state.overview?.clients || []), state.overview?.clients || {});
        }
      }

      function handleClientSetupClick(event) {
        const control = event.target.closest("[data-client-setup-action]");
        if (!control) return;
        const action = control.dataset.clientSetupAction;
        const clientId = control.dataset.clientId || control.closest("[data-client-id]")?.dataset?.clientId;
        const path = control.dataset.clientPath || control.closest("[data-client-path]")?.dataset?.clientPath;
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
          runClientSetupAction("client-install", { clientId, dryRun: true, diff: true }, control, "Previewing…");
          return;
        }
        if (action === "install-client") {
          runClientSetupAction("client-install", { clientId, dryRun: false, diff: false }, control, "Applying…");
          return;
        }
        if (action === "restore-client") {
          if (!window.confirm(`Restore the latest MCPace client backup for ${clientId}?`)) return;
          runClientSetupAction("client-restore", { clientId, backup: "latest" }, control, "Restoring…");
        }
      }

      function renderAutomation(overview = {}, servers = [], instances = []) {
        if (!els.automationGrid) return;
        const plan = autoPolicyPlan(servers, instances);
        const cache = overview.cache || {};
        const cachedTools = overview.cachedToolEvidence || {};
        const automation = overview.automation || {};
        const discoveryControl = overview.discoveryControl || automation.discoveryJob || {};
        const runtimeControl = overview.runtimeControlPlane?.summary || {};
        const refreshLabel = state.refreshMode === "paused" ? "Paused" : `${Math.round((REFRESH_MS[state.refreshMode] || automation.overviewRefresh?.intervalMs || 0) / 1000)}s`;
        const refreshTone = state.refreshMode === "paused" ? "warn" : "good";
        const cacheTone = cache.stale || cache.refreshError ? "warn" : "good";
        const toolServerCount = num(cachedTools.serverCount ?? automation.toolEvidenceCache?.serverCount);
        const toolOk = num(cachedTools.okCount);
        const toolMiss = num(cachedTools.cacheMissCount);
        const toolFailed = num(cachedTools.failedCount);
        const toolTone = toolFailed ? "bad" : toolMiss ? "warn" : toolServerCount ? "good" : "warn";
        const policyTone = plan.changes.length ? "warn" : servers.length ? "good" : "warn";
        const importTone = automation.serverSources?.importSupported ? "good" : "warn";
        const registryCache = discoveryControl.registryCache || automation.discoveryJob?.registryCache || {};
        const registryTone = registryCache.exists ? "good" : discoveryControl.enabled || automation.discoveryJob?.enabled ? "warn" : "bad";
        const discoveryTone = discoveryControl.enabled || automation.discoveryJob?.enabled ? "good" : "warn";
        if (els.automationTitle) {
          els.automationTitle.textContent = plan.changes.length
            ? `${plan.changes.length} conservative policy change${plan.changes.length === 1 ? "" : "s"} ready`
            : "Automatic work is under control";
        }
        if (els.automationBody) {
          els.automationBody.textContent = "Live fields refresh automatically; import/discovery/config changes stay explicit. The dashboard separates live state, stored config, derived policy, and hidden secrets.";
        }
        els.automationGrid.innerHTML = [
          `<article class="automation-card ${refreshTone}"><span>Auto refresh</span><strong>${escapeHtml(refreshLabel)}</strong><em>local view preference</em></article>`,
          `<article class="automation-card ${cacheTone}"><span>Overview cache</span><strong>${escapeHtml(cache.hit ? "hit" : cache.bypassed ? "bypass" : "fresh")}</strong><em>age ${escapeHtml(fmtMs(cache.ageMs))} · ttl ${escapeHtml(fmtMs(cache.ttlMs))}</em></article>`,
          `<article class="automation-card ${importTone}"><span>Import existing</span><strong>${escapeHtml(automation.serverSources?.baseFile || "mcp_settings")}</strong><em>${num(automation.serverSources?.includeDirCount)} include dirs · preview first</em></article>`,
          `<article class="automation-card ${discoveryTone}"><span>Discovery</span><strong>${escapeHtml(discoveryControl.mode || automation.discoveryJob?.mode || "manual")}</strong><em>${escapeHtml(discoveryControl.autoInstall || automation.discoveryJob?.autoInstall || "manual-only")} · unknown ${escapeHtml(discoveryControl.installUnknown || automation.discoveryJob?.unknownServers || "plan-only")}</em></article>`,
          `<article class="automation-card ${registryTone}"><span>Registry cache</span><strong>${escapeHtml(registryCache.exists ? "ready" : "missing")}</strong><em>${escapeHtml(registryCache.configuredPath || "catalog/registry-cache.json")}</em></article>`,
          `<article class="automation-card ${toolTone}"><span>Tool cache</span><strong>${toolOk}/${toolServerCount || servers.length}</strong><em>${num(cachedTools.toolCount ?? automation.toolEvidenceCache?.toolCount)} tools · ${toolMiss} miss · ${toolFailed} fail</em></article>`,
          `<article class="automation-card ${policyTone}"><span>Policy plan</span><strong>${plan.changes.length}</strong><em>${num(runtimeControl.serialized)} serialized · ${num(runtimeControl.sharedOk)} shared/pool</em></article>`
        ].join("");
      }

      function discoveryPayload(value) {
        return value?.result || value || {};
      }

      function renderDiscoveryPanel() {
        if (!els.serverDiscoveryResults) return;
        if (state.discovery.loading) {
          els.serverDiscoveryResults.innerHTML = `<article class="item warn"><div class="item-head"><div class="name">Searching candidates…</div>${chip("running", "warn")}</div><div class="meta">Preview checks local approved/registry cache and returns a plan. No server is enabled from this step.</div></article>`;
          return;
        }
        if (state.discovery.error) {
          els.serverDiscoveryResults.innerHTML = `<article class="item bad"><div class="item-head"><div class="name">Discovery failed</div>${chip("error", "bad")}</div><div class="meta">${escapeHtml(state.discovery.error)}</div></article>`;
          return;
        }
        const payload = discoveryPayload(state.discovery.result);
        const candidates = Array.isArray(payload.candidates) ? payload.candidates : [];
        const automatic = Array.isArray(payload.automaticInstallResults) ? payload.automaticInstallResults : [];
        const probes = Array.isArray(payload.postInstallProbeResults) ? payload.postInstallProbeResults : [];
        if (!state.discovery.result) {
          els.serverDiscoveryResults.innerHTML = `<article class="item"><div class="item-head"><div class="name">No discovery run yet</div>${chip("idle", "warn")}</div><div class="meta">Use Preview first. MCPace should not install, enable, or expose a server without an explicit user action.</div></article>`;
          return;
        }
        const decision = payload.installDecision || (state.discovery.lastMode === "install" ? "install requested" : "preview");
        const block = payload.installBlockReason || payload.warning || "";
        const summary = `<article class="item ${automatic.length ? "good" : candidates.length ? "warn" : ""}">
          <div class="item-head"><div class="name">${escapeHtml(num(payload.candidateCount, candidates.length))} candidate${num(payload.candidateCount, candidates.length) === 1 ? "" : "s"}</div>${chip(decision, automatic.length ? "good" : candidates.length ? "warn" : "bad")}</div>
          <div class="meta">${escapeHtml(block || "Preview returned a ranked plan. Install still requires an explicit mode and keeps sources disabled by default.")}</div>
          ${automatic.length ? `<div class="tags">${tags(automatic.slice(0, 4).map(item => `${item.name || "server"}: ${item.decision || "result"}`))}</div>` : ""}
          ${probes.length ? `<div class="tags">${tags(probes.slice(0, 4).map(item => `${item.server || item.name || "server"} probed`))}</div>` : ""}
        </article>`;
        const candidateHtml = candidates.slice(0, 5).map(candidate => {
          const trust = candidate.trustLevel || "review";
          const tone = candidate.installed ? "good" : trust === "approved" || trust === "trusted" ? "good" : trust === "review" ? "warn" : "bad";
          const title = candidate.title || candidate.name || "candidate";
          const spec = candidate.installSpec || candidate.package || candidate.url || "";
          const meta = [candidate.source, candidate.registryType, candidate.transport, candidate.recommendedMode ? `mode ${candidate.recommendedMode}` : "", candidate.score !== undefined ? `score ${candidate.score}` : ""].filter(Boolean).join(" · ");
          return `<article class="item discovery-candidate ${itemClass(tone)}">
            <div>
              <div class="item-head"><div class="name">${escapeHtml(title)}</div>${chip(trust, tone)}</div>
              <div class="meta">${escapeHtml(candidate.description || meta || "No description returned.")}</div>
              ${meta ? `<div class="tags">${tags(meta.split(" · "))}</div>` : ""}
              ${spec ? `<code class="discovery-install-spec">${escapeHtml(spec)}</code>` : ""}
            </div>
            <span class="chip">${candidate.installed ? `${dot("good")}installed` : `${dot("warn")}not installed`}</span>
          </article>`;
        }).join("");
        els.serverDiscoveryResults.innerHTML = `${summary}${candidateHtml ? `<div class="discovery-candidate-list">${candidateHtml}</div>` : `<div class="empty-state"><strong>No candidates matched.</strong><p>Try a broader term or paste a command manually.</p><div class="empty-actions"><button class="primary" type="button" data-empty-action="add-server">Paste command</button></div></div>`}`;
      }

      function normalizeOperatorPlan(value) {
        const plan = value && typeof value === "object" ? value : {};
        return {
          schema: plan.schema || "mcpace.operatorPlan.v0",
          summary: plan.summary || {},
          items: Array.isArray(plan.items) ? plan.items : [],
          flow: Array.isArray(plan.flow) ? plan.flow : []
        };
      }

      function operatorPlanForServer(name) {
        const plan = normalizeOperatorPlan(state.overview?.operatorPlan);
        const wanted = String(name || "");
        return plan.items.find(item => String(item?.name || "") === wanted) || null;
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
        if (!items.length) return `<span class="note">No runbook commands yet.</span>`;
        return items.map(command => `<span class="operator-command"><strong>${escapeHtml(command.label || "Run")}</strong>${escapeHtml(shortText(command.command || "", 118))}</span>`).join("");
      }

      function renderOperatorPlan(rawPlan, servers = [], instances = []) {
        const plan = normalizeOperatorPlan(rawPlan);
        const summary = plan.summary || {};
        const items = [...plan.items].sort((left, right) => num(left.priority, 9) - num(right.priority, 9) || String(left.name || "").localeCompare(String(right.name || "")));
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
          els.operatorPlanStats.innerHTML = [
            chip(`${total} total`, total ? "good" : "warn"),
            chip(`${blocked} blocked`, blocked ? "bad" : "good"),
            chip(`${unchecked} unchecked`, unchecked ? "warn" : "good"),
            chip(`${guarded} guarded`, guarded ? "warn" : "good"),
            chip(`${ready} ready`, ready ? "good" : "warn"),
            chip(`${changes} policy fixes`, changes ? "warn" : "good")
          ].join("");
        }
        if (els.operatorPlanLanes) {
          const laneOrder = ["blocked", "unchecked", "guarded", "ready", "off"];
          const cards = laneOrder.map(lane => {
            const laneItems = items.filter(item => item.lane === lane);
            if (!laneItems.length) return "";
            const lead = laneItems[0];
            const tone = operatorPlanTone(lead);
            return `<article class="operator-plan-card ${itemClass(tone)}">
              <div class="label">${escapeHtml(lane)} · ${laneItems.length}</div>
              <strong>${escapeHtml(lead.name || "server")}: ${escapeHtml(lead.nextAction || "review")}</strong>
              <p>${escapeHtml(lead.rationale || lead.evidence || "No rationale available.")}</p>
              <div class="operator-command-list" style="margin-top: 8px;">${operatorCommandChips(lead.commands, 2)}</div>
            </article>`;
          }).filter(Boolean);
          els.operatorPlanLanes.innerHTML = cards.length ? cards.join("") : `<div class="empty">No server operator lanes yet.</div>`;
        }
        if (els.operatorPlanFlow) {
          const flow = plan.flow.length ? plan.flow : [
            { stage: "Client", description: "User clients talk to /mcp, not directly to server commands." },
            { stage: "Broker", description: "MCPace checks policy, leases, route state, and evidence." },
            { stage: "Source", description: "Server source stays explicit, reversible, and disabled when blocked." },
            { stage: "Evidence", description: "initialize + tools/list proof decides what the normal view may trust." }
          ];
          els.operatorPlanFlow.innerHTML = flow.slice(0, 4).map((step, index) => `<article class="flow-card"><span class="flow-index">${String(index + 1).padStart(2, "0")}</span><strong>${escapeHtml(step.stage || step.label || "stage")}</strong><p>${escapeHtml(step.description || step.body || "Safe broker stage.")}</p></article>`).join("");
        }
      }

      function runtimeControlForServer(name) {
        const items = state.overview?.runtimeControlPlane?.items;
        if (!Array.isArray(items)) return null;
        return items.find(item => item.name === name) || null;
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
          `budget ${text(budget.class, "unknown")}`
        ];
        const signals = Array.isArray(risk.signals) && risk.signals.length ? risk.signals.slice(0, 4).join(", ") : "no tool-risk signals yet";
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
        const blockerList = blockers.length ? `<section><div class="label">Blockers</div><ul class="server-checklist">${blockers.map(item => `<li>${escapeHtml(item)}</li>`).join("")}</ul></section>` : "";
        const safeguardList = safeguards.length ? `<section><div class="label">Safeguards</div><ul class="server-checklist">${safeguards.map(item => `<li>${escapeHtml(item)}</li>`).join("")}</ul></section>` : "";
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
          if (escaped) { escaped = false; continue; }
          if (ch === "\\" && !singleQuoted) { escaped = true; continue; }
          if (ch === "'" && !doubleQuoted) { singleQuoted = !singleQuoted; continue; }
          if (ch === '"' && !singleQuoted) { doubleQuoted = !doubleQuoted; continue; }
          if (singleQuoted || doubleQuoted) continue;
          if (["`", ";", "|", "<", ">"].includes(ch)) return true;
          if (ch === "&" && chars[index + 1] === "&") return true;
          if (ch === "$" && chars[index + 1] === "(") return true;
        }
        return false;
      }

      function installCommandIntent(value) {
        const raw = String(value || "").trim();
        if (!raw) return { tone: "warn", label: "Waiting", body: "Paste one launcher command, package spec, local path, or Streamable HTTP URL." };
        if (commandLineLooksComposed(raw)) {
          return { tone: "bad", label: "Rejected", body: "Use one command or URL only. Remove shell chaining, pipes, redirects, backticks, or command substitutions." };
        }
        if (/^https?:\/\//i.test(raw)) {
          return { tone: "good", label: "HTTP source", body: "Will save a Streamable HTTP source. MCPace will not call it until you explicitly test and enable it." };
        }
        const launcher = raw.split(/\s+/)[0] || "";
        if (["npx", "pnpm", "yarn", "bunx", "uvx", "python", "python3", "node", "deno"].includes(launcher)) {
          return { tone: "warn", label: "Launcher", body: `Will save ${launcher} as a server launcher. Keep it parked, then enable deliberately and run Test to collect tools/list evidence.` };
        }
        if (/^(\.|~|\/|[A-Za-z]:[\\/])/.test(raw)) {
          return { tone: "warn", label: "Local path", body: "Will save a local command/path. Check working directory and environment names before enabling." };
        }
        return { tone: "warn", label: "Package or command", body: "Will save this as a server source. Prefer trusted registries and review the resolved launch command before enabling." };
      }

      function updateServerInstallPreflight() {
        const intent = installCommandIntent(els.serverInstallCommand?.value || "");
        setInstallNote(`${intent.label}: ${intent.body}`, intent.tone);
        setFieldError(els.serverInstallError, els.serverInstallCommand, intent.tone === "bad" ? intent.body : "");
        if (els.serverInstallButton) els.serverInstallButton.disabled = intent.tone === "bad";
      }

      function importPathIntent(value) {
        const raw = String(value || "").trim();
        if (!raw) return { tone: "warn", label: "Waiting", body: "Paste a local MCP settings JSON path exported by another client." };
        if (raw.includes(String.fromCharCode(0)) || raw.includes(String.fromCharCode(10)) || raw.includes(String.fromCharCode(13))) return { tone: "bad", label: "Rejected", body: "Use a single local file path without newlines or control characters." };
        if (/^https?:\/\//i.test(raw)) return { tone: "bad", label: "Remote URL", body: "Import accepts local config files only. Add remote HTTP servers through Add server instead." };
        if (!/\.json$/i.test(raw)) return { tone: "warn", label: "Check path", body: "This can still work, but MCP settings imports are usually JSON files." };
        return { tone: "good", label: "Ready", body: "Preview will read the file and list copied servers without enabling them." };
      }

      function updateServerImportPreflight() {
        const intent = importPathIntent(els.serverImportPath?.value || "");
        const dryRun = els.serverImportDryRun?.checked !== false;
        setImportNote(`${intent.label}: ${intent.body}`, intent.tone);
        setFieldError(els.serverImportError, els.serverImportPath, intent.tone === "bad" ? intent.body : "");
        if (els.serverImportButton) {
          els.serverImportButton.disabled = intent.tone === "bad" || state.importer.loading;
          const disabledLabel = els.serverImportDisabled?.checked !== false ? "disabled" : "as saved";
          els.serverImportButton.textContent = dryRun ? "Preview import" : `Import ${disabledLabel}`;
        }
      }

      function serverSensitivity(server, risk) {
        const fields = `${server.effectClass || ""} ${server.stateClass || ""} ${server.credentialBinding || ""} ${server.scopeClass || ""}`.toLowerCase();
        if (/credential|external|remote|stateful|session|write|host/.test(fields)) return "Guarded";
        if (risk.rank <= 3) return "Unchecked";
        return "Low";
      }

      function serverFacts(server, risk, related) {
        const recommendation = recommendedPolicy(server, related);
        const tuned = !policyNeedsTuning(server, related, recommendation);
        const evidence = serverToolEvidence(server);
        const launch = launchCommand(server);
        const launchFact = launch ? `<span class="fact"><strong>Launch</strong> ${escapeHtml(shortText(launch, 72))}</span>` : "";
        const toolLabel = evidence.checked
          ? evidence.ok
            ? `${evidence.toolCount} listed`
            : "failed"
          : "not checked";
        if (!server.effectiveEnabled) {
          return [
            `<span class="fact"><strong>Status</strong> ${profileSourceMismatch(server) ? "Needs setup" : "Off"}</span>`,
            `<span class="fact"><strong>Tools</strong> ${escapeHtml(toolLabel)}</span>`,
            `<span class="fact"><strong>Source</strong> ${server.sourceEnabled ? "on" : "off"}</span>`,
            launchFact,
            `<span class="fact"><strong>Policy</strong> ${escapeHtml(serverSensitivity(server, risk))}</span>`
          ].filter(Boolean).join("");
        }
        return [
          `<span class="fact"><strong>Status</strong> On</span>`,
          `<span class="fact"><strong>Tools</strong> ${escapeHtml(toolLabel)}</span>`,
          launchFact,
          `<span class="fact"><strong>Policy</strong> ${escapeHtml(serverSensitivity(server, risk))}</span>`,
          tuned ? "" : `<span class="fact"><strong>Fix</strong> ${escapeHtml(recommendation.label)}</span>`
        ].filter(Boolean).join("");
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
              <button class="primary" type="button" data-server-name="${name}" data-server-action="settings">Review</button>
              <button class="server-toggle off" type="button" aria-pressed="false" data-server-name="${name}" data-server-action="enable-test">Enable &amp; test</button>
            `;
          }
          if (needsTuning) {
            return `
              <button class="primary" type="button" data-server-name="${name}" data-server-action="auto">${escapeHtml(applyLabel)}</button>
              <button type="button" data-server-name="${name}" data-server-action="test">${escapeHtml(testLabel)}</button>
              <button class="quiet" type="button" data-server-name="${name}" data-server-action="settings">Details</button>
            `;
          }
          return `
            <button class="primary" type="button" data-server-name="${name}" data-server-action="test">${escapeHtml(testLabel)}</button>
            <button class="server-toggle on" type="button" aria-pressed="true" data-server-name="${name}" data-server-action="toggle">Turn off</button>
            <button class="quiet" type="button" data-server-name="${name}" data-server-action="settings">Details</button>
          `;
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
        return servers.map(server => {
          const risk = riskForServer(server, groups.get(server.name) || []);
          const policy = server.runtimeType === "stateless"
            ? `Can share safely · ${server.effectClass || "read-only"}`
            : `${server.runtimeType || "unknown"} · ${server.concurrencyPolicy || "routing unknown"}`;
          return { server, risk, policy };
        }).sort((a, b) => a.risk.rank - b.risk.rank || String(a.server.name || "").localeCompare(String(b.server.name || "")));
      }

      function shortNames(rows, limit = 6) {
        const names = rows.map(row => row.server?.name || row.serverName || row.name || "server");
        const visible = names.slice(0, limit).join(", ");
        return names.length > limit ? `${visible}, +${names.length - limit} more` : visible;
      }

      function buildAttentionItems(hub, readiness, policyRows, leases) {
        const items = [];
        const hubWarnings = Array.isArray(hub.warnings) ? hub.warnings.map(String).filter(Boolean) : [];
        if (hubWarnings.length) {
          items.push({
            title: "Runtime state can be repaired",
            meta: `${hubWarnings.length} warning${hubWarnings.length === 1 ? "" : "s"}. Use Repair if runtime status looks stale; server auto policy is already handled separately.`,
            tone: "warn",
            tag: "repair"
          });
        }
        if (Array.isArray(readiness.missingRequiredSourceEnablement)) {
          for (const name of readiness.missingRequiredSourceEnablement) {
            items.push({ title: "Required source is disabled", meta: String(name), tone: "bad", tag: "required" });
          }
        }
        if (Array.isArray(readiness.missingRequiredCommands)) {
          for (const command of readiness.missingRequiredCommands) {
            items.push({ title: "Required command is missing", meta: String(command), tone: "bad", tag: "setup" });
          }
        }

        const activePolicyRows = policyRows.filter(row => row.server?.effectiveEnabled || row.risk.rank <= 1);
        const requiredDisabled = activePolicyRows.filter(row => row.risk.rank === 1);

        if (requiredDisabled.length) {
          items.push({
            title: `${requiredDisabled.length} server${requiredDisabled.length === 1 ? "" : "s"} need source/profile setup`,
            meta: `${shortNames(requiredDisabled)}. Fix enablement, then run Test to collect tools/list evidence.`,
            tone: "bad",
            tag: "setup"
          });
        }
        const seen = new Set();
        return items.filter(item => {
          const key = `${item.title}\n${item.meta}`;
          if (seen.has(key)) return false;
          seen.add(key);
          return true;
        });
      }

      function updateRefreshChip() {
        let label = "manual";
        let tone = "warn";
        if (state.refreshing) { label = "refreshing"; tone = "warn"; }
        else if (state.lastError) { label = "refresh failed"; tone = "bad"; }
        else if (state.refreshMode === "paused") { label = "paused"; tone = "warn"; }
        else if (document.visibilityState === "hidden") { label = "background"; tone = "warn"; }
        else { label = `auto ${Math.round((REFRESH_MS[state.refreshMode] || 0) / 1000)}s`; tone = "good"; }
        setChip(els.refreshChip, label, tone);
      }

      function scheduleRefresh() {
        if (state.timer !== null) window.clearTimeout(state.timer);
        const base = REFRESH_MS[state.refreshMode] || 0;
        if (!base) { state.timer = null; updateRefreshChip(); return; }
        let delay = document.visibilityState === "hidden" || state.lifecycle.frozen ? Math.max(base, HIDDEN_REFRESH_MS) : base;
        if (state.lastError && state.failureCount > 0) {
          const backoff = Math.min(MAX_REFRESH_FAILURE_BACKOFF_MS, 1000 * Math.pow(2, Math.min(state.failureCount, 7)));
          delay = Math.max(delay, backoff);
        }
        state.timer = window.setTimeout(() => refreshDashboard({ reason: "auto" }), delay);
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
        if (typeof AbortSignal !== "undefined" && typeof AbortSignal.timeout === "function") {
          return AbortSignal.timeout(ms);
        }
        if (typeof AbortController === "undefined") return null;
        const controller = new AbortController();
        window.setTimeout(() => controller.abort(new DOMException("Request timed out", "TimeoutError")), ms);
        return controller.signal;
      }

      function combineSignals(signals) {
        const active = signals.filter(Boolean);
        if (!active.length) return null;
        if (typeof AbortSignal !== "undefined" && typeof AbortSignal.any === "function") {
          return AbortSignal.any(active);
        }
        if (typeof AbortController === "undefined") return active[0];
        const controller = new AbortController();
        const abort = event => {
          if (!controller.signal.aborted) controller.abort(event?.target?.reason || new DOMException("Request aborted", "AbortError"));
        };
        for (const signal of active) {
          if (signal.aborted) { abort({ target: signal }); break; }
          signal.addEventListener("abort", abort, { once: true });
        }
        return controller.signal;
      }

      function apiErrorMessage(error) {
        if (!error) return "Request failed";
        if (error.name === "TimeoutError") return "Request timed out";
        if (error.name === "AbortError") return "Request aborted";
        return error.message || String(error);
      }

      async function fetchJson(url, options = {}) {
        const { timeoutMs = REQUEST_TIMEOUT_MS, headers = {}, ...fetchOptions } = options;
        const signal = combineSignals([fetchOptions.signal, timeoutMs > 0 ? timeoutSignal(timeoutMs) : null]);
        const response = await fetch(url, {
          ...fetchOptions,
          signal: signal || fetchOptions.signal,
          cache: fetchOptions.cache || "no-store",
          headers: { accept: "application/json", ...headers }
        });
        const raw = await response.text();
        let payload = null;
        if (raw) {
          try { payload = JSON.parse(raw); } catch (_) { payload = { error: raw }; }
        }
        if (!response.ok) {
          const message = payload?.error?.message || payload?.error || `${response.status} ${response.statusText}`;
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
          return { ok: true, value, ms: Math.round(performance.now() - started), url, at: Date.now() };
        } catch (error) {
          return { ok: false, error, ms: Math.round(performance.now() - started), url, at: Date.now() };
        }
      }

      async function refreshDashboard(options = {}) {
        if ((document.visibilityState === "hidden" || state.lifecycle.frozen) && !options.force && !options.allowHidden) { scheduleRefresh(); return; }
        if (state.refreshing && !options.forceAbort) { scheduleRefresh(); return; }
        const now = Date.now();
        if (options.reason === "visible" && state.lastRefreshFinishedAt && now - state.lastRefreshFinishedAt < VISIBLE_REFRESH_MIN_INTERVAL_MS) { scheduleRefresh(); return; }
        const seq = state.seq + 1;
        state.seq = seq;
        if (state.controller) {
          if (options.forceAbort) state.controller.abort();
          else { scheduleRefresh(); return; }
        }
        const controller = typeof AbortController !== "undefined" ? new AbortController() : null;
        state.controller = controller;
        state.lastRefreshStartedAt = now;
        setBusy(true);
        try {
          const request = controller ? { signal: controller.signal } : {};
          const overviewUrl = options.force ? "/api/overview?refresh=1" : "/api/overview";
          const [overviewResult, logsResult, resourcesResult] = await Promise.allSettled([
            timedFetchJson(overviewUrl, request),
            timedFetchJson("/api/logs?tail=40", request),
            timedFetchJson("/api/resources", request)
          ]);
          if (seq !== state.seq) return;
          const overviewCheck = overviewResult.status === "fulfilled" ? overviewResult.value : { ok: false, error: overviewResult.reason, ms: 0, url: overviewUrl, at: Date.now() };
          const logsCheck = logsResult.status === "fulfilled" ? logsResult.value : { ok: false, error: logsResult.reason, ms: 0, url: "/api/logs?tail=40", at: Date.now() };
          const resourcesCheck = resourcesResult.status === "fulfilled" ? resourcesResult.value : { ok: false, error: resourcesResult.reason, ms: 0, url: "/api/resources", at: Date.now() };
          state.backend.overview = overviewCheck;
          state.backend.logs = logsCheck;
          state.backend.resources = resourcesCheck;
          state.backend.checkedAt = Date.now();
          if (!overviewCheck.ok) throw overviewCheck.error;
          state.overview = overviewCheck.value;
          state.lastSuccessAt = Date.now();
          state.failureCount = 0;
          if (logsCheck.ok) state.logs = Array.isArray(logsCheck.value) ? logsCheck.value : [];
          state.lastError = logsCheck.ok && resourcesCheck.ok ? null : [
            logsCheck.ok ? "" : `Logs: ${apiErrorMessage(logsCheck.error)}`,
            resourcesCheck.ok ? "" : `Resources: ${apiErrorMessage(resourcesCheck.error)}`
          ].filter(Boolean).join(" · ");
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
        setSignalTones([[els.systemState, "bad"], [els.attentionCount, "bad"], [els.serverCount, "warn"], [els.loadState, "bad"]]);
        els.systemState.textContent = "Degraded";
        els.systemNote.textContent = `Dashboard refresh failed: ${message}`;
        if (els.opsDot) els.opsDot.className = "dot bad";
        if (els.opsTitle) els.opsTitle.textContent = "Dashboard backend is not connected";
        if (els.opsBody) els.opsBody.textContent = `Last refresh failed: ${message}`;
        if (els.opsCommandRow) {
          els.opsCommandRow.innerHTML = [
            `<button class="primary" type="button" data-global-action="start-hub">Start hub</button>`,
            `<button type="button" data-global-action="check-link">Check link</button>`,
            `<button class="quiet" type="button" data-global-action="refresh">Refresh overview</button>`
          ].join("");
        }
        if (els.opsSteps) els.opsSteps.innerHTML = [
          stepCard("Backend offline", message, "bad"),
          stepCard("Runtime unknown", "No fresh overview", "warn"),
          stepCard("Actions not verified", "Use Check link after backend is reachable", "warn")
        ].join("");
        renderDecisionRunway({
          overview: { userReadiness: { confidence: 0, primaryAction: "Connect backend", endpoint: "/mcp" } },
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
          warnPolicies: 0
        });
        renderBaseSetup({ overview: { userReadiness: { endpoint: "/mcp" } }, hub: { status: "offline" }, servers: [], clients: [], runtimeReady: false });
        renderAccessReview({ status: "bad", title: "Access review paused", body: "Reconnect /api/overview before trusting tool permissions, secrets, or remote origins.", counts: { servers: 0, enabled: 0 }, items: [
          { label: "Approval", count: 0, status: "warn", body: "Backend offline; approval state is unknown." },
          { label: "Secrets", count: 0, status: "warn", body: "Secret values remain hidden while offline." },
          { label: "Remote/Auth", count: 0, status: "warn", body: "Remote origins require live overview." },
          { label: "Evidence", count: 0, status: "bad", body: "No tools/list evidence while backend is offline." }
        ] }, []);
        renderNextAction({
          overview: { userReadiness: { confidence: 0, primaryAction: "Connect backend", endpoint: "/mcp" } },
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
          warnPolicies: 0
        });
        renderConnectionMap({ userReadiness: { confidence: 0, endpoint: "/mcp" }, hub: { status: "offline" }, readiness: { runtimePrerequisitesReady: false } }, [], []);
        renderProtocolCompatibility({ userReadiness: { confidence: 0, endpoint: "/mcp" }, hub: { status: "offline" }, readiness: { runtimePrerequisitesReady: false } }, [], []);
        renderSetupQueue({ overview: {}, hub: { status: "offline" }, servers: [], clients: [], instances: [], attentionItems: [{ title: "Backend offline" }], attentionTotal: 1, runtimeReady: false });
        if (els.backendState) els.backendState.textContent = "Backend not connected";
        if (els.backendGrid) els.backendGrid.innerHTML = [
          readout("/api/overview", "failed", message, "bad"),
          readout("/api/logs", "unknown", "overview failed first", "warn"),
          readout("/api/resources", state.backend.resources?.ok ? "ok" : "unknown", state.backend.resources ? `${fmtMs(state.backend.resources.ms)} · ${fmtDate(state.backend.resources.at)}` : "waiting", state.backend.resources?.ok ? "good" : "warn"),
          readout("action ping", state.backend.action?.ok ? "ok" : "not checked", state.backend.action ? `${fmtMs(state.backend.action.ms)} · ${fmtDate(state.backend.action.at)}` : "waiting", state.backend.action?.ok ? "good" : "warn")
        ].join("");
        els.loadState.textContent = "Failed";
        els.loadNote.textContent = "Backend overview request failed.";
        els.attentionCount.textContent = "1";
        els.attentionNote.textContent = "Refresh failed.";
        els.attentionList.innerHTML = `<article class="item bad"><div class="item-head"><div class="name">Dashboard refresh failed</div>${chip("error", "bad")}</div><div class="meta">${escapeHtml(message)}</div></article>`;
        if (els.serverCommandCenter) {
          setCardTone(els.serverCommandCenter, "bad");
          if (els.serverCommandTitle) els.serverCommandTitle.textContent = "Server fleet is not trustworthy without live backend state.";
          if (els.serverCommandBody) els.serverCommandBody.textContent = "Reconnect /api/overview before applying policy, testing servers, or interpreting inventory counts.";
          if (els.serverMetricRow) els.serverMetricRow.innerHTML = [
            fleetMetric("Visible", "—", "backend offline", "bad"),
            fleetMetric("Evidence", "—", "not loaded", "warn"),
            fleetMetric("Policy", "—", "not loaded", "warn"),
            fleetMetric("Capacity", "—", "not loaded", "warn")
          ].join("");
        }
        if (els.serverWorkbench) els.serverWorkbench.innerHTML = `<div class="workbench-summary"><span class="workbench-index">!</span><div><strong>Reconnect before tuning.</strong><p>Server actions stay available, but the safest path is Start hub → Check link → Refresh overview.</p></div></div>`;
        if (els.serverList) els.serverList.innerHTML = `<div class="empty-state bad"><strong>Server list is paused.</strong><p>${escapeHtml(message)}</p><div class="empty-actions"><button class="primary" type="button" data-empty-action="check-link">Check link</button><button type="button" data-empty-action="refresh">Refresh</button></div></div>`;
        updateRefreshChip();
      }

      async function runAction(path, button, confirmMessage, busyLabel) {
        if (confirmMessage && !window.confirm(confirmMessage)) return;
        const original = button?.textContent || "";
        try {
          if (button) { button.disabled = true; button.textContent = busyLabel || "Working…"; }
          const result = await timedFetchJson(path, { method: "POST", timeoutMs: ACTION_TIMEOUT_MS });
          state.backend.action = {
            ok: result.ok,
            ms: result.ms,
            at: result.at,
            endpoint: path.replace("/api/actions/", ""),
            error: result.ok ? "" : apiErrorMessage(result.error)
          };
          if (!result.ok) throw result.error;
          await refreshDashboard({ force: true, reason: "action" });
        } catch (error) {
          state.lastError = apiErrorMessage(error);
          renderError(state.lastError);
        } finally {
          if (button) { button.disabled = false; button.textContent = original; }
        }
      }

      function render() {
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
        const attentionItems = buildAttentionItems(hub, readiness, policyRows, leases);
        const activePolicyRows = policyRows.filter(row => row.server?.effectiveEnabled || row.risk.rank <= 1);
        const active = num(http.activeConnections);
        const max = num(http.maxConnections);
        const failed = num(http.failedConnections);
        const completed = num(http.completedConnections) || num(http.acceptedConnections);
        const saturation = max ? Math.round((active / max) * 100) : 0;
        const enabledCount = servers.filter(server => server.effectiveEnabled).length;
        const requiredCount = servers.filter(server => server.required).length;
        const statelessCount = servers.filter(server => server.runtimeType === "stateless").length;
        const statefulCount = servers.filter(server => ["stateful", "external", "interactive", "side-effecting"].includes(server.runtimeType)).length;
        const needsTrustCount = activePolicyRows.filter(row => row.risk.rank === 2).length;
        const badPolicies = activePolicyRows.filter(row => row.risk.tone === "bad").length;
        const warnPolicies = activePolicyRows.filter(row => row.risk.tone === "warn").length;
        const attentionTotal = badPolicies + attentionItems.filter(item => item.tone !== "bad").length;

        const runtimeReady = Boolean(readiness.runtimePrerequisitesReady ?? hub.readyForRuntimeOps);
        const systemTone = !runtimeReady ? "bad" : attentionItems.length ? "warn" : "good";
        document.body.dataset.systemTone = systemTone;
        setSignalTones([
          [els.systemState, systemTone],
          [els.attentionCount, attentionTotal ? systemTone : "good"],
          [els.serverCount, servers.length ? (enabledCount ? "good" : "warn") : "warn"],
          [els.loadState, state.backend.overview?.ok ? msTone(state.backend.overview.ms) : "bad"]
        ]);
        els.systemState.textContent = !runtimeReady ? "Blocked" : attentionItems.length ? "Needs action" : "Ready";
        els.systemNote.innerHTML = `${escapeHtml(hub.status || hub.health || "unknown")} · updated ${escapeHtml(fmtDate(overview.generatedAtMs))}`;
        els.attentionCount.textContent = String(attentionTotal);
        els.attentionNote.textContent = attentionItems.length ? `${badPolicies} blocker(s) · ${warnPolicies} guarded` : "No active action.";
        els.serverCount.textContent = `${enabledCount}/${servers.length}`;
        els.serverNote.textContent = `${needsTrustCount} guarded · ${statefulCount} stateful/effectful`;
        const overviewMs = state.backend.overview?.ok ? fmtMs(state.backend.overview.ms) : "offline";
        els.loadState.textContent = state.backend.overview?.ok ? overviewMs : "Check";
        els.loadNote.textContent = `API ${state.backend.overview?.ok ? "connected" : "not connected"} · ${active}/${max || "?"} active HTTP`;

        document.body.dataset.density = state.density;
        const bucketLabel = state.bucket === "all" ? (state.enabledOnly ? "enabled only" : "all servers") : `${state.bucket} servers`;
        setChip(els.serverFilterChip, bucketLabel, state.bucket === "blocked" ? "bad" : state.bucket === "protected" || state.bucket === "off" ? "warn" : "good");
        els.toggleEnabled.textContent = state.enabledOnly ? "Show All" : "Show Enabled";
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
          warnPolicies
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
          warnPolicies
        });
        renderBaseSetup({
          overview,
          hub,
          servers,
          clients,
          runtimeReady
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
          warnPolicies
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
        renderSetupQueue({ overview, hub, servers, clients, instances, attentionItems, attentionTotal, runtimeReady });
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
        renderSummaryChips(enabledCount, requiredCount, statelessCount, statefulCount, needsTrustCount, servers.length);
        if (els.serverDialog?.open && state.selectedServer) renderServerDialogByName(state.selectedServer);
      }

      function renderAttention(items, systemTone) {
        if (!items.length) {
          els.attentionList.innerHTML = `<article class="item good"><div class="item-head"><div class="name">Nothing needs attention</div>${chip("ready", "good")}</div><div class="meta">Runtime, routing, and server inventory have no visible blockers. Advanced diagnostics can stay closed.</div></article>`;
          return;
        }
        els.attentionList.innerHTML = items.slice(0, 5).map(item => `
          <article class="${itemClass(item.tone || systemTone)}">
            <div class="item-head"><div class="name">${escapeHtml(item.title)}</div>${chip(item.tag || "attention", item.tone || systemTone)}</div>
            <div class="meta">${escapeHtml(item.meta)}</div>
          </article>
        `).join("") + (items.length > 5 ? `<div class="note">${items.length - 5} more grouped item(s) in Advanced diagnostics.</div>` : "");
      }

      function renderSummaryChips(enabledCount, requiredCount, statelessCount, statefulCount, needsTrustCount, total) {
        const offCount = Math.max(total - enabledCount, 0);
        const lowRiskCount = Math.max(enabledCount - needsTrustCount, 0);
        els.serverChips.innerHTML = [
          chip(`${enabledCount} on`, enabledCount ? "good" : "warn"),
          chip(`${offCount} parked`, offCount ? "warn" : "good"),
          chip(`${needsTrustCount} guarded`, needsTrustCount ? "warn" : "good"),
          chip(`${lowRiskCount} low-risk`, lowRiskCount ? "good" : "warn")
        ].join("");
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

      function decisionCard(label, value, meta, tone = "warn", progress = 0, icon = "•") {
        const bounded = Math.max(0, Math.min(100, Math.round(progress)));
        return `<article class="decision-card ${escapeHtml(tone)}">
          <span class="decision-icon">${escapeHtml(icon)}</span>
          <div>${chip(label, tone)}<strong>${escapeHtml(value)}</strong><p>${escapeHtml(meta)}</p></div>
          <div class="decision-meter" aria-hidden="true"><span style="width: ${bounded}%"></span></div>
        </article>`;
      }



      function baseStepCard(index, step = {}, currentKey = "") {
        const safeTone = ["good", "warn", "bad"].includes(String(step.tone)) ? String(step.tone) : "warn";
        const key = String(step.key || step.label || index).replace(/[^a-z0-9-]/gi, "").toLowerCase() || `step-${index}`;
        const isCurrent = Boolean(currentKey && key === currentKey);
        const currentClass = isCurrent ? " active" : "";
        const currentAttr = isCurrent ? ' aria-current="step"' : "";
        const label = text(step.label, `Step ${index}`);
        const action = String(step.action || "").trim();
        const actionLabel = text(step.actionLabel, labelForBaseStepAction(action, key));
        const actionButton = action && isCurrent ? `<button type="button" data-global-action="${escapeHtml(action)}" aria-label="${escapeHtml(actionLabel)} for ${escapeHtml(label)}">${escapeHtml(actionLabel)}</button>` : "";
        return `<article class="base-step ${safeTone}${currentClass}"${currentAttr} data-base-step="${escapeHtml(key)}"><span>${String(index).padStart(2, "0")}</span><strong>${escapeHtml(step.title || label)}</strong><p>${escapeHtml(step.body || "")}</p>${actionButton}</article>`;
      }

      function labelForBaseStepAction(action, key = "") {
        if (action === "repair") return "Repair";
        if (action === "check-link") return "Check link";
        if (action === "refresh") return "Refresh";
        if (action === "clients" || action === "client") return key === "client" ? "Connect" : "Open";
        if (action === "import-server") return "Import";
        if (action === "add-server") return "Add";
        if (action === "servers") {
          if (key === "tools") return "Run test";
          if (key === "routing") return "Review";
          if (key === "source") return "Open sources";
          return "Open servers";
        }
        if (action === "discover") return "Discover";
        return action ? "Open" : "";
      }

      function normalizeFoundationAction(action, fallback = "refresh") {
        const safeAction = String(action?.action || fallback || "refresh").replace(/[^a-z-]/g, "");
        return {
          label: text(action?.label, humanizeKey(safeAction || fallback || "refresh")),
          action: safeAction || fallback || "refresh"
        };
      }

      function normalizeFoundationStep(step, index) {
        const tone = ["good", "warn", "bad"].includes(String(step?.status || step?.tone)) ? String(step.status || step.tone) : "warn";
        const key = String(step?.key || step?.label || `step-${index + 1}`).replace(/[^a-z0-9-]/gi, "").toLowerCase();
        const action = String(step?.action || "").trim();
        return {
          tone,
          key,
          label: text(step?.label || step?.key, `Step ${index + 1}`),
          title: text(step?.title, step?.label || `Step ${index + 1}`),
          body: text(step?.body, "No detail reported."),
          action,
          actionLabel: text(step?.actionLabel, labelForBaseStepAction(action, key))
        };
      }

      function buildFoundationModelFromOverview(foundation) {
        if (!foundation || !Array.isArray(foundation.steps) || !foundation.steps.length) return null;
        const steps = foundation.steps.slice(0, 5).map(normalizeFoundationStep);
        const done = Math.min(num(foundation.complete, steps.filter(step => step.tone === "good").length), steps.length);
        const pct = Math.max(0, Math.min(100, num(foundation.progressPct, Math.round((done / steps.length) * 100))));
        const tone = ["good", "warn", "bad"].includes(String(foundation.status)) ? String(foundation.status) : steps.some(step => step.tone === "bad") ? "bad" : done === steps.length ? "good" : "warn";
        const rawActions = Array.isArray(foundation.actions) && foundation.actions.length
          ? foundation.actions.map((action, index) => normalizeFoundationAction(action, index ? "servers" : "refresh"))
          : [{ label: "Refresh", action: "refresh" }, { label: "Import", action: "import-server" }, { label: "Client", action: "clients" }, { label: "Servers", action: "servers" }];
        const actions = [];
        const seenActions = new Set();
        for (const action of rawActions) {
          if (!action.action || seenActions.has(action.action)) continue;
          seenActions.add(action.action);
          actions.push(action);
          if (actions.length >= 4) break;
        }
        const nextStep = foundation.nextStep ? normalizeFoundationStep(foundation.nextStep, done) : steps.find(step => step.tone !== "good") || { label: "Ready", title: "Base setup is ready", body: "Normal use can stay on the server rows.", tone: "good", action: "servers" };
        return {
          steps,
          done,
          pct,
          tone,
          blocked: tone === "bad",
          stateKey: text(foundation.stateKey, nextStep.key || "unknown"),
          nextStepKey: text(foundation.nextStepKey || foundation.nextStep?.key, nextStep.key || "ready"),
          title: text(foundation.title, done === steps.length ? "Base setup is ready" : "Finish base setup"),
          body: text(foundation.body, "Start with backend, client, source, tools, and routing before opening advanced controls."),
          actions,
          nextStep,
          primaryAction: actions[0] || { label: "Refresh", action: "refresh" },
          secondaryAction: actions[3] || actions[1] || { label: "Servers", action: "servers" },
          safety: normalizeFoundationSafety(foundation.safety || {})
        };
      }

      function buildBaseSetupModel(context = {}) {
        return setupFoundationModel(context);
      }

      function isLoopbackUrl(value) {
        return /^https?:\/\/(localhost|127\.0\.0\.1|\[::1\])(?::\d+)?(?:[/?#]|$)/i.test(String(value || ""));
      }

      function isRemoteServer(server = {}) {
        const url = String(server.sourceUrl || server.url || "").trim();
        return /^https?:\/\//i.test(url) && !isLoopbackUrl(url);
      }

      function hasSecretBoundary(server = {}) {
        const credential = String(server.credentialBinding || "").toLowerCase();
        const envNames = Array.isArray(server.sourceEnvNames) ? server.sourceEnvNames : [];
        const headerNames = Array.isArray(server.sourceHeaderNames) ? server.sourceHeaderNames : [];
        return envNames.length > 0 || headerNames.length > 0 || /credential|secret|token|api[-_ ]?key|oauth|auth|header|env/.test(credential);
      }

      function normalizeFoundationSafety(safety = {}) {
        const counts = safety && typeof safety.counts === "object" ? safety.counts : {};
        const tone = ["good", "warn", "bad"].includes(String(safety.status)) ? String(safety.status) : "warn";
        return {
          tone,
          title: text(safety.title, "Review source, evidence, and secrets."),
          body: text(safety.body, "Keep new imports parked. Review first. Enable deliberately, then run Test."),
          counts: {
            unchecked: num(counts.enabledWithoutEvidence ?? counts.unchecked, 0),
            remote: num(counts.remoteSources ?? counts.remote, 0),
            secretBearing: num(counts.secretBearingSources ?? counts.secretBearing, 0)
          }
        };
      }

      function renderBaseSafety(safety = {}) {
        if (!els.baseSafety) return;
        const model = normalizeFoundationSafety(safety);
        els.baseSafety.dataset.tone = model.tone;
        if (els.baseSafetyTitle) els.baseSafetyTitle.textContent = model.title;
        if (els.baseSafetyBody) els.baseSafetyBody.textContent = model.body;
        if (els.baseSafetyGrid) {
          els.baseSafetyGrid.innerHTML = [
            { label: `${model.counts.unchecked} unchecked`, tone: model.counts.unchecked ? "warn" : "good" },
            { label: `${model.counts.remote} remote`, tone: model.counts.remote ? "warn" : "good" },
            { label: `${model.counts.secretBearing} secret-bearing`, tone: model.counts.secretBearing ? "warn" : "good" }
          ].map(item => chip(item.label, item.tone)).join("");
        }
      }

      function setupFoundationModel(context = {}) {
        const overview = context.overview || {};
        const backendOwned = buildFoundationModelFromOverview(overview.dashboardFoundation);
        if (backendOwned) return backendOwned;
        const hub = context.hub || {};
        const servers = Array.isArray(context.servers) ? context.servers : [];
        const clients = Array.isArray(context.clients) ? context.clients : [];
        const clientCatalog = overview.clients || {};
        const backendOk = Boolean(state.backend.overview?.ok);
        const runtimeReady = Boolean(context.runtimeReady);
        const enabled = servers.filter(server => server?.effectiveEnabled).length;
        const parked = Math.max(servers.length - enabled, 0);
        const tested = servers.filter(server => serverToolEvidence(server).checked).length;
        const usable = servers.some(server => server?.effectiveEnabled && serverToolEvidence(server).checked && serverToolEvidence(server).ok !== false);
        const policyPlan = autoPolicyPlan(servers, currentInstances());
        const riskyEnabled = servers.filter(server => {
          const evidence = serverToolEvidence(server);
          const risk = riskForServer(server, []);
          return server?.effectiveEnabled && risk.rank <= 3 && (!evidence.checked || evidence.ok === false);
        }).length;
        const routingSafe = Boolean(runtimeReady && enabled > 0 && usable && !policyPlan.changes.length && !riskyEnabled);
        const routingIssue = !runtimeReady
          ? "Runtime prerequisites are not ready, so routing should stay conservative."
          : !servers.length
            ? "Routing becomes meaningful after at least one source is saved."
            : !enabled
              ? "Saved sources are still parked. Review one, enable deliberately, then run Test before use."
              : !usable
                ? "Keep routing conservative until Test creates tools/list evidence."
                : `${policyPlan.changes.length} policy fix${policyPlan.changes.length === 1 ? "" : "es"} · ${riskyEnabled} risky enabled.`;
        const endpoint = overview.userReadiness?.endpoint || hub.endpoint || overview.publicMcpUrl || "/mcp";
        const localTargets = clients.filter(client => String(client?.surfaceClass || client?.clientTargetSurfaceClass || "").toLowerCase() === "local");
        const patchableClients = localTargets.filter(client => client?.installSupported || client?.clientInstallImplemented || client?.installSupport);
        const configuredClientKey = String(clientCatalog?.configuredClientKeyName || "").trim();
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
          { tone: backendOk ? "good" : "bad", key: "backend", label: "Backend", title: backendOk ? "Backend online" : "Connect backend", body: backendOk ? `${fmtMs(state.backend.overview?.ms)} · /api/overview responded. Runtime is checked before use.` : "Start hub or check /api/overview before changing config.", action: backendOk ? "refresh" : "check-link", actionLabel: backendOk ? "Refresh" : "Check link" },
          { tone: clientTone, key: "client", label: "Client", title: clientTitle, body: clientBody, action: "clients", actionLabel: clientReady ? "Open client" : "Connect" },
          { tone: servers.length ? "good" : "warn", key: "source", label: "Source", title: servers.length ? `${servers.length} source${servers.length === 1 ? "" : "s"} saved` : "Add one source", body: servers.length ? `${enabled} on · ${parked} parked.` : "Import existing config first; otherwise discover or add manually.", action: servers.length ? "servers" : "import-server", actionLabel: servers.length ? "Open sources" : "Import" },
          { tone: usable ? "good" : servers.length ? "warn" : "warn", key: "tools", label: "Tools", title: usable ? "One tools path tested" : servers.length ? "Run Test" : "Test after adding", body: usable ? `${tested}/${servers.length} source${servers.length === 1 ? "" : "s"} have initialize/tools evidence.` : "Keep sources parked until reviewed; after enabling, run Test before use.", action: "servers", actionLabel: usable ? "Open tools" : "Run test" },
          { tone: routingSafe ? "good" : "warn", key: "routing", label: "Routing", title: routingSafe ? "Routing conservative" : !runtimeReady ? "Repair runtime" : !enabled ? "Enable one source" : "Review routing", body: routingSafe ? "Tools evidence exists and no obvious safe-policy fix is waiting." : routingIssue, action: runtimeReady ? "servers" : "repair", actionLabel: runtimeReady ? (routingSafe ? "Open routing" : !enabled ? "Enable" : "Review") : "Repair" }
        ];
        const done = steps.filter(step => step.tone === "good").length;
        const blocked = steps.some(step => step.tone === "bad");
        const pct = Math.round((done / steps.length) * 100);
        const tone = blocked ? "bad" : done === steps.length ? "good" : "warn";
        const nextStep = steps.find(step => step.tone !== "good") || { key: "ready", label: "Ready", title: "Base setup is ready", body: "Normal use can stay on the server rows.", tone: "good", action: "servers", actionLabel: "Open servers" };
        const titleByStep = {
          backend: "Start with the local backend",
          client: "Connect a local client",
          source: "Bring in one MCP server source",
          tools: "Test tools before trust",
          routing: !runtimeReady ? "Repair runtime before use" : !enabled ? "Enable one reviewed source" : "Review conservative routing",
          ready: "The base is ready"
        };
        const bodyByStep = {
          backend: "The safest base path is: start hub, check the backend link, then refresh. Do not read server state as final while the backend is offline.",
          client: "Choose a supported local client, preview its patch, then apply only after the diff looks right.",
          source: "Use an existing MCP config when you have one. Imported sources stay parked by default; enable intentionally, then test before normal use.",
          tools: "A saved server is not the same as a usable server. Run Test to collect initialize and tools/list evidence first.",
          routing: !runtimeReady ? "Runtime prerequisites are a use-boundary problem. Repair them after client, source, and tool setup are clear." : !enabled ? "Saved sources are parked. Review one, enable it deliberately, then run Test before normal routing." : "Keep normal users on the safe path: apply conservative policy fixes before changing worker counts or trusting guarded sources.",
          ready: "Normal use can stay simple: use the configured client, and open Details only for overrides or diagnostics."
        };
        const title = titleByStep[nextStep.key] || "Finish base setup";
        const body = bodyByStep[nextStep.key] || nextStep.body || "Finish the next basic step before opening advanced controls.";
        const primaryAction = { label: text(nextStep.actionLabel, "Open"), action: text(nextStep.action, "refresh") };
        const secondaryAction = !servers.length ? { label: "Add manually", action: "add-server" } : { label: "Servers", action: "servers" };
        const foundationSafety = normalizeFoundationSafety({
          status: riskyEnabled || servers.some(server => isRemoteServer(server)) ? "warn" : "good",
          counts: {
            enabledWithoutEvidence: riskyEnabled,
            remoteSources: servers.filter(server => isRemoteServer(server)).length,
            secretBearingSources: servers.filter(server => hasSecretBoundary(server)).length
          }
        });
        const actions = [primaryAction, { label: "Import config", action: "import-server" }, { label: "Connect client", action: "clients" }, secondaryAction];
        return { steps, done, blocked, pct, tone, stateKey: nextStep.key, nextStepKey: nextStep.key, title, body, actions, nextStep, primaryAction, secondaryAction, safety: foundationSafety };
      }

      function renderBaseSetup(context = {}) {
        if (!els.baseStepGrid) return;
        const model = buildBaseSetupModel(context);
        setChip(els.baseStateChip, model.done === model.steps.length ? "ready" : model.blocked ? "blocked" : `${model.done}/5 basics`, model.tone);
        setCardTone(els.baseSetup, model.tone);
        if (els.baseSetup) {
          els.baseSetup.dataset.foundationState = model.stateKey || "unknown";
          els.baseSetup.dataset.nextStep = model.nextStepKey || model.nextStep?.key || "ready";
        }
        if (els.baseBody) els.baseBody.textContent = model.body;
        if (els.baseProgressFill) els.baseProgressFill.style.width = `${model.pct}%`;
        if (els.baseProgressLabel) els.baseProgressLabel.textContent = `${model.done} of 5 basics complete. Next: ${text(model.nextStep?.title, model.title)}.`;
        const currentBaseStepKey = String(model.nextStepKey || model.nextStep?.key || "").replace(/[^a-z0-9-]/gi, "").toLowerCase();
        els.baseStepGrid.innerHTML = model.steps.map((step, index) => baseStepCard(index + 1, step, currentBaseStepKey)).join("");
        renderBaseSafety(model.safety || {});
        if (els.baseActionRow) {
          const actions = (Array.isArray(model.actions) && model.actions.length ? model.actions : [
            model.primaryAction,
            { label: "Import config", action: "import-server" },
            { label: "Connect client", action: "clients" },
            model.secondaryAction
          ]).filter(Boolean);
          const seen = new Set();
          const buttons = actions.filter(item => {
            if (!item?.action || seen.has(item.action)) return false;
            seen.add(item.action);
            return true;
          }).slice(0, 4);
          els.baseActionRow.innerHTML = buttons.map((item, index) => `<button ${index === 0 ? 'class="primary" ' : ''}type="button" data-global-action="${escapeHtml(item.action)}">${escapeHtml(item.label)}</button>`).join("");
        }
      }


      function normalizeAccessReviewItem(item, fallback = {}) {
        const tone = ["good", "warn", "bad"].includes(String(item?.status || item?.tone)) ? String(item?.status || item?.tone) : fallback.tone || "warn";
        return {
          label: text(item?.label, fallback.label || "Review"),
          count: num(item?.count, fallback.count || 0),
          tone,
          body: text(item?.body, fallback.body || "Review before enabling.")
        };
      }

      function fallbackAccessReview(servers = []) {
        const enabled = servers.filter(server => server?.effectiveEnabled);
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
          const approvalNeeded = server?.approvalRequired === true || risk.rank <= 3 || /write|mutation|credential|remote|external|unknown/i.test(`${server?.effectClass || ""} ${server?.credentialBinding || ""} ${server?.runtimeType || ""}`);
          if (approvalNeeded) approval += 1;
          if (remoteSource) remote += 1;
          if (secretSource) secrets += 1;
          if (server?.effectiveEnabled && !evidence.checked) evidenceMissing += 1;
          if (server?.effectiveEnabled && !evidence.checked && (approvalNeeded || remoteSource || secretSource)) sensitiveWithoutEvidence += 1;
        }
        const status = sensitiveWithoutEvidence ? "bad" : (!servers.length || approval || remote || secrets || evidenceMissing) ? "warn" : "good";
        const title = !servers.length ? "Access review waits for one source" : sensitiveWithoutEvidence ? "Review access before enabling" : approval || remote || secrets ? "Access needs explicit review" : "Access boundary looks quiet";
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
          counts: { servers: servers.length, enabled: enabled.length, approvalRequired: approval, hiddenSecretNames: secrets, remoteHttp: remote, enabledWithoutEvidence: evidenceMissing, sensitiveWithoutEvidence },
          items: [
            { label: "Approval", count: approval, status: approval ? "warn" : "good", body: "Write, destructive, open-world, credential, and unknown tools should ask before use." },
            { label: "Secrets", count: secrets, status: secrets ? "warn" : "good", body: "Show env/header names only. Never render secret values in the dashboard." },
            { label: "Remote/Auth", count: remote, status: remote ? "warn" : "good", body: "Remote HTTP and auth-backed sources need explicit origin and scope review." },
            { label: "Evidence", count: evidenceMissing, status: evidenceMissing ? "bad" : "good", body: "Enabled sources need initialize/tools-list evidence before normal routing." }
          ]
        };
      }

      function renderAccessReview(review = {}, servers = []) {
        if (!els.accessReview || !els.accessReviewList) return;
        const model = review && typeof review === "object" && review.schema ? review : fallbackAccessReview(servers);
        const tone = ["good", "warn", "bad"].includes(String(model.status)) ? String(model.status) : "warn";
        setCardTone(els.accessReview, tone);
        if (els.accessReviewTitle) els.accessReviewTitle.textContent = text(model.title, "Trust boundary");
        if (els.accessReviewBody) els.accessReviewBody.textContent = text(model.body, "Review approval, secrets, remote access, and evidence before enabling sources.");
        const counts = model.counts || {};
        const serverCount = num(counts.servers, servers.length);
        const enabledCount = num(counts.enabled, servers.filter(server => server?.effectiveEnabled).length);
        setChip(els.accessReviewChip, serverCount ? `${enabledCount}/${serverCount} enabled` : "no sources", tone);
        const fallbackItems = fallbackAccessReview(servers).items;
        const items = (Array.isArray(model.items) && model.items.length ? model.items : fallbackItems).slice(0, 4).map((item, index) => normalizeAccessReviewItem(item, fallbackItems[index] || {}));
        els.accessReviewList.innerHTML = items.map(item => `<article class="access-review-card ${escapeHtml(item.tone)}"><span>${escapeHtml(item.label)}</span><strong>${escapeHtml(String(item.count))}</strong><p>${escapeHtml(item.body)}</p></article>`).join("");
      }

      function renderNextAction(context) {
        if (!els.nextActionBoard) return;
        const servers = Array.isArray(context?.servers) ? context.servers : [];
        const backendOk = Boolean(state.backend.overview?.ok);
        const plan = autoPolicyPlan(servers, currentInstances());
        const unchecked = servers.filter(server => {
          const evidence = serverToolEvidence(server);
          return server.effectiveEnabled && (!evidence.checked || !evidence.ok);
        }).length;
        const confidence = Math.max(0, Math.min(100, Math.round(num(context?.overview?.userReadiness?.confidence, 0) * 100)));
        const firstAttention = Array.isArray(context?.attentionItems) ? context.attentionItems[0] : null;
        let tone = "good";
        let eyebrow = "Normal route";
        let title = "Ready for normal use";
        let body = "Keep diagnostics folded. Use the server list only when adding, testing, or changing a source.";
        let primary = { label: "Refresh", action: "refresh" };
        let secondary = { label: "Servers", action: "servers" };
        let tertiary = { label: "Diagnostics", action: "diagnostics" };
        const map = [
          ["Connect", backendOk ? "good" : "bad"],
          ["Prove", unchecked ? "warn" : servers.length ? "good" : "warn"],
          ["Enable", context?.enabledCount ? "good" : servers.length ? "warn" : "bad"],
          ["Use", confidence >= 80 && !context?.attentionTotal ? "good" : context?.attentionTotal ? "warn" : "good"]
        ];

        if (!backendOk) {
          tone = "bad";
          eyebrow = "Connection first";
          title = "Reconnect the local backend";
          body = "Do not interpret inventory or policy while /api/overview is offline. Start the hub, check the link, then refresh evidence.";
          primary = { label: "Check link", action: "check-link" };
          secondary = { label: "Start hub", action: "start-hub" };
          tertiary = { label: "Refresh", action: "refresh" };
        } else if (!context?.runtimeReady) {
          tone = "bad";
          eyebrow = "Runtime blocker";
          title = "Repair runtime before using servers";
          body = "The backend is reachable, but runtime prerequisites are not ready. Repair first so later server state is meaningful.";
          primary = { label: "Repair runtime", action: "repair" };
          secondary = { label: "Check link", action: "check-link" };
          tertiary = { label: "Diagnostics", action: "diagnostics" };
        } else if (!servers.length) {
          tone = "warn";
          eyebrow = "Inventory empty";
          title = "Import or add one server disabled";
          body = "Start from an existing MCP config when one exists; otherwise discover a trusted candidate or paste one command. Keep the new source parked until review; after enabling, run Test to collect tools/list evidence.";
          primary = { label: "Import config", action: "import-server" };
          secondary = { label: "Discover", action: "servers" };
          tertiary = { label: "Add manually", action: "add-server" };
        } else if (context?.badPolicies || firstAttention?.tone === "bad") {
          tone = "bad";
          eyebrow = "Blocker route";
          title = firstAttention?.title || `${context.badPolicies} server blocker${context.badPolicies === 1 ? "" : "s"}`;
          body = firstAttention?.meta || "Resolve source/profile mismatch or runtime setup before widening policy or trusting tools.";
          primary = { label: "Open servers", action: "servers" };
          secondary = { label: "Refresh evidence", action: "refresh" };
          tertiary = { label: "Diagnostics", action: "diagnostics" };
        } else if (plan.changes.length) {
          tone = "warn";
          eyebrow = "Safe policy plan";
          title = `${plural(plan.changes.length, "safe policy fix", "safe policy fixes")} ready`;
          body = "Apply the backend-backed low-resource route before changing worker counts manually. This keeps active sources conservative by default.";
          primary = { label: `Apply ${plan.changes.length}`, action: "auto-tune" };
          secondary = { label: "Review servers", action: "servers" };
          tertiary = { label: "Refresh", action: "refresh" };
        } else if (unchecked) {
          tone = "warn";
          eyebrow = "Evidence route";
          title = `${plural(unchecked, "enabled server")} need evidence`;
          body = "Run Test on guarded rows before relying on capabilities. Keep the server enabled only if the workflow actually needs it.";
          primary = { label: "Open servers", action: "servers" };
          secondary = { label: "Refresh evidence", action: "refresh" };
          tertiary = { label: "Add server", action: "add-server" };
        } else if (context?.attentionTotal) {
          tone = "warn";
          eyebrow = "Watchlist";
          title = firstAttention?.title || `${context.attentionTotal} watch item${context.attentionTotal === 1 ? "" : "s"}`;
          body = firstAttention?.meta || "Review the attention panel, then return to normal mode.";
          primary = { label: "Review attention", action: "attention" };
          secondary = { label: "Servers", action: "servers" };
          tertiary = { label: "Refresh", action: "refresh" };
        }

        const route = map.map(([label, nodeTone], index) => `<span class="next-action-node ${escapeHtml(nodeTone)}"><strong>${String(index + 1).padStart(2, "0")}</strong>${escapeHtml(label)}</span>`).join("");
        els.nextActionBoard.innerHTML = `
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
        `;
      }


      function serverTransportKind(server = {}) {
        const sourceType = String(server.sourceType || server.type || server.transport || server.runtimeType || "unknown").toLowerCase();
        const url = String(server.sourceUrl || server.url || server.launch || "");
        const command = String(server.sourceCommand || server.command || "");
        if (/sse|legacy/.test(`${sourceType} ${url}`)) return { tone: "bad", kind: "legacy", label: "legacy SSE", detail: "replace or keep out of automatic routing" };
        if (/http|url|streamable/.test(sourceType) || /^https?:\/\//i.test(url)) {
          const local = /^https?:\/\/(127\.0\.0\.1|localhost|\[::1\])/i.test(url);
          return { tone: local ? "good" : "warn", kind: "http", label: local ? "local Streamable HTTP" : "remote Streamable HTTP", detail: local ? "local HTTP boundary" : "auth and origin review needed" };
        }
        if (/stdio|command|process|npm|pypi|oci/.test(sourceType) || command) return { tone: server.effectiveEnabled ? "good" : "warn", kind: "stdio", label: "stdio process", detail: server.effectiveEnabled ? "local child process" : "parked until enabled and tested" };
        return { tone: "warn", kind: "unknown", label: "unknown", detail: "keep parked until Test succeeds" };
      }

      function localClientTargets(clients = []) {
        const list = Array.isArray(clients) ? clients : normalizeClients(clients);
        return list.filter(client => client?.surfaceClass === "local" || client?.installSupported || client?.installSupport);
      }

      function connectionStep(label, title, body, tone = "warn") {
        return `<article class="connection-step ${itemClass(tone)}"><span>${escapeHtml(label)}</span><strong>${escapeHtml(title)}</strong><p>${escapeHtml(body)}</p></article>`;
      }

      function renderConnectionMap(overview = {}, servers = [], clients = []) {
        if (!els.connectionGrid) return;
        const user = normalizeUserReadiness(overview.userReadiness || {});
        const localClients = localClientTargets(clients);
        const enabled = servers.filter(server => server.effectiveEnabled).length;
        const tested = servers.filter(server => { const evidence = serverToolEvidence(server); return evidence.checked && evidence.ok; }).length;
        const firstTransport = servers[0] ? serverTransportKind(servers[0]) : null;
        const endpoint = user.endpoint || overview.hub?.endpoint || "/mcp";
        const title = servers.length ? `${enabled}/${servers.length} source${servers.length === 1 ? "" : "s"} active` : "No upstreams yet";
        if (els.connectionMapTitle) els.connectionMapTitle.textContent = "Client → MCPace → Server → Tools";
        if (els.connectionMapBody) els.connectionMapBody.textContent = servers.length
          ? "Read this left to right: patch a local client, route through MCPace, test each upstream, then expose only evidenced tools."
          : "Start by importing an existing client config or patching one local client; MCPace stays between the client and upstream tools.";
        els.connectionGrid.innerHTML = [
          connectionStep("Client", localClients.length ? countLabel(localClients.length, "local target") : "No client target", localClients.length ? "Preview/apply patches from Clients; every write has restore." : "Use Clients or import an existing config path first.", localClients.length ? "good" : "warn"),
          connectionStep("MCPace", endpoint, state.backend.overview?.ok ? "Local broker overview is reachable." : "Start hub or check /api/overview first.", state.backend.overview?.ok ? "good" : "bad"),
          connectionStep("Server", title, firstTransport ? `${firstTransport.label}: ${firstTransport.detail}.` : "Import, discover, or add one source disabled.", servers.length ? (enabled ? "good" : "warn") : "warn"),
          connectionStep("Tools", tested ? countLabel(tested, "tested server") : "Not tested", tested ? "tools/list evidence exists for at least one source." : "Run Test before relying on capabilities.", tested ? "good" : "warn")
        ].join("");
        setSurfaceTone(els.connectionMap, servers.length && tested ? "good" : servers.length ? "warn" : "warn");
      }

      function setupQueueItems(context = {}) {
        const servers = context.servers || [];
        const clients = context.clients || [];
        const hub = context.hub || {};
        const runtimeReady = Boolean(context.runtimeReady);
        const plan = context.overview?.operatorPlan || { changes: [] };
        const unchecked = servers.filter(server => server.effectiveEnabled && !serverToolEvidence(server).checked).length;
        const localClients = localClientTargets(clients).filter(client => client.installSupported);
        const items = [];
        if (!state.backend.overview?.ok) items.push({ label: "1", title: "Connect backend", body: "Start hub, check link, then refresh overview.", tone: "bad", action: "check-link" });
        else if (!runtimeReady) items.push({ label: "1", title: "Repair runtime", body: `${hub.status || "runtime"} is not ready for routing.`, tone: "bad", action: "repair" });
        if (!servers.length) items.push({ label: items.length + 1, title: "Import existing config", body: "Use what the user already has before discovery or manual add.", tone: "warn", action: "import-server" });
        else if (unchecked) items.push({ label: items.length + 1, title: "Test enabled sources", body: `${unchecked} enabled source${unchecked === 1 ? "" : "s"} need tools/list evidence.`, tone: "warn", action: "servers" });
        if (localClients.length) items.push({ label: items.length + 1, title: "Preview client patch", body: `${localClients.length} local client target${localClients.length === 1 ? "" : "s"} can be patched and restored.`, tone: "good", action: "client" });
        if (Array.isArray(plan.changes) && plan.changes.length) items.push({ label: items.length + 1, title: "Apply safe policy", body: `${plan.changes.length} conservative route change${plan.changes.length === 1 ? "" : "s"} available.`, tone: "warn", action: "auto-tune" });
        if (!items.length) items.push({ label: "OK", title: "No queued setup", body: "Routine use can stay on the server rows; diagnostics are optional.", tone: "good", action: "servers" });
        return items.slice(0, 1);
      }

      function renderSetupQueue(context = {}) {
        if (!els.setupQueueList) return;
        const items = setupQueueItems(context);
        if (els.setupQueueBody) els.setupQueueBody.textContent = "One safe next step is shown here; advanced setup stays folded below.";
        els.setupQueueList.innerHTML = items.map(item => `<article class="setup-queue-item ${itemClass(item.tone)}"><span>${escapeHtml(item.label)}</span><strong>${escapeHtml(item.title)}</strong><p>${escapeHtml(item.body)}</p>${item.action ? `<button type="button" data-global-action="${escapeHtml(item.action)}">Open</button>` : ""}</article>`).join("");
        setSurfaceTone(els.setupQueue, items.some(item => item.tone === "bad") ? "bad" : items.some(item => item.tone === "warn") ? "warn" : "good");
      }

      function protocolDescriptor(server = {}, instances = []) {
        const base = serverTransportKind(server);
        const matchedInstances = instances.filter(instance => (instance.server || instance.serverName || instance.name) === server.name);
        const modes = [...new Set(matchedInstances.map(instance => instance.mode || instance.schedulerLane || instance.routingMode).filter(Boolean))];
        return { ...base, modes };
      }

      function renderProtocolCompatibility(overview, servers, clients, instances = []) {
        overview = overview || {};
        servers = Array.isArray(servers) ? servers : [];
        clients = Array.isArray(clients) ? clients : normalizeClients(overview.clients || []);
        instances = Array.isArray(instances) ? instances : normalizeInstances(overview.instances);
        if (!els.protocolCompatGrid) return;
        const descriptors = servers.map(server => ({ server, info: protocolDescriptor(server, instances) }));
        const counts = descriptors.reduce((acc, item) => { acc[item.info.kind] = (acc[item.info.kind] || 0) + 1; return acc; }, { stdio: 0, http: 0, legacy: 0, unknown: 0 });
        const remote = descriptors.filter(item => /remote/i.test(item.info.label)).length;
        const authHints = servers.filter(server => listValues(server.sourceHeaderNames).length || /oauth|credential|token|auth/i.test(`${server.credentialBinding || ""} ${server.sourceHeaderNames || ""}`)).length;
        const clientIngresses = [...new Set(clients.flatMap(client => listValues(client?.supportedIngresses)))].sort();
        const cache = overview.cachedToolEvidence || {};
        const cacheTone = num(cache.failedCount) ? "bad" : num(cache.serverCount) ? "good" : "warn";
        const tone = counts.legacy ? "bad" : remote || authHints || counts.unknown ? "warn" : servers.length ? "good" : "warn";
        setChip(els.protocolCompatChip, counts.legacy ? `${counts.legacy} legacy` : remote ? `${remote} remote` : servers.length ? "compatible" : "pending", tone);
        const summaryCards = [
          `<article class="protocol-compat-card ${clientIngresses.length ? "good" : "warn"}"><span>Client ingress</span><strong>${escapeHtml(clientIngresses.length ? clientIngresses.join(" + ") : "not loaded")}</strong><p>${clients.length ? "Patch/restore clients from Clients; keep config changes reversible." : "Client surfaces appear after backend catalog loads."}</p></article>`,
          `<article class="protocol-compat-card ${counts.stdio ? "good" : "warn"}"><span>stdio</span><strong>${counts.stdio}</strong><p>Local process servers. Keep parked until Test collects initialize + tools/list evidence.</p></article>`,
          `<article class="protocol-compat-card ${counts.http ? remote || authHints ? "warn" : "good" : "warn"}"><span>Streamable HTTP</span><strong>${counts.http}</strong><p>${remote || authHints ? "HTTP upstreams need explicit origin/auth review; secret values stay hidden." : "HTTP upstreams use the broker boundary when configured."}</p></article>`,
          `<article class="protocol-compat-card ${counts.legacy ? "bad" : "good"}"><span>Legacy / blocked</span><strong>${counts.legacy}</strong><p>Legacy SSE or unsupported transports stay out of automatic routing.</p></article>`,
          `<article class="protocol-compat-card ${cacheTone}"><span>Tool evidence</span><strong>${num(cache.serverCount) ? `${num(cache.okCount)}/${num(cache.serverCount)} ok` : "not cached"}</strong><p>${num(cache.failedCount) ? `${num(cache.failedCount)} failed cache entr${num(cache.failedCount) === 1 ? "y" : "ies"}.` : "Run Test before treating tools as usable."}</p></article>`
        ];
        const serverCards = descriptors.slice(0, 5).map(({ server, info }) => `<article class="protocol-compat-card ${itemClass(info.tone)}"><span>${escapeHtml(server.name || "server")}</span><strong>${escapeHtml(info.label)}</strong><p>${escapeHtml(info.detail)}${info.modes.length ? ` · route ${escapeHtml(info.modes.join(", "))}` : ""}</p></article>`);
        if (!serverCards.length) serverCards.push(`<article class="protocol-compat-card warn"><span>No upstreams</span><strong>Import first</strong><p>No protocol surface is configured yet. Import, discover, or add one server, then run Test.</p></article>`);
        els.protocolCompatGrid.innerHTML = [...summaryCards, ...serverCards].join("");
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
          warnPolicies = 0
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
        els.decisionGrid.innerHTML = [
          decisionCard("Connection", backendOk ? "API online" : "API offline", backendOk ? `/api/overview ${fmtMs(backend.ms)}` : text(backend?.error?.message || backend?.error, "start/check local hub"), backendOk ? msTone(backend.ms) : "bad", backendOk ? 100 : 12, "01"),
          decisionCard("Runtime", runtimeReady ? "Ready to route" : "Prerequisites blocked", `${hub.status || hub.health || "unknown"} · ${active}/${max || "?"} active HTTP`, runtimeReady ? "good" : "bad", runtimeReady ? 100 : 24, "02"),
          decisionCard("Fleet", serverTotal ? `${enabledCount}/${serverTotal} enabled` : "Inventory missing", `${badPolicies} blocked · ${warnPolicies} guarded`, badPolicies ? "bad" : warnPolicies ? "warn" : serverTotal ? "good" : "warn", serverTotal ? enabledPct : 12, "03"),
          decisionCard("User", userBand.label, `${clients.length} client surface${clients.length === 1 ? "" : "s"} · ${user.endpoint || "/mcp"}`, userBand.tone, userBand.pct, "04")
        ].join("");
      }

      function handleGlobalAction(control) {
        const action = control?.dataset?.globalAction || "";
        if (action === "start-hub") runAction("/api/actions/hub-up", control, "", "Starting…");
        else if (action === "repair") runAction("/api/actions/repair", control, "Run MCPace repair now? This may update local runtime wiring and client config files.", "Repairing…");
        else if (action === "check-link") checkBackendLink(control);
        else if (action === "refresh") refreshDashboard({ force: true, reason: "hero" });
        else if (action === "servers") document.getElementById("servers-title")?.scrollIntoView?.({ behavior: "smooth", block: "start" });
        else if (action === "diagnostics") document.getElementById("deep-diagnostics")?.scrollIntoView?.({ behavior: "smooth", block: "start" });
        else if (action === "help") document.getElementById("help-page")?.scrollIntoView?.({ behavior: "smooth", block: "start" });
        else if (action === "attention") document.getElementById("attention-title")?.scrollIntoView?.({ behavior: "smooth", block: "start" });
        else if (action === "import-server") focusImportPath();
        else if (action === "add-server") focusInstallCommand();
        else if (action === "client" || action === "clients") { updateSetupToolsState("client"); revealElementById("client-setup-panel", "center"); window.setTimeout(() => els.clientPreviewAll?.focus?.(), 120); }
        else if (action === "discover") { updateSetupToolsState("discover"); revealElementById("server-discovery-panel", "center"); }
        else if (action === "auto-tune") autoTuneVisibleServers(control);
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
          saturation
        } = context;
        const overview = state.backend.overview;
        const logs = state.backend.logs;
        const resources = state.backend.resources;
        const action = state.backend.action;
        const backendOk = Boolean(overview?.ok);
        const logsOk = !logs || logs.ok;
        const resourcesOk = !resources || resources.ok;
        const actionLabel = action ? (action.ok ? `${action.endpoint} ok` : `${action.endpoint} failed`) : "action not checked";
        const actionTone = !action ? "warn" : action.ok ? "good" : "bad";
        const logsMeta = logs?.ok ? `updated ${fmtDate(logs.at)}` : logs ? apiErrorMessage(logs.error) : "waiting";
        const resourcesMeta = resources?.ok ? `${active}/${max || "?"} active HTTP` : resources ? apiErrorMessage(resources.error) : "waiting";
        setSurfaceTone(els.opsTitle, systemTone);
        setSurfaceTone(els.backendState, backendOk && logsOk && resourcesOk ? "good" : backendOk ? "warn" : "bad");

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
              ? "Fix the watchlist first. Everything else is folded into fleet groups and advanced diagnostics."
              : "No server-by-server tuning is required. Watch backend link, auto plan, and fleet groups; open Settings only for manual overrides.";
        }
        if (els.opsCommandRow) {
          els.opsCommandRow.innerHTML = !backendOk
            ? [
                `<button class="primary" type="button" data-global-action="start-hub">Start hub</button>`,
                `<button type="button" data-global-action="check-link">Check link</button>`,
                `<button class="quiet" type="button" data-global-action="refresh">Refresh overview</button>`
              ].join("")
            : !runtimeReady
              ? [
                  `<button class="primary" type="button" data-global-action="repair">Repair runtime</button>`,
                  `<button type="button" data-global-action="check-link">Check link</button>`,
                  `<button class="quiet" type="button" data-global-action="refresh">Refresh overview</button>`
                ].join("")
              : attentionTotal
                ? [
                    `<button class="primary" type="button" data-global-action="refresh">Refresh evidence</button>`,
                    `<button type="button" data-global-action="check-link">Check link</button>`,
                    `<button class="quiet" type="button" data-global-action="repair">Repair</button>`
                  ].join("")
                : [
                    `<button class="primary" type="button" data-global-action="refresh">Refresh</button>`,
                    `<button type="button" data-global-action="check-link">Verify link</button>`
                  ].join("");
        }
        if (els.opsSteps) {
          els.opsSteps.innerHTML = [
            stepCard(backendOk ? "Backend connected" : "Backend offline", backendOk ? `/api/overview ${fmtMs(overview.ms)}` : text(overview?.error?.message || overview?.error, "waiting"), backendOk ? msTone(overview.ms) : "bad"),
            stepCard(runtimeReady ? "Runtime usable" : "Runtime setup needed", `${hub.status || hub.health || "unknown"} · ${readiness.profileSelectionSource || "profile"}`, runtimeReady ? "good" : "bad"),
            stepCard(attentionTotal ? "Watchlist has work" : "No visible blockers", attentionItems[0]?.title || `${enabledCount} enabled server(s) under auto plan`, attentionTotal ? "warn" : "good")
          ].join("");
        }
        if (els.backendState) {
          els.backendState.textContent = backendOk && logsOk && resourcesOk ? "Live backend connected" : backendOk ? "Partial backend link" : "Backend not connected";
        }
        if (els.backendGrid) {
          els.backendGrid.innerHTML = [
            readout("/api/overview", backendOk ? fmtMs(overview.ms) : "failed", backendOk ? `updated ${fmtDate(overview.at)}` : text(overview?.error?.message || overview?.error, "waiting"), backendOk ? msTone(overview.ms) : "bad"),
            readout("/api/logs", logs?.ok ? fmtMs(logs.ms) : logs ? "failed" : "pending", logsMeta, logs?.ok ? msTone(logs.ms) : "warn"),
            readout("/api/resources", resources?.ok ? fmtMs(resources.ms) : resources ? "failed" : "pending", resourcesMeta, resources?.ok ? msTone(resources.ms) : "warn"),
            readout("action ping", actionLabel, action ? `${fmtMs(action.ms)} · ${fmtDate(action.at)}` : "use Check link or any action", actionTone)
          ].join("");
        }
      }

      function normalizeUserReadiness(value) {
        const item = value && typeof value === "object" ? value : {};
        return {
          schema: item.schema || "mcpace.userReadiness.v0",
          headline: item.headline || "User readiness unknown",
          body: item.body || "Backend did not return a user-readiness summary yet.",
          confidence: Number.isFinite(Number(item.confidence)) ? Number(item.confidence) : 0,
          primaryAction: item.primaryAction || "Refresh overview",
          primaryReason: item.primaryReason || "Live backend state is required before trusting the UI.",
          shouldSee: Array.isArray(item.shouldSee) ? item.shouldSee : [],
          shouldHide: Array.isArray(item.shouldHide) ? item.shouldHide : [],
          missing: Array.isArray(item.missing) ? item.missing : [],
          endpoint: item.endpoint || "/mcp"
        };
      }

      function listSentence(items, fallback) {
        const values = Array.isArray(items) ? items.filter(Boolean).map(value => String(value)) : [];
        if (!values.length) return fallback;
        return values.slice(0, 4).join(" · ") + (values.length > 4 ? ` · +${values.length - 4}` : "");
      }

      function renderUserReadiness(rawReadiness, servers = [], clients = []) {
        const readiness = normalizeUserReadiness(rawReadiness);
        const band = readinessBand(readiness.confidence);
        const tone = band.tone;
        setSurfaceTone(els.userReadinessTitle, tone);
        if (els.userReadinessTitle) els.userReadinessTitle.textContent = readiness.headline;
        if (els.userReadinessBody) els.userReadinessBody.textContent = readiness.body;
        if (els.userConfidenceChip) setChip(els.userConfidenceChip, band.label, tone);
        if (!els.userReadinessGrid) return;
        const visible = listSentence(readiness.shouldSee, "status, endpoint, server launch commands, live tool evidence");
        const hidden = listSentence(readiness.shouldHide, "secret values, raw JSON, manual worker settings, advanced logs");
        const missing = listSentence(readiness.missing, "nothing critical from the current user view");
        els.userReadinessGrid.innerHTML = [
          readout("Can I use it?", readiness.primaryAction, readiness.primaryReason, tone),
          readout("Visible now", `${servers.length} server${servers.length === 1 ? "" : "s"}`, visible, "good"),
          readout("Hidden by default", "safe defaults", hidden, "warn"),
          readout("Missing", readiness.missing.length ? `${readiness.missing.length} gap${readiness.missing.length === 1 ? "" : "s"}` : "clean", missing, readiness.missing.length ? "warn" : "good")
        ].join("");
      }

      function renderFleetBoard(servers, instances) {
        const groups = groupByServer(instances);
        const buckets = [
          { key: "all", label: "All", tone: "good", rows: [...servers] },
          { key: "blocked", label: "Blocked", tone: "bad", rows: [] },
          { key: "protected", label: "Guarded", tone: "warn", rows: [] },
          { key: "ready", label: "Ready", tone: "good", rows: [] },
          { key: "off", label: "Off", tone: "warn", rows: [] }
        ];
        for (const server of servers) {
          const risk = riskForServer(server, groups.get(server.name) || []);
          const key = serverBucket(server, risk);
          buckets.find(bucket => bucket.key === key)?.rows.push(server);
        }
        if (!els.serverFleetBoard) return;
        const metaForBucket = bucket => {
          if (bucket.key === "all") return "reset group filter";
          if (bucket.key === "blocked") return bucket.rows.length ? "fix these first" : "no blockers";
          if (bucket.key === "protected") return bucket.rows.length ? "on · conservative policy" : "none";
          if (bucket.key === "ready") return bucket.rows.length ? "evidence listed" : "none";
          if (bucket.key === "off") return bucket.rows.length ? "parked · enable when needed" : "none";
          return "";
        };
        els.serverFleetBoard.innerHTML = buckets.map(bucket => {
          const meta = metaForBucket(bucket);
          const pressed = state.bucket === bucket.key;
          return `<button class="fleet-card ${bucket.tone}" type="button" data-server-bucket="${bucket.key}" aria-pressed="${pressed}"><strong>${bucket.rows.length} ${bucket.label}</strong><span>${escapeHtml(meta)}</span></button>`;
        }).join("");
        if (els.serverGuide) {
          const counts = Object.fromEntries(buckets.map(bucket => [bucket.key, bucket.rows.length]));
          const allPrefix = counts.blocked
            ? `${counts.blocked} blocked server${counts.blocked === 1 ? "" : "s"} need setup before use.`
            : "No blocked servers right now.";
          const guidance = {
            all: ["Fleet brief", `${allPrefix} Read each row as: live evidence, current state, then buttons. Workers are hidden in Details.`],
            blocked: ["Blocked view", "Fix source/profile setup first; then run Test to collect tools/list evidence."],
            protected: ["Guarded view", "These are on with conservative policy or incomplete evidence. Run Test if tools are not listed."],
            ready: ["Ready view", "These have live or cached tools/list evidence. Re-test after source changes."],
            off: ["Off view", "These are parked. Turn one on only when a workflow explicitly needs that source, then run Test."]
          }[state.bucket] || ["Fleet brief", allPrefix];
          els.serverGuide.innerHTML = `<strong>${escapeHtml(guidance[0])}.</strong> ${escapeHtml(guidance[1])}`;
        }
      }

      function fleetMetric(label, value, meta, tone = "warn") {
        return `<div class="server-metric ${escapeHtml(tone)}"><span>${escapeHtml(label)}</span><strong>${escapeHtml(value)}</strong><em>${escapeHtml(meta)}</em></div>`;
      }

      function serverStage(label, value, tone = "warn") {
        return `<span class="server-stage ${escapeHtml(tone)}"><span class="stage-dot"></span><strong>${escapeHtml(label)}</strong>${escapeHtml(value)}</span>`;
      }

      function renderServerCommandCenter(servers, rows, groups) {
        if (!els.serverCommandCenter) return;
        const all = Array.isArray(servers) ? servers : [];
        const visible = Array.isArray(rows) ? rows : [];
        const visibleModels = visible.map(server => serverViewModel(server, groups.get(server.name) || []));
        const enabled = all.filter(server => server.effectiveEnabled).length;
        const blocked = visibleModels.filter(model => model.risk.tone === "bad").length;
        const guarded = visibleModels.filter(model => model.risk.tone === "warn" && model.risk.rank <= 3).length;
        const ready = visibleModels.filter(model => model.risk.tone === "good").length;
        const off = visibleModels.filter(model => !model.server?.effectiveEnabled).length;
        const evidenceChecked = visible.filter(server => serverToolEvidence(server).checked).length;
        const evidenceFailed = visible.filter(server => {
          const evidence = serverToolEvidence(server);
          return evidence.checked && !evidence.ok;
        }).length;
        const policyFixes = visibleModels.filter(model => model.needsTuning && model.server?.effectiveEnabled).length;
        const workerTotal = visibleModels.reduce((sum, model) => sum + Math.max(1, num(model.workers, 1)), 0);
        const tone = blocked ? "bad" : guarded || policyFixes || off ? "warn" : visible.length ? "good" : "warn";
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
                  : "Keep diagnostics folded. Routine work should happen from the fleet view and row actions.";
        if (els.serverCommandTitle) els.serverCommandTitle.textContent = title;
        if (els.serverCommandBody) els.serverCommandBody.textContent = body;
        if (els.serverMetricRow) {
          els.serverMetricRow.innerHTML = [
            fleetMetric("Visible", String(visible.length), `${enabled}/${all.length || 0} enabled total`, visible.length ? "good" : "warn"),
            fleetMetric("Evidence", `${evidenceChecked}/${visible.length || 0}`, evidenceFailed ? `${evidenceFailed} failed` : "tools/list coverage", evidenceFailed ? "bad" : evidenceChecked === visible.length && visible.length ? "good" : "warn"),
            fleetMetric("Policy", policyFixes ? String(policyFixes) : "clean", policyFixes ? "backend fixes ready" : `${guarded} guarded · ${ready} ready`, policyFixes ? "warn" : blocked ? "bad" : "good"),
            fleetMetric("Capacity", String(workerTotal), `${off} off in lens`, workerTotal ? "good" : "warn")
          ].join("");
        }
        if (els.serverWorkbench) {
          const focus = visibleModels.find(model => model.risk.tone === "bad") || visibleModels.find(model => model.needsTuning) || visibleModels.find(model => model.risk.rank <= 3) || visibleModels[0];
          els.serverWorkbench.innerHTML = focus
            ? `<div class="workbench-summary ${escapeHtml(focus.verdict.tone)}">
                <span class="workbench-index">${escapeHtml(String((visible.findIndex(row => row.name === focus.server?.name) + 1) || 1).padStart(2, "0"))}</span>
                <div><strong>${escapeHtml(focus.server?.name || "server")}</strong><p>${escapeHtml(focus.decision?.body || focus.nextStep || "Open details for the next safe action.")}</p></div>
                <button type="button" data-server-name="${escapeHtml(focus.server?.name || "")}" data-server-action="settings">Open details</button>
              </div>`
            : `<div class="workbench-summary"><span class="workbench-index">00</span><div><strong>No current server lens.</strong><p>Clear filters or add a server to create an actionable row.</p></div></div>`;
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
        let rows = servers.filter(server => {
          const risk = riskForServer(server, groups.get(server.name) || []);
          if (state.enabledOnly && !server.effectiveEnabled && risk.rank > 1) return false;
          if (state.bucket !== "all" && serverBucket(server, risk) !== state.bucket) return false;
          if (state.scope === "attention" && risk.rank > 3) return false;
          if (!query) return true;
          const evidence = serverToolEvidence(server);
          return [server.name, server.kind, server.runtimeType, server.stateClass, server.effectClass, server.scopeClass, server.concurrencyPolicy, server.routingGroup, server.transportPreference, server.launcherKind, server.startupStrategy, server.sourceType, server.sourceCommand, server.sourceUrl, server.sourcePath, ...(server.sourceArgs || []), ...(server.sourceEnvNames || []), ...(server.sourceHeaderNames || []), evidence.status, ...(evidence.toolNames || [])]
            .some(value => String(value || "").toLowerCase().includes(query));
        });
        rows = rows.sort((left, right) => {
          const leftInstances = groups.get(left.name) || [];
          const rightInstances = groups.get(right.name) || [];
          if (state.sort === "name") return String(left.name || "").localeCompare(String(right.name || ""));
          if (state.sort === "instances") return rightInstances.length - leftInstances.length || String(left.name || "").localeCompare(String(right.name || ""));
          return riskForServer(left, leftInstances).rank - riskForServer(right, rightInstances).rank || String(left.name || "").localeCompare(String(right.name || ""));
        });
        renderServerCommandCenter(servers, rows, groups);
        if (!rows.length) {
          const hasServers = servers.length > 0;
          const reason = state.query
            ? `No server matches “${state.query}”.`
            : hasServers
              ? "Current filters hide every server."
              : "No servers are configured yet.";
          els.serverList.innerHTML = `<div class="empty-state">
            <strong>${escapeHtml(reason)}</strong>
            <p>${hasServers ? "Restore the lens before changing policy." : "Start with an existing client config when possible, then preview discovery or add one server manually."}</p>
            <div class="empty-actions">
              ${state.query ? `<button class="primary" type="button" data-empty-action="clear-search">Clear search</button>` : ""}
              ${hasServers ? `<button type="button" data-empty-action="show-all">Show all servers</button>` : `<button class="primary" type="button" data-empty-action="import-config">Import config</button><button type="button" data-empty-action="discover">Discover</button><button class="quiet" type="button" data-empty-action="add-server">Add manually</button>`}
              <button class="quiet" type="button" data-empty-action="refresh">Refresh</button>
            </div>
          </div>`;
          els.serverOverflowNote.textContent = "";
          return;
        }
        const visible = rows.slice(0, MAX_SERVER_ROWS);
        els.serverList.innerHTML = visible.map(server => {
          const related = groups.get(server.name) || [];
          const model = serverViewModel(server, related);
          const risk = model.risk;
          const verdict = model.verdict;
          const human = model.human;
          const plan = model.operatorPlan;
          const evidence = serverToolEvidence(server);
          const evidenceTone = evidence.checked ? (evidence.ok ? "good" : "bad") : "warn";
          const evidenceLabel = evidence.checked ? (evidence.ok ? `${evidence.toolCount || evidence.toolNames?.length || 0} tools` : evidence.status || "failed") : "run test";
          const policyTone = model.needsTuning ? "warn" : server.effectiveEnabled ? "good" : "warn";
          const policyLabel = model.needsTuning ? "safe fix" : routeLabel(model.routeMode);
          const useTone = !server.effectiveEnabled ? "warn" : risk.tone;
          const useLabel = !server.effectiveEnabled ? "disabled" : verdict.label;
          const statusRail = [
            serverStage("Evidence", evidenceLabel, evidenceTone),
            serverStage("Policy", policyLabel, policyTone),
            serverStage("Use", useLabel, useTone)
          ].join("");
          const showNext = risk.rank <= 1 || Boolean(plan?.needsPolicyChange);
          const name = escapeHtml(server.name || "server");
          return `
            <article class="server-row ${itemClass(verdict.tone)}" data-server-name="${name}" data-server-bucket="${model.bucket}" data-enabled="${server.effectiveEnabled ? "true" : "false"}">
              <div class="server-main">
                <div class="server-id">
                  <div class="server-title-row">
                    <div class="name">${name}</div>
                    ${chip(verdict.label, verdict.tone)}
                  </div>
                   <div class="server-purpose">${escapeHtml(model.category)} · ${escapeHtml(model.impact)}</div>
                   ${launchCommand(server) ? `<code class="launch-line">${escapeHtml(launchCommand(server))}</code>` : ""}
                   <div class="server-status-rail" aria-label="Setup path for ${name}">${statusRail}</div>
                   <div class="server-human-card" aria-label="Plain-language setup for ${name}">
                     <section>
                       <div class="label">Live evidence</div>
                       <strong>${escapeHtml(human.capabilityTitle)}</strong>
                       <p>${escapeHtml(human.capabilityBody)}</p>
                     </section>
                     <section>
                       <div class="label">Current state</div>
                       <strong>${escapeHtml(human.nowTitle)}</strong>
                       <p>${escapeHtml(human.nowBody)}</p>
                     </section>
                  </div>
                </div>
                <div class="server-quick-controls" aria-label="Actions for ${name}">
                  ${serverControls(server, related, "row")}
                </div>
              </div>
              <div class="server-fast-facts" aria-label="Useful server facts">
                ${model.facts}
              </div>
              ${showNext ? `<div class="server-action-note"><strong>Next</strong> ${escapeHtml(plan?.nextAction || model.nextStep)} <br><strong>Backend plan</strong> ${escapeHtml(plan?.rationale || model.recommendation.reason)}</div>` : ""}
            </article>
          `;
        }).join("");
        els.serverOverflowNote.textContent = rows.length > visible.length ? `${rows.length - visible.length} more server(s) hidden by the compact list. Search or change filters to narrow it.` : "";
      }

      function detail(label, value) {
        return `<div class="detail-box"><div class="label">${escapeHtml(label)}</div><div class="detail-value">${escapeHtml(text(value))}</div></div>`;
      }

      function openServerDialog(name) {
        state.selectedServer = name;
        renderServerDialogByName(name);
        if (els.serverDialog && !els.serverDialog.open) {
          if (typeof els.serverDialog.showModal === "function") els.serverDialog.showModal();
          else els.serverDialog.setAttribute("open", "");
        }
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
        const recommendation = model.recommendation;
        const verdict = model.verdict;
        const decision = model.decision;
        const settings = model.settings;
        const checklist = serverChecklist(server, risk).map(item => `<li>${escapeHtml(item)}</li>`).join("");
        const operatorRunbook = renderServerRunbook(model.operatorPlan);
        const runtimeControl = runtimeControlForServer(server.name);
        const lockDomains = Array.isArray(server.lockDomains) ? server.lockDomains.join(", ") : text(server.hostLock || server.hostLockKey || server.conflictDomain, "none");
        const requiredCommands = Array.isArray(server.requiredCommands) ? server.requiredCommands.join(", ") : "none";
        const launch = launchCommand(server);
        const sourceArgs = compactList(server.sourceArgs);
        const sourceEnvNames = compactList(server.sourceEnvNames);
        const sourceHeaderNames = compactList(server.sourceHeaderNames);
        const sourcePath = text(server.sourcePath, "default MCP settings");
        const idName = domId(server.name || "server");
        const nameEsc = escapeHtml(server.name || "server");

        els.serverDialogTitle.textContent = server.name || "server";
        els.serverDialogSubtitle.textContent = `${verdict.label} · ${model.category} · ${server.effectiveEnabled ? "enabled" : "disabled"}`;
        els.serverDialogBody.innerHTML = `
          <div class="server-dialog-summary">
            <div>
              <div class="label">Recommended next step</div>
              <strong>${escapeHtml(decision.title)}</strong>
              <p>${escapeHtml(decision.body)}</p>
            </div>
            <div class="server-dialog-actions" aria-label="Primary actions for ${nameEsc}">
              ${serverControls(server, related, "dialog")}
            </div>
          </div>
          <div class="server-setting-brief" aria-label="Recommended settings for ${nameEsc}">
            ${settingCard("Best setting", settings.stateTitle, settings.stateBody)}
            ${settingCard("Routing and workers", settings.routeTitle, settings.routeBody)}
            ${settingCard("Tool evidence", settings.useTitle, settings.useBody)}
          </div>
          <details class="manual-settings">
            <summary>Manual override: routing and workers</summary>
            <section class="server-settings-grid" aria-label="Editable server routing settings">
              <div class="server-setting-box">
                <div class="label">Workers</div>
                <div class="setting-inline" style="margin-top: 8px;">
                  <label class="sr-only" for="dialog-workers-${idName}">Worker count</label>
                  <input id="dialog-workers-${idName}" type="number" min="1" step="1" value="${escapeHtml(workers)}" data-server-input="workers">
                  <button type="button" data-server-name="${nameEsc}" data-server-action="apply-policy">Apply</button>
                </div>
              </div>
              <div class="server-setting-box">
                <div class="label">In-flight per worker</div>
                <div class="setting-inline" style="margin-top: 8px;">
                  <label class="sr-only" for="dialog-inflight-${idName}">In-flight requests per worker</label>
                  <input id="dialog-inflight-${idName}" type="number" min="1" step="1" value="${escapeHtml(inFlight)}" data-server-input="inFlight">
                  <span class="fact"><strong>Current</strong> ${escapeHtml(routeLabel(mode))}</span>
                </div>
              </div>
              <div class="server-setting-box">
                <div class="label">Routing mode</div>
                <label class="sr-only" for="dialog-mode-${idName}">Routing mode for ${nameEsc}</label>
                <select class="server-mode-select" id="dialog-mode-${idName}" data-server-input="mode" style="margin-top: 8px;">
                  ${modeOptions(mode)}
                </select>
              </div>
            </section>
          </details>
          <div class="server-action-note"><strong>Auto plan</strong> ${escapeHtml(model.needsTuning ? `${recommendation.label} available` : "active")} · ${escapeHtml(model.operatorPlan?.nextAction || model.nextStep)}</div>
          ${operatorRunbook}
          ${renderRuntimeControl(runtimeControl)}
          <div class="server-explain-grid">
            <section class="server-explain-box">
              <div class="label">Why this route</div>
              <p>${escapeHtml(routing)}</p>
            </section>
            <section class="server-explain-box">
              <div class="label">Safety notes</div>
              <ul class="server-checklist">${checklist}</ul>
            </section>
          </div>
          <section class="server-explain-box">
            <div class="label">Evidence</div>
            <p>${escapeHtml(evidence)}</p>
          </section>
          <div class="detail-grid">
            ${detail("Kind", server.kind)}
            ${detail("Profile enabled", server.profileEnabled ? "yes" : "no")}
            ${detail("Source enabled", server.sourceEnabled ? "yes" : "no")}
            ${detail("Effective enabled", server.effectiveEnabled ? "yes" : "no")}
            ${detail("Scope", server.scopeClass)}
            ${detail("Effect", server.effectClass)}
            ${detail("State", server.stateClass)}
            ${detail("State binding", server.stateBinding)}
            ${detail("Credential binding", server.credentialBinding)}
            ${detail("Pool model", server.defaultPoolModel)}
            ${detail("Scheduler lane", server.schedulerLane)}
            ${detail("Request strategy", server.requestStrategy)}
            ${detail("Conflict domain", server.conflictDomain)}
            ${detail("Locks", lockDomains)}
            ${detail("Launcher", server.launcherKind)}
            ${detail("Startup", server.startupStrategy)}
            ${detail("Launch command", launch || "none")}
            ${detail("Source file", sourcePath)}
            ${detail("Source args", sourceArgs)}
            ${detail("Env names", sourceEnvNames)}
            ${detail("Header names", sourceHeaderNames)}
            ${detail("Transport", server.transportPreference || server.sourceType || "stdio")}
            ${detail("Transport status", server.transportStatus)}
            ${detail("Required commands", requiredCommands)}
          </div>
        `;
      }

      function actionPayloadForPolicy(server, related, overrides = {}) {
        return {
          server: server.name,
          mode: overrides.mode ?? serverMode(server, related),
          maxWorkers: overrides.maxWorkers ?? maxWorkers(server, related),
          maxInFlightPerWorker: overrides.maxInFlightPerWorker ?? maxInFlight(server, related)
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
          body: JSON.stringify(payload)
        });
        state.backend.action = {
          ok: result.ok,
          ms: result.ms,
          at: result.at,
          endpoint,
          error: result.ok ? "" : apiErrorMessage(result.error)
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
          state.importer.error = !sourcePath ? "Enter a local MCP settings JSON path to preview." : intent.body;
          setFieldError(els.serverImportError, els.serverImportPath, state.importer.error);
          renderServerImportPanel();
          els.serverImportPath?.focus?.();
          return;
        }
        setFieldError(els.serverImportError, els.serverImportPath, "");
        const payload = {
          sourcePath,
          dryRun: els.serverImportDryRun?.checked !== false,
          disabled: els.serverImportDisabled?.checked !== false
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
          if (!payload.dryRun) await refreshDashboard({ force: true, reason: "server-import-config" });
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
          state.discovery.error = "Install mode needs a search term so MCPace does not run a broad automatic sweep from the dashboard.";
          setFieldError(els.serverDiscoverError, els.serverDiscoverQuery, state.discovery.error);
          renderDiscoveryPanel();
          els.serverDiscoverQuery?.focus?.();
          return;
        }
        setFieldError(els.serverDiscoverError, els.serverDiscoverQuery, "");
        const payload = {
          mode: mode === "install" ? "apply" : "preview",
          dryRun: mode === "preview",
          disabled: true
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
          if (mode === "install") await refreshDashboard({ force: true, reason: "server-discover" });
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
          const message = "Paste a command, package, local path, or Streamable HTTP URL first.";
          setInstallNote(message, "bad");
          setFieldError(els.serverInstallError, els.serverInstallCommand, message);
          els.serverInstallCommand?.focus();
          return;
        }
        const intent = installCommandIntent(commandLine);
        if (intent.tone === "bad") {
          setInstallNote(`${intent.label}: ${intent.body}`, "bad");
          setFieldError(els.serverInstallError, els.serverInstallCommand, intent.body);
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
          dryRun: Boolean(els.serverInstallDryRun?.checked)
        };
        if (server) payload.server = server;
        try {
          if (button) {
            button.disabled = true;
            button.textContent = payload.dryRun ? "Previewing…" : "Saving…";
          }
          setFieldError(els.serverInstallError, els.serverInstallCommand, "");
          setInstallNote(payload.dryRun ? "Previewing install plan…" : "Saving server source…", "warn");
          const response = await postServerAction("server-install-command", payload);
          const planned = response?.result?.plan?.name || response?.result?.write?.name || server || "server";
          setInstallNote(payload.dryRun ? `Preview ready for ${planned}; nothing was written.` : `Saved ${planned}. Run Test on its row before relying on its tools.`, "good");
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
          state.serverTests[payload.server] = normalizeProbeEvidence(payload.server, response);
          return;
        }
        if (endpoint === "server-policy") {
          const result = response?.result || {};
          const policy = result.policy || {};
          const execution = result.execution || {};
          const firstDefined = (...values) => values.find(value => value !== undefined && value !== null && value !== "");
          server.maxWorkers = Number(firstDefined(result.maxWorkers, execution.maxWorkers, payload.maxWorkers, server.maxWorkers, 1));
          server.maxInFlightPerWorker = Number(firstDefined(result.maxInFlightPerWorker, execution.maxInFlightPerWorker, payload.maxInFlightPerWorker, server.maxInFlightPerWorker, 1));
          for (const key of ["scopeClass", "concurrencyPolicy", "stateBinding", "credentialBinding", "parallelismLimit", "conflictDomain", "projectRootMode", "worktreeBinding", "stateProfileMode", "hostLock", "startupStrategy", "routingGroup", "discoveryRequiresLease"]) {
            if (policy[key] !== undefined) server[key] = policy[key];
          }
        }
      }

      function syncAfterServerAction(delay = 250) {
        window.setTimeout(() => refreshDashboard({ force: true, reason: "server-action-sync" }), delay);
      }

      async function runServerAction(endpoint, payload, control, busyLabel = "Working…", options = {}) {
        const originalText = control && "textContent" in control ? control.textContent : "";
        try {
          if (control) {
            control.disabled = true;
            if (control.tagName === "BUTTON") control.textContent = busyLabel;
          }
          const response = await postServerAction(endpoint, payload);
          applyOptimisticServerAction(endpoint, payload, response);
          state.lastError = null;
          render();
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
        return runServerAction("server-test", { server: serverName, timeoutMs: 10000 }, control, "Testing…", options);
      }

      async function enableAndTestServer(serverName, control) {
        const message = `Enable ${serverName} and run Test now? This can launch the upstream command or call a remote endpoint, so review source and secrets first.`;
        if (typeof window.confirm === "function" && !window.confirm(message)) return null;
        const enabled = await runServerAction("server-enable", { server: serverName }, control, "Enabling…", { sync: false });
        if (!enabled) return null;
        return runServerTest(serverName, control);
      }

      async function handleServerControl(control) {
        const action = control?.dataset?.serverAction;
        const name = control?.dataset?.serverName || control?.closest("[data-server-name]")?.dataset?.serverName;
        if (!action || !name) return;
        const server = findServer(name);
        const related = relatedInstances(name);
        if (!server && action !== "settings") return;

        if (action === "settings") {
          openServerDialog(name);
          return;
        }
        if (action === "enable-test") {
          if (server.effectiveEnabled) {
            await runServerTest(name, control);
          } else {
            await enableAndTestServer(name, control);
          }
          return;
        }
        if (action === "toggle") {
          if (!server.effectiveEnabled) {
            const message = `Turn on ${name}? This changes routing state but does not launch the upstream command. Run Test next if you need tools evidence.`;
            if (typeof window.confirm === "function" && !window.confirm(message)) return;
            await runServerAction("server-enable", { server: name }, control, "Turning on…");
          } else {
            await runServerAction("server-disable", { server: name }, control, "Turning off…");
          }
          return;
        }
        if (action === "test") {
          await runServerTest(name, control);
          return;
        }
        if (action === "workers-dec" || action === "workers-inc") {
          const delta = action === "workers-inc" ? 1 : -1;
          const nextWorkers = Math.max(1, maxWorkers(server, related) + delta);
          await runServerAction("server-policy", actionPayloadForPolicy(server, related, { maxWorkers: nextWorkers }), control, "Saving…");
          return;
        }
        if (action === "auto") {
          const recommendation = recommendedPolicy(server, related);
          await runServerAction("server-policy", actionPayloadForPolicy(server, related, recommendation), control, "Auto…");
          return;
        }
        if (action === "mode") {
          await runServerAction("server-policy", actionPayloadForPolicy(server, related, { mode: control.value }), control, "Saving…");
          return;
        }
        if (action === "apply-policy") {
          const nextWorkers = positiveInputValue('[data-server-input="workers"]', maxWorkers(server, related));
          const nextInFlight = positiveInputValue('[data-server-input="inFlight"]', maxInFlight(server, related));
          const mode = els.serverDialogBody?.querySelector('[data-server-input="mode"]')?.value || serverMode(server, related);
          await runServerAction("server-policy", actionPayloadForPolicy(server, related, {
            mode,
            maxWorkers: nextWorkers,
            maxInFlightPerWorker: nextInFlight
          }), control, "Applying…");
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
          const response = await postServerAction("server-autotune", { changes: plan.changes });
          const results = response?.result?.results || [];
          for (let index = 0; index < plan.changes.length; index += 1) {
            applyOptimisticServerAction("server-policy", plan.changes[index], { result: results[index] || {} });
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
            timedFetchJson("/api/actions/ping", { method: "POST", timeoutMs: ACTION_TIMEOUT_MS })
          ]);
          state.backend.overview = overview;
          state.backend.logs = logs;
          state.backend.resources = resources;
          state.backend.action = {
            ok: ping.ok,
            ms: ping.ms,
            at: ping.at,
            endpoint: "ping",
            error: ping.ok ? "" : apiErrorMessage(ping.error)
          };
          state.backend.checkedAt = Date.now();
          if (overview.ok) state.overview = overview.value;
          if (logs.ok) state.logs = Array.isArray(logs.value) ? logs.value : state.logs;
          state.lastError = [
            overview.ok ? "" : `Overview: ${apiErrorMessage(overview.error)}`,
            logs.ok ? "" : `Logs: ${apiErrorMessage(logs.error)}`,
            resources.ok ? "" : `Resources: ${apiErrorMessage(resources.error)}`,
            ping.ok ? "" : `Action ping: ${apiErrorMessage(ping.error)}`
          ].filter(Boolean).join(" · ") || null;
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

      function renderContext(overview, readiness, project, hub, cache, runtime) {
        const rows = [
          ["Workspace", overview.rootPath || "—"],
          ["Hub", hub.status || hub.health || "unknown"],
          ["Profile", hub.activeProfile || readiness.activeProfile || "—"],
          ["Cache", cache.hit ? `hit · ttl ${fmtMs(cache.ttlMs)}` : cache.bypassed ? "bypass" : "fresh"],
          ["Surface", runtime.surface || "dashboard-http"],
          ["Prereqs", `Rust ${project.rustSourceReady ? "ok" : "missing"} · npm ${project.npmSurfaceReady ? "ok" : "missing"} · Docker ${project.containerToolingReady ? "ok" : "missing"}`]
        ];
        els.contextList.innerHTML = rows.map(([name, value]) => `
          <article class="item"><div class="item-head"><div class="name">${escapeHtml(name)}</div></div><div class="meta">${escapeHtml(value)}</div></article>
        `).join("");
      }

      function renderInstances(instances, summary) {
        const serialized = instances.filter(instance => instance.mode === "serialized" || instance.requestMutexKey).length;
        setChip(els.instanceChip, `${instances.length || summary.serverCount || 0} planned`, instances.length ? "good" : "warn");
        if (!instances.length) {
          els.instanceList.innerHTML = `<div class="empty">No planned instances returned for this context.</div>`;
          return;
        }
        els.instanceList.innerHTML = instances.slice(0, 10).map(instance => `
          <article class="${itemClass(instance.mode === "shared" || instance.mode === "pool" ? "good" : "warn")}">
            <div class="item-head"><div class="name">${escapeHtml(instance.server || instance.serverName || "server")}</div>${chip(instance.mode || "planned", instance.mode === "shared" || instance.mode === "pool" ? "good" : "warn")}</div>
            <div class="meta">${escapeHtml(instance.trace || instance.instanceId || "no trace")}</div>
            <div class="tags">${tags([`workers ${text(instance.maxWorkers, 1)}`, `in-flight ${text(instance.maxInFlightPerWorker, 1)}`, instance.schedulerLane, instance.requestStrategy, instance.requestMutexKey ? `mutex ${instance.requestMutexKey}` : "no mutex"])}</div>
          </article>
        `).join("") + (instances.length > 10 ? `<div class="note">${instances.length - 10} more planned lane(s).</div>` : "");
      }

      function renderRuntime(runtime, hub, readiness, project) {
        const http = runtime.http || {};
        const pool = runtime.upstreamSessionPool || {};
        const control = state.overview?.runtimeControlPlane?.summary || {};
        const monitor = runtime.serverResourceMonitoring || {};
        const rows = [
          ["Runtime actions", hub.readyForRuntimeOps ? "ready" : "blocked", hub.readyForRuntimeOps ? "good" : "bad"],
          ["Read-only actions", hub.readyForReadOnlyOps ? "ready" : "blocked", hub.readyForReadOnlyOps ? "good" : "bad"],
          ["HTTP workers", `${num(http.activeConnections)}/${num(http.maxConnections) || "?"} active`, "warn"],
          ["Upstream pool", `${num(pool.size)}/${num(pool.maxSize) || "?"} sessions · ${num(pool.shardCount, 1)} shard(s)`, "warn"],
          ["Server resources", `${num(monitor.sessionCount, 0)} live session(s) · ${text(monitor.status, "waiting")}`, num(monitor.sessionCount, 0) ? "good" : "warn"],
          ["Runtime control", `${num(control.noLiveEvidence, 0)} need probe · ${num(control.approvalRequired, 0)} need approval · ${num(control.containerRequired, 0)} need container`, num(control.containerRequired, 0) ? "bad" : num(control.noLiveEvidence, 0) ? "warn" : "good"],
          ["Parallelism", `${runtime.availableParallelism ?? "?"} reported by OS`, num(runtime.availableParallelism) > 1 ? "good" : "warn"],
          ["Project", `Rust ${project.rustSourceReady ? "ok" : "missing"} · npm ${project.npmSurfaceReady ? "ok" : "missing"}`, project.rustSourceReady && project.npmSurfaceReady ? "good" : "warn"]
        ];
        setChip(els.runtimeChip, readiness.runtimePrerequisitesReady ? "ready" : "blocked", readiness.runtimePrerequisitesReady ? "good" : "bad");
        els.runtimeList.innerHTML = rows.map(([name, meta, tone]) => `<article class="${itemClass(tone)}"><div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(tone === "good" ? "ok" : "watch", tone)}</div><div class="meta">${escapeHtml(meta)}</div></article>`).join("");
      }

      function renderPolicies(rows) {
        const bad = rows.filter(row => row.risk.tone === "bad").length;
        setChip(els.policyChip, bad ? `${bad} need decisions` : "clean enough", bad ? "bad" : "good");
        if (!rows.length) {
          els.policyList.innerHTML = `<div class="empty">No configured servers to route.</div>`;
          return;
        }
        els.policyList.innerHTML = rows.slice(0, 10).map(row => `
          <article class="${itemClass(row.risk.tone)}">
            <div class="item-head"><div class="name">${escapeHtml(row.server.name || "server")}</div>${chip(row.risk.label, row.risk.tone)}</div>
            <div class="meta">${escapeHtml(row.policy)} · group ${escapeHtml(text(row.server.routingGroup))}</div>
          </article>
        `).join("");
      }

      function renderCapacity(runtime, cache, active, max, pool, sessions) {
        const http = runtime.http || {};
        const caches = runtime.caches || {};
        const saturated = max && active >= max;
        setChip(els.capacityChip, saturated ? "saturated" : "within limits", saturated ? "bad" : "good");
        const rows = [
          ["HTTP capacity", `${active}/${max || "?"} active · max observed ${num(http.maxObservedActiveConnections)} · timeout ${fmtMs(http.ioTimeoutMs)}`, saturated ? "bad" : "good", [`body ${fmtBytes(http.maxBodyBytes)}`, `${http.maxHeaderCount ?? "?"} headers`]],
          ["Dashboard cache", `${cache.hit ? "hit" : cache.bypassed ? "bypass" : "fresh"} · age ${fmtMs(cache.ageMs)} · ttl ${fmtMs(cache.ttlMs)}`, cache.stale || cache.refreshError ? "warn" : "good", [`overview ${fmtMs(caches.overviewTtlMs)}`, `health ${fmtMs(caches.healthTtlMs)}`]],
          ["HTTP sessions", `${num(sessions.size)}/${num(sessions.maxSize) || "?"} sessions · ${num(sessions.prunedExpiredSessions)} pruned`, num(sessions.size) >= num(sessions.maxSize, 1) ? "warn" : "good", [`ttl ${fmtMs(sessions.ttlMs)}`]],
          ["Upstream pool", `${num(pool.size)}/${num(pool.maxSize) || "?"} sessions · ${num(pool.lockedShardCount)}/${num(pool.shardCount) || "?"} shards locked`, num(pool.size) >= num(pool.maxSize, 1) ? "warn" : "good", [`idle ${fmtMs(pool.idleTtlMs)}`]]
        ];
        els.capacityList.innerHTML = rows.map(([name, meta, tone, tagList]) => `<article class="${itemClass(tone)}"><div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(tone === "good" ? "ok" : "watch", tone)}</div><div class="meta">${escapeHtml(meta)}</div><div class="tags">${tags(tagList)}</div></article>`).join("");
      }

      function renderTelemetry(servers, instances, http, logs) {
        const rows = [
          ["Server inventory", servers.length ? `${servers.length} servers with runtime policy fields` : "No server inventory returned", servers.length ? "good" : "warn", ["runtimeType", "stateClass", "effectClass"]],
          ["Instance plan", instances.length ? `${instances.length} lanes with workers/mutex hints` : "No server instances payload returned", instances.length ? "good" : "warn", ["mode", "trace", "workers"]],
          ["Traffic / errors", Object.keys(http).length ? `${num(http.acceptedConnections)} accepted · ${num(http.failedConnections)} failed` : "HTTP counters unavailable", Object.keys(http).length ? "good" : "warn", ["accepted", "completed", "failed"]],
          ["Request duration", Object.keys(http).length ? `avg ${fmtMs(http.requestDurationAverageMs)} · max ${fmtMs(http.requestDurationMaxMs)}` : "HTTP duration counters unavailable", Object.keys(http).length ? msTone(http.requestDurationAverageMs) : "warn", ["avg", "max", "request duration"]],
          ["Per-server CPU/RAM", "Not collected yet: current payload has limits/sessions, not OS process usage per upstream worker.", "warn", ["missing", "process telemetry"]],
          ["Logs and safe audit", Array.isArray(logs) ? `${logs.length} recent log entries loaded` : "Logs endpoint missing", Array.isArray(logs) ? "good" : "warn", ["lifecycle", "safe fingerprints"]]
        ];
        els.telemetryList.innerHTML = rows.map(([name, meta, tone, tagList]) => `<article class="${itemClass(tone)}"><div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(tone === "good" ? "available" : "gap", tone)}</div><div class="meta">${escapeHtml(meta)}</div><div class="tags">${tags(tagList)}</div></article>`).join("");
      }

      function renderActivity(leases, http, pool, sessions) {
        setChip(els.activityChip, leases.length ? `${leases.length} active locks` : "no locks", leases.length ? "warn" : "good");
        const rows = [
          ["HTTP workers", `${num(http.activeConnections)}/${num(http.maxConnections) || "?"} active · ${num(http.failedConnections)} failed`, num(http.failedConnections) ? "warn" : "good"],
          ["HTTP sessions", `${num(sessions.size)}/${num(sessions.maxSize) || "?"} sessions · ttl ${fmtMs(sessions.ttlMs)}`, "good"],
          ["Upstream pool", `${num(pool.size)}/${num(pool.maxSize) || "?"} sessions · idle ${fmtMs(pool.idleTtlMs)}`, num(pool.size) ? "warn" : "good"],
          ...(leases.length ? leases.slice(0, 6).map(lease => [lease.server || lease.serverName || lease.id || "lock", `${lease.status || "active"} · ${lease.requestMutexKey || lease.mutexKey || lease.conflictDomain || "held by scheduler"}`, "warn"]) : [["Locks", "No active locks returned by hub.", "good"]])
        ];
        els.activityList.innerHTML = rows.map(([name, meta, tone]) => `<article class="${itemClass(tone)}"><div class="item-head"><div class="name">${escapeHtml(name)}</div>${chip(tone === "good" ? "ok" : "active", tone)}</div><div class="meta">${escapeHtml(meta)}</div></article>`).join("");
      }

      function renderClients(clients, catalog) {
        setChip(els.clientChip, `${clients.length} clients`, clients.length ? "good" : "warn");
        if (!clients.length) {
          els.clientList.innerHTML = `<div class="empty">No client surfaces returned.</div>`;
          return;
        }
        els.clientList.innerHTML = clients.slice(0, 8).map(client => `<article class="item"><div class="item-head"><div class="name">${escapeHtml(client.displayName || client.id || "client")}</div>${chip(client.surfaceClass || "surface", client.surfaceClass === "local" ? "good" : "warn")}</div><div class="meta">${escapeHtml(client.id || "client")} · ${escapeHtml(client.surfaceKind || "surface")} · ingress ${escapeHtml((client.supportedIngresses || []).join(", ") || "—")}</div></article>`).join("");
      }

      function renderLogs(logs) {
        const audits = Array.isArray(logs) ? logs.filter(entry => entry.event === "tool_call_audit" || entry.event === "tool_batch_audit").slice(-6).reverse() : [];
        setChip(els.logChip, `${logs.length} logs`, logs.length ? "good" : "warn");
        els.auditList.innerHTML = audits.length ? audits.map(entry => `<article class="item"><div class="item-head"><div class="name">${escapeHtml(entry.server || "server")} · ${escapeHtml(entry.tool || entry.event || "tool")}</div>${chip(entry.bridgeOk && entry.upstreamOk ? "ok" : "watch", entry.bridgeOk && entry.upstreamOk ? "good" : "warn")}</div><div class="meta">${escapeHtml(entry.trace || "no trace")} · ${fmtDate(entry.tsMs)}</div></article>`).join("") : `<div class="empty">No tool-call audit entries yet.</div>`;
        els.logList.innerHTML = Array.isArray(logs) && logs.length ? logs.slice(-8).reverse().map(entry => `<article class="item"><div class="item-head"><div class="name">${escapeHtml(entry.event || "event")}</div>${chip(entry.level || "info", entry.level === "error" ? "bad" : entry.level === "warn" ? "warn" : "good")}</div><div class="meta">${fmtDate(entry.tsMs)}</div><details class="server-settings"><summary>Raw payload</summary><pre>${escapeHtml(JSON.stringify(entry, null, 2))}</pre></details></article>`).join("") : `<div class="empty">No recent log entries.</div>`;
      }

      function syncControls() {
        els.refreshSelect.value = state.refreshMode;
        els.serverSort.value = state.sort;
        els.serverScope.value = state.scope;
        els.densitySelect.value = state.density;
        els.serverSearch.value = state.query;
        document.body.dataset.density = state.density;
      }

      function bindSelect(element, key, pref, onChange = render) {
        element.addEventListener("change", event => {
          state[key] = event.target.value;
          writePref(pref, state[key]);
          onChange();
        });
      }

      syncControls();
      bindSelect(els.refreshSelect, "refreshMode", "refreshMode", () => { scheduleRefresh(); updateRefreshChip(); });
      bindSelect(els.serverSort, "sort", "sort");
      bindSelect(els.serverScope, "scope", "scope");
      bindSelect(els.densitySelect, "density", "density", () => { document.body.dataset.density = state.density; render(); });

      els.refreshButton.addEventListener("click", () => refreshDashboard({ force: true, reason: "manual" }));
      els.focusAddServer?.addEventListener("click", focusInstallCommand);
      els.opsCommandRow?.addEventListener("click", event => {
        const control = event.target.closest("[data-global-action]");
        if (control) handleGlobalAction(control);
      });
      els.baseSetup?.addEventListener("click", event => {
        const control = event.target.closest("[data-global-action]");
        if (control) handleGlobalAction(control);
      });
      els.nextActionBoard?.addEventListener("click", event => {
        const control = event.target.closest("[data-global-action]");
        if (control) handleGlobalAction(control);
      });
      els.mobileActionDock?.addEventListener("click", event => {
        const control = event.target.closest("[data-global-action]");
        if (control) handleGlobalAction(control);
      });
      els.setupQueue?.addEventListener("click", event => {
        const control = event.target.closest("[data-global-action]");
        if (control) handleGlobalAction(control);
      });
      els.backendCheckButton?.addEventListener("click", event => checkBackendLink(event.currentTarget));
      els.startButton.addEventListener("click", event => runAction("/api/actions/hub-up", event.currentTarget, "", "Starting…"));
      els.stopButton.addEventListener("click", event => runAction("/api/actions/hub-down", event.currentTarget, "Stop the local MCPace hub? Active clients may lose routing until it starts again.", "Stopping…"));
      els.repairButton.addEventListener("click", event => runAction("/api/actions/repair", event.currentTarget, "Run MCPace repair now? This may update local runtime wiring and client config files.", "Repairing…"));
      els.serverSearch.addEventListener("input", event => { state.query = event.target.value; render(); });
      els.clearSearch.addEventListener("click", () => { state.query = ""; els.serverSearch.value = ""; render(); els.serverSearch.focus(); });
      els.toggleEnabled.addEventListener("click", () => { state.enabledOnly = !state.enabledOnly; writePref("enabledOnly", String(state.enabledOnly)); render(); });
      els.serverImportForm?.addEventListener("submit", submitServerImportConfig);
      els.clientSetupPanel?.addEventListener("click", handleClientSetupClick);
      els.clientPreviewAll?.addEventListener("click", event => runClientSetupAction("client-install", { clientId: "all", dryRun: true, diff: true }, event.currentTarget, "Previewing…"));
      els.clientApplyAll?.addEventListener("click", event => {
        if (!window.confirm("Apply the MCPace client patch to every supported local client? Preview first if you have not reviewed the diff.")) return;
        runClientSetupAction("client-install", { clientId: "all", dryRun: false, diff: false }, event.currentTarget, "Applying…");
      });
      els.clientRestoreAll?.addEventListener("click", event => {
        if (!window.confirm("Restore the latest MCPace backup for every supported local client?")) return;
        runClientSetupAction("client-restore", { clientId: "all", backup: "latest" }, event.currentTarget, "Restoring…");
      });
      els.serverImportPath?.addEventListener("input", updateServerImportPreflight);
      els.serverImportDryRun?.addEventListener("change", updateServerImportPreflight);
      els.serverImportDisabled?.addEventListener("change", updateServerImportPreflight);
      els.serverDiscoverForm?.addEventListener("submit", submitServerDiscovery);
      els.serverInstallForm.addEventListener("submit", submitServerInstallCommand);
      els.serverInstallCommand.addEventListener("input", updateServerInstallPreflight);
      updateServerImportPreflight();
      updateServerInstallPreflight();
      els.autoTuneVisible.addEventListener("click", event => autoTuneVisibleServers(event.currentTarget));
      els.serverFleetBoard.addEventListener("click", event => {
        const card = event.target.closest("[data-server-bucket]");
        if (!card) return;
        const next = card.dataset.serverBucket || "all";
        state.bucket = state.bucket === next ? "all" : next;
        if (state.bucket === "off") {
          state.enabledOnly = false;
          writePref("enabledOnly", "false");
        }
        writePref("bucket", state.bucket);
        render();
      });
      els.serverList.addEventListener("click", event => {
        const emptyAction = event.target.closest("[data-empty-action]");
        if (emptyAction) {
          handleEmptyStateAction(emptyAction);
          return;
        }
        const control = event.target.closest("[data-server-action]");
        if (!control || control.tagName === "SELECT") return;
        handleServerControl(control);
      });
      els.serverList.addEventListener("change", event => {
        const control = event.target.closest("[data-server-action]");
        if (control) handleServerControl(control);
      });
      els.serverDialogBody.addEventListener("click", event => {
        const control = event.target.closest("[data-server-action]");
        if (!control || control.tagName === "SELECT") return;
        handleServerControl(control);
      });
      els.serverDialogBody.addEventListener("change", event => {
        const control = event.target.closest("[data-server-action]");
        if (control) handleServerControl(control);
      });
      els.serverDialogClose.addEventListener("click", closeServerDialog);
      els.serverDialog.addEventListener("click", event => {
        if (event.target === els.serverDialog) closeServerDialog();
      });
      els.serverDialog.addEventListener("close", () => { state.selectedServer = null; });
      document.addEventListener("visibilitychange", () => {
        if (document.visibilityState === "hidden" || state.refreshMode === "paused") scheduleRefresh();
        else if (Date.now() - (state.lastRefreshFinishedAt || 0) < VISIBLE_REFRESH_MIN_INTERVAL_MS) scheduleRefresh();
        else refreshDashboard({ reason: "visible" });
      });
      document.addEventListener("freeze", () => {
        state.lifecycle.frozen = true;
        state.lifecycle.freezeCount += 1;
        if (state.controller) state.controller.abort(new DOMException("Page frozen", "AbortError"));
        scheduleRefresh();
      });
      document.addEventListener("resume", () => {
        state.lifecycle.frozen = false;
        state.lifecycle.resumeCount += 1;
        state.lifecycle.lastResumeAt = Date.now();
        if (state.refreshMode === "paused") return;
        if (Date.now() - (state.lastRefreshFinishedAt || 0) < LIFECYCLE_RESUME_MIN_INTERVAL_MS) scheduleRefresh();
        else refreshDashboard({ reason: "resume" });
      });
      window.addEventListener("pageshow", event => {
        const discarded = Boolean(document.wasDiscarded);
        state.lifecycle.wasDiscarded = state.lifecycle.wasDiscarded || discarded;
        if (event.persisted || discarded) {
          state.failureCount = 0;
          refreshDashboard({ allowHidden: true, reason: "pageshow" });
        }
      });

      window.__mcpaceDashboard = { state, refreshDashboard, runAction, checkBackendLink, render, openServerDialog };
      refreshDashboard({ allowHidden: true, reason: "initial" });
