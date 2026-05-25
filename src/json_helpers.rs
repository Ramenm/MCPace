use crate::json::JsonValue;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub fn read_json_file(path: &Path) -> Result<JsonValue, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {}", path.display(), error))?;
    crate::json::parse_str(&raw)
        .map_err(|error| format!("failed to parse {}: {}", path.display(), error))
}

pub fn object_at_path<'a>(
    value: &'a JsonValue,
    path: &[&str],
) -> Option<&'a BTreeMap<String, JsonValue>> {
    value_at_path(value, path)?.as_object()
}

pub fn array_at_path<'a>(value: &'a JsonValue, path: &[&str]) -> Option<&'a [JsonValue]> {
    value_at_path(value, path)?.as_array()
}

pub fn string_at_path<'a>(value: &'a JsonValue, path: &[&str]) -> Option<&'a str> {
    value_at_path(value, path)?.as_str()
}

pub fn bool_at_path(value: &JsonValue, path: &[&str]) -> Option<bool> {
    value_at_path(value, path)?.as_bool()
}

pub fn value_at_path<'a>(value: &'a JsonValue, path: &[&str]) -> Option<&'a JsonValue> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    Some(current)
}

/// Returns the MCP server map from either common config shape.
///
/// MCP clients and examples commonly use either `mcpServers` or `servers`;
/// keeping this selector centralized prevents import, source listing, and setup
/// flows from drifting apart.
pub fn mcp_servers_object(value: &JsonValue) -> Option<&BTreeMap<String, JsonValue>> {
    mcp_servers_object_with_key(value).map(|(_, servers)| servers)
}

/// Returns the selected MCP server map plus the source key that was used.
pub fn mcp_servers_object_with_key(
    value: &JsonValue,
) -> Option<(&'static str, &BTreeMap<String, JsonValue>)> {
    object_at_path(value, &["mcpServers"])
        .map(|servers| ("mcpServers", servers))
        .or_else(|| object_at_path(value, &["servers"]).map(|servers| ("servers", servers)))
}

pub fn strings_from_array(value: Option<&[JsonValue]>) -> Vec<String> {
    value
        .unwrap_or(&[])
        .iter()
        .filter_map(|item| item.as_str().map(|text| text.trim().to_string()))
        .filter(|item| !item.is_empty())
        .collect()
}

pub fn json_string_or_null(value: Option<String>) -> JsonValue {
    match value {
        Some(value) => JsonValue::string(value),
        None => JsonValue::Null,
    }
}

pub fn empty_object() -> JsonValue {
    JsonValue::Object(BTreeMap::new())
}

pub fn optional_number<T: ToString>(value: Option<T>) -> JsonValue {
    value.map(JsonValue::number).unwrap_or(JsonValue::Null)
}
