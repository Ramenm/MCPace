use super::*;

pub(crate) fn remove_json_mcp_server_entry(
    existing: &str,
    adapter_key_name: &str,
    servers_object_key: &str,
    expected_server_url: &str,
    config_path: &Path,
) -> ConfigEditResult<ClientConfigUpdate> {
    if existing.trim().is_empty() {
        return Ok(ClientConfigUpdate {
            contents: existing.to_string(),
            replaced_existing_block: false,
        });
    }

    let mut root = parse_str(existing).map_err(|error| ConfigEditError::JsonParse {
        path: config_path_label(config_path),
        message: error.to_string(),
    })?;
    let root_object = root
        .as_object_mut()
        .ok_or_else(|| ConfigEditError::JsonTopLevelObject {
            path: config_path_label(config_path),
        })?;
    let servers_key = if servers_object_key.trim().is_empty() {
        "mcpServers"
    } else {
        servers_object_key.trim()
    };
    let Some(servers_value) = root_object.get_mut(servers_key) else {
        return Ok(ClientConfigUpdate {
            contents: existing.to_string(),
            replaced_existing_block: false,
        });
    };
    let servers_object =
        servers_value
            .as_object_mut()
            .ok_or_else(|| ConfigEditError::JsonServersObject {
                path: config_path_label(config_path),
                key: servers_key.to_string(),
            })?;
    let owned = servers_object
        .get(adapter_key_name)
        .is_some_and(|entry| json_value_contains_exact_string(entry, expected_server_url));
    if !owned {
        return Ok(ClientConfigUpdate {
            contents: existing.to_string(),
            replaced_existing_block: false,
        });
    }

    servers_object.remove(adapter_key_name);
    Ok(ClientConfigUpdate {
        contents: root.to_pretty_string(),
        replaced_existing_block: true,
    })
}

fn json_value_contains_exact_string(value: &JsonValue, expected: &str) -> bool {
    match value {
        JsonValue::String(value) => value == expected,
        JsonValue::Array(items) => items
            .iter()
            .any(|item| json_value_contains_exact_string(item, expected)),
        JsonValue::Object(entries) => entries
            .values()
            .any(|entry| json_value_contains_exact_string(entry, expected)),
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => false,
    }
}

pub(crate) fn remove_toml_mcp_server_block(
    existing: &str,
    adapter_key_name: &str,
    config_path: &Path,
) -> ConfigEditResult<ClientConfigUpdate> {
    let Some(span) = locate_toml_managed_block(existing, adapter_key_name, config_path)? else {
        return Ok(ClientConfigUpdate {
            contents: existing.to_string(),
            replaced_existing_block: false,
        });
    };
    Ok(ClientConfigUpdate {
        contents: apply_toml_managed_block_update(existing, "", span),
        replaced_existing_block: true,
    })
}

pub(crate) fn remove_yaml_mcp_server_entry(
    existing: &str,
    adapter_key_name: &str,
    config_path: &Path,
) -> ConfigEditResult<ClientConfigUpdate> {
    let Some((start, end)) = locate_managed_block(existing, adapter_key_name, config_path)? else {
        return Ok(ClientConfigUpdate {
            contents: existing.to_string(),
            replaced_existing_block: false,
        });
    };
    let mut updated = String::with_capacity(existing.len().saturating_sub(end - start));
    updated.push_str(&existing[..start]);
    updated.push_str(&existing[end..]);
    Ok(ClientConfigUpdate {
        contents: updated,
        replaced_existing_block: true,
    })
}
