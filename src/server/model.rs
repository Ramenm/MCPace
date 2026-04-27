use crate::json::JsonValue;

#[derive(Debug, Clone)]
pub struct ServerRecord {
    pub name: String,
    pub kind: String,
    pub required: bool,
    pub default_enabled: bool,
    pub profile_enabled: bool,
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
    pub conflict_domain: String,
    pub project_root_mode: String,
    pub worktree_binding: String,
    pub browser_profile_mode: String,
    pub host_lock: String,
    pub startup_strategy: String,
    pub routing_group: String,
    pub health_url: String,
    pub source_enabled: bool,
    pub source_type: String,
    pub source_command: String,
    pub source_url: String,
    pub installer_target: String,
    pub installer_method: String,
    pub installer_package: String,
    pub installer_verify_command: String,
}

#[derive(Debug, Clone)]
pub(super) struct SourceServerRecord {
    pub(super) enabled: bool,
    pub(super) source_type: String,
    pub(super) command: String,
    pub(super) url: String,
}

impl ServerRecord {
    pub(super) fn summary_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("name", JsonValue::string(self.name.clone())),
            ("kind", JsonValue::string(self.kind.clone())),
            ("required", JsonValue::bool(self.required)),
            ("defaultEnabled", JsonValue::bool(self.default_enabled)),
            ("profileEnabled", JsonValue::bool(self.profile_enabled)),
            ("sourceEnabled", JsonValue::bool(self.source_enabled)),
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
                "browserProfileMode",
                JsonValue::string(self.browser_profile_mode.clone()),
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
        ])
    }

    pub(super) fn capabilities_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("name", JsonValue::string(self.name.clone())),
            ("kind", JsonValue::string(self.kind.clone())),
            ("required", JsonValue::bool(self.required)),
            ("autoStart", JsonValue::bool(self.auto_start)),
            ("profileEnabled", JsonValue::bool(self.profile_enabled)),
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
                        "browserProfileMode",
                        JsonValue::string(self.browser_profile_mode.clone()),
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
                ]),
            ),
            ("healthUrl", JsonValue::string(self.health_url.clone())),
            ("sourceEnabled", JsonValue::bool(self.source_enabled)),
            ("sourceType", JsonValue::string(self.source_type.clone())),
            (
                "sourceCommand",
                JsonValue::string(self.source_command.clone()),
            ),
            ("sourceUrl", JsonValue::string(self.source_url.clone())),
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
