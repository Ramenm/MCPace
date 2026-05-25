use super::paths::resolve_under_root;
use super::{default_mcp_server_fragment_path, normalize_server_name};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct McpServerImportOptions {
    pub source_path: PathBuf,
    pub settings_path: Option<PathBuf>,
    pub dry_run: bool,
    pub force: bool,
}

#[derive(Clone, Debug)]
pub struct McpServerImportEntry {
    pub name: String,
    pub normalized_name: String,
    pub path: String,
    pub action: String,
    pub dry_run: bool,
    pub existed_before: bool,
}

#[derive(Clone, Debug)]
pub struct McpServerImportResult {
    pub source_path: String,
    pub dry_run: bool,
    pub force: bool,
    pub imported_count: usize,
    pub target_file_count: usize,
    pub entries: Vec<McpServerImportEntry>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
struct ImportPlanEntry {
    name: String,
    normalized_name: String,
    target_path: PathBuf,
    value: JsonValue,
}

pub fn import_mcp_server_entries(
    root_path: &Path,
    options: McpServerImportOptions,
) -> Result<McpServerImportResult, String> {
    let source_path = resolve_under_root(root_path, &options.source_path);
    if !source_path.is_file() {
        return Err(format!(
            "MCP settings import source '{}' does not exist",
            source_path.display()
        ));
    }
    let source_value = json_helpers::read_json_file(&source_path)?;
    let Some(source_servers) = json_helpers::mcp_servers_object(&source_value) else {
        return Err(format!(
            "MCP settings import source '{}' must contain an mcpServers or servers object",
            source_path.display()
        ));
    };
    if source_servers.is_empty() {
        return Err(format!(
            "MCP settings import source '{}' contains no servers",
            source_path.display()
        ));
    }

    let mut warnings = Vec::new();
    let mut seen_input_names = BTreeMap::<String, String>::new();
    let mut plan_entries = Vec::new();
    for (name, server_value) in source_servers {
        let normalized_name = normalize_server_name(name);
        if normalized_name.is_empty() {
            warnings.push(format!(
                "MCP settings import source '{}' contains an unusable server name; skipping",
                source_path.display()
            ));
            continue;
        }
        if let Some(previous) = seen_input_names.insert(normalized_name.clone(), name.clone()) {
            return Err(format!(
                "MCP settings import source '{}' contains duplicate normalized server names '{}' and '{}'",
                source_path.display(),
                previous,
                name
            ));
        }
        let Some(value) = normalize_import_server_value(
            root_path,
            name,
            server_value,
            &source_path,
            &mut warnings,
        ) else {
            continue;
        };
        let target_path = options
            .settings_path
            .clone()
            .map(|path| resolve_under_root(root_path, path))
            .unwrap_or_else(|| default_mcp_server_fragment_path(root_path, &normalized_name));
        plan_entries.push(ImportPlanEntry {
            name: name.clone(),
            normalized_name,
            target_path,
            value,
        });
    }
    if plan_entries.is_empty() {
        return Err(format!(
            "MCP settings import source '{}' contains no usable servers",
            source_path.display()
        ));
    }

    let mut target_values = BTreeMap::<PathBuf, JsonValue>::new();
    for plan in &plan_entries {
        if !target_values.contains_key(&plan.target_path) {
            target_values.insert(
                plan.target_path.clone(),
                read_or_new_settings(&plan.target_path)?,
            );
        }
    }

    let mut conflicts = Vec::new();
    let mut result_entries = Vec::new();
    for plan in &plan_entries {
        let target_value = target_values
            .get(&plan.target_path)
            .ok_or_else(|| format!("missing import target '{}'", plan.target_path.display()))?;
        let existed_before = has_normalized_server(target_value, &plan.normalized_name)?;
        if existed_before && !options.force {
            conflicts.push(format!("{} in {}", plan.name, plan.target_path.display()));
        }
        let action = if existed_before { "replace" } else { "add" };
        result_entries.push(McpServerImportEntry {
            name: plan.name.clone(),
            normalized_name: plan.normalized_name.clone(),
            path: plan.target_path.display().to_string(),
            action: if options.dry_run {
                format!("dry-run-{}", action)
            } else {
                action.to_string()
            },
            dry_run: options.dry_run,
            existed_before,
        });
    }
    if !conflicts.is_empty() {
        return Err(format!(
            "MCP settings import would replace existing server entries: {}; rerun with --force to replace them",
            conflicts.join(", ")
        ));
    }

    if !options.dry_run {
        for plan in &plan_entries {
            let target_value = target_values
                .get_mut(&plan.target_path)
                .ok_or_else(|| format!("missing import target '{}'", plan.target_path.display()))?;
            insert_server_value(
                target_value,
                &plan.name,
                plan.value.clone(),
                &plan.target_path,
            )?;
        }
        for (target_path, target_value) in &target_values {
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    format!(
                        "failed to create MCP settings directory '{}': {}",
                        parent.display(),
                        error
                    )
                })?;
            }
            let mut serialized = target_value.to_pretty_string();
            serialized.push('\n');
            runtimepaths::write_text_atomic(target_path, &serialized).map_err(|error| {
                format!(
                    "failed to write MCP settings source '{}': {}",
                    target_path.display(),
                    error
                )
            })?;
        }
    }

    let mut target_paths = result_entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    target_paths.sort();
    target_paths.dedup();
    warnings.sort();
    warnings.dedup();
    Ok(McpServerImportResult {
        source_path: source_path.display().to_string(),
        dry_run: options.dry_run,
        force: options.force,
        imported_count: result_entries.len(),
        target_file_count: target_paths.len(),
        entries: result_entries,
        warnings,
    })
}

fn normalize_import_server_value(
    root_path: &Path,
    name: &str,
    server_value: &JsonValue,
    source_path: &Path,
    warnings: &mut Vec<String>,
) -> Option<JsonValue> {
    let JsonValue::Object(object) = server_value else {
        warnings.push(format!(
            "MCP settings import source '{}' has non-object server '{}'; skipping",
            source_path.display(),
            name
        ));
        return None;
    };
    if looks_like_mcpace_self_entry(root_path, name, object) {
        warnings.push(format!(
            "MCP settings import source '{}' contains MCPace's own client entry '{}'; skipping to avoid a self-loop",
            source_path.display(),
            name
        ));
        return None;
    }

    let command = trimmed_string_field(object, "command").unwrap_or("");
    let url = first_server_url_field(object).unwrap_or("");
    if command.is_empty() && url.is_empty() {
        warnings.push(format!(
            "MCP settings import source '{}' server '{}' has neither command nor url/serverUrl/httpUrl/endpoint; skipping",
            source_path.display(),
            name
        ));
        return None;
    }

    let mut normalized = object.clone();
    if !url.is_empty() {
        let existing_url = trimmed_string_field(&normalized, "url").unwrap_or("");
        if existing_url.is_empty() {
            normalized.insert("url".to_string(), JsonValue::string(url));
        }
    }
    if !normalized.contains_key("enabled") {
        let enabled = !normalized
            .get("disabled")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false);
        normalized.insert("enabled".to_string(), JsonValue::bool(enabled));
    }

    let raw_type = normalized
        .get("type")
        .or_else(|| normalized.get("transport"))
        .and_then(JsonValue::as_str)
        .unwrap_or("");
    let inferred_type = infer_import_server_type(raw_type, !command.is_empty(), !url.is_empty());
    normalized.insert("type".to_string(), JsonValue::string(inferred_type));

    Some(JsonValue::Object(normalized))
}

fn trimmed_string_field<'a>(object: &'a BTreeMap<String, JsonValue>, key: &str) -> Option<&'a str> {
    object
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn first_server_url_field(object: &BTreeMap<String, JsonValue>) -> Option<&str> {
    ["url", "serverUrl", "httpUrl", "endpoint"]
        .iter()
        .find_map(|key| trimmed_string_field(object, key))
}

fn infer_import_server_type(raw_type: &str, has_command: bool, has_url: bool) -> String {
    crate::source_type::infer_public_source_type(
        raw_type,
        if has_command { "command" } else { "" },
        if has_url {
            "https://example.invalid/mcp"
        } else {
            ""
        },
    )
}

fn looks_like_mcpace_self_entry(
    root_path: &Path,
    name: &str,
    object: &BTreeMap<String, JsonValue>,
) -> bool {
    let normalized_name = normalize_server_name(name);
    if normalized_name == "mcpace" || normalized_name == "mcp-pace" {
        return true;
    }

    let configured_url = runtimepaths::configured_mcp_url(root_path).to_ascii_lowercase();
    let url = object
        .get("url")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !url.is_empty()
        && (url == configured_url
            || url.starts_with("http://127.0.0.1:39022/mcp")
            || url.starts_with("http://localhost:39022/mcp"))
    {
        return true;
    }

    let command = object
        .get("command")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim_end_matches(".exe")
        .to_ascii_lowercase();
    if command != "mcpace" {
        return false;
    }
    json_helpers::strings_from_array(object.get("args").and_then(JsonValue::as_array))
        .into_iter()
        .any(|arg| arg == "mcp-server" || arg == "stdio-shim")
}

fn read_or_new_settings(path: &Path) -> Result<JsonValue, String> {
    if path.is_file() {
        return json_helpers::read_json_file(path);
    }
    Ok(JsonValue::object([(
        "mcpServers",
        JsonValue::Object(BTreeMap::new()),
    )]))
}

fn has_normalized_server(value: &JsonValue, normalized_name: &str) -> Result<bool, String> {
    let Some(servers) = json_helpers::object_at_path(value, &["mcpServers"]) else {
        return match value {
            JsonValue::Object(_) => Ok(false),
            _ => Err("MCP settings target must contain a JSON object".to_string()),
        };
    };
    Ok(servers
        .keys()
        .any(|key| normalize_server_name(key) == normalized_name))
}

fn insert_server_value(
    value: &mut JsonValue,
    name: &str,
    server_value: JsonValue,
    target_path: &Path,
) -> Result<(), String> {
    let JsonValue::Object(root_object) = value else {
        return Err(format!(
            "MCP settings target '{}' must contain a JSON object",
            target_path.display()
        ));
    };
    if !root_object.contains_key("mcpServers") {
        root_object.insert("mcpServers".to_string(), JsonValue::Object(BTreeMap::new()));
    }
    let Some(JsonValue::Object(servers)) = root_object.get_mut("mcpServers") else {
        return Err(format!(
            "MCP settings target '{}' has non-object mcpServers",
            target_path.display()
        ));
    };
    let normalized_name = normalize_server_name(name);
    let key_to_replace = servers
        .keys()
        .find(|key| normalize_server_name(key) == normalized_name)
        .cloned();
    if let Some(key) = key_to_replace {
        servers.remove(&key);
    }
    servers.insert(name.to_string(), server_value);
    Ok(())
}

impl McpServerImportResult {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("sourcePath", JsonValue::string(self.source_path.clone())),
            ("dryRun", JsonValue::bool(self.dry_run)),
            ("force", JsonValue::bool(self.force)),
            ("importedCount", JsonValue::number(self.imported_count)),
            ("targetFileCount", JsonValue::number(self.target_file_count)),
            (
                "entries",
                JsonValue::array(self.entries.iter().map(McpServerImportEntry::to_json_value)),
            ),
            (
                "warnings",
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

impl McpServerImportEntry {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("name", JsonValue::string(self.name.clone())),
            (
                "normalizedName",
                JsonValue::string(self.normalized_name.clone()),
            ),
            ("path", JsonValue::string(self.path.clone())),
            ("action", JsonValue::string(self.action.clone())),
            ("dryRun", JsonValue::bool(self.dry_run)),
            ("existedBefore", JsonValue::bool(self.existed_before)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_millis();
        let path = std::env::temp_dir().join(format!(
            "mcpace-import-test-{}-{}-{}",
            label,
            std::process::id(),
            millis
        ));
        fs::create_dir_all(&path).expect("create temp root");
        path
    }

    #[test]
    fn imports_servers_shape_with_url_alias_disabled_and_inferred_type() {
        let root = temp_root("remote-alias");
        let source = root.join("source.json");
        let target = root.join("imported.json");
        fs::write(
            &source,
            r#"{
  "servers": {
    "Remote API": {
      "serverUrl": "https://example.com/mcp",
      "disabled": true
    }
  }
}"#,
        )
        .expect("write source");

        let result = import_mcp_server_entries(
            &root,
            McpServerImportOptions {
                source_path: source,
                settings_path: Some(target.clone()),
                dry_run: false,
                force: false,
            },
        )
        .expect("import servers shape");

        assert_eq!(result.imported_count, 1);
        let written = json_helpers::read_json_file(&target).expect("read import target");
        let servers = json_helpers::object_at_path(&written, &["mcpServers"]).expect("mcpServers");
        let remote = servers.get("Remote API").expect("Remote API");
        assert_eq!(
            remote.get("url").and_then(JsonValue::as_str),
            Some("https://example.com/mcp")
        );
        assert_eq!(
            remote.get("type").and_then(JsonValue::as_str),
            Some("streamable-http")
        );
        assert_eq!(
            remote.get("enabled").and_then(JsonValue::as_bool),
            Some(false)
        );
    }

    #[test]
    fn skips_mcpace_self_entry_during_import() {
        let root = temp_root("self-entry");
        let source = root.join("source.json");
        fs::write(
            &source,
            r#"{
  "mcpServers": {
    "mcp pace": {
      "url": "http://127.0.0.1:39022/mcp"
    }
  }
}"#,
        )
        .expect("write source");

        let error = import_mcp_server_entries(
            &root,
            McpServerImportOptions {
                source_path: source,
                settings_path: Some(root.join("imported.json")),
                dry_run: false,
                force: false,
            },
        )
        .expect_err("self entry should leave no usable servers");
        assert!(
            error.contains("no usable servers"),
            "unexpected error: {error}"
        );
    }
}
