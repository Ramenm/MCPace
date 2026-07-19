use super::model::{ServerRecord, SourceServerRecord};
use crate::execution::ExecutionPolicy;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_sources;
use crate::platform_utils;
use crate::profile;
use crate::text_utils;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerLoaderError {
    ConfigRead { path: PathBuf, reason: String },
    Sources { reason: String },
    RuntimeProfile { reason: String },
}

pub type ServerLoaderResult<T> = Result<T, ServerLoaderError>;

impl fmt::Display for ServerLoaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigRead { path, reason } => {
                write!(
                    formatter,
                    "failed to read server config {}: {}",
                    path.display(),
                    reason
                )
            }
            Self::Sources { reason } => {
                write!(formatter, "failed to load MCP source settings: {}", reason)
            }
            Self::RuntimeProfile { reason } => write!(
                formatter,
                "failed to load runtime profile selection: {}",
                reason
            ),
        }
    }
}

impl std::error::Error for ServerLoaderError {}

impl From<ServerLoaderError> for String {
    fn from(error: ServerLoaderError) -> Self {
        error.to_string()
    }
}

#[derive(Debug, Clone)]
struct GenericSourcePolicy {
    scope_class: &'static str,
    concurrency_policy: &'static str,
    state_binding: &'static str,
    credential_binding: &'static str,
    parallelism_limit: usize,
    conflict_domain_prefix: &'static str,
    project_root_mode: &'static str,
    worktree_binding: &'static str,
    state_profile_mode: &'static str,
    host_lock: &'static str,
    startup_strategy: &'static str,
    routing_group: &'static str,
    discovery_requires_lease: bool,
}

#[derive(Debug, Clone)]
struct RuntimeClassification {
    runtime_type: &'static str,
    state_class: &'static str,
    effect_class: &'static str,
}

#[derive(Debug, Clone)]
struct EvidenceDecision {
    score: f64,
    level: &'static str,
    automatic_action: &'static str,
    next_step: &'static str,
    sources: Vec<&'static str>,
}

pub fn load_server_records(root_path: &Path) -> ServerLoaderResult<Vec<ServerRecord>> {
    let config_path = root_path.join("mcpace.config.json");
    let config = json_helpers::read_json_file(&config_path).map_err(|error| {
        ServerLoaderError::ConfigRead {
            path: config_path.clone(),
            reason: error.to_string(),
        }
    })?;
    let source_settings = load_source_settings(root_path)?;
    let execution_defaults = ExecutionPolicy::from_config_root(&config);
    let runtime_profile = profile::load_runtime_profile_selection(root_path).map_err(|error| {
        ServerLoaderError::RuntimeProfile {
            reason: error.to_string(),
        }
    })?;

    let mut records = Vec::new();
    let mut declared_names = BTreeSet::new();
    if let Some(servers_object) = json_helpers::object_at_path(&config, &["servers"]) {
        for (name, value) in servers_object {
            let normalized_name = mcp_sources::normalize_server_name(name);
            if normalized_name.is_empty() {
                continue;
            }
            declared_names.insert(normalized_name.clone());
            if let Some(record) = normalize_server_record(
                name,
                value,
                source_settings.get(&normalized_name),
                &execution_defaults,
                runtime_profile
                    .server_overrides
                    .get(&normalized_name)
                    .copied(),
            ) {
                records.push(record);
            }
        }
    }

    for (normalized_name, source_record) in &source_settings {
        if declared_names.contains(normalized_name) {
            continue;
        }
        records.push(generic_source_server_record(
            normalized_name,
            source_record,
            &execution_defaults,
        ));
    }

    records.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    Ok(records)
}

fn load_source_settings(
    root_path: &Path,
) -> ServerLoaderResult<BTreeMap<String, SourceServerRecord>> {
    let registry = mcp_sources::load_mcp_server_registry(root_path).map_err(|error| {
        ServerLoaderError::Sources {
            reason: error.to_string(),
        }
    })?;
    let mut map = BTreeMap::new();
    for entry in registry.servers.values() {
        let value = &entry.value;
        let enabled = source_enabled(value);
        let raw_source_type = value
            .get("type")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let command = value
            .get("command")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let url = value
            .get("url")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let args =
            json_helpers::strings_from_array(value.get("args").and_then(JsonValue::as_array));
        let env_names = object_keys(value.get("env"));
        let header_names = object_keys(value.get("headers"));
        let mut profile_hints = json_helpers::strings_from_array(
            value
                .get("mcpaceProfileHints")
                .or_else(|| value.get("profileHints"))
                .and_then(JsonValue::as_array),
        );
        if let Some(description) = value
            .get("description")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            profile_hints.push(description.to_string());
        }
        profile_hints.sort();
        profile_hints.dedup();
        let source_type = infer_source_type(&raw_source_type, &command, &url);
        map.insert(
            entry.normalized_name.clone(),
            SourceServerRecord {
                name: entry.name.trim().to_string(),
                enabled,
                source_type,
                command,
                url,
                args,
                env_names,
                header_names,
                source_path: entry.source.clone(),
                profile_hints,
                execution: value.get("execution").cloned(),
                policy: value.get("policy").cloned(),
            },
        );
    }
    Ok(map)
}

fn object_keys(value: Option<&JsonValue>) -> Vec<String> {
    value
        .and_then(JsonValue::as_object)
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn generic_source_server_record(
    normalized_name: &str,
    source_record: &SourceServerRecord,
    execution_defaults: &ExecutionPolicy,
) -> ServerRecord {
    let source_type = infer_source_type(
        &source_record.source_type,
        &source_record.command,
        &source_record.url,
    );
    let policy = infer_generic_source_policy(normalized_name, source_record, &source_type);
    let signal_args = source_signal_args(&source_record.args, &source_record.profile_hints);
    let kind = format!("source-{}", source_type);
    let display_name = if source_record.name.trim().is_empty() {
        normalized_name
    } else {
        source_record.name.trim()
    };
    let required_commands = if source_type == "stdio" && !source_record.command.trim().is_empty() {
        vec![source_record.command.clone()]
    } else {
        Vec::new()
    };
    let conflict_domain = if policy.conflict_domain_prefix.is_empty() {
        normalized_name.to_string()
    } else {
        format!("{}:{}", policy.conflict_domain_prefix, normalized_name)
    };
    let runtime_classification = infer_runtime_classification(
        &source_type,
        policy.scope_class,
        policy.concurrency_policy,
        policy.state_binding,
        policy.credential_binding,
        &source_record.command,
        &source_record.url,
        &signal_args,
        &[],
    );
    let fallback_mode = ExecutionPolicy::inferred_mode(
        policy.scope_class,
        policy.concurrency_policy,
        policy.state_binding,
        runtime_classification.state_class,
    );
    let execution_defaults_json = execution_defaults.to_config_json_value();
    let execution = ExecutionPolicy::resolve_with_canonical(
        Some(&execution_defaults_json),
        source_record.policy.as_ref(),
        source_record.execution.as_ref(),
        fallback_mode,
    );
    let max_workers = execution.worker_limit();
    let max_in_flight_per_worker = execution.effective_max_in_flight_per_worker(&source_type);
    let effective_enabled = source_record.enabled
        && !execution.is_disabled()
        && !runtime_policy_disabled(
            policy.scope_class,
            policy.concurrency_policy,
            policy.startup_strategy,
            policy.routing_group,
            max_workers,
        );

    ServerRecord {
        name: display_name.to_string(),
        kind,
        required: false,
        default_enabled: false,
        profile_enabled: source_record.enabled,
        platform_supported: true,
        effective_enabled,
        auto_start: false,
        transport_preference: source_type.clone(),
        supported_transports: supported_transports_for_source_type(&source_type),
        platforms: Vec::new(),
        required_commands,
        scope_class: policy.scope_class.to_string(),
        concurrency_policy: policy.concurrency_policy.to_string(),
        state_binding: policy.state_binding.to_string(),
        credential_binding: policy.credential_binding.to_string(),
        parallelism_limit: policy.parallelism_limit,
        parallel_safety_class: infer_parallel_safety_class(
            &source_type,
            policy.scope_class,
            policy.concurrency_policy,
            policy.state_binding,
            policy.credential_binding,
            &source_record.command,
            &source_record.url,
            &signal_args,
            &[],
        ),
        runtime_type: runtime_classification.runtime_type.to_string(),
        state_class: runtime_classification.state_class.to_string(),
        effect_class: runtime_classification.effect_class.to_string(),
        default_pool_model: infer_default_pool_model(
            &source_type,
            policy.scope_class,
            policy.concurrency_policy,
            policy.state_binding,
            policy.credential_binding,
        ),
        max_workers,
        max_in_flight_per_worker,
        execution,
        transport_status: transport_status_for_source_type(&source_type),
        launcher_kind: infer_launcher_kind(
            &source_record.command,
            &source_record.url,
            "user-supplied",
            "",
        ),
        lock_domains: infer_lock_domains(
            policy.scope_class,
            policy.concurrency_policy,
            policy.state_binding,
            policy.credential_binding,
            normalized_name,
        ),
        profile_evidence: profile_evidence_records(ProfileEvidenceInput {
            source_type: &source_type,
            scope_class: policy.scope_class,
            concurrency_policy: policy.concurrency_policy,
            state_binding: policy.state_binding,
            credential_binding: policy.credential_binding,
            runtime_type: runtime_classification.runtime_type,
            state_class: runtime_classification.state_class,
            effect_class: runtime_classification.effect_class,
            command: &source_record.command,
            url: &source_record.url,
            args: &signal_args,
        }),
        conflict_domain,
        project_root_mode: policy.project_root_mode.to_string(),
        worktree_binding: policy.worktree_binding.to_string(),
        state_profile_mode: policy.state_profile_mode.to_string(),
        host_lock: policy.host_lock.to_string(),
        startup_strategy: policy.startup_strategy.to_string(),
        routing_group: policy.routing_group.to_string(),
        discovery_requires_lease: policy.discovery_requires_lease,
        health_url: String::new(),
        source_enabled: source_record.enabled,
        source_type,
        source_path: source_record.source_path.clone(),
        source_command: source_record.command.clone(),
        source_args: source_record.args.clone(),
        source_env_names: source_record.env_names.clone(),
        source_header_names: source_record.header_names.clone(),
        source_url: source_record.url.clone(),
        tool_policies: Vec::new(),
        installer_target: "none".to_string(),
        installer_method: "user-supplied".to_string(),
        installer_package: String::new(),
        installer_verify_command: String::new(),
    }
}

fn source_signal_args(args: &[String], profile_hints: &[String]) -> Vec<String> {
    let mut signal_args: Vec<String> = args
        .iter()
        .filter(|value| raw_arg_is_semantic_signal(value))
        .cloned()
        .collect();
    signal_args.extend(
        profile_hints
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    );
    signal_args.sort();
    signal_args.dedup();
    signal_args
}

fn raw_arg_is_semantic_signal(value: &str) -> bool {
    let normalized = value
        .trim()
        .trim_matches(|character: char| character == '"' || character == '\'')
        .to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }
    let exact_signals = [
        "stdio",
        "http",
        "https",
        "sse",
        "streamable-http",
        "read-only",
        "readonly",
        "no-write",
    ];
    if exact_signals.contains(&normalized.as_str()) {
        return true;
    }
    if looks_like_launcher_package_signal(&normalized) {
        return true;
    }
    if !normalized.starts_with('-') {
        return false;
    }
    let semantic_flags = [
        "--transport",
        "--stdio",
        "--http",
        "--sse",
        "--streamable-http",
        "--read-only",
        "--readonly",
        "--no-write",
        "--write",
        "--allow-read",
        "--allow-write",
        "--workspace",
        "--project",
        "--root",
        "--path",
        "--db",
        "--database",
        "--browser-profile",
        "--profile",
    ];
    semantic_flags
        .iter()
        .any(|flag| normalized == *flag || normalized.starts_with(&format!("{}=", flag)))
}

fn looks_like_launcher_package_signal(value: &str) -> bool {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.starts_with('-')
        || value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with("~/")
        || value.starts_with("http://")
        || value.starts_with("https://")
        || value.contains('\\')
    {
        return false;
    }
    if value.contains('/') && !value.starts_with('@') {
        return false;
    }
    value.starts_with('@')
        || value.contains("mcp")
        || value.contains("modelcontextprotocol")
        || value.contains("server-")
        || value.contains("-server")
}

fn infer_generic_source_policy(
    normalized_name: &str,
    source_record: &SourceServerRecord,
    source_type: &str,
) -> GenericSourcePolicy {
    let signal_args = source_signal_args(&source_record.args, &source_record.profile_hints);
    let signals = source_signals(
        normalized_name,
        &source_record.name,
        &source_record.command,
        &source_record.url,
        &signal_args,
    );
    let remote = source_type == "streamable-http"
        || source_type == "http"
        || !source_record.url.trim().is_empty();
    let mutable_tools = signals.contains("mutable-tools");

    if signals.contains("sdk-or-example") {
        return GenericSourcePolicy {
            scope_class: "not-runnable",
            concurrency_policy: "plan-only",
            state_binding: "none",
            credential_binding: "none",
            parallelism_limit: 0,
            conflict_domain_prefix: "not-runnable",
            project_root_mode: "none",
            worktree_binding: "none",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "disabled",
            routing_group: "sdk-or-example",
            discovery_requires_lease: false,
        };
    }

    if signals.contains("remote-browser-session") {
        return GenericSourcePolicy {
            scope_class: "credential-scoped",
            concurrency_policy: "single-session",
            state_binding: "remote-browser-session",
            credential_binding: "credential-profile",
            parallelism_limit: 1,
            conflict_domain_prefix: "remote-browser",
            project_root_mode: "optional",
            worktree_binding: "none",
            state_profile_mode: "optional",
            host_lock: "none",
            startup_strategy: "lazy-per-profile",
            routing_group: "remote-browser",
            discovery_requires_lease: true,
        };
    }

    if signals.contains("browser-observation") {
        return GenericSourcePolicy {
            scope_class: "host-readonly",
            concurrency_policy: "multi-reader",
            state_binding: "host-readonly",
            credential_binding: "browser-profile",
            parallelism_limit: 2,
            conflict_domain_prefix: "browser-readonly",
            project_root_mode: "optional",
            worktree_binding: "none",
            state_profile_mode: "optional",
            host_lock: "browser-profile-read",
            startup_strategy: "lazy-shared",
            routing_group: "browser-observation",
            discovery_requires_lease: true,
        };
    }

    if signals.contains("browser-or-desktop") {
        return GenericSourcePolicy {
            scope_class: "shared-exclusive",
            concurrency_policy: "single-session",
            state_binding: "host-desktop",
            credential_binding: "browser-profile",
            parallelism_limit: 1,
            conflict_domain_prefix: "browser-profile",
            project_root_mode: "optional",
            worktree_binding: "none",
            state_profile_mode: "required",
            host_lock: "browser-profile",
            startup_strategy: "singleton-host",
            routing_group: "browser",
            discovery_requires_lease: true,
        };
    }

    if signals.contains("shell-or-process") {
        return GenericSourcePolicy {
            scope_class: "shared-exclusive",
            concurrency_policy: "single-session",
            state_binding: "host-session",
            credential_binding: "source-config",
            parallelism_limit: 1,
            conflict_domain_prefix: "host-process",
            project_root_mode: "optional",
            worktree_binding: "none",
            state_profile_mode: "required",
            host_lock: "host-session",
            startup_strategy: "singleton-host",
            routing_group: "dangerous-process",
            discovery_requires_lease: true,
        };
    }

    if remote {
        return GenericSourcePolicy {
            scope_class: "credential-scoped",
            concurrency_policy: "single-writer",
            state_binding: "remote-session",
            credential_binding: "remote-origin-or-credential",
            parallelism_limit: 1,
            conflict_domain_prefix: "remote-mcp",
            project_root_mode: "optional",
            worktree_binding: "none",
            state_profile_mode: "optional",
            host_lock: "none",
            startup_strategy: "lazy-shared",
            routing_group: "remote-mcp",
            discovery_requires_lease: true,
        };
    }

    if signals.contains("memory-or-context") {
        return GenericSourcePolicy {
            scope_class: "state-profile",
            concurrency_policy: "single-session",
            state_binding: "context-store",
            credential_binding: "none",
            parallelism_limit: 1,
            conflict_domain_prefix: "state-profile",
            project_root_mode: "optional",
            worktree_binding: "none",
            state_profile_mode: "required",
            host_lock: "none",
            startup_strategy: "lazy-per-profile",
            routing_group: "memory-context",
            discovery_requires_lease: true,
        };
    }

    if signals.contains("git-repository") {
        return GenericSourcePolicy {
            scope_class: "project-local",
            concurrency_policy: "single-writer",
            state_binding: "repo",
            credential_binding: "git-config",
            parallelism_limit: 1,
            conflict_domain_prefix: "git-repository",
            project_root_mode: "required",
            worktree_binding: "repository-root",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "lazy-per-project",
            routing_group: "project-git",
            discovery_requires_lease: true,
        };
    }

    if signals.contains("database") {
        if signals.contains("network-database") || signals.contains("credentials-or-auth") {
            return GenericSourcePolicy {
                scope_class: "credential-scoped",
                concurrency_policy: "single-writer",
                state_binding: "db-connection",
                credential_binding: "database-connection",
                parallelism_limit: 1,
                conflict_domain_prefix: "database-connection",
                project_root_mode: "optional",
                worktree_binding: "none",
                state_profile_mode: "optional",
                host_lock: "none",
                startup_strategy: "lazy-per-profile",
                routing_group: "database-connection",
                discovery_requires_lease: true,
            };
        }
        return GenericSourcePolicy {
            scope_class: "project-local",
            concurrency_policy: "single-writer",
            state_binding: "db",
            credential_binding: "none",
            parallelism_limit: 1,
            conflict_domain_prefix: "database",
            project_root_mode: "required",
            worktree_binding: "project-root",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "lazy-per-project",
            routing_group: "project-database",
            discovery_requires_lease: true,
        };
    }

    if signals.contains("project-analysis")
        && !signals.contains("network-or-external-api")
        && !signals.contains("credentials-or-auth")
        && !signals.contains("cloud-admin")
        && !signals.contains("external-read-api")
        && !signals.contains("documentation-lookup")
    {
        return GenericSourcePolicy {
            scope_class: "project-local",
            concurrency_policy: "isolated-per-project",
            state_binding: "project-index",
            credential_binding: "none",
            parallelism_limit: 1,
            conflict_domain_prefix: "project-analysis",
            project_root_mode: "required",
            worktree_binding: "project-root",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "lazy-per-project",
            routing_group: "project-analysis",
            discovery_requires_lease: true,
        };
    }

    if signals.contains("filesystem") {
        return GenericSourcePolicy {
            scope_class: "project-local",
            concurrency_policy: "isolated-per-project",
            state_binding: "file",
            credential_binding: "none",
            parallelism_limit: 1,
            conflict_domain_prefix: "project-filesystem",
            project_root_mode: "required",
            worktree_binding: "project-root",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "lazy-per-project",
            routing_group: "project-filesystem",
            discovery_requires_lease: true,
        };
    }

    if signals.contains("transport-gateway") {
        return GenericSourcePolicy {
            scope_class: "stateless-local",
            concurrency_policy: "multi-reader",
            state_binding: "none",
            credential_binding: "none",
            parallelism_limit: 2,
            conflict_domain_prefix: "transport-gateway",
            project_root_mode: "optional",
            worktree_binding: "none",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "lazy-shared",
            routing_group: "transport-gateway",
            discovery_requires_lease: false,
        };
    }

    if (signals.contains("documentation-lookup") || signals.contains("external-read-api"))
        && !signals.contains("mutable-tools")
        && !signals.contains("cloud-admin")
        && !signals.contains("identity-admin")
        && !signals.contains("cluster-control")
        && !signals.contains("secrets-manager")
        && !signals.contains("payments-financial")
        && !signals.contains("blockchain-wallet")
    {
        return GenericSourcePolicy {
            scope_class: "credential-scoped",
            concurrency_policy: "multi-reader",
            state_binding: "none",
            credential_binding: if signals.contains("credentials-or-auth")
                || signals.contains("network-or-external-api")
            {
                "provider-budget"
            } else {
                "none"
            },
            parallelism_limit: 2,
            conflict_domain_prefix: "external-read",
            project_root_mode: "optional",
            worktree_binding: "none",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "lazy-shared",
            routing_group: "external-read",
            discovery_requires_lease: false,
        };
    }

    if signals.contains("cloud-admin")
        || signals.contains("identity-admin")
        || signals.contains("cluster-control")
        || signals.contains("secrets-manager")
        || signals.contains("payments-financial")
        || signals.contains("blockchain-wallet")
        || signals.contains("credentials-or-auth")
        || signals.contains("network-or-external-api")
    {
        return GenericSourcePolicy {
            scope_class: "credential-scoped",
            concurrency_policy: "single-writer",
            state_binding: "identity-tenant",
            credential_binding: "credential-profile",
            parallelism_limit: 1,
            conflict_domain_prefix: "credential-provider",
            project_root_mode: "optional",
            worktree_binding: "none",
            state_profile_mode: "optional",
            host_lock: "none",
            startup_strategy: "lazy-per-profile",
            routing_group: "credential-provider",
            discovery_requires_lease: true,
        };
    }

    if signals.contains("network-fetch") && !mutable_tools {
        return GenericSourcePolicy {
            scope_class: "credential-scoped",
            concurrency_policy: "multi-reader",
            state_binding: "none",
            credential_binding: "provider-budget",
            parallelism_limit: 2,
            conflict_domain_prefix: "network-fetch",
            project_root_mode: "optional",
            worktree_binding: "none",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "lazy-shared",
            routing_group: "network-fetch",
            discovery_requires_lease: false,
        };
    }

    if (signals.contains("local-utility") || signals.contains("readonly-tools")) && !mutable_tools {
        return GenericSourcePolicy {
            scope_class: "stateless-local",
            concurrency_policy: "multi-reader",
            state_binding: "none",
            credential_binding: "none",
            parallelism_limit: 4,
            conflict_domain_prefix: "stateless-local",
            project_root_mode: "none",
            worktree_binding: "none",
            state_profile_mode: "none",
            host_lock: "none",
            startup_strategy: "lazy-shared",
            routing_group: "stateless-local",
            discovery_requires_lease: false,
        };
    }

    GenericSourcePolicy {
        scope_class: "configured-source",
        concurrency_policy: "single-writer",
        state_binding: "runtime-source",
        credential_binding: "source-config",
        parallelism_limit: 1,
        conflict_domain_prefix: "settings-source",
        project_root_mode: "optional",
        worktree_binding: "none",
        state_profile_mode: "none",
        host_lock: "none",
        startup_strategy: "lazy-shared",
        routing_group: "unknown-source",
        discovery_requires_lease: true,
    }
}

fn source_signals(
    normalized_name: &str,
    display_name: &str,
    command: &str,
    url: &str,
    args: &[String],
) -> BTreeSet<String> {
    let _identity = (normalized_name, display_name);
    let haystack = format!(
        "{} {} {}",
        command_semantic_signal(command),
        url,
        args.join(" ")
    )
    .to_ascii_lowercase();
    let tokens = signal_tokens(&haystack);
    let mut signals = BTreeSet::new();
    let remote_file_api = has_any_substring(
        &haystack,
        &[
            "google-drive",
            "google drive",
            "google-docs",
            "google docs",
            "google-sheets",
            "google sheets",
            "google-slides",
            "google slides",
            "google-workspace",
            "google workspace",
            "onedrive",
            "dropbox",
            "sharepoint",
        ],
    ) || (tokens.contains("google")
        && (tokens.contains("drive")
            || tokens.contains("docs")
            || tokens.contains("sheets")
            || tokens.contains("slides")
            || tokens.contains("workspace")));

    if !remote_file_api
        && (has_any_token(
            &tokens,
            &[
                "filesystem",
                "file-system",
                "file",
                "files",
                "directory",
                "directories",
                "folder",
                "path",
                "read_file",
                "write_file",
            ],
        ) || has_any_substring(
            &haystack,
            &[
                "server-filesystem",
                "@modelcontextprotocol/server-filesystem",
            ],
        ))
    {
        signals.insert("filesystem".to_string());
    }
    if remote_file_api {
        signals.insert("network-or-external-api".to_string());
        signals.insert("credentials-or-auth".to_string());
    }

    let runnable_server_marker =
        has_any_token(
            &tokens,
            &["server", "mcp-server", "stdio", "streamable", "sse"],
        ) || has_any_substring(&haystack, &["mcp server", "model context protocol server"]);
    let explicit_sdk_or_framework = has_any_token(
        &tokens,
        &[
            "sdk",
            "framework",
            "middleware",
            "inspector-client",
            "inspector-cli",
            "eval",
            "evaluation",
            "instrumentation",
            "fastmcp",
            "nestjs",
            "genkit",
        ],
    ) || has_any_substring(
        &haystack,
        &[
            "mcp-framework",
            "shared utilities",
            "client-side application",
            "model context protocol inspector",
            "mcp apps middleware",
            "mcp sdk",
            "mcp utilities",
        ],
    );
    let weak_library_signal = has_any_token(
        &tokens,
        &["plugin", "module", "library", "utils", "utilities"],
    ) && !runnable_server_marker;
    let example_or_demo = has_any_token(
        &tokens,
        &["demo", "example", "starter", "sample", "basic", "preact"],
    ) || has_any_substring(
        &haystack,
        &[
            "cohort heatmap",
            "budget allocator",
            "customer segmentation",
            "basic mcp app",
            "mcp app server example",
        ],
    );
    if explicit_sdk_or_framework
        || weak_library_signal
        || (example_or_demo && !runnable_server_marker)
    {
        signals.insert("sdk-or-example".to_string());
    }

    let code_hosting_api = has_any_token(
        &tokens,
        &["github", "gitlab", "gitea", "bitbucket", "sourcegraph"],
    );
    if (has_any_token(
        &tokens,
        &[
            "git",
            "worktree",
            "branch",
            "commit",
            "diff",
            "checkout",
            "repo-path",
            "repository-root",
        ],
    ) || has_any_substring(&haystack, &["server-git", "mcp-server-git"]))
        && !code_hosting_api
    {
        signals.insert("git-repository".to_string());
    }

    let network_database = has_any_token(
        &tokens,
        &[
            "postgres",
            "postgresql",
            "mysql",
            "mssql",
            "mariadb",
            "redis",
            "supabase",
        ],
    );
    let file_database = has_any_token(&tokens, &["sqlite", "duckdb", "db-path", "database-file"]);
    if has_any_token(
        &tokens,
        &[
            "sqlite",
            "postgres",
            "postgresql",
            "mysql",
            "mssql",
            "mariadb",
            "redis",
            "duckdb",
            "database",
            "sql",
            "db",
            "db-path",
            "table",
        ],
    ) {
        signals.insert("database".to_string());
    }
    if network_database {
        signals.insert("network-database".to_string());
    }
    if file_database {
        signals.insert("file-database".to_string());
    }

    let browser_data_only = has_any_token(
        &tokens,
        &[
            "caniuse",
            "compat",
            "compatibility",
            "browserslist",
            "feature-table",
        ],
    ) || has_any_substring(
        &haystack,
        &["browser feature", "browser support", "compat-data"],
    );
    let browser_observation_surface = has_any_token(
        &tokens,
        &[
            "browser",
            "chrome",
            "chromium",
            "extension",
            "tab",
            "tabs",
            "bookmark",
            "bookmarks",
            "history",
        ],
    ) || has_any_substring(
        &haystack,
        &["browser tabs", "chrome tabs", "browser bookmarks"],
    );
    let browser_observation_only = !browser_data_only
        && browser_observation_surface
        && has_any_token(
            &tokens,
            &[
                "tab",
                "tabs",
                "bookmark",
                "bookmarks",
                "history",
                "active-tab",
                "open-tabs",
            ],
        )
        && !has_any_token(
            &tokens,
            &[
                "click",
                "navigate",
                "automation",
                "automate",
                "playwright",
                "puppeteer",
                "webdriver",
                "safaridriver",
                "stagehand",
            ],
        );
    let remote_browser_provider = has_any_token(
        &tokens,
        &[
            "browserbase",
            "browserstack",
            "apify",
            "stagehand",
            "remote-browser",
            "cloud-browser",
        ],
    ) || has_any_substring(
        &haystack,
        &[
            "browserbase",
            "browserstack",
            "remote browser",
            "cloud browser",
        ],
    );
    let browser_control_action = has_any_token(
        &tokens,
        &[
            "playwright",
            "puppeteer",
            "screenshot",
            "click",
            "navigate",
            "devtools",
            "cdp",
            "webdriver",
            "safaridriver",
            "stagehand",
            "android",
            "adb",
            "ios",
            "simulator",
            "mobile-automation",
        ],
    ) || has_any_substring(
        &haystack,
        &[
            "browser automation",
            "control browser",
            "automate browser",
            "mcpbrowser",
            "sessionmcp",
            "mcp-browser",
            "browser-kit",
            "chrome-devtools",
        ],
    );
    let chrome_control = has_any_token(&tokens, &["chrome", "chromium"])
        && has_any_token(
            &tokens,
            &["devtools", "cdp", "extension", "debug", "debugger"],
        );
    let browser_automation = remote_browser_provider || browser_control_action || chrome_control;
    if browser_observation_only {
        signals.insert("browser-observation".to_string());
    } else if browser_automation && !browser_data_only {
        if remote_browser_provider {
            signals.insert("remote-browser-session".to_string());
        } else {
            signals.insert("browser-or-desktop".to_string());
        }
    }

    if has_any_token(
        &tokens,
        &[
            "memory",
            "sequential-thinking",
            "context-store",
            "thinking",
            "remember",
            "notes",
            "note",
        ],
    ) {
        signals.insert("memory-or-context".to_string());
    }

    if has_any_token(
        &tokens,
        &[
            "time",
            "timezone",
            "clock",
            "date",
            "calculator",
            "calculate",
            "math",
        ],
    ) {
        signals.insert("local-utility".to_string());
    }

    if has_any_token(
        &tokens,
        &[
            "fetch", "http", "https", "url", "web", "scrape", "crawler", "search",
        ],
    ) || !url.trim().is_empty()
    {
        signals.insert("network-fetch".to_string());
    }
    if (has_any_token(&tokens, &["gateway", "proxy", "bridge"])
        || has_any_substring(
            &haystack,
            &[
                "stdio to",
                "stdio over",
                "over sse",
                "streamable http bridge",
            ],
        ))
        && !signals.contains("browser-or-desktop")
        && !signals.contains("remote-browser-session")
    {
        signals.insert("transport-gateway".to_string());
    }

    if has_any_token(
        &tokens,
        &[
            "context7",
            "docs",
            "documentation",
            "caniuse",
            "reference",
            "manual",
            "knowledgebase",
            "knowledge-base",
            "browser-compat",
            "design-system",
            "components",
            "shadcn",
            "magicui",
            "magic-ui",
            "primer",
            "primevue",
            "excalidraw",
        ],
    ) || has_any_substring(
        &haystack,
        &[
            "feature support",
            "support tables",
            "developer docs",
            "design system",
            "component references",
        ],
    ) {
        signals.insert("documentation-lookup".to_string());
    }

    if has_any_token(
        &tokens,
        &[
            "eslint",
            "lint",
            "linter",
            "tsserver",
            "language-server",
            "code-index",
            "code-intelligence",
            "knip",
            "pandacss",
            "panda",
            "semantic",
            "lsp",
            "theia",
        ],
    ) || has_any_substring(
        &haystack,
        &[
            "static analysis",
            "code analysis",
            "semantic code",
            "language server protocol",
        ],
    ) {
        signals.insert("project-analysis".to_string());
    }

    if has_any_token(
        &tokens,
        &[
            "brave",
            "search",
            "firecrawl",
            "scrape",
            "scraper",
            "scraping",
            "crawl",
            "crawler",
            "maps",
            "mapbox",
            "geocode",
            "geocoding",
            "weather",
            "lookup",
            "worldbank",
            "airbnb",
            "openbnb",
            "dataforseo",
        ],
    ) {
        signals.insert("external-read-api".to_string());
    }

    if code_hosting_api
        || has_any_token(
            &tokens,
            &[
                "notion",
                "sentry",
                "slack",
                "linear",
                "jira",
                "confluence",
                "atlassian",
                "clickup",
                "redmine",
                "google-drive",
                "google-docs",
                "google-sheets",
                "google-slides",
                "google-workspace",
                "salesforce",
                "netlify",
                "currents",
                "motiff",
                "esa",
                "hubspot",
                "asana",
                "shortcut",
                "contentful",
                "bitrix24",
                "resend",
                "databricks",
                "mediawiki",
                "vendure",
                "microsoft",
                "workiq",
                "descope",
                "freee",
                "firehydrant",
                "postman",
                "targetprocess",
                "transkribus",
                "trustpager",
                "z_ai",
                "zai",
                "variflight",
                "langwatch",
                "transcend",
                "supabase",
                "heroku",
                "azure-devops",
                "apify",
                "browserstack",
                "datadog",
                "dynatrace",
                "transcend",
                "rancher",
                "boondmanager",
                "appflowy",
                "shortcut",
                "appnest",
                "openfec",
                "pipedrive",
                "ramp",
                "retellai",
                "deploysapp",
                "octopusdeploy",
                "api",
                "rest",
                "graphql",
                "contentful",
                "ramp",
                "wordpress",
                "outlook",
                "calendar",
                "mail",
                "crm",
                "canvas",
                "lms",
                "gohighlevel",
                "ghl",
            ],
        )
    {
        signals.insert("network-or-external-api".to_string());
    }

    if has_any_token(
        &tokens,
        &[
            "azure",
            "aws",
            "gcp",
            "cloudflare",
            "terraform",
            "pulumi",
            "heroku",
        ],
    ) {
        signals.insert("cloud-admin".to_string());
    }
    if has_any_token(&tokens, &["kubernetes", "k8s", "kubectl", "helm"]) {
        signals.insert("cluster-control".to_string());
    }
    if has_any_token(&tokens, &["okta", "auth0", "entra", "identity", "scim"])
        || haystack.contains("active directory")
    {
        signals.insert("identity-admin".to_string());
    }
    if has_any_token(
        &tokens,
        &[
            "vault",
            "1password",
            "bitwarden",
            "keychain",
            "secret",
            "secrets",
        ],
    ) || haystack.contains("secret manager")
    {
        signals.insert("secrets-manager".to_string());
    }
    if has_any_token(
        &tokens,
        &["stripe", "paypal", "billing", "payment", "payments"],
    ) {
        signals.insert("payments-financial".to_string());
    }
    if has_any_token(
        &tokens,
        &[
            "wallet",
            "phantom",
            "ethereum",
            "blockchain",
            "web3",
            "crypto",
        ],
    ) {
        signals.insert("blockchain-wallet".to_string());
    }
    if has_any_token(
        &tokens,
        &[
            "token",
            "tokens",
            "api_key",
            "apikey",
            "auth",
            "oauth",
            "bearer",
            "credential",
            "credentials",
        ],
    ) {
        signals.insert("credentials-or-auth".to_string());
    }
    if has_any_token(
        &tokens,
        &[
            "shell",
            "terminal",
            "exec",
            "execute",
            "command-runner",
            "code-runner",
            "run_command",
            "ssh",
            "sftp",
        ],
    ) {
        signals.insert("shell-or-process".to_string());
    }

    let mutation_terms = [
        "write",
        "delete",
        "remove",
        "rm",
        "rmdir",
        "mkdir",
        "create",
        "update",
        "insert",
        "modify",
        "move",
        "rename",
        "upload",
        "send",
        "post",
        "commit",
        "push",
        "apply",
        "drop",
        "truncate",
        "deploy",
        "publish",
        "execute",
        "run_command",
        "patch",
        "edit",
        "upsert",
        "append",
        "add",
    ];
    let readonly_terms = [
        "read",
        "list",
        "search",
        "query",
        "find",
        "get",
        "lookup",
        "inspect",
        "schema",
        "describe",
        "status",
        "weather",
        "time",
        "calculate",
        "calculator",
        "show",
        "view",
    ];
    let has_mutation_terms = has_any_token(&tokens, &mutation_terms);
    let has_readonly_terms = has_any_token(&tokens, &readonly_terms);
    if has_mutation_terms {
        signals.insert("mutable-tools".to_string());
    }
    if has_readonly_terms && !has_mutation_terms {
        signals.insert("readonly-tools".to_string());
    }

    if signals.is_empty() {
        signals.insert("unknown-side-effects".to_string());
    }
    signals
}

fn command_semantic_signal(command: &str) -> String {
    let command_name = command
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim_matches(|character: char| character == '"' || character == '\'')
        .to_ascii_lowercase();
    match command_name.as_str() {
        "sh" | "bash" | "zsh" | "fish" | "cmd" | "cmd.exe" | "powershell" | "powershell.exe"
        | "pwsh" | "pwsh.exe" | "ssh" | "ssh.exe" | "sftp" | "sftp.exe" | "terminal" => {
            command_name
        }
        _ if looks_like_launcher_package_signal(&command_name) => command_name,
        _ => String::new(),
    }
}

fn signal_tokens(text: &str) -> BTreeSet<String> {
    text.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn has_any_token(tokens: &BTreeSet<String>, needles: &[&str]) -> bool {
    needles.iter().any(|needle| tokens.contains(*needle))
}

fn has_any_substring(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn infer_source_type(raw_source_type: &str, command: &str, url: &str) -> String {
    crate::source_type::infer_public_source_type(raw_source_type, command, url)
}

fn source_enabled(value: &JsonValue) -> bool {
    if value
        .get("disabled")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false)
    {
        return false;
    }
    value
        .get("enabled")
        .and_then(JsonValue::as_bool)
        .unwrap_or(true)
}

fn supported_transports_for_source_type(source_type: &str) -> Vec<String> {
    match source_type {
        "stdio" => vec!["stdio".to_string()],
        "streamable-http" | "http" => vec!["streamable-http".to_string()],
        "sse-legacy" | "sse" => vec!["sse".to_string()],
        other if !other.is_empty() => vec![other.to_string()],
        _ => Vec::new(),
    }
}

fn normalize_server_record(
    name: &str,
    value: &JsonValue,
    source_record: Option<&SourceServerRecord>,
    execution_defaults: &ExecutionPolicy,
    profile_override_enabled: Option<bool>,
) -> Option<ServerRecord> {
    let object = value.as_object()?;
    let policy = object.get("policy").and_then(JsonValue::as_object);
    let installer = object.get("installer").and_then(JsonValue::as_object);
    let required = object
        .get("required")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let default_enabled = object
        .get("defaultEnabled")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let profile_enabled = if required {
        true
    } else {
        profile_override_enabled.unwrap_or(default_enabled)
    };
    let source_enabled = source_record.map(|record| record.enabled).unwrap_or(false);
    let platforms =
        json_helpers::strings_from_array(object.get("platforms").and_then(JsonValue::as_array));
    let platform_supported = platform_utils::supports_current_platform(&platforms);
    let base_effective_enabled = profile_enabled && source_enabled && platform_supported;

    let raw_source_type = source_record
        .map(|record| record.source_type.clone())
        .unwrap_or_default();
    let source_command = source_record
        .map(|record| record.command.clone())
        .unwrap_or_default();
    let source_url = source_record
        .map(|record| record.url.clone())
        .unwrap_or_default();
    let source_path = source_record
        .map(|record| record.source_path.clone())
        .unwrap_or_default();
    let source_env_names = source_record
        .map(|record| record.env_names.clone())
        .unwrap_or_default();
    let source_header_names = source_record
        .map(|record| record.header_names.clone())
        .unwrap_or_default();
    let source_type = if source_record.is_some() {
        infer_source_type(&raw_source_type, &source_command, &source_url)
    } else {
        String::new()
    };
    let source_args = source_record
        .map(|record| record.args.clone())
        .unwrap_or_default();
    let mut source_profile_hints = source_record
        .map(|record| record.profile_hints.clone())
        .unwrap_or_default();
    source_profile_hints.extend(json_helpers::strings_from_array(
        object
            .get("mcpaceProfileHints")
            .or_else(|| object.get("profileHints"))
            .and_then(JsonValue::as_array),
    ));
    source_profile_hints.sort();
    source_profile_hints.dedup();
    let signal_args = source_signal_args(&source_args, &source_profile_hints);
    let inferred_source_record = SourceServerRecord {
        name: if source_record
            .map(|record| record.name.trim().is_empty())
            .unwrap_or(true)
        {
            name.trim().to_string()
        } else {
            source_record
                .map(|record| record.name.clone())
                .unwrap_or_else(|| name.trim().to_string())
        },
        enabled: source_enabled,
        source_type: source_type.clone(),
        command: source_command.clone(),
        url: source_url.clone(),
        args: source_args.clone(),
        env_names: source_env_names.clone(),
        header_names: source_header_names.clone(),
        source_path: source_path.clone(),
        profile_hints: source_profile_hints.clone(),
        execution: source_record.and_then(|record| record.execution.clone()),
        policy: source_record.and_then(|record| record.policy.clone()),
    };
    let normalized_name_for_policy = name.trim().to_ascii_lowercase();
    let inferred_policy = infer_generic_source_policy(
        &normalized_name_for_policy,
        &inferred_source_record,
        &source_type,
    );

    let scope_class = policy_token(policy, "scopeClass", inferred_policy.scope_class);
    let concurrency_policy = policy_token(
        policy,
        "concurrencyPolicy",
        inferred_policy.concurrency_policy,
    );
    let state_binding = policy_token(policy, "stateBinding", inferred_policy.state_binding);
    let credential_binding = policy_token(
        policy,
        "credentialBinding",
        inferred_policy.credential_binding,
    );
    let parallelism_limit = policy_usize(
        policy,
        "parallelismLimit",
        inferred_policy.parallelism_limit,
    );
    let default_conflict_domain = if inferred_policy.conflict_domain_prefix.is_empty() {
        name.trim().to_string()
    } else {
        format!(
            "{}:{}",
            inferred_policy.conflict_domain_prefix, normalized_name_for_policy
        )
    };
    let conflict_domain = policy_string(policy, "conflictDomain", &default_conflict_domain);
    let project_root_mode =
        policy_token(policy, "projectRootMode", inferred_policy.project_root_mode);
    let worktree_binding =
        policy_token(policy, "worktreeBinding", inferred_policy.worktree_binding);
    let state_profile_mode = policy_token(
        policy,
        "stateProfileMode",
        inferred_policy.state_profile_mode,
    );
    let host_lock = policy_token(policy, "hostLock", inferred_policy.host_lock);
    let startup_strategy =
        policy_token(policy, "startupStrategy", inferred_policy.startup_strategy);
    let routing_group = policy_token(policy, "routingGroup", inferred_policy.routing_group);
    let discovery_requires_lease = policy_bool(
        policy,
        "discoveryRequiresLease",
        inferred_policy.discovery_requires_lease,
    );
    let kind = object
        .get("kind")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let tool_policies = object
        .get("toolPolicies")
        .and_then(JsonValue::as_array)
        .map(|items| items.to_vec())
        .unwrap_or_default();
    let installer_method = installer
        .and_then(|installer| installer.get("installMethod"))
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let installer_package = installer
        .and_then(|installer| installer.get("installPackage"))
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let parallel_safety_class = infer_parallel_safety_class(
        &source_type,
        &scope_class,
        &concurrency_policy,
        &state_binding,
        &credential_binding,
        &source_command,
        &source_url,
        &signal_args,
        &tool_policies,
    );
    let runtime_classification = infer_runtime_classification(
        &source_type,
        &scope_class,
        &concurrency_policy,
        &state_binding,
        &credential_binding,
        &source_command,
        &source_url,
        &signal_args,
        &tool_policies,
    );
    let runtime_type = policy_token(policy, "runtimeType", runtime_classification.runtime_type);
    let state_class = policy_token(policy, "stateClass", runtime_classification.state_class);
    let effect_class = policy_token(policy, "effectClass", runtime_classification.effect_class);
    let default_pool_model = infer_default_pool_model(
        &source_type,
        &scope_class,
        &concurrency_policy,
        &state_binding,
        &credential_binding,
    );
    let fallback_mode = ExecutionPolicy::inferred_mode(
        &scope_class,
        &concurrency_policy,
        &state_binding,
        &state_class,
    );
    let execution_defaults_json = execution_defaults.to_config_json_value();
    let canonical_execution_policy = object
        .get("policy")
        .or_else(|| source_record.and_then(|record| record.policy.as_ref()));
    let server_execution = object
        .get("execution")
        .or_else(|| source_record.and_then(|record| record.execution.as_ref()));
    let execution = ExecutionPolicy::resolve_with_canonical(
        Some(&execution_defaults_json),
        canonical_execution_policy,
        server_execution,
        fallback_mode,
    );
    let max_workers = execution.worker_limit();
    let max_in_flight_per_worker = execution.effective_max_in_flight_per_worker(&source_type);
    let effective_enabled = base_effective_enabled
        && !execution.is_disabled()
        && !runtime_policy_disabled(
            &scope_class,
            &concurrency_policy,
            &startup_strategy,
            &routing_group,
            max_workers,
        );
    let lock_domains = infer_lock_domains(
        &scope_class,
        &concurrency_policy,
        &state_binding,
        &credential_binding,
        name,
    );
    let profile_evidence = profile_evidence_records(ProfileEvidenceInput {
        source_type: &source_type,
        scope_class: &scope_class,
        concurrency_policy: &concurrency_policy,
        state_binding: &state_binding,
        credential_binding: &credential_binding,
        runtime_type: &runtime_type,
        state_class: &state_class,
        effect_class: &effect_class,
        command: &source_command,
        url: &source_url,
        args: &signal_args,
    });

    Some(ServerRecord {
        name: name.to_string(),
        kind,
        required,
        default_enabled,
        profile_enabled,
        platform_supported,
        effective_enabled,
        auto_start: object
            .get("autoStart")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false),
        transport_preference: inferred_transport_preference(object, &source_type),
        supported_transports: inferred_supported_transports(object, &source_type),
        platforms,
        required_commands: inferred_required_commands(object, &source_type, &source_command),
        scope_class,
        concurrency_policy,
        state_binding,
        credential_binding,
        parallelism_limit,
        parallel_safety_class,
        runtime_type,
        state_class,
        effect_class,
        default_pool_model,
        max_workers,
        max_in_flight_per_worker,
        execution,
        transport_status: transport_status_for_source_type(&source_type),
        launcher_kind: infer_launcher_kind(
            &source_command,
            &source_url,
            &installer_method,
            &installer_package,
        ),
        lock_domains,
        profile_evidence,
        conflict_domain,
        project_root_mode,
        worktree_binding,
        state_profile_mode,
        host_lock,
        startup_strategy,
        routing_group,
        discovery_requires_lease,
        health_url: object
            .get("healthUrl")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        source_enabled,
        source_type,
        source_path,
        source_command,
        source_args,
        source_env_names,
        source_header_names,
        source_url,
        tool_policies,
        installer_target: installer
            .and_then(|installer| installer.get("installTarget"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        installer_method,
        installer_package,
        installer_verify_command: installer
            .and_then(|installer| installer.get("verifyCommand"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
    })
}

fn policy_token(policy: Option<&BTreeMap<String, JsonValue>>, key: &str, fallback: &str) -> String {
    text_utils::normalize_flag(&policy_string(policy, key, fallback))
}

fn policy_string(
    policy: Option<&BTreeMap<String, JsonValue>>,
    key: &str,
    fallback: &str,
) -> String {
    policy
        .and_then(|policy| policy.get(key))
        .and_then(JsonValue::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn policy_usize(policy: Option<&BTreeMap<String, JsonValue>>, key: &str, fallback: usize) -> usize {
    policy
        .and_then(|policy| policy.get(key))
        .and_then(JsonValue::as_i64)
        .filter(|value| *value >= 0)
        .map(|value| value as usize)
        .unwrap_or(fallback)
}

fn policy_bool(policy: Option<&BTreeMap<String, JsonValue>>, key: &str, fallback: bool) -> bool {
    policy
        .and_then(|policy| policy.get(key))
        .and_then(JsonValue::as_bool)
        .unwrap_or(fallback)
}

fn runtime_policy_disabled(
    scope_class: &str,
    concurrency_policy: &str,
    startup_strategy: &str,
    routing_group: &str,
    max_workers: usize,
) -> bool {
    let scope_class = scope_class.trim().to_ascii_lowercase();
    let concurrency_policy = concurrency_policy.trim().to_ascii_lowercase();
    let startup_strategy = startup_strategy.trim().to_ascii_lowercase();
    let routing_group = routing_group.trim().to_ascii_lowercase();
    startup_strategy == "disabled"
        || routing_group == "disabled"
        || concurrency_policy == "plan-only"
        || scope_class == "not-runnable"
        || max_workers == 0
}

fn inferred_transport_preference(
    object: &BTreeMap<String, JsonValue>,
    source_type: &str,
) -> String {
    let configured = object
        .get("transportPreference")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if configured.is_empty() {
        source_type.to_string()
    } else {
        configured
    }
}

fn inferred_supported_transports(
    object: &BTreeMap<String, JsonValue>,
    source_type: &str,
) -> Vec<String> {
    let mut transports = json_helpers::strings_from_array(
        object
            .get("supportedTransports")
            .and_then(JsonValue::as_array),
    );
    if transports.is_empty() {
        transports = supported_transports_for_source_type(source_type);
    }
    transports.sort();
    transports.dedup();
    transports
}

fn inferred_required_commands(
    object: &BTreeMap<String, JsonValue>,
    source_type: &str,
    source_command: &str,
) -> Vec<String> {
    let mut commands = json_helpers::strings_from_array(
        object.get("requiredCommands").and_then(JsonValue::as_array),
    );
    let command = source_command.trim();
    if source_type == "stdio" && !command.is_empty() {
        commands.push(command.to_string());
    }
    commands.sort();
    commands.dedup();
    commands
}

fn transport_status_for_source_type(source_type: &str) -> String {
    match source_type {
        "sse-legacy" | "sse" => "legacy-compat".to_string(),
        "streamable-http" | "http" | "stdio" => "stable".to_string(),
        "" => "inferred".to_string(),
        _ => "custom".to_string(),
    }
}

fn infer_launcher_kind(
    command: &str,
    url: &str,
    installer_method: &str,
    installer_package: &str,
) -> String {
    let command = command.trim().to_ascii_lowercase();
    let method = installer_method.trim().to_ascii_lowercase();
    let package = installer_package.trim().to_ascii_lowercase();
    if !url.trim().is_empty() {
        return "remote-url".to_string();
    }
    if command.contains("npx") || method == "npm" || package.starts_with("npm:") {
        return "npx".to_string();
    }
    if command.contains("uvx") || method == "pypi" || package.starts_with("pypi:") {
        return "uvx".to_string();
    }
    if command.contains("docker") || method == "oci" || package.starts_with("oci:") {
        return "oci".to_string();
    }
    if command.is_empty() && method.is_empty() && package.is_empty() {
        return "unspecified".to_string();
    }
    "local-command".to_string()
}

#[allow(clippy::too_many_arguments)]
fn infer_runtime_classification(
    source_type: &str,
    scope_class: &str,
    concurrency_policy: &str,
    state_binding: &str,
    credential_binding: &str,
    command: &str,
    url: &str,
    args: &[String],
    tool_policies: &[JsonValue],
) -> RuntimeClassification {
    let signals = source_signals("", "", command, url, args);
    let remote =
        source_type == "streamable-http" || source_type == "http" || !url.trim().is_empty();
    let explicit_stateless = policy_is_explicit_stateless(concurrency_policy, state_binding);
    let read_only_tools = tool_policies.iter().any(policy_mentions_readonly)
        || signals.contains("readonly-tools")
        || signals.contains("local-utility");
    let destructive_tools =
        tool_policies.iter().any(policy_mentions_destructive) || signals.contains("mutable-tools");

    if source_type == "sse-legacy" || source_type == "sse" {
        return RuntimeClassification {
            runtime_type: "legacy",
            state_class: "legacy-transport",
            effect_class: "unknown",
        };
    }

    if signals.contains("sdk-or-example")
        || scope_class == "not-runnable"
        || concurrency_policy == "plan-only"
    {
        return RuntimeClassification {
            runtime_type: "package-artifact",
            state_class: "not-a-server",
            effect_class: "not-runnable",
        };
    }

    if signals.contains("shell-or-process") {
        return RuntimeClassification {
            runtime_type: "side-effecting",
            state_class: "host-stateful",
            effect_class: "process-exec",
        };
    }

    if signals.contains("remote-browser-session") {
        return RuntimeClassification {
            runtime_type: "interactive",
            state_class: "remote-session-stateful",
            effect_class: "external-mutating",
        };
    }

    if (signals.contains("browser-observation") || state_binding == "host-readonly")
        && !destructive_tools
    {
        return RuntimeClassification {
            runtime_type: "stateful",
            state_class: "host-stateful",
            effect_class: "read-only",
        };
    }

    if signals.contains("browser-observation") || state_binding == "host-readonly" {
        return RuntimeClassification {
            runtime_type: "side-effecting",
            state_class: "host-stateful",
            effect_class: "host-mutating",
        };
    }

    if signals.contains("browser-or-desktop") {
        return RuntimeClassification {
            runtime_type: "interactive",
            state_class: "host-stateful",
            effect_class: "host-mutating",
        };
    }

    if signals.contains("network-database") || state_binding == "db-connection" {
        return RuntimeClassification {
            runtime_type: "external",
            state_class: "credential-stateful",
            effect_class: "external-mutating",
        };
    }

    if signals.contains("project-analysis")
        && !signals.contains("network-or-external-api")
        && !signals.contains("credentials-or-auth")
        && !signals.contains("cloud-admin")
        && !signals.contains("external-read-api")
        && !signals.contains("documentation-lookup")
    {
        return RuntimeClassification {
            runtime_type: "stateful",
            state_class: "project-stateful",
            effect_class: if destructive_tools {
                "project-mutating"
            } else {
                "read-only"
            },
        };
    }

    if scope_class == "project-local"
        || concurrency_policy == "isolated-per-project"
        || matches!(
            state_binding,
            "repo"
                | "repo-path"
                | "file"
                | "file-path"
                | "db"
                | "db-file-path"
                | "project"
                | "project-index"
        )
        || signals.contains("filesystem")
        || signals.contains("git-repository")
        || signals.contains("database")
    {
        return RuntimeClassification {
            runtime_type: "stateful",
            state_class: "project-stateful",
            effect_class: "project-mutating",
        };
    }

    if scope_class == "state-profile"
        || concurrency_policy == "single-session"
        || matches!(state_binding, "context-store" | "memory" | "host-session")
        || signals.contains("memory-or-context")
    {
        return RuntimeClassification {
            runtime_type: "stateful",
            state_class: "session-stateful",
            effect_class: "ephemeral-state",
        };
    }

    if signals.contains("transport-gateway") && !destructive_tools {
        return RuntimeClassification {
            runtime_type: "stateless",
            state_class: "stateless",
            effect_class: "read-only",
        };
    }

    if (signals.contains("documentation-lookup")
        || signals.contains("external-read-api")
        || (signals.contains("network-fetch") && !destructive_tools))
        && !signals.contains("network-or-external-api")
        && !signals.contains("credentials-or-auth")
        && !signals.contains("cloud-admin")
        && !signals.contains("cluster-control")
    {
        return RuntimeClassification {
            runtime_type: "stateless",
            state_class: "stateless",
            effect_class: "external-read",
        };
    }

    if remote {
        if read_only_tools && !destructive_tools && explicit_stateless {
            return RuntimeClassification {
                runtime_type: "stateless",
                state_class: "stateless",
                effect_class: "external-read",
            };
        }
        return RuntimeClassification {
            runtime_type: "external",
            state_class: "remote-session-stateful",
            effect_class: if destructive_tools {
                "external-mutating"
            } else {
                "external-unknown"
            },
        };
    }

    if scope_class == "credential-scoped"
        || (!credential_binding.trim().is_empty() && credential_binding != "none")
        || signals.contains("cloud-admin")
        || signals.contains("identity-admin")
        || signals.contains("cluster-control")
        || signals.contains("secrets-manager")
        || signals.contains("payments-financial")
        || signals.contains("blockchain-wallet")
        || signals.contains("network-or-external-api")
    {
        if signals.contains("network-fetch") && read_only_tools && !destructive_tools {
            return RuntimeClassification {
                runtime_type: "stateless",
                state_class: "stateless",
                effect_class: "external-read",
            };
        }
        return RuntimeClassification {
            runtime_type: "external",
            state_class: "credential-stateful",
            effect_class: if destructive_tools {
                "external-mutating"
            } else {
                "external-unknown"
            },
        };
    }

    if explicit_stateless
        || (concurrency_policy == "multi-reader"
            && matches!(state_binding, "" | "none" | "stateless")
            && (read_only_tools || signals.contains("network-fetch")))
    {
        return RuntimeClassification {
            runtime_type: "stateless",
            state_class: "stateless",
            effect_class: if signals.contains("network-fetch") {
                "external-read"
            } else {
                "read-only"
            },
        };
    }

    if destructive_tools {
        return RuntimeClassification {
            runtime_type: "side-effecting",
            state_class: "unknown-stateful",
            effect_class: "unknown-mutating",
        };
    }

    RuntimeClassification {
        runtime_type: "unknown",
        state_class: "unknown-conservative",
        effect_class: "unknown",
    }
}

fn policy_mentions_destructive(value: &JsonValue) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object
        .get("destructiveHint")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false)
        || object
            .get("readOnlyHint")
            .and_then(JsonValue::as_bool)
            .map(|value| !value)
            .unwrap_or(false)
        || object
            .get("readOnly")
            .and_then(JsonValue::as_bool)
            .map(|value| !value)
            .unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
fn infer_parallel_safety_class(
    source_type: &str,
    scope_class: &str,
    concurrency_policy: &str,
    state_binding: &str,
    credential_binding: &str,
    command: &str,
    url: &str,
    args: &[String],
    tool_policies: &[JsonValue],
) -> String {
    let signals = source_signals("", "", command, url, args);
    let destructive_tools =
        tool_policies.iter().any(policy_mentions_destructive) || signals.contains("mutable-tools");
    if source_type == "sse-legacy" || source_type == "sse" {
        return "PX_legacy_compat".to_string();
    }
    if (signals.contains("browser-observation") || state_binding == "host-readonly")
        && !destructive_tools
    {
        return "P1_host_readonly_candidate".to_string();
    }
    if scope_class == "shared-exclusive"
        || state_binding == "host-desktop"
        || state_binding == "remote-browser-session"
        || signals.contains("browser-or-desktop")
        || signals.contains("remote-browser-session")
        || signals.contains("shell-or-process")
    {
        return "PX_forbidden".to_string();
    }
    if scope_class == "project-local" || concurrency_policy == "isolated-per-project" {
        return "P3_project_safe".to_string();
    }
    if signals.contains("filesystem")
        || signals.contains("git-repository")
        || signals.contains("database")
    {
        return "P3_project_safe".to_string();
    }
    if destructive_tools {
        return "P0_mutating_requires_serialization".to_string();
    }
    if policy_is_explicit_stateless(concurrency_policy, state_binding) {
        if source_type == "streamable-http" || source_type == "http" || !url.trim().is_empty() {
            return "P4_stateless_remote_candidate".to_string();
        }
        return "P1_readonly_candidate".to_string();
    }
    if !credential_binding.trim().is_empty() && credential_binding != "none" {
        return "P2_session_safe".to_string();
    }
    if concurrency_policy == "single-session" || signals.contains("memory-or-context") {
        return "P2_session_safe".to_string();
    }
    if source_type == "streamable-http" || source_type == "http" || !url.trim().is_empty() {
        return "P2_session_safe".to_string();
    }
    if signals.contains("network-fetch")
        || signals.contains("documentation-lookup")
        || signals.contains("external-read-api")
        || signals.contains("local-utility")
        || signals.contains("transport-gateway")
    {
        return "P1_readonly_candidate".to_string();
    }
    if concurrency_policy == "multi-reader" {
        return "P1_readonly_candidate".to_string();
    }
    if tool_policies.iter().any(policy_mentions_readonly) {
        return "P1_readonly_candidate".to_string();
    }
    if !command.trim().is_empty() {
        return "P0_unknown_stdio".to_string();
    }
    "P0_unknown".to_string()
}

fn policy_is_explicit_stateless(concurrency_policy: &str, state_binding: &str) -> bool {
    let state_binding = state_binding.trim();
    (concurrency_policy == "multi-reader"
        || concurrency_policy == "read-only"
        || concurrency_policy == "readonly")
        && (state_binding == "none" || state_binding == "stateless")
}

fn policy_mentions_readonly(value: &JsonValue) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object
        .get("readOnlyHint")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false)
        || object
            .get("readOnly")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
}

fn infer_default_pool_model(
    source_type: &str,
    scope_class: &str,
    concurrency_policy: &str,
    state_binding: &str,
    credential_binding: &str,
) -> String {
    if source_type == "sse-legacy" || source_type == "sse" {
        return "legacy-disabled".to_string();
    }
    if scope_class == "not-runnable" || concurrency_policy == "plan-only" {
        return "disabled".to_string();
    }
    if state_binding == "host-readonly" && concurrency_policy == "multi-reader" {
        return "host-readonly-pool".to_string();
    }
    if scope_class == "shared-exclusive"
        || state_binding == "host-desktop"
        || concurrency_policy == "single-session"
    {
        return "singleton".to_string();
    }
    if scope_class == "project-local" || concurrency_policy == "isolated-per-project" {
        return "project-pool".to_string();
    }
    if policy_is_explicit_stateless(concurrency_policy, state_binding) && source_type == "stdio" {
        return "process-pool".to_string();
    }
    if source_type == "streamable-http" || source_type == "http" {
        return "remote-http-session-pool".to_string();
    }
    if !credential_binding.trim().is_empty() && credential_binding != "none" {
        return "credential-session-pool".to_string();
    }
    if source_type == "stdio" {
        return "process-pool".to_string();
    }
    "singleton".to_string()
}

fn infer_lock_domains(
    scope_class: &str,
    concurrency_policy: &str,
    state_binding: &str,
    credential_binding: &str,
    fallback_domain: &str,
) -> Vec<String> {
    let mut domains = Vec::new();
    if !credential_binding.trim().is_empty() && credential_binding != "none" {
        domains.push(format!("credential:{}", credential_binding.trim()));
    }
    if scope_class == "project-local" || concurrency_policy == "isolated-per-project" {
        domains.push("project".to_string());
    }
    match state_binding {
        "repo" | "repo-path" => domains.push("repo".to_string()),
        "file" | "file-path" => domains.push("file".to_string()),
        "db" | "db-file-path" => domains.push("db".to_string()),
        "host-desktop" => domains.push("browser-or-desktop-session".to_string()),
        "host-readonly" => domains.push("browser-readonly-session".to_string()),
        "host-session" => domains.push("host-session".to_string()),
        "remote-session" => domains.push("transport-session".to_string()),
        "context-store" | "memory" => domains.push("context-store".to_string()),
        "identity-tenant" | "tenant" => domains.push("tenant".to_string()),
        _ => {}
    }
    if concurrency_policy == "single-session" {
        domains.push("session".to_string());
    }
    if domains.is_empty() {
        domains.push(format!("server:{}", fallback_domain));
    }
    domains.sort();
    domains.dedup();
    domains
}

struct ProfileEvidenceInput<'a> {
    source_type: &'a str,
    scope_class: &'a str,
    concurrency_policy: &'a str,
    state_binding: &'a str,
    credential_binding: &'a str,
    runtime_type: &'a str,
    state_class: &'a str,
    effect_class: &'a str,
    command: &'a str,
    url: &'a str,
    args: &'a [String],
}

fn evidence_decision(
    input: &ProfileEvidenceInput<'_>,
    signals: &BTreeSet<String>,
) -> EvidenceDecision {
    let mut score = 0.20_f64;
    let mut sources = Vec::new();
    if !input.source_type.trim().is_empty() {
        score += 0.10;
        sources.push("transport-shape");
    }
    if !input.command.trim().is_empty() || !input.url.trim().is_empty() {
        score += 0.10;
        sources.push("launcher-shape");
    }
    if !input.args.is_empty() {
        score += 0.15;
        sources.push("profile-hints-or-safe-flags");
    }
    if !signals.contains("unknown-side-effects") {
        score += 0.20;
        sources.push("indirect-runtime-signals");
    }
    if input.credential_binding != "none" && !input.credential_binding.trim().is_empty() {
        score += 0.10;
        sources.push("credential-binding");
    }
    if policy_is_explicit_stateless(input.concurrency_policy, input.state_binding)
        || matches!(
            input.concurrency_policy,
            "single-writer" | "single-session" | "isolated-per-project" | "plan-only"
        )
    {
        score += 0.10;
        sources.push("conservative-policy-binding");
    }
    if input.source_type == "sse" || input.source_type == "sse-legacy" {
        score += 0.05;
        sources.push("legacy-transport-detection");
    }
    if !signals.contains("unknown-side-effects")
        && (input.runtime_type == "stateless" || input.effect_class == "read-only")
    {
        score += 0.10;
        sources.push("read-only-or-stateless-evidence");
    }
    if score > 0.95 {
        score = 0.95;
    }

    if input.scope_class == "not-runnable" || input.concurrency_policy == "plan-only" {
        return EvidenceDecision {
            score: score.max(0.70),
            level: "high",
            automatic_action: "plan-only",
            next_step: "do not run as an MCP server unless a package entrypoint and safe probe prove it is runnable",
            sources,
        };
    }
    if signals.contains("shell-or-process") {
        return EvidenceDecision {
            score: score.max(0.85),
            level: "high",
            automatic_action: "blocked-high-risk",
            next_step:
                "require explicit local approval and sandboxing before any live probe or tool call",
            sources,
        };
    }
    if signals.contains("unknown-side-effects") || input.state_class == "unknown-conservative" {
        return EvidenceDecision {
            score: score.min(0.49),
            level: "low",
            automatic_action: "needs-safe-probe",
            next_step: "run mcpace advanced dev lab probe to collect initialize and tools/list evidence before widening policy",
            sources,
        };
    }
    if input.runtime_type == "external"
        || input.state_class == "credential-stateful"
        || input.credential_binding != "none"
    {
        return EvidenceDecision {
            score: score.max(0.65),
            level: "medium",
            automatic_action: "static-safe-policy",
            next_step: "keep conservative credential-scoped policy until safe probe and tool schema review prove read-only behavior",
            sources,
        };
    }
    if input.runtime_type == "interactive" || input.state_class == "host-stateful" {
        return EvidenceDecision {
            score: score.max(0.70),
            level: "medium",
            automatic_action: "static-safe-policy",
            next_step: "keep single-session or host lock policy; safe probe may confirm tools but must not call actions",
            sources,
        };
    }

    EvidenceDecision {
        score: score.max(0.60),
        level: if score >= 0.75 { "high" } else { "medium" },
        automatic_action: "static-safe-policy",
        next_step:
            "use static conservative policy now; optional safe probe can raise evidence confidence",
        sources,
    }
}

fn profile_evidence_records(input: ProfileEvidenceInput<'_>) -> Vec<JsonValue> {
    let mut records = Vec::new();
    let signals = source_signals("", "", input.command, input.url, input.args);
    let decision = evidence_decision(&input, &signals);
    let signal_values: Vec<JsonValue> = signals.iter().cloned().map(JsonValue::string).collect();
    let evidence_sources = decision
        .sources
        .iter()
        .copied()
        .map(JsonValue::string)
        .collect::<Vec<_>>();
    records.push(JsonValue::object([
        ("kind", JsonValue::string("static")),
        ("confidence", JsonValue::number(decision.score)),
        ("evidenceScore", JsonValue::number(decision.score)),
        ("evidenceLevel", JsonValue::string(decision.level)),
        (
            "automaticAction",
            JsonValue::string(decision.automatic_action),
        ),
        ("nextStep", JsonValue::string(decision.next_step)),
        (
            "summary",
            JsonValue::string("Initial adaptive profile inferred from indirect config, transport, package hints, source command semantics and policy fields; weak evidence must be confirmed with a safe live probe before widening concurrency."),
        ),
        (
            "data",
            JsonValue::object([
                ("sourceType", JsonValue::string(input.source_type.to_string())),
                ("scopeClass", JsonValue::string(input.scope_class.to_string())),
                (
                    "concurrencyPolicy",
                    JsonValue::string(input.concurrency_policy.to_string()),
                ),
                (
                    "stateBinding",
                    JsonValue::string(input.state_binding.to_string()),
                ),
                (
                    "credentialBinding",
                    JsonValue::string(input.credential_binding.to_string()),
                ),
                ("runtimeType", JsonValue::string(input.runtime_type.to_string())),
                ("stateClass", JsonValue::string(input.state_class.to_string())),
                ("effectClass", JsonValue::string(input.effect_class.to_string())),
                (
                    "hasCommand",
                    JsonValue::bool(!input.command.trim().is_empty()),
                ),
                ("hasUrl", JsonValue::bool(!input.url.trim().is_empty())),
                ("argCount", JsonValue::number(input.args.len())),
                ("sourceSignals", JsonValue::array(signal_values)),
                ("evidenceSources", JsonValue::array(evidence_sources)),
            ]),
        ),
    ]));
    if input.source_type == "sse-legacy" || input.source_type == "sse" {
        records.push(JsonValue::object([
            ("kind", JsonValue::string("policy")),
            ("confidence", JsonValue::number(1)),
            (
                "summary",
                JsonValue::string("Legacy SSE compatibility is not treated as the stable default transport; prefer Streamable HTTP or stdio."),
            ),
        ]));
    }
    records
}
