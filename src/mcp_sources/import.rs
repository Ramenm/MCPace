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
    let Some(source_servers) = json_helpers::object_at_path(&source_value, &["mcpServers"]) else {
        return Err(format!(
            "MCP settings import source '{}' must contain an mcpServers object",
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
        let target_path = options
            .settings_path
            .clone()
            .map(|path| resolve_under_root(root_path, path))
            .unwrap_or_else(|| default_mcp_server_fragment_path(root_path, &normalized_name));
        plan_entries.push(ImportPlanEntry {
            name: name.clone(),
            normalized_name,
            target_path,
            value: server_value.clone(),
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
