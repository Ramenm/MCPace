use super::model::{ServerRecord, SourceServerRecord};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::profile;
use std::collections::BTreeMap;
use std::path::Path;

pub fn load_server_records(root_path: &Path) -> Result<Vec<ServerRecord>, String> {
    let config_path = root_path.join("mcpace.config.json");
    let config = json_helpers::read_json_file(&config_path)?;
    let source_settings = load_source_settings(root_path)?;
    let runtime_profile = profile::load_runtime_profile_selection(root_path)?;

    let mut records = Vec::new();
    if let Some(servers_object) = json_helpers::object_at_path(&config, &["servers"]) {
        for (name, value) in servers_object {
            let normalized_name = name.trim().to_ascii_lowercase();
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

    records.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    Ok(records)
}

fn load_source_settings(root_path: &Path) -> Result<BTreeMap<String, SourceServerRecord>, String> {
    let path = root_path.join("mcp_settings.json");
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }

    let json = json_helpers::read_json_file(&path)?;
    let mut map = BTreeMap::new();
    if let Some(servers_object) = json_helpers::object_at_path(&json, &["mcpServers"]) {
        for (name, value) in servers_object {
            let enabled = value
                .get("enabled")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false);
            let source_type = value
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
            map.insert(
                name.trim().to_ascii_lowercase(),
                SourceServerRecord {
                    enabled,
                    source_type,
                    command,
                    url,
                },
            );
        }
    }
    Ok(map)
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
    let platform_supported = server_supports_current_platform(&platforms);
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
    let browser_profile_mode = policy_string(policy, "browserProfileMode", "none");
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
        default_routing_group(&scope_class, &state_binding, &browser_profile_mode),
    );

    Some(ServerRecord {
        name: name.to_string(),
        kind: object
            .get("kind")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
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
        conflict_domain,
        project_root_mode,
        worktree_binding,
        browser_profile_mode,
        host_lock,
        startup_strategy,
        routing_group,
        health_url: object
            .get("healthUrl")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        source_enabled,
        source_type: source_record
            .map(|record| record.source_type.clone())
            .unwrap_or_default(),
        source_command: source_record
            .map(|record| record.command.clone())
            .unwrap_or_default(),
        source_url: source_record
            .map(|record| record.url.clone())
            .unwrap_or_default(),
        tool_policies: object
            .get("toolPolicies")
            .and_then(JsonValue::as_array)
            .map(|items| items.to_vec())
            .unwrap_or_default(),
        installer_target: installer
            .and_then(|installer| installer.get("installTarget"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        installer_method: installer
            .and_then(|installer| installer.get("installMethod"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        installer_package: installer
            .and_then(|installer| installer.get("installPackage"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
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

fn server_supports_current_platform(platforms: &[String]) -> bool {
    if platforms.is_empty() {
        return true;
    }
    let current = current_platform_alias();
    platforms.iter().any(|platform| {
        let normalized = normalize_platform(platform);
        normalized == current || normalized == "any" || normalized == "all" || normalized == "*"
    })
}

fn current_platform_alias() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        "linux" => "linux",
        other => other,
    }
}

fn normalize_platform(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "darwin" | "mac" | "osx" | "macos" => "macos".to_string(),
        "win" | "windows" => "windows".to_string(),
        "linux" => "linux".to_string(),
        other => other.to_string(),
    }
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

fn default_routing_group<'a>(
    scope_class: &'a str,
    state_binding: &'a str,
    browser_profile_mode: &'a str,
) -> &'a str {
    if state_binding == "host-session" || browser_profile_mode != "none" {
        "browser"
    } else if scope_class == "shared-exclusive" || state_binding == "host-desktop" {
        "desktop"
    } else if scope_class == "project-local" {
        "project"
    } else {
        "shared"
    }
}
