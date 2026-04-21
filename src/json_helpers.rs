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

pub fn strings_from_array(value: Option<&[JsonValue]>) -> Vec<String> {
    value
        .unwrap_or(&[])
        .iter()
        .filter_map(|item| item.as_str().map(|text| text.trim().to_string()))
        .filter(|item| !item.is_empty())
        .collect()
}
