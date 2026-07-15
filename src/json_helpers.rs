use crate::json::{JsonParseError, JsonValue};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_JSON_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JsonFileError {
    Read {
        path: PathBuf,
        source: String,
    },
    Parse {
        path: PathBuf,
        source: JsonParseError,
    },
}

impl JsonFileError {
    #[cfg(test)]
    pub fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl fmt::Display for JsonFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonFileError::Read { path, source } => {
                write!(formatter, "failed to read {}: {}", path.display(), source)
            }
            JsonFileError::Parse { path, source } => {
                write!(formatter, "failed to parse {}: {}", path.display(), source)
            }
        }
    }
}

impl std::error::Error for JsonFileError {}

impl From<JsonFileError> for String {
    fn from(error: JsonFileError) -> Self {
        error.to_string()
    }
}

pub fn read_json_file(path: &Path) -> Result<JsonValue, JsonFileError> {
    let read_error = |source: String| JsonFileError::Read {
        path: path.to_path_buf(),
        source,
    };
    let file = File::open(path).map_err(|error| read_error(error.to_string()))?;
    let mut raw = String::new();
    file.take(MAX_JSON_FILE_BYTES.saturating_add(1))
        .read_to_string(&mut raw)
        .map_err(|error| read_error(error.to_string()))?;
    if raw.len() as u64 > MAX_JSON_FILE_BYTES {
        return Err(read_error(format!(
            "JSON file exceeds the {}-byte safety limit",
            MAX_JSON_FILE_BYTES
        )));
    }
    crate::json::parse_str(&raw).map_err(|source| JsonFileError::Parse {
        path: path.to_path_buf(),
        source,
    })
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
