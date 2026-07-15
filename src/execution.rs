use crate::json::JsonValue;
use crate::json_helpers;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};

pub(crate) const EXECUTION_PROTOCOL: &str = "mcpace.execution.v1";
pub(crate) const DEFAULT_MAX_QUEUE_DEPTH: usize = 256;
pub(crate) const DEFAULT_IDLE_TTL_MS: u64 = 5 * 60 * 1_000;
pub(crate) const MAX_QUEUE_TIMEOUT_MS: u64 = 5 * 60 * 1_000;
pub(crate) const MAX_WORKERS: usize = 64;
pub(crate) const MAX_IN_FLIGHT_PER_WORKER: usize = 64;
pub(crate) const MAX_QUEUE_DEPTH: usize = 10_000;
pub(crate) const MAX_IDLE_TTL_MS: u64 = 24 * 60 * 60 * 1_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ExecutionPolicyError {
    UnsupportedMode { mode: String },
    MissingAffinity { mode: String, dimension: String },
}

impl fmt::Display for ExecutionPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedMode { mode } => write!(
                formatter,
                "unsupported execution mode '{}'; expected shared, serialized, session-isolated, project-isolated, pool, or disabled",
                mode
            ),
            Self::MissingAffinity { mode, dimension } => write!(
                formatter,
                "execution mode '{}' requires '{}' affinity, but the request did not provide a usable value",
                mode, dimension
            ),
        }
    }
}

impl std::error::Error for ExecutionPolicyError {}

impl From<ExecutionPolicyError> for String {
    fn from(error: ExecutionPolicyError) -> Self {
        error.to_string()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum ExecutionMode {
    Shared,
    Serialized,
    SessionIsolated,
    ProjectIsolated,
    Pool,
    Disabled,
}

impl ExecutionMode {
    pub(crate) fn parse(raw: &str) -> Result<Self, ExecutionPolicyError> {
        let normalized = normalize_token(raw);
        match normalized.as_str() {
            "shared" | "parallel" | "parallel-safe" | "multi-reader" => Ok(Self::Shared),
            "serialized" | "serial" | "single-writer" | "queue" | "queued" => Ok(Self::Serialized),
            "session" | "session-isolated" | "per-session" | "single-session" => {
                Ok(Self::SessionIsolated)
            }
            "project" | "project-isolated" | "per-project" | "isolated-per-project" => {
                Ok(Self::ProjectIsolated)
            }
            "pool" | "process-pool" | "worker-pool" => Ok(Self::Pool),
            "disabled" | "off" | "plan-only" | "not-runnable" => Ok(Self::Disabled),
            other => Err(ExecutionPolicyError::UnsupportedMode {
                mode: other.to_string(),
            }),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Serialized => "serialized",
            Self::SessionIsolated => "session-isolated",
            Self::ProjectIsolated => "project-isolated",
            Self::Pool => "pool",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionPolicy {
    pub(crate) mode: ExecutionMode,
    pub(crate) affinity: Vec<String>,
    pub(crate) queue_timeout_ms: u64,
    pub(crate) reuse_policy: String,
    pub(crate) min_workers: usize,
    pub(crate) max_workers: usize,
    pub(crate) max_in_flight_per_worker: usize,
    pub(crate) max_queue_depth: usize,
    pub(crate) idle_ttl_ms: u64,
    pub(crate) source: String,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ExecutionAffinityContext {
    pub(crate) client_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) project_root: Option<String>,
    pub(crate) transport: Option<String>,
    pub(crate) metadata: Option<JsonValue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionAffinityKey {
    pub(crate) client_id: String,
    pub(crate) session_id: String,
    pub(crate) project_root: String,
    pub(crate) transport: String,
    pub(crate) fingerprint: String,
}

impl Default for ExecutionPolicy {
    fn default() -> Self {
        Self::for_mode(ExecutionMode::Serialized)
    }
}

impl ExecutionPolicy {
    pub(crate) fn for_mode(mode: ExecutionMode) -> Self {
        let (affinity, queue_timeout_ms, reuse_policy, min_workers, max_workers, max_in_flight) =
            match mode {
                ExecutionMode::Shared => (vec![], 2_000, "shared", 0, 1, 4),
                ExecutionMode::Serialized => (vec![], 10_000, "sticky", 0, 1, 1),
                ExecutionMode::SessionIsolated => {
                    (vec!["client", "chat"], 10_000, "sticky-session", 0, 1, 1)
                }
                ExecutionMode::ProjectIsolated => {
                    (vec!["project"], 10_000, "sticky-project", 0, 1, 1)
                }
                ExecutionMode::Pool => (vec![], 5_000, "least-busy", 1, 4, 1),
                ExecutionMode::Disabled => (vec![], 0, "never", 0, 0, 0),
            };

        Self {
            mode,
            affinity: affinity.into_iter().map(str::to_string).collect(),
            queue_timeout_ms,
            reuse_policy: reuse_policy.to_string(),
            min_workers,
            max_workers,
            max_in_flight_per_worker: max_in_flight,
            max_queue_depth: DEFAULT_MAX_QUEUE_DEPTH,
            idle_ttl_ms: DEFAULT_IDLE_TTL_MS,
            source: "mode-preset".to_string(),
            warnings: Vec::new(),
        }
    }

    pub(crate) fn conservative() -> Self {
        Self::for_mode(ExecutionMode::Serialized)
    }

    pub(crate) fn from_config_root(config: &JsonValue) -> Self {
        let defaults = config.get("executionDefaults");
        let fallback_mode = mode_at(defaults).unwrap_or(ExecutionMode::Serialized);
        Self::resolve(defaults, None, fallback_mode)
    }

    pub(crate) fn for_server(defaults: &Self, raw_server: &JsonValue) -> Self {
        let canonical = raw_server.get("policy");
        let server_execution = raw_server.get("execution");
        let fallback_mode = canonical
            .map(inferred_mode_from_canonical)
            .unwrap_or(defaults.mode);
        let defaults_json = defaults.to_config_json_value();
        Self::resolve_with_canonical(
            Some(&defaults_json),
            canonical,
            server_execution,
            fallback_mode,
        )
    }

    pub(crate) fn resolve(
        execution_defaults: Option<&JsonValue>,
        server_execution: Option<&JsonValue>,
        fallback_mode: ExecutionMode,
    ) -> Self {
        Self::resolve_with_canonical(execution_defaults, None, server_execution, fallback_mode)
    }

    /// Produces the one policy consumed by inventory, route planning, admission,
    /// and process reuse. Precedence is: explicit server execution mode, inferred
    /// canonical policy mode, matching root defaults, then the conservative preset.
    pub(crate) fn resolve_with_canonical(
        execution_defaults: Option<&JsonValue>,
        canonical_policy: Option<&JsonValue>,
        server_execution: Option<&JsonValue>,
        fallback_mode: ExecutionMode,
    ) -> Self {
        let explicit_server_mode = mode_at(server_execution);
        let default_mode = mode_at(execution_defaults);
        let canonical_disabled = canonical_policy
            .map(canonical_policy_disabled)
            .unwrap_or(false);
        let mode = if canonical_disabled {
            ExecutionMode::Disabled
        } else {
            explicit_server_mode.unwrap_or(fallback_mode)
        };
        let mut resolved = Self::for_mode(mode);

        if !canonical_disabled {
            if explicit_server_mode.is_none() && default_mode == Some(mode) {
                if let Some(defaults) = execution_defaults {
                    resolved.apply_json(defaults, "executionDefaults");
                }
            } else if let Some(defaults) = execution_defaults {
                resolved.apply_global_limits(defaults);
            }

            if let Some(canonical) = canonical_policy {
                resolved.apply_canonical_json(canonical);
            }
            if let Some(server) = server_execution {
                resolved.apply_json(server, "server.execution");
            }
            resolved.mode = mode;
        }

        resolved.source = if canonical_disabled {
            "server.policy".to_string()
        } else if explicit_server_mode.is_some() {
            "server.execution".to_string()
        } else if canonical_policy.is_some() {
            "server.policy".to_string()
        } else if default_mode == Some(mode) {
            "executionDefaults".to_string()
        } else {
            "inferred-policy".to_string()
        };
        resolved.normalize();
        resolved
    }

    pub(crate) fn inferred_mode(
        scope_class: &str,
        concurrency_policy: &str,
        state_binding: &str,
        state_class: &str,
    ) -> ExecutionMode {
        let scope_class = normalize_token(scope_class);
        let concurrency_policy = normalize_token(concurrency_policy);
        let state_binding = normalize_token(state_binding);
        let state_class = normalize_token(state_class);

        if concurrency_policy == "plan-only" || scope_class == "not-runnable" {
            return ExecutionMode::Disabled;
        }
        if concurrency_policy == "isolated-per-project"
            || scope_class == "project-local"
            || state_class == "project-stateful"
        {
            return ExecutionMode::ProjectIsolated;
        }
        if concurrency_policy == "single-session"
            || state_class == "session-stateful"
            || state_binding == "host-session"
        {
            return ExecutionMode::SessionIsolated;
        }
        if concurrency_policy == "multi-reader"
            && matches!(
                state_class.as_str(),
                "stateless" | "credential-stateful" | "remote-session-stateful"
            )
        {
            return ExecutionMode::Shared;
        }
        ExecutionMode::Serialized
    }

    pub(crate) fn is_disabled(&self) -> bool {
        self.mode == ExecutionMode::Disabled || self.max_workers == 0
    }

    pub(crate) fn worker_limit(&self) -> usize {
        match self.mode {
            ExecutionMode::Disabled => 0,
            ExecutionMode::Pool => self.max_workers.max(1),
            _ => 1,
        }
    }

    pub(crate) fn effective_max_in_flight_per_worker(&self, transport: &str) -> usize {
        if self.is_disabled() {
            return 0;
        }
        // One stdio child has one response consumer. Parallel stdio work is
        // therefore represented by multiple isolated workers, never by
        // concurrent writes into one mutable process.
        if transport.eq_ignore_ascii_case("stdio") {
            return 1;
        }
        self.max_in_flight_per_worker.max(1)
    }

    pub(crate) fn effective_capacity(&self, transport: &str) -> usize {
        self.worker_limit()
            .saturating_mul(self.effective_max_in_flight_per_worker(transport))
    }

    pub(crate) fn has_affinity(&self, value: &str) -> bool {
        let normalized = normalize_affinity_token(value);
        self.affinity
            .iter()
            .any(|candidate| candidate == &normalized)
    }

    pub(crate) fn fingerprint(&self) -> String {
        format!(
            "protocol={}|mode={}|affinity={}|queueTimeoutMs={}|reusePolicy={}|minWorkers={}|maxWorkers={}|maxInFlightPerWorker={}|maxQueueDepth={}|idleTtlMs={}",
            EXECUTION_PROTOCOL,
            self.mode.as_str(),
            self.affinity.join(","),
            self.queue_timeout_ms,
            self.reuse_policy,
            self.min_workers,
            self.max_workers,
            self.max_in_flight_per_worker,
            self.max_queue_depth,
            self.idle_ttl_ms
        )
    }

    pub(crate) fn affinity_key(
        &self,
        context: &ExecutionAffinityContext,
    ) -> Result<ExecutionAffinityKey, ExecutionPolicyError> {
        let default_client = trimmed(context.client_id.as_deref())
            .unwrap_or_else(|| "mcpace-upstream-bridge".to_string());
        let default_session =
            application_session_id(context).or_else(|| trimmed(context.session_id.as_deref()));
        let default_project = trimmed(context.project_root.as_deref());
        let default_transport =
            trimmed(context.transport.as_deref()).unwrap_or_else(|| "stdio".to_string());

        let mut values = Vec::new();
        for dimension in &self.affinity {
            let value = match dimension.as_str() {
                "client" => Some(default_client.clone()),
                "project" => default_project.clone(),
                "chat" | "session" | "application" => default_session.clone(),
                "credential" => metadata_string(
                    context,
                    &[
                        &["mcpace", "credentialId"],
                        &["mcpace", "credential"],
                        &["credentialId"],
                        &["credential"],
                    ],
                ),
                "metadata" => context
                    .metadata
                    .as_ref()
                    .map(|value| fingerprint_text(&value.to_compact_string())),
                "transport" => Some(default_transport.clone()),
                "transport-session" => metadata_string(
                    context,
                    &[&["mcpace", "transportSessionId"], &["transportSessionId"]],
                )
                .or_else(|| trimmed(context.session_id.as_deref())),
                "client-instance" => metadata_string(
                    context,
                    &[&["mcpace", "clientInstanceId"], &["clientInstanceId"]],
                )
                .or_else(|| Some(default_client.clone())),
                _ => None,
            }
            .ok_or_else(|| ExecutionPolicyError::MissingAffinity {
                mode: self.mode.as_str().to_string(),
                dimension: dimension.clone(),
            })?;
            values.push(format!("{}={}", dimension, value));
        }

        let fingerprint = if values.is_empty() {
            format!("mode={}|affinity=global", self.mode.as_str())
        } else {
            let protected = values
                .iter()
                .map(|value| {
                    let (dimension, raw) = value.split_once('=').unwrap_or(("unknown", value));
                    format!("{}={}", dimension, fingerprint_text(raw))
                })
                .collect::<Vec<_>>();
            format!("mode={}|{}", self.mode.as_str(), protected.join("|"))
        };
        Ok(ExecutionAffinityKey {
            client_id: affinity_display_value(&values, "client")
                .unwrap_or_else(|| "shared".to_string()),
            session_id: affinity_display_value(&values, "chat")
                .or_else(|| affinity_display_value(&values, "session"))
                .or_else(|| affinity_display_value(&values, "application"))
                .or_else(|| affinity_display_value(&values, "transport-session"))
                .unwrap_or_else(|| "shared".to_string()),
            project_root: affinity_display_value(&values, "project")
                .unwrap_or_else(|| "shared".to_string()),
            transport: default_transport,
            fingerprint,
        })
    }

    pub(crate) fn to_config_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("protocol", JsonValue::string(EXECUTION_PROTOCOL)),
            ("mode", JsonValue::string(self.mode.as_str())),
            (
                "affinity",
                JsonValue::array(self.affinity.iter().cloned().map(JsonValue::string)),
            ),
            ("queueTimeoutMs", JsonValue::number(self.queue_timeout_ms)),
            ("reusePolicy", JsonValue::string(self.reuse_policy.clone())),
            ("minWorkers", JsonValue::number(self.min_workers)),
            ("maxWorkers", JsonValue::number(self.max_workers)),
            (
                "maxInFlightPerWorker",
                JsonValue::number(self.max_in_flight_per_worker),
            ),
            ("maxQueueDepth", JsonValue::number(self.max_queue_depth)),
            ("idleTtlMs", JsonValue::number(self.idle_ttl_ms)),
        ])
    }

    pub(crate) fn to_json_value(&self) -> JsonValue {
        let mut value = self.to_config_json_value();
        if let JsonValue::Object(object) = &mut value {
            object.insert("source".to_string(), JsonValue::string(self.source.clone()));
            object.insert(
                "warnings".to_string(),
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            );
        }
        value
    }

    fn apply_json(&mut self, value: &JsonValue, label: &str) {
        let Some(object) = value.as_object() else {
            self.warnings.push(format!(
                "{} is not an object; mode defaults remain active",
                label
            ));
            return;
        };

        if let Some(raw_mode) = object.get("mode").and_then(JsonValue::as_str) {
            match ExecutionMode::parse(raw_mode) {
                Ok(mode) if mode == self.mode => {}
                Ok(mode) => self.warnings.push(format!(
                    "{} mode '{}' was ignored because the resolved mode is '{}'",
                    label,
                    mode.as_str(),
                    self.mode.as_str()
                )),
                Err(error) => self.warnings.push(error.to_string()),
            }
        }
        if let Some(values) = object.get("affinity").and_then(JsonValue::as_array) {
            self.affinity = json_helpers::strings_from_array(Some(values));
        }
        if let Some(value) = non_negative_u64(object.get("queueTimeoutMs")) {
            self.queue_timeout_ms = value;
        }
        if let Some(value) = object
            .get("reusePolicy")
            .and_then(JsonValue::as_str)
            .map(normalize_token)
            .filter(|value| !value.is_empty())
        {
            self.reuse_policy = value;
        }
        if let Some(value) = non_negative_usize(object.get("minWorkers")) {
            self.min_workers = value;
        }
        if let Some(value) = non_negative_usize(object.get("maxWorkers")) {
            self.max_workers = value;
        }
        if let Some(value) = non_negative_usize(object.get("maxInFlightPerWorker")) {
            self.max_in_flight_per_worker = value;
        }
        if let Some(value) = positive_usize(object.get("maxQueueDepth")) {
            self.max_queue_depth = value;
        }
        if let Some(value) = positive_u64(object.get("idleTtlMs")) {
            self.idle_ttl_ms = value;
        }
    }

    fn apply_global_limits(&mut self, value: &JsonValue) {
        let Some(object) = value.as_object() else {
            return;
        };
        if let Some(value) = positive_usize(object.get("maxQueueDepth")) {
            self.max_queue_depth = value;
        }
        if let Some(value) = positive_u64(object.get("idleTtlMs")) {
            self.idle_ttl_ms = value;
        }
    }

    fn apply_canonical_json(&mut self, value: &JsonValue) {
        if canonical_policy_disabled(value) {
            self.mode = ExecutionMode::Disabled;
            self.max_workers = 0;
            self.max_in_flight_per_worker = 0;
            return;
        }
        if let Some(value) = json_helpers::value_at_path(value, &["maxWorkers"])
            .and_then(JsonValue::as_i64)
            .and_then(|value| usize::try_from(value).ok())
        {
            self.max_workers = value;
        } else if let Some(value) = json_helpers::value_at_path(value, &["parallelismLimit"])
            .and_then(JsonValue::as_i64)
            .and_then(|value| usize::try_from(value).ok())
        {
            self.max_workers = value;
        }
        if let Some(value) = json_helpers::value_at_path(value, &["maxInFlightPerWorker"])
            .and_then(JsonValue::as_i64)
            .and_then(|value| usize::try_from(value).ok())
        {
            self.max_in_flight_per_worker = value;
        }
    }

    pub(crate) fn normalize(&mut self) {
        normalize_affinity(&mut self.affinity, &mut self.warnings);

        if self.mode == ExecutionMode::Disabled
            || self.max_workers == 0
            || self.max_in_flight_per_worker == 0
        {
            self.mode = ExecutionMode::Disabled;
            self.affinity.clear();
            self.queue_timeout_ms = 0;
            self.reuse_policy = "never".to_string();
            self.min_workers = 0;
            self.max_workers = 0;
            self.max_in_flight_per_worker = 0;
            return;
        }

        self.max_workers = self.max_workers.max(1);
        self.max_in_flight_per_worker = self.max_in_flight_per_worker.max(1);
        self.max_queue_depth = self.max_queue_depth.max(1);
        self.idle_ttl_ms = self.idle_ttl_ms.max(1_000);

        clamp_with_warning(
            &mut self.queue_timeout_ms,
            MAX_QUEUE_TIMEOUT_MS,
            "queueTimeoutMs",
            &mut self.warnings,
        );
        clamp_with_warning(
            &mut self.max_workers,
            MAX_WORKERS,
            "maxWorkers",
            &mut self.warnings,
        );
        clamp_with_warning(
            &mut self.max_in_flight_per_worker,
            MAX_IN_FLIGHT_PER_WORKER,
            "maxInFlightPerWorker",
            &mut self.warnings,
        );
        clamp_with_warning(
            &mut self.max_queue_depth,
            MAX_QUEUE_DEPTH,
            "maxQueueDepth",
            &mut self.warnings,
        );
        clamp_with_warning(
            &mut self.idle_ttl_ms,
            MAX_IDLE_TTL_MS,
            "idleTtlMs",
            &mut self.warnings,
        );

        if self.mode != ExecutionMode::Pool && self.max_workers > 1 {
            self.warnings.push(format!(
                "mode '{}' uses one process per affinity group; maxWorkers was clamped from {} to 1",
                self.mode.as_str(),
                self.max_workers
            ));
            self.max_workers = 1;
        }
        self.min_workers = self.min_workers.min(self.max_workers);

        if self.mode == ExecutionMode::ProjectIsolated && !self.has_affinity("project") {
            self.affinity.push("project".to_string());
            self.warnings.push(
                "project-isolated requires project affinity; 'project' was added automatically"
                    .to_string(),
            );
        }
        if self.mode == ExecutionMode::SessionIsolated
            && !self.affinity.iter().any(|value| {
                matches!(
                    value.as_str(),
                    "chat" | "session" | "application" | "transport-session"
                )
            })
        {
            self.affinity.push("session".to_string());
            self.warnings.push(
                "session-isolated requires a session-like affinity; 'session' was added automatically"
                    .to_string(),
            );
        }
        normalize_affinity(&mut self.affinity, &mut self.warnings);

        let allowed_reuse = [
            "shared",
            "sticky",
            "sticky-session",
            "sticky-project",
            "least-busy",
            "ttl",
            "never",
        ];
        if !allowed_reuse.contains(&self.reuse_policy.as_str()) {
            let fallback = Self::for_mode(self.mode).reuse_policy;
            self.warnings.push(format!(
                "unsupported reusePolicy '{}'; using '{}'",
                self.reuse_policy, fallback
            ));
            self.reuse_policy = fallback;
        }
    }
}

fn mode_at(value: Option<&JsonValue>) -> Option<ExecutionMode> {
    value
        .and_then(|value| value.get("mode"))
        .and_then(JsonValue::as_str)
        .and_then(|value| ExecutionMode::parse(value).ok())
}

fn inferred_mode_from_canonical(value: &JsonValue) -> ExecutionMode {
    let scope_class = json_helpers::string_at_path(value, &["scopeClass"]).unwrap_or("");
    let concurrency_policy =
        json_helpers::string_at_path(value, &["concurrencyPolicy"]).unwrap_or("");
    let state_binding = json_helpers::string_at_path(value, &["stateBinding"]).unwrap_or("");
    let state_class = json_helpers::string_at_path(value, &["stateClass"]).unwrap_or("");
    ExecutionPolicy::inferred_mode(scope_class, concurrency_policy, state_binding, state_class)
}

fn canonical_policy_disabled(value: &JsonValue) -> bool {
    ["startupStrategy", "routingGroup"]
        .iter()
        .filter_map(|key| json_helpers::string_at_path(value, &[*key]))
        .map(normalize_token)
        .any(|value| value == "disabled")
        || json_helpers::string_at_path(value, &["concurrencyPolicy"])
            .map(normalize_token)
            .map(|value| value == "plan-only")
            .unwrap_or(false)
        || json_helpers::string_at_path(value, &["scopeClass"])
            .map(normalize_token)
            .map(|value| value == "not-runnable")
            .unwrap_or(false)
}

fn normalize_affinity(values: &mut Vec<String>, warnings: &mut Vec<String>) {
    let mut normalized = Vec::new();
    for raw in values.drain(..) {
        let value = normalize_affinity_token(&raw);
        if value.is_empty() {
            continue;
        }
        if is_supported_affinity(&value) {
            normalized.push(value);
        } else {
            warnings.push(format!(
                "unsupported execution affinity '{}'; it was ignored",
                raw.trim()
            ));
        }
    }
    normalized.sort();
    normalized.dedup();
    *values = normalized;
}

fn normalize_affinity_token(value: &str) -> String {
    match normalize_token(value).as_str() {
        "conversation" => "chat".to_string(),
        "workspace" => "project".to_string(),
        "credentials" => "credential".to_string(),
        other => other.to_string(),
    }
}

fn is_supported_affinity(value: &str) -> bool {
    matches!(
        value,
        "client"
            | "project"
            | "chat"
            | "session"
            | "credential"
            | "application"
            | "metadata"
            | "transport"
            | "transport-session"
            | "client-instance"
    )
}

fn non_negative_usize(value: Option<&JsonValue>) -> Option<usize> {
    value
        .and_then(JsonValue::as_i64)
        .and_then(|value| usize::try_from(value).ok())
}

fn positive_usize(value: Option<&JsonValue>) -> Option<usize> {
    non_negative_usize(value).filter(|value| *value > 0)
}

fn non_negative_u64(value: Option<&JsonValue>) -> Option<u64> {
    value
        .and_then(JsonValue::as_i64)
        .and_then(|value| u64::try_from(value).ok())
}

fn positive_u64(value: Option<&JsonValue>) -> Option<u64> {
    non_negative_u64(value).filter(|value| *value > 0)
}

fn clamp_with_warning<T>(value: &mut T, maximum: T, label: &str, warnings: &mut Vec<String>)
where
    T: Copy + Ord + fmt::Display,
{
    if *value > maximum {
        warnings.push(format!(
            "{}={} exceeded the runtime safety maximum {}; the value was clamped",
            label, *value, maximum
        ));
        *value = maximum;
    }
}

fn trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn application_session_id(context: &ExecutionAffinityContext) -> Option<String> {
    metadata_string(
        context,
        &[
            &["mcpace", "applicationSessionId"],
            &["mcpace", "chatId"],
            &["applicationSessionId"],
            &["chatId"],
            &["conversationId"],
        ],
    )
}

fn metadata_string(context: &ExecutionAffinityContext, paths: &[&[&str]]) -> Option<String> {
    let metadata = context.metadata.as_ref()?;
    paths.iter().find_map(|path| {
        json_helpers::string_at_path(metadata, path)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn affinity_display_value(values: &[String], dimension: &str) -> Option<String> {
    let prefix = format!("{}=", dimension);
    values
        .iter()
        .find_map(|value| value.strip_prefix(&prefix).map(ToOwned::to_owned))
}

fn fingerprint_text(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("len{}-hash{:016x}", value.len(), hasher.finish())
}

fn normalize_token(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}

#[cfg(test)]
mod tests;
