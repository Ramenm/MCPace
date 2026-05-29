use crate::json::JsonValue;

#[derive(Debug, Clone)]
pub struct ServerRecord {
    pub name: String,
    pub kind: String,
    pub required: bool,
    pub default_enabled: bool,
    pub profile_enabled: bool,
    pub platform_supported: bool,
    pub effective_enabled: bool,
    pub auto_start: bool,
    pub transport_preference: String,
    pub supported_transports: Vec<String>,
    pub platforms: Vec<String>,
    pub required_commands: Vec<String>,
    pub scope_class: String,
    pub concurrency_policy: String,
    pub state_binding: String,
    pub credential_binding: String,
    pub parallelism_limit: usize,
    pub parallel_safety_class: String,
    pub runtime_type: String,
    pub state_class: String,
    pub effect_class: String,
    pub default_pool_model: String,
    pub max_workers: usize,
    pub max_in_flight_per_worker: usize,
    pub transport_status: String,
    pub launcher_kind: String,
    pub lock_domains: Vec<String>,
    pub profile_evidence: Vec<JsonValue>,
    pub conflict_domain: String,
    pub project_root_mode: String,
    pub worktree_binding: String,
    pub state_profile_mode: String,
    pub host_lock: String,
    pub startup_strategy: String,
    pub routing_group: String,
    pub discovery_requires_lease: bool,
    pub health_url: String,
    pub source_enabled: bool,
    pub source_type: String,
    pub source_path: String,
    pub source_command: String,
    pub source_args: Vec<String>,
    pub source_env_names: Vec<String>,
    pub source_header_names: Vec<String>,
    pub source_url: String,
    pub tool_policies: Vec<JsonValue>,
    pub installer_target: String,
    pub installer_method: String,
    pub installer_package: String,
    pub installer_verify_command: String,
}

#[derive(Debug, Clone)]
pub(super) struct SourceServerRecord {
    pub(super) name: String,
    pub(super) enabled: bool,
    pub(super) source_type: String,
    pub(super) command: String,
    pub(super) url: String,
    pub(super) args: Vec<String>,
    pub(super) env_names: Vec<String>,
    pub(super) header_names: Vec<String>,
    pub(super) source_path: String,
    pub(super) profile_hints: Vec<String>,
}

impl ServerRecord {
    pub(super) fn summary_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("name", JsonValue::string(self.name.clone())),
            ("kind", JsonValue::string(self.kind.clone())),
            ("required", JsonValue::bool(self.required)),
            ("defaultEnabled", JsonValue::bool(self.default_enabled)),
            ("profileEnabled", JsonValue::bool(self.profile_enabled)),
            (
                "platformSupported",
                JsonValue::bool(self.platform_supported),
            ),
            ("sourceEnabled", JsonValue::bool(self.source_enabled)),
            ("sourceType", JsonValue::string(self.source_type.clone())),
            ("sourcePath", JsonValue::string(self.source_path.clone())),
            (
                "sourceCommand",
                JsonValue::string(self.source_command.clone()),
            ),
            (
                "sourceArgs",
                JsonValue::array(self.source_args.iter().cloned().map(JsonValue::string)),
            ),
            (
                "sourceEnvNames",
                JsonValue::array(self.source_env_names.iter().cloned().map(JsonValue::string)),
            ),
            (
                "sourceHeaderNames",
                JsonValue::array(
                    self.source_header_names
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            ("sourceUrl", JsonValue::string(self.source_url.clone())),
            ("effectiveEnabled", JsonValue::bool(self.effective_enabled)),
            (
                "transportPreference",
                JsonValue::string(self.transport_preference.clone()),
            ),
            ("scopeClass", JsonValue::string(self.scope_class.clone())),
            (
                "concurrencyPolicy",
                JsonValue::string(self.concurrency_policy.clone()),
            ),
            (
                "stateBinding",
                JsonValue::string(self.state_binding.clone()),
            ),
            (
                "credentialBinding",
                JsonValue::string(self.credential_binding.clone()),
            ),
            (
                "parallelismLimit",
                JsonValue::number(self.parallelism_limit),
            ),
            (
                "parallelSafetyClass",
                JsonValue::string(self.parallel_safety_class.clone()),
            ),
            ("runtimeType", JsonValue::string(self.runtime_type.clone())),
            ("stateClass", JsonValue::string(self.state_class.clone())),
            ("effectClass", JsonValue::string(self.effect_class.clone())),
            (
                "defaultPoolModel",
                JsonValue::string(self.default_pool_model.clone()),
            ),
            ("maxWorkers", JsonValue::number(self.max_workers)),
            (
                "maxInFlightPerWorker",
                JsonValue::number(self.max_in_flight_per_worker),
            ),
            (
                "transportStatus",
                JsonValue::string(self.transport_status.clone()),
            ),
            (
                "launcherKind",
                JsonValue::string(self.launcher_kind.clone()),
            ),
            (
                "lockDomains",
                JsonValue::array(self.lock_domains.iter().cloned().map(JsonValue::string)),
            ),
            (
                "profileEvidence",
                JsonValue::array(self.profile_evidence.clone()),
            ),
            (
                "conflictDomain",
                JsonValue::string(self.conflict_domain.clone()),
            ),
            (
                "projectRootMode",
                JsonValue::string(self.project_root_mode.clone()),
            ),
            (
                "worktreeBinding",
                JsonValue::string(self.worktree_binding.clone()),
            ),
            (
                "stateProfileMode",
                JsonValue::string(self.state_profile_mode.clone()),
            ),
            ("hostLock", JsonValue::string(self.host_lock.clone())),
            (
                "startupStrategy",
                JsonValue::string(self.startup_strategy.clone()),
            ),
            (
                "routingGroup",
                JsonValue::string(self.routing_group.clone()),
            ),
            (
                "discoveryRequiresLease",
                JsonValue::bool(self.discovery_requires_lease),
            ),
        ])
    }

    pub(super) fn capabilities_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("name", JsonValue::string(self.name.clone())),
            ("kind", JsonValue::string(self.kind.clone())),
            ("required", JsonValue::bool(self.required)),
            ("autoStart", JsonValue::bool(self.auto_start)),
            ("profileEnabled", JsonValue::bool(self.profile_enabled)),
            (
                "platformSupported",
                JsonValue::bool(self.platform_supported),
            ),
            ("effectiveEnabled", JsonValue::bool(self.effective_enabled)),
            (
                "supportedTransports",
                JsonValue::array(
                    self.supported_transports
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "platforms",
                JsonValue::array(self.platforms.iter().cloned().map(JsonValue::string)),
            ),
            (
                "requiredCommands",
                JsonValue::array(
                    self.required_commands
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "routingPolicy",
                JsonValue::object([
                    ("scopeClass", JsonValue::string(self.scope_class.clone())),
                    (
                        "concurrencyPolicy",
                        JsonValue::string(self.concurrency_policy.clone()),
                    ),
                    (
                        "stateBinding",
                        JsonValue::string(self.state_binding.clone()),
                    ),
                    (
                        "credentialBinding",
                        JsonValue::string(self.credential_binding.clone()),
                    ),
                    (
                        "parallelismLimit",
                        JsonValue::number(self.parallelism_limit),
                    ),
                    (
                        "parallelSafetyClass",
                        JsonValue::string(self.parallel_safety_class.clone()),
                    ),
                    ("runtimeType", JsonValue::string(self.runtime_type.clone())),
                    ("stateClass", JsonValue::string(self.state_class.clone())),
                    ("effectClass", JsonValue::string(self.effect_class.clone())),
                    (
                        "defaultPoolModel",
                        JsonValue::string(self.default_pool_model.clone()),
                    ),
                    ("maxWorkers", JsonValue::number(self.max_workers)),
                    (
                        "maxInFlightPerWorker",
                        JsonValue::number(self.max_in_flight_per_worker),
                    ),
                    (
                        "transportStatus",
                        JsonValue::string(self.transport_status.clone()),
                    ),
                    (
                        "launcherKind",
                        JsonValue::string(self.launcher_kind.clone()),
                    ),
                    (
                        "lockDomains",
                        JsonValue::array(self.lock_domains.iter().cloned().map(JsonValue::string)),
                    ),
                    (
                        "profileEvidence",
                        JsonValue::array(self.profile_evidence.clone()),
                    ),
                    (
                        "conflictDomain",
                        JsonValue::string(self.conflict_domain.clone()),
                    ),
                    (
                        "projectRootMode",
                        JsonValue::string(self.project_root_mode.clone()),
                    ),
                    (
                        "worktreeBinding",
                        JsonValue::string(self.worktree_binding.clone()),
                    ),
                    (
                        "stateProfileMode",
                        JsonValue::string(self.state_profile_mode.clone()),
                    ),
                    ("hostLock", JsonValue::string(self.host_lock.clone())),
                    (
                        "startupStrategy",
                        JsonValue::string(self.startup_strategy.clone()),
                    ),
                    (
                        "routingGroup",
                        JsonValue::string(self.routing_group.clone()),
                    ),
                    (
                        "discoveryRequiresLease",
                        JsonValue::bool(self.discovery_requires_lease),
                    ),
                ]),
            ),
            ("healthUrl", JsonValue::string(self.health_url.clone())),
            ("sourceEnabled", JsonValue::bool(self.source_enabled)),
            ("sourceType", JsonValue::string(self.source_type.clone())),
            ("sourcePath", JsonValue::string(self.source_path.clone())),
            (
                "sourceCommand",
                JsonValue::string(self.source_command.clone()),
            ),
            (
                "sourceArgs",
                JsonValue::array(self.source_args.iter().cloned().map(JsonValue::string)),
            ),
            (
                "sourceEnvNames",
                JsonValue::array(self.source_env_names.iter().cloned().map(JsonValue::string)),
            ),
            (
                "sourceHeaderNames",
                JsonValue::array(
                    self.source_header_names
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            ("sourceUrl", JsonValue::string(self.source_url.clone())),
            ("toolPolicies", JsonValue::array(self.tool_policies.clone())),
            (
                "installer",
                JsonValue::object([
                    ("target", JsonValue::string(self.installer_target.clone())),
                    ("method", JsonValue::string(self.installer_method.clone())),
                    ("package", JsonValue::string(self.installer_package.clone())),
                    (
                        "verifyCommand",
                        JsonValue::string(self.installer_verify_command.clone()),
                    ),
                ]),
            ),
        ])
    }
}
