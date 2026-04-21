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
                runtime_profile.server_overrides.get(&normalized_name).copied(),
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
    let effective_enabled = profile_enabled && source_enabled;

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
        platforms: json_helpers::strings_from_array(
            object.get("platforms").and_then(JsonValue::as_array),
        ),
        required_commands: json_helpers::strings_from_array(
            object.get("requiredCommands").and_then(JsonValue::as_array),
        ),
        scope_class: policy
            .and_then(|policy| policy.get("scopeClass"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        concurrency_policy: policy
            .and_then(|policy| policy.get("concurrencyPolicy"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        state_binding: policy
            .and_then(|policy| policy.get("stateBinding"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        credential_binding: policy
            .and_then(|policy| policy.get("credentialBinding"))
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
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
