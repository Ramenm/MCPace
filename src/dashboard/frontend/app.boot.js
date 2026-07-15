// MCPace dashboard event wiring and lifecycle bootstrap chunk. Loaded after shared render/action chunks.
function syncControls() {
	els.refreshSelect.value = state.refreshMode;
	els.serverSort.value = state.sort;
	els.serverScope.value = state.scope;
	els.densitySelect.value = state.density;
	els.serverSearch.value = state.query;
	document.body.dataset.density = state.density;
}

function bindSelect(element, key, pref, onChange = render) {
	element.addEventListener("change", (event) => {
		state[key] = event.target.value;
		writePref(pref, state[key]);
		onChange();
	});
}

syncControls();
setSetupTab(state.setupTab);
setDiagnosticTab(state.diagnosticTab);
bindSelect(els.refreshSelect, "refreshMode", "refreshMode", () => {
	scheduleRefresh();
	updateRefreshChip();
});
bindSelect(els.serverSort, "sort", "sort");
bindSelect(els.serverScope, "scope", "scope");
bindSelect(els.densitySelect, "density", "density", () => {
	document.body.dataset.density = state.density;
	render();
});

els.refreshButton.addEventListener("click", () =>
	refreshDashboard({ force: true, reason: "manual" }),
);
els.updateCheckButton?.addEventListener("click", (event) =>
	checkForUpdates(event.currentTarget, { manual: true }),
);
els.updateNotice?.addEventListener("click", (event) => {
	const control = event.target.closest("[data-update-action]");
	if (!control) return;
	if (control.dataset.updateAction === "copy") copyUpdateCommand(control);
	else if (control.dataset.updateAction === "check")
		checkForUpdates(control, { manual: true });
});
document.addEventListener("click", (event) => {
	const control = event.target.closest("[data-global-action]");
	if (control) handleGlobalAction(control);
});
const setupTablist = document
	.querySelector('[role="tablist"] [data-setup-target]')
	?.closest('[role="tablist"]');
setupTablist?.addEventListener("click", (event) => {
	const tab = event.target.closest("[data-setup-target]");
	if (tab) setSetupTab(tab.dataset.setupTarget);
});
setupTablist?.addEventListener("keydown", (event) =>
	handleTablistKeydown(
		event,
		"[data-setup-target]",
		"setupTarget",
		setSetupTab,
	),
);
const diagnosticTablist = document
	.querySelector('[role="tablist"] [data-diagnostic-target]')
	?.closest('[role="tablist"]');
diagnosticTablist?.addEventListener("click", (event) => {
	const tab = event.target.closest("[data-diagnostic-target]");
	if (tab) setDiagnosticTab(tab.dataset.diagnosticTarget);
});
diagnosticTablist?.addEventListener("keydown", (event) =>
	handleTablistKeydown(
		event,
		"[data-diagnostic-target]",
		"diagnosticTarget",
		setDiagnosticTab,
	),
);
const serverDialogTablist = document.querySelector(".server-dialog-tabs");
serverDialogTablist?.addEventListener("click", (event) => {
	const tab = event.target.closest("[data-server-dialog-tab]");
	if (tab) setServerDialogTab(tab.dataset.serverDialogTab);
});
els.backendCheckButton?.addEventListener("click", (event) =>
	checkBackendLink(event.currentTarget),
);
els.startButton.addEventListener("click", (event) =>
	runAction("/api/actions/hub-up", event.currentTarget, "", "Starting…"),
);
els.stopButton.addEventListener("click", (event) =>
	runAction(
		"/api/actions/hub-down",
		event.currentTarget,
		"Stop the local MCPace hub? Active clients may lose routing until it starts again.",
		"Stopping…",
	),
);
els.repairButton.addEventListener("click", (event) =>
	runAction(
		"/api/actions/repair",
		event.currentTarget,
		"Run MCPace repair now? This may update local runtime wiring and client config files.",
		"Repairing…",
	),
);
els.serverSearch.addEventListener("input", (event) => {
	state.query = event.target.value;
	render();
});
els.clearSearch.addEventListener("click", () => {
	state.query = "";
	els.serverSearch.value = "";
	render();
	els.serverSearch.focus();
});
els.toggleEnabled.addEventListener("click", () => {
	state.enabledOnly = !state.enabledOnly;
	writePref("enabledOnly", String(state.enabledOnly));
	render();
});
els.serverImportForm?.addEventListener("submit", submitServerImportConfig);
els.clientSetupPanel?.addEventListener("click", handleClientSetupClick);
els.clientPreviewAll?.addEventListener("click", (event) =>
	runClientSetupAction(
		"client-install",
		{ clientId: "all", dryRun: true, diff: true },
		event.currentTarget,
		"Previewing…",
	),
);
els.clientApplyAll?.addEventListener("click", (event) => {
	if (
		!window.confirm(
			"Apply the MCPace client patch to every supported local client? Preview first if you have not reviewed the diff.",
		)
	)
		return;
	runClientSetupAction(
		"client-install",
		{ clientId: "all", dryRun: false, diff: false },
		event.currentTarget,
		"Applying…",
	);
});
els.clientRestoreAll?.addEventListener("click", (event) => {
	if (
		!window.confirm(
			"Restore the latest MCPace backup for every supported local client?",
		)
	)
		return;
	runClientSetupAction(
		"client-restore",
		{ clientId: "all", backup: "latest" },
		event.currentTarget,
		"Restoring…",
	);
});
els.serverImportPath?.addEventListener("input", updateServerImportPreflight);
els.serverImportDryRun?.addEventListener("change", updateServerImportPreflight);
els.serverImportDisabled?.addEventListener(
	"change",
	updateServerImportPreflight,
);
els.serverDiscoverForm?.addEventListener("submit", submitServerDiscovery);
els.serverInstallForm.addEventListener("submit", submitServerInstallCommand);
els.serverInstallCommand.addEventListener(
	"input",
	updateServerInstallPreflight,
);
updateServerImportPreflight();
updateServerInstallPreflight();
els.autoTuneVisible.addEventListener("click", (event) =>
	autoTuneVisibleServers(event.currentTarget),
);
els.serverFleetBoard.addEventListener("click", (event) => {
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
els.serverList.addEventListener("click", (event) => {
	const emptyAction = event.target.closest("[data-empty-action]");
	if (emptyAction) {
		handleEmptyStateAction(emptyAction);
		return;
	}
	const control = event.target.closest("[data-server-action]");
	if (!control || control.tagName === "SELECT") return;
	handleServerControl(control);
});
els.serverList.addEventListener("change", (event) => {
	const control = event.target.closest("[data-server-action]");
	if (control) handleServerControl(control);
});
els.serverDialogBody.addEventListener("click", (event) => {
	const control = event.target.closest("[data-server-action]");
	if (!control || control.tagName === "SELECT") return;
	handleServerControl(control);
});
els.serverDialogBody.addEventListener("change", (event) => {
	const control = event.target.closest("[data-server-action]");
	if (control) handleServerControl(control);
});
els.serverDialogClose.addEventListener("click", closeServerDialog);
els.serverDialog.addEventListener("click", (event) => {
	if (event.target === els.serverDialog) closeServerDialog();
});
els.serverDialog.addEventListener("close", () => {
	state.selectedServer = null;
});
document.addEventListener("visibilitychange", () => {
	if (document.visibilityState === "hidden" || state.refreshMode === "paused")
		scheduleRefresh();
	else if (
		Date.now() - (state.lastRefreshFinishedAt || 0) <
		VISIBLE_REFRESH_MIN_INTERVAL_MS
	)
		scheduleRefresh();
	else refreshDashboard({ reason: "visible" });
});
document.addEventListener("freeze", () => {
	state.lifecycle.frozen = true;
	state.lifecycle.freezeCount += 1;
	if (state.controller)
		state.controller.abort(new DOMException("Page frozen", "AbortError"));
	scheduleRefresh();
});
document.addEventListener("resume", () => {
	state.lifecycle.frozen = false;
	state.lifecycle.resumeCount += 1;
	state.lifecycle.lastResumeAt = Date.now();
	if (state.refreshMode === "paused") return;
	if (
		Date.now() - (state.lastRefreshFinishedAt || 0) <
		LIFECYCLE_RESUME_MIN_INTERVAL_MS
	)
		scheduleRefresh();
	else refreshDashboard({ reason: "resume" });
});
window.addEventListener("pageshow", (event) => {
	const discarded = Boolean(document.wasDiscarded);
	state.lifecycle.wasDiscarded = state.lifecycle.wasDiscarded || discarded;
	if (event.persisted || discarded) {
		state.failureCount = 0;
		refreshDashboard({ allowHidden: true, reason: "pageshow" });
	}
});

window.__mcpaceDashboard = {
	state,
	refreshDashboard,
	runAction,
	checkBackendLink,
	checkForUpdates,
	render,
	openServerDialog,
	setSetupTab,
	setDiagnosticTab,
	requestProductView,
};
refreshDashboard({ allowHidden: true, reason: "initial" });
window.setTimeout(() => {
	if (document.visibilityState !== "hidden")
		checkForUpdates(null, { manual: false });
}, 750);
