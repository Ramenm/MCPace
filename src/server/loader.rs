use super::model::{ServerRecord, SourceServerRecord};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_sources;
use crate::platform_utils;
use crate::profile;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

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

pub fn load_server_records(root_path: &Path) -> Result<Vec<ServerRecord>, String> {
    let config_path = root_path.join("mcpace.config.json");
    let config = json_helpers::read_json_file(&config_path)?;
    let source_settings = load_source_settings(root_path)?;
    let runtime_profile = profile::load_runtime_profile_selection(root_path)?;

    let mut records = Vec::new();
    let mut declared_names = BTreeSet::new();
    if let Some(servers_object) = json_helpers::object_at_path(&config, &["servers"]) {
        for (name, value) in servers_object {
            let normalized_name = name.trim().to_ascii_lowercase();
            declared_names.insert(normalized_name.clone());
            if let Some(record) = normalize_server_record(
                name,
                value,
                source_settings.get(&normalized_name),
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
        records.push(generic_source_server_record(normalized_name, source_record));
    }

    records.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    Ok(records)
}

fn load_source_settings(root_path: &Path) -> Result<BTreeMap<String, SourceServerRecord>, String> {
    let registry = mcp_sources::load_mcp_server_registry(root_path)?;
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
            },
        );
    }
    Ok(map)
}

fn generic_source_server_record(
    normalized_name: &str,
    source_record: &SourceServerRecord,
) -> ServerRecord {
    let source_type = infer_source_type(
        &source_record.source_type,
        &source_record.command,
        &source_record.url,
    );
    let policy = infer_generic_source_policy(normalized_name, source_record, &source_type);
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

    ServerRecord {
        name: display_name.to_string(),
        kind,
        required: false,
        default_enabled: false,
        profile_enabled: source_record.enabled,
        platform_supported: true,
        effective_enabled: source_record.enabled,
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
            &source_record.args,
            &Vec::new(),
        ),
        default_pool_model: infer_default_pool_model(
            &source_type,
            policy.scope_class,
            policy.concurrency_policy,
            policy.state_binding,
            policy.credential_binding,
        ),
        max_workers: infer_max_workers(
            &source_type,
            policy.scope_class,
            policy.concurrency_policy,
            policy.parallelism_limit,
        ),
        max_in_flight_per_worker: 1,
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
            command: &source_record.command,
            url: &source_record.url,
            args: &source_record.args,
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
        source_command: source_record.command.clone(),
        source_url: source_record.url.clone(),
        tool_policies: Vec::new(),
        installer_target: "none".to_string(),
        installer_method: "user-supplied".to_string(),
        installer_package: String::new(),
        installer_verify_command: String::new(),
    }
}

fn infer_generic_source_policy(
    normalized_name: &str,
    source_record: &SourceServerRecord,
    source_type: &str,
) -> GenericSourcePolicy {
    let signals = source_signals(
        normalized_name,
        &source_record.name,
        &source_record.command,
        &source_record.url,
        &source_record.args,
    );
    let remote = source_type == "streamable-http"
        || source_type == "http"
        || !source_record.url.trim().is_empty();

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
        return GenericSourcePolicy {
            scope_class: "project-local",
            concurrency_policy: "single-writer",
            state_binding: "db",
            credential_binding: if signals.contains("credentials-or-auth") {
                "database-connection"
            } else {
                "none"
            },
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

    if signals.contains("network-fetch") {
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

    if signals.contains("local-utility") || signals.contains("readonly-tools") {
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
    let haystack = format!(
        "{} {} {} {} {}",
        normalized_name,
        display_name,
        command,
        url,
        args.join(" ")
    )
    .to_ascii_lowercase();
    let mut signals = BTreeSet::new();
    if haystack.contains("filesystem")
        || haystack.contains("file-system")
        || haystack.contains("server-filesystem")
        || haystack.contains("read_file")
        || haystack.contains("write_file")
    {
        signals.insert("filesystem".to_string());
    }
    if haystack.contains("git") || haystack.contains("repository") || haystack.contains("worktree")
    {
        signals.insert("git-repository".to_string());
    }
    if haystack.contains("sqlite")
        || haystack.contains("postgres")
        || haystack.contains("database")
        || haystack.contains("sql")
        || haystack.contains("db-path")
    {
        signals.insert("database".to_string());
    }
    if haystack.contains("playwright")
        || haystack.contains("puppeteer")
        || haystack.contains("chrome")
        || haystack.contains("browser")
        || haystack.contains("desktop")
    {
        signals.insert("browser-or-desktop".to_string());
    }
    if haystack.contains("memory")
        || haystack.contains("sequential-thinking")
        || haystack.contains("context-store")
        || haystack.contains("knowledge")
        || haystack.contains("thinking")
    {
        signals.insert("memory-or-context".to_string());
    }
    if haystack.contains("time") || haystack.contains("timezone") || haystack.contains("calculator")
    {
        signals.insert("local-utility".to_string());
    }
    if haystack.contains("fetch")
        || haystack.contains("http")
        || haystack.contains("url")
        || haystack.contains("web")
    {
        signals.insert("network-fetch".to_string());
    }
    if haystack.contains("context7")
        || haystack.contains("docs")
        || haystack.contains("documentation")
        || haystack.contains("github")
        || haystack.contains("notion")
        || haystack.contains("sentry")
        || haystack.contains("slack")
        || haystack.contains("maps")
        || haystack.contains("api")
    {
        signals.insert("network-or-external-api".to_string());
    }
    if haystack.contains("azure")
        || haystack.contains("aws")
        || haystack.contains("gcp")
        || haystack.contains("cloudflare")
        || haystack.contains("terraform")
        || haystack.contains("pulumi")
    {
        signals.insert("cloud-admin".to_string());
    }
    if haystack.contains("kubernetes") || haystack.contains("k8s") || haystack.contains("kubectl") {
        signals.insert("cluster-control".to_string());
    }
    if haystack.contains("okta")
        || haystack.contains("auth0")
        || haystack.contains("entra")
        || haystack.contains("active directory")
        || haystack.contains("identity")
        || haystack.contains("scim")
    {
        signals.insert("identity-admin".to_string());
    }
    if haystack.contains("vault")
        || haystack.contains("secret manager")
        || haystack.contains("1password")
        || haystack.contains("bitwarden")
        || haystack.contains("keychain")
    {
        signals.insert("secrets-manager".to_string());
    }
    if haystack.contains("stripe")
        || haystack.contains("paypal")
        || haystack.contains("billing")
        || haystack.contains("payment")
    {
        signals.insert("payments-financial".to_string());
    }
    if haystack.contains("wallet")
        || haystack.contains("ethereum")
        || haystack.contains("blockchain")
        || haystack.contains("web3")
    {
        signals.insert("blockchain-wallet".to_string());
    }
    if haystack.contains("token")
        || haystack.contains("api_key")
        || haystack.contains("apikey")
        || haystack.contains("auth")
        || haystack.contains("oauth")
        || haystack.contains("bearer")
    {
        signals.insert("credentials-or-auth".to_string());
    }
    if haystack.contains("shell")
        || haystack.contains("terminal")
        || haystack.contains("exec")
        || haystack.contains("command-runner")
        || haystack.contains("code-runner")
    {
        signals.insert("shell-or-process".to_string());
    }
    if signals.is_empty() {
        signals.insert("unknown-side-effects".to_string());
    }
    signals
}

fn infer_source_type(raw_source_type: &str, command: &str, url: &str) -> String {
    crate::source_type::infer_public_source_type(raw_source_type, command, url)
}

fn source_enabled(value: &JsonValue) -> bool {
    if let Some(enabled) = value.get("enabled").and_then(JsonValue::as_bool) {
        return enabled;
    }
    !value
        .get("disabled")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false)
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
    let effective_enabled = profile_enabled && source_enabled && platform_supported;

    let scope_class = policy_string(policy, "scopeClass", "");
    let concurrency_policy = policy_string(policy, "concurrencyPolicy", "");
    let state_binding = policy_string(policy, "stateBinding", "");
    let credential_binding = policy_string(policy, "credentialBinding", "");
    let parallelism_limit = policy_usize(
        policy,
        "parallelismLimit",
        default_parallelism_limit(&concurrency_policy),
    );
    let conflict_domain = policy_string(policy, "conflictDomain", name);
    let project_root_mode = policy_string(
        policy,
        "projectRootMode",
        default_project_root_mode(&scope_class, &concurrency_policy),
    );
    let worktree_binding = policy_string(
        policy,
        "worktreeBinding",
        default_worktree_binding(&scope_class, &state_binding),
    );
    let state_profile_mode = policy_string(policy, "stateProfileMode", "none");
    let host_lock = policy_string(
        policy,
        "hostLock",
        default_host_lock(&scope_class, &state_binding),
    );
    let startup_strategy = policy_string(
        policy,
        "startupStrategy",
        default_startup_strategy(&scope_class, &concurrency_policy, &state_binding),
    );
    let routing_group = policy_string(
        policy,
        "routingGroup",
        default_routing_group(&scope_class, &state_binding, &state_profile_mode),
    );
    let discovery_requires_lease = policy_bool(
        policy,
        "discoveryRequiresLease",
        default_discovery_requires_lease(&scope_class, &concurrency_policy, &state_binding),
    );
    let source_type = source_record
        .map(|record| record.source_type.clone())
        .unwrap_or_default();
    let source_command = source_record
        .map(|record| record.command.clone())
        .unwrap_or_default();
    let source_url = source_record
        .map(|record| record.url.clone())
        .unwrap_or_default();
    let source_args = source_record
        .map(|record| record.args.clone())
        .unwrap_or_default();
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
        &source_args,
        &tool_policies,
    );
    let default_pool_model = infer_default_pool_model(
        &source_type,
        &scope_class,
        &concurrency_policy,
        &state_binding,
        &credential_binding,
    );
    let max_workers = infer_max_workers(
        &source_type,
        &scope_class,
        &concurrency_policy,
        parallelism_limit,
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
        command: &source_command,
        url: &source_url,
        args: &source_args,
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
        transport_preference: object
            .get("transportPreference")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        supported_transports: json_helpers::strings_from_array(
            object
                .get("supportedTransports")
                .and_then(JsonValue::as_array),
        ),
        platforms,
        required_commands: json_helpers::strings_from_array(
            object.get("requiredCommands").and_then(JsonValue::as_array),
        ),
        scope_class,
        concurrency_policy,
        state_binding,
        credential_binding,
        parallelism_limit,
        parallel_safety_class,
        default_pool_model,
        max_workers,
        max_in_flight_per_worker: 1,
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
        source_command,
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
    if source_type == "sse-legacy" || source_type == "sse" {
        return "PX_legacy_compat".to_string();
    }
    if scope_class == "shared-exclusive"
        || state_binding == "host-desktop"
        || signals.contains("browser-or-desktop")
        || signals.contains("shell-or-process")
    {
        return "PX_forbidden".to_string();
    }
    if scope_class == "project-local" || concurrency_policy == "isolated-per-project" {
        return "P3_project_safe".to_string();
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
    if signals.contains("filesystem")
        || signals.contains("git-repository")
        || signals.contains("database")
    {
        return "P3_project_safe".to_string();
    }
    if signals.contains("network-fetch") || signals.contains("local-utility") {
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

fn infer_max_workers(
    source_type: &str,
    scope_class: &str,
    concurrency_policy: &str,
    parallelism_limit: usize,
) -> usize {
    if source_type == "sse-legacy" || source_type == "sse" {
        return 0;
    }
    if concurrency_policy == "single-session" || scope_class == "shared-exclusive" {
        return 1;
    }
    if parallelism_limit > 1 {
        return parallelism_limit;
    }
    if source_type == "streamable-http" || source_type == "http" {
        return 8;
    }
    if source_type == "stdio"
        && (concurrency_policy == "multi-reader" || scope_class == "project-local")
    {
        return 4;
    }
    1
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
    command: &'a str,
    url: &'a str,
    args: &'a [String],
}

fn profile_evidence_records(input: ProfileEvidenceInput<'_>) -> Vec<JsonValue> {
    let mut records = Vec::new();
    let signals = source_signals("", "", input.command, input.url, input.args);
    let signal_values: Vec<JsonValue> = signals.iter().cloned().map(JsonValue::string).collect();
    records.push(JsonValue::object([
        ("kind", JsonValue::string("static")),
        ("confidence", JsonValue::number(0.45)),
        (
            "summary",
            JsonValue::string("Initial adaptive profile inferred from local config, transport, source command/url, and policy fields; runtime probes can only lower or raise this with evidence."),
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
                (
                    "hasCommand",
                    JsonValue::bool(!input.command.trim().is_empty()),
                ),
                ("hasUrl", JsonValue::bool(!input.url.trim().is_empty())),
                ("argCount", JsonValue::number(input.args.len())),
                ("sourceSignals", JsonValue::array(signal_values)),
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

fn default_parallelism_limit(concurrency_policy: &str) -> usize {
    match concurrency_policy {
        "multi-reader" => 0,
        "isolated-per-project" | "single-writer" | "single-session" => 1,
        _ => 1,
    }
}

fn default_project_root_mode<'a>(scope_class: &'a str, concurrency_policy: &'a str) -> &'a str {
    if scope_class == "project-local" || concurrency_policy == "isolated-per-project" {
        "required"
    } else {
        "optional"
    }
}

fn default_worktree_binding<'a>(scope_class: &'a str, state_binding: &'a str) -> &'a str {
    if scope_class == "project-local" || matches!(state_binding, "repo" | "file" | "db" | "project")
    {
        "project-root"
    } else {
        "none"
    }
}

fn default_host_lock<'a>(scope_class: &'a str, state_binding: &'a str) -> &'a str {
    if scope_class == "shared-exclusive" || state_binding == "host-desktop" {
        "host-session"
    } else if state_binding == "host-session" {
        "instance"
    } else {
        "none"
    }
}

fn default_startup_strategy<'a>(
    scope_class: &'a str,
    concurrency_policy: &'a str,
    state_binding: &'a str,
) -> &'a str {
    if scope_class == "project-local" || concurrency_policy == "isolated-per-project" {
        "lazy-per-project"
    } else if scope_class == "shared-exclusive" || state_binding == "host-desktop" {
        "singleton-host"
    } else if state_binding == "host-session" {
        "lazy-per-profile"
    } else {
        "lazy-shared"
    }
}

fn default_discovery_requires_lease(
    scope_class: &str,
    concurrency_policy: &str,
    state_binding: &str,
) -> bool {
    scope_class == "shared-exclusive"
        || scope_class == "project-local"
        || scope_class == "state-profile"
        || concurrency_policy == "single-session"
        || concurrency_policy == "single-writer"
        || concurrency_policy == "isolated-per-project"
        || matches!(
            state_binding,
            "host-desktop"
                | "host-session"
                | "repo"
                | "file"
                | "db"
                | "database-file"
                | "context-store"
        )
}

fn default_routing_group<'a>(
    scope_class: &'a str,
    state_binding: &'a str,
    state_profile_mode: &'a str,
) -> &'a str {
    if state_binding == "host-session" || state_profile_mode != "none" {
        "stateful"
    } else if scope_class == "shared-exclusive" || state_binding == "host-desktop" {
        "desktop"
    } else if scope_class == "project-local" {
        "project"
    } else {
        "shared"
    }
}
