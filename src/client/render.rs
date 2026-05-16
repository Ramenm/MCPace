use crate::json::JsonValue;
use std::collections::BTreeMap;
use std::io::Write;

use super::model::{ClientPlan, ServerCoordinationPlan};

impl ClientPlan {
    pub(super) fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("rootPath", JsonValue::string(self.root_path.clone())),
            (
                "configVersion",
                match &self.config_version {
                    Some(value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "configuredClientKeyName",
                match &self.configured_client_key_name {
                    Some(value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "clientTargetId",
                match &self.client_target_id {
                    Some(value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "clientTargetFamilyId",
                match &self.client_target_family_id {
                    Some(value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "clientTargetMaturity",
                JsonValue::string(self.client_target_maturity.clone()),
            ),
            (
                "clientTargetSurfaceClass",
                JsonValue::string(self.client_target_surface_class.clone()),
            ),
            (
                "clientTargetSurfaceKind",
                JsonValue::string(self.client_target_surface_kind.clone()),
            ),
            (
                "clientTargetDocumentedFeatures",
                JsonValue::array(
                    self.client_target_documented_features
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "clientTargetDocumentedConstraints",
                JsonValue::array(
                    self.client_target_documented_constraints
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "entrypointMode",
                JsonValue::string(self.entrypoint_mode.clone()),
            ),
            (
                "launcherCommand",
                JsonValue::string(self.launcher_command.clone()),
            ),
            (
                "currentGroupedAction",
                JsonValue::string(self.current_grouped_action.clone()),
            ),
            (
                "preferredIngress",
                JsonValue::string(self.preferred_ingress.clone()),
            ),
            (
                "preferredIngressSource",
                JsonValue::string(self.preferred_ingress_source.clone()),
            ),
            (
                "supportedIngresses",
                JsonValue::array(
                    self.supported_ingresses
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "hubLifecycleImplemented",
                JsonValue::bool(self.hub_lifecycle_implemented),
            ),
            (
                "clientInstallImplemented",
                JsonValue::bool(self.client_install_implemented),
            ),
            (
                "clientExportImplemented",
                JsonValue::bool(self.client_export_implemented),
            ),
            (
                "sessionBindingKey",
                JsonValue::string(self.session_binding_key.clone()),
            ),
            (
                "requiresHubOwnedStdio",
                JsonValue::bool(self.requires_hub_owned_stdio),
            ),
            (
                "parallelSafeServerCount",
                JsonValue::number(self.parallel_safe_servers),
            ),
            (
                "serializedServerCount",
                JsonValue::number(self.serialized_servers),
            ),
            (
                "exclusiveServerCount",
                JsonValue::number(self.exclusive_servers),
            ),
            (
                "context",
                JsonValue::object([
                    (
                        "clientId",
                        JsonValue::string(self.context.client_id.clone()),
                    ),
                    (
                        "clientIdSource",
                        JsonValue::string(self.context.client_id_source.clone()),
                    ),
                    (
                        "sessionId",
                        match &self.context.session_id {
                            Some(value) => JsonValue::string(value.clone()),
                            None => JsonValue::Null,
                        },
                    ),
                    (
                        "sessionIdSource",
                        JsonValue::string(self.context.session_id_source.clone()),
                    ),
                    (
                        "sessionLeaseId",
                        JsonValue::string(self.context.session_lease_id.clone()),
                    ),
                    (
                        "sessionLeaseSource",
                        JsonValue::string(self.context.session_lease_source.clone()),
                    ),
                    (
                        "conversationId",
                        match &self.context.conversation_id {
                            Some(value) => JsonValue::string(value.clone()),
                            None => JsonValue::Null,
                        },
                    ),
                    (
                        "conversationIdSource",
                        JsonValue::string(self.context.conversation_id_source.clone()),
                    ),
                    (
                        "clientInstanceId",
                        match &self.context.client_instance_id {
                            Some(value) => JsonValue::string(value.clone()),
                            None => JsonValue::Null,
                        },
                    ),
                    (
                        "clientInstanceIdSource",
                        JsonValue::string(self.context.client_instance_id_source.clone()),
                    ),
                    (
                        "transportSessionId",
                        match &self.context.transport_session_id {
                            Some(value) => JsonValue::string(value.clone()),
                            None => JsonValue::Null,
                        },
                    ),
                    (
                        "transportSessionIdSource",
                        JsonValue::string(self.context.transport_session_id_source.clone()),
                    ),
                    (
                        "credentialProfileId",
                        match &self.context.credential_profile_id {
                            Some(value) => JsonValue::string(value.clone()),
                            None => JsonValue::Null,
                        },
                    ),
                    (
                        "credentialProfileIdSource",
                        JsonValue::string(self.context.credential_profile_id_source.clone()),
                    ),
                    (
                        "projectRoot",
                        match &self.context.project_root {
                            Some(value) => JsonValue::string(value.clone()),
                            None => JsonValue::Null,
                        },
                    ),
                    (
                        "projectRootSource",
                        JsonValue::string(self.context.project_root_source.clone()),
                    ),
                    (
                        "workspaceRoots",
                        JsonValue::array(
                            self.context
                                .workspace_roots
                                .iter()
                                .cloned()
                                .map(JsonValue::string),
                        ),
                    ),
                    (
                        "cwd",
                        match &self.context.cwd {
                            Some(value) => JsonValue::string(value.clone()),
                            None => JsonValue::Null,
                        },
                    ),
                    (
                        "cwdSource",
                        JsonValue::string(self.context.cwd_source.clone()),
                    ),
                ]),
            ),
            (
                "warnings",
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            ),
            (
                "servers",
                JsonValue::array(
                    self.servers
                        .iter()
                        .map(ServerCoordinationPlan::to_json_value),
                ),
            ),
        ])
    }
}

impl ServerCoordinationPlan {
    pub(super) fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("name", JsonValue::string(self.name.clone())),
            (
                "admissionState",
                JsonValue::string(self.admission_state.clone()),
            ),
            ("scopeClass", JsonValue::string(self.scope_class.clone())),
            (
                "concurrencyPolicy",
                JsonValue::string(self.concurrency_policy.clone()),
            ),
            (
                "upstreamTransport",
                JsonValue::string(self.upstream_transport.clone()),
            ),
            (
                "processPartition",
                JsonValue::string(self.process_partition.clone()),
            ),
            (
                "processScopeKey",
                JsonValue::string(self.process_scope_key.clone()),
            ),
            (
                "projectBindingKey",
                match &self.project_binding_key {
                    Some(value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "worktreeBindingKey",
                match &self.worktree_binding_key {
                    Some(value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "conflictDomain",
                JsonValue::string(self.conflict_domain.clone()),
            ),
            (
                "hostLockKey",
                match &self.host_lock_key {
                    Some(value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "stateProfileKey",
                match &self.state_profile_key {
                    Some(value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "parallelismLimit",
                JsonValue::number(self.parallelism_limit),
            ),
            (
                "parallelSafetyClass",
                JsonValue::string(self.parallel_safety_class.clone()),
            ),
            (
                "defaultPoolModel",
                JsonValue::string(self.default_pool_model.clone()),
            ),
            ("workerPoolKey", JsonValue::string(self.worker_pool_key.clone())),
            ("maxWorkers", JsonValue::number(self.max_workers)),
            (
                "maxInFlightPerWorker",
                JsonValue::number(self.max_in_flight_per_worker),
            ),
            (
                "lockDomains",
                JsonValue::array(self.lock_domains.iter().cloned().map(JsonValue::string)),
            ),
            (
                "transportStatus",
                JsonValue::string(self.transport_status.clone()),
            ),
            ("launcherKind", JsonValue::string(self.launcher_kind.clone())),
            (
                "schedulerLane",
                JsonValue::string(self.scheduler_lane.clone()),
            ),
            (
                "startupStrategy",
                JsonValue::string(self.startup_strategy.clone()),
            ),
            (
                "requestStrategy",
                JsonValue::string(self.request_strategy.clone()),
            ),
            (
                "requestMutexKey",
                match &self.request_mutex_key {
                    Some(value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "sessionAffinityKey",
                match &self.session_affinity_key {
                    Some(value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "warnings",
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

pub(super) fn write_text_plan(plan: &ClientPlan, stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Single entry point: {} ({})",
        plan.launcher_command, plan.entrypoint_mode
    );
    let _ = writeln!(
        stdout,
        "Current grouped action: {}",
        plan.current_grouped_action
    );
    let _ = writeln!(
        stdout,
        "Configured adapter key name: {}",
        plan.configured_client_key_name.as_deref().unwrap_or("none")
    );
    let _ = writeln!(
        stdout,
        "Recognized client target: {} [{}]",
        plan.client_target_id.as_deref().unwrap_or("unknown"),
        plan.client_target_maturity
    );
    let _ = writeln!(
        stdout,
        "Preferred ingress: {} ({})",
        plan.preferred_ingress, plan.preferred_ingress_source
    );
    let _ = writeln!(
        stdout,
        "Supported ingresses: {}",
        plan.supported_ingresses.join(", ")
    );
    let _ = writeln!(
        stdout,
        "Client id: {} ({})",
        plan.context.client_id, plan.context.client_id_source
    );
    let _ = writeln!(
        stdout,
        "External session id: {} ({})",
        plan.context.session_id.as_deref().unwrap_or("none"),
        plan.context.session_id_source
    );
    let _ = writeln!(
        stdout,
        "Session lease id: {} ({})",
        plan.context.session_lease_id, plan.context.session_lease_source
    );
    let _ = writeln!(
        stdout,
        "Conversation id: {} ({})",
        plan.context.conversation_id.as_deref().unwrap_or("none"),
        plan.context.conversation_id_source
    );
    let _ = writeln!(
        stdout,
        "Client instance id: {} ({})",
        plan.context.client_instance_id.as_deref().unwrap_or("none"),
        plan.context.client_instance_id_source
    );
    let _ = writeln!(
        stdout,
        "Transport session id: {} ({})",
        plan.context
            .transport_session_id
            .as_deref()
            .unwrap_or("none"),
        plan.context.transport_session_id_source
    );
    let _ = writeln!(
        stdout,
        "Credential profile id: {} ({})",
        plan.context
            .credential_profile_id
            .as_deref()
            .unwrap_or("none"),
        plan.context.credential_profile_id_source
    );
    let _ = writeln!(
        stdout,
        "Workspace roots: {}",
        join_or_none(&plan.context.workspace_roots)
    );
    let _ = writeln!(
        stdout,
        "Cwd: {} ({})",
        plan.context.cwd.as_deref().unwrap_or("none"),
        plan.context.cwd_source
    );
    let _ = writeln!(
        stdout,
        "Project root: {} ({})",
        plan.context.project_root.as_deref().unwrap_or("unresolved"),
        plan.context.project_root_source
    );
    let _ = writeln!(stdout, "Session binding key: {}", plan.session_binding_key);
    let _ = writeln!(
        stdout,
        "Hub-owned stdio required: {}",
        yes_no(plan.requires_hub_owned_stdio)
    );
    let _ = writeln!(
        stdout,
        "Parallel-safe servers: {}",
        plan.parallel_safe_servers
    );
    let _ = writeln!(stdout, "Serialized servers: {}", plan.serialized_servers);
    let _ = writeln!(stdout, "Exclusive servers: {}", plan.exclusive_servers);
    let _ = writeln!(stdout, "Warnings: {}", join_or_none(&plan.warnings));
    let _ = writeln!(stdout, "Server arbitration:");
    for server in &plan.servers {
        let _ = writeln!(
            stdout,
            "- {} admission={} scope={} concurrency={} transport={}",
            server.name,
            server.admission_state,
            server.scope_class,
            server.concurrency_policy,
            server.upstream_transport
        );
        let _ = writeln!(stdout, "    processPartition={}", server.process_partition);
        let _ = writeln!(stdout, "    process={}", server.process_scope_key);
        let _ = writeln!(
            stdout,
            "    projectBinding={}",
            server.project_binding_key.as_deref().unwrap_or("none")
        );
        let _ = writeln!(
            stdout,
            "    worktreeBinding={}",
            server.worktree_binding_key.as_deref().unwrap_or("none")
        );
        let _ = writeln!(stdout, "    conflictDomain={}", server.conflict_domain);
        let _ = writeln!(
            stdout,
            "    hostLock={}",
            server.host_lock_key.as_deref().unwrap_or("none")
        );
        let _ = writeln!(
            stdout,
            "    stateProfile={}",
            server.state_profile_key.as_deref().unwrap_or("none")
        );
        let _ = writeln!(stdout, "    parallelismLimit={}", server.parallelism_limit);
        let _ = writeln!(
            stdout,
            "    adaptiveProfile={} pool={} workers={} maxInFlightPerWorker={} transportStatus={}",
            server.parallel_safety_class,
            server.default_pool_model,
            server.max_workers,
            server.max_in_flight_per_worker,
            server.transport_status
        );
        let _ = writeln!(stdout, "    workerPoolKey={}", server.worker_pool_key);
        if !server.lock_domains.is_empty() {
            let _ = writeln!(stdout, "    lockDomains={}", server.lock_domains.join(", "));
        }
        let _ = writeln!(stdout, "    schedulerLane={}", server.scheduler_lane);
        let _ = writeln!(stdout, "    startupStrategy={}", server.startup_strategy);
        let _ = writeln!(stdout, "    requestStrategy={}", server.request_strategy);
        let _ = writeln!(
            stdout,
            "    requestMutex={}",
            server.request_mutex_key.as_deref().unwrap_or("none")
        );
        let _ = writeln!(
            stdout,
            "    sessionAffinity={}",
            server.session_affinity_key.as_deref().unwrap_or("none")
        );
        let _ = writeln!(stdout, "    warnings={}", join_or_none(&server.warnings));
    }
}

pub(super) fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join("; ")
    }
}

pub(super) fn count_static<I>(values: I) -> BTreeMap<String, usize>
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value.into()).or_default() += 1;
    }
    counts
}

pub(super) fn join_count_map(values: &BTreeMap<String, usize>) -> String {
    if values.is_empty() {
        return "none".to_string();
    }
    values
        .iter()
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<_>>()
        .join(", ")
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
