use super::paths::resolve_under_root;
use super::write_helpers::{
    build_server_entry, normalize_server_type, parse_key_value_pairs, validate_env_name,
    validate_http_header_name, validate_remote_mcp_url,
};
use super::{load_mcp_server_registry, normalize_server_name, DEFAULT_SETTINGS_DIR};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::runtimepaths;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct McpServerWriteOptions {
    pub name: String,
    pub server_type: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: Vec<String>,
    pub headers: Vec<String>,
    pub settings_path: Option<PathBuf>,
    pub enabled: bool,
    pub dry_run: bool,
    pub force: bool,
}

#[derive(Clone, Debug)]
pub struct McpServerWriteResult {
    pub name: String,
    pub normalized_name: String,
    pub path: String,
    pub action: String,
    pub dry_run: bool,
    pub existed_before: bool,
    pub server_type: String,
    pub command: Option<String>,
    pub url: Option<String>,
    pub args_count: usize,
    pub env_count: usize,
    pub header_count: usize,
}

#[derive(Clone, Debug, Default)]
pub struct McpServerRemoveOptions {
    pub name: String,
    pub settings_path: Option<PathBuf>,
    pub dry_run: bool,
}

#[derive(Clone, Debug)]
pub struct McpServerRemoveResult {
    pub name: String,
    pub normalized_name: String,
    pub path: String,
    pub action: String,
    pub dry_run: bool,
    pub existed_before: bool,
    pub remaining_server_count: usize,
}

#[derive(Clone, Debug, Default)]
pub struct McpServerToggleOptions {
    pub name: String,
    pub settings_path: Option<PathBuf>,
    pub enabled: bool,
    pub dry_run: bool,
}

#[derive(Clone, Debug)]
pub struct McpServerToggleResult {
    pub name: String,
    pub normalized_name: String,
    pub path: String,
    pub action: String,
    pub dry_run: bool,
    pub existed_before: bool,
    pub enabled: bool,
    pub previous_enabled: Option<bool>,
    pub server_count: usize,
}

pub fn default_mcp_server_fragment_path(root_path: &Path, server_name: &str) -> PathBuf {
    let normalized = normalize_server_name(server_name);
    root_path
        .join(DEFAULT_SETTINGS_DIR)
        .join(format!("{}.json", normalized))
}

pub fn write_mcp_server_entry(
    root_path: &Path,
    options: McpServerWriteOptions,
) -> Result<McpServerWriteResult, String> {
    let display_name = options.name.trim().to_string();
    if display_name.is_empty() {
        return Err("server add requires a non-empty server name".to_string());
    }
    let normalized_name = normalize_server_name(&display_name);
    if normalized_name.is_empty() {
        return Err(format!(
            "server name '{}' does not contain a usable ASCII token",
            display_name
        ));
    }

    let has_command = options
        .command
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let has_url = options
        .url
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if has_command && has_url {
        return Err("server add accepts either --command or --url, not both".to_string());
    }
    if !has_command && !has_url {
        return Err(
            "server add requires --command <cmd> for stdio or --url <url> for HTTP".to_string(),
        );
    }

    let server_type = normalize_server_type(options.server_type.as_deref(), has_command, has_url)?;
    if has_url {
        validate_remote_mcp_url(options.url.as_deref().unwrap_or(""))?;
    }
    if server_type == "stdio" && !has_command {
        return Err("server type stdio requires --command <cmd>".to_string());
    }
    if server_type != "stdio" && !has_url {
        return Err(format!("server type {} requires --url <url>", server_type));
    }

    let env = parse_key_value_pairs(&options.env, "--env", validate_env_name)?;
    let headers = parse_key_value_pairs(&options.headers, "--header", validate_http_header_name)?;
    let target_path = options
        .settings_path
        .clone()
        .map(|path| resolve_under_root(root_path, path))
        .unwrap_or_else(|| default_mcp_server_fragment_path(root_path, &normalized_name));

    let mut root_value = if target_path.is_file() {
        json_helpers::read_json_file(&target_path)?
    } else {
        JsonValue::object([(
            String::from("mcpServers"),
            JsonValue::Object(BTreeMap::new()),
        )])
    };
    let JsonValue::Object(root_object) = &mut root_value else {
        return Err(format!(
            "MCP settings source '{}' must contain a JSON object",
            target_path.display()
        ));
    };
    if !root_object.contains_key("mcpServers") {
        root_object.insert("mcpServers".to_string(), JsonValue::Object(BTreeMap::new()));
    }
    let Some(JsonValue::Object(servers)) = root_object.get_mut("mcpServers") else {
        return Err(format!(
            "MCP settings source '{}' has non-object mcpServers",
            target_path.display()
        ));
    };

    let existed_before = servers
        .keys()
        .any(|key| normalize_server_name(key) == normalized_name);
    if existed_before && !options.force {
        return Err(format!(
            "server '{}' already exists in {}; rerun with --force to replace it",
            display_name,
            target_path.display()
        ));
    }

    let entry = build_server_entry(&server_type, &options, &env, &headers);
    let action = if existed_before { "replace" } else { "add" }.to_string();
    let result = McpServerWriteResult {
        name: display_name.clone(),
        normalized_name: normalized_name.clone(),
        path: target_path.display().to_string(),
        action: if options.dry_run {
            format!("dry-run-{}", action)
        } else {
            action
        },
        dry_run: options.dry_run,
        existed_before,
        server_type,
        command: options.command.clone(),
        url: options.url.clone(),
        args_count: options.args.len(),
        env_count: env.len(),
        header_count: headers.len(),
    };

    if options.dry_run {
        return Ok(result);
    }

    servers.insert(display_name, entry);
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create MCP settings directory '{}': {}",
                parent.display(),
                error
            )
        })?;
    }
    let mut serialized = root_value.to_pretty_string();
    serialized.push('\n');
    runtimepaths::write_text_atomic(&target_path, &serialized)?;
    Ok(result)
}

pub fn remove_mcp_server_entry(
    root_path: &Path,
    options: McpServerRemoveOptions,
) -> Result<McpServerRemoveResult, String> {
    let display_name = options.name.trim().to_string();
    if display_name.is_empty() {
        return Err("server remove requires a non-empty server name".to_string());
    }
    let normalized_name = normalize_server_name(&display_name);
    if normalized_name.is_empty() {
        return Err(format!(
            "server name '{}' does not contain a usable ASCII token",
            display_name
        ));
    }

    let target_path = match options.settings_path.as_ref() {
        Some(path) => resolve_under_root(root_path, path),
        None => find_source_path_for_server(root_path, &normalized_name)?.ok_or_else(|| {
            format!(
                "MCP server '{}' was not found in known sources; run 'mcpace server sources' or pass --settings <path>",
                display_name
            )
        })?,
    };

    if !target_path.is_file() {
        return Err(format!(
            "MCP settings source '{}' does not exist",
            target_path.display()
        ));
    }

    let mut root_value = json_helpers::read_json_file(&target_path)?;
    let mut existed_before = false;
    let remaining_server_count;
    match &mut root_value {
        JsonValue::Object(root_object) => match root_object.get_mut("mcpServers") {
            Some(JsonValue::Object(servers)) => {
                let key_to_remove = servers
                    .keys()
                    .find(|key| normalize_server_name(key) == normalized_name)
                    .cloned();
                if let Some(key) = key_to_remove {
                    existed_before = true;
                    if !options.dry_run {
                        servers.remove(&key);
                    }
                }
                remaining_server_count = if options.dry_run {
                    servers
                        .keys()
                        .filter(|key| normalize_server_name(key) != normalized_name)
                        .count()
                } else {
                    servers.len()
                };
            }
            Some(_) => {
                return Err(format!(
                    "MCP settings source '{}' has a non-object mcpServers value",
                    target_path.display()
                ));
            }
            None => {
                return Err(format!(
                    "MCP settings source '{}' has no mcpServers object",
                    target_path.display()
                ));
            }
        },
        _ => {
            return Err(format!(
                "MCP settings source '{}' must be a JSON object",
                target_path.display()
            ));
        }
    }

    if !existed_before {
        return Err(format!(
            "MCP server '{}' was not found in '{}'",
            display_name,
            target_path.display()
        ));
    }

    if !options.dry_run {
        let mut serialized = root_value.to_pretty_string();
        serialized.push('\n');
        runtimepaths::write_text_atomic(&target_path, &serialized)?;
    }

    Ok(McpServerRemoveResult {
        name: display_name,
        normalized_name,
        path: target_path.display().to_string(),
        action: "remove".to_string(),
        dry_run: options.dry_run,
        existed_before,
        remaining_server_count,
    })
}

pub fn set_mcp_server_enabled(
    root_path: &Path,
    options: McpServerToggleOptions,
) -> Result<McpServerToggleResult, String> {
    let display_name = options.name.trim().to_string();
    if display_name.is_empty() {
        return Err("server enable/disable requires a non-empty server name".to_string());
    }
    let normalized_name = normalize_server_name(&display_name);
    if normalized_name.is_empty() {
        return Err(format!(
            "server name '{}' does not contain a usable ASCII token",
            display_name
        ));
    }

    let target_path = match options.settings_path.as_ref() {
        Some(path) => resolve_under_root(root_path, path),
        None => find_source_path_for_server(root_path, &normalized_name)?.ok_or_else(|| {
            format!(
                "MCP server '{}' was not found in known sources; run 'mcpace server sources' or pass --settings <path>",
                display_name
            )
        })?,
    };

    if !target_path.is_file() {
        return Err(format!(
            "MCP settings source '{}' does not exist",
            target_path.display()
        ));
    }

    let mut root_value = json_helpers::read_json_file(&target_path)?;
    let mut existed_before = false;
    let mut previous_enabled = None;
    let server_count;
    match &mut root_value {
        JsonValue::Object(root_object) => match root_object.get_mut("mcpServers") {
            Some(JsonValue::Object(servers)) => {
                server_count = servers.len();
                let key_to_update = servers
                    .keys()
                    .find(|key| normalize_server_name(key) == normalized_name)
                    .cloned();
                if let Some(key) = key_to_update {
                    existed_before = true;
                    let Some(JsonValue::Object(server_object)) = servers.get_mut(&key) else {
                        return Err(format!(
                            "MCP server '{}' in '{}' must be a JSON object to enable/disable it",
                            display_name,
                            target_path.display()
                        ));
                    };
                    previous_enabled = server_object.get("enabled").and_then(JsonValue::as_bool);
                    if !options.dry_run {
                        server_object
                            .insert("enabled".to_string(), JsonValue::bool(options.enabled));
                    }
                }
            }
            Some(_) => {
                return Err(format!(
                    "MCP settings source '{}' has a non-object mcpServers value",
                    target_path.display()
                ));
            }
            None => {
                return Err(format!(
                    "MCP settings source '{}' has no mcpServers object",
                    target_path.display()
                ));
            }
        },
        _ => {
            return Err(format!(
                "MCP settings source '{}' must be a JSON object",
                target_path.display()
            ));
        }
    }

    if !existed_before {
        return Err(format!(
            "MCP server '{}' was not found in '{}'",
            display_name,
            target_path.display()
        ));
    }

    if !options.dry_run {
        let mut serialized = root_value.to_pretty_string();
        serialized.push('\n');
        runtimepaths::write_text_atomic(&target_path, &serialized)?;
    }

    let action = if options.enabled { "enable" } else { "disable" }.to_string();
    Ok(McpServerToggleResult {
        name: display_name,
        normalized_name,
        path: target_path.display().to_string(),
        action,
        dry_run: options.dry_run,
        existed_before,
        enabled: options.enabled,
        previous_enabled,
        server_count,
    })
}

impl McpServerWriteResult {
    pub fn to_json_value(&self) -> JsonValue {
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
            ("type", JsonValue::string(self.server_type.clone())),
            (
                "command",
                self.command
                    .clone()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "url",
                self.url
                    .clone()
                    .map(JsonValue::string)
                    .unwrap_or(JsonValue::Null),
            ),
            ("argsCount", JsonValue::number(self.args_count)),
            ("envCount", JsonValue::number(self.env_count)),
            ("headerCount", JsonValue::number(self.header_count)),
            (
                "suggestedNextCommands",
                JsonValue::array(if self.server_type == "stdio" {
                    vec![
                        JsonValue::string(format!(
                            "mcpace server test {} --refresh --json",
                            self.normalized_name
                        )),
                        JsonValue::string("mcpace client install <client|all> --dry-run --diff"),
                    ]
                } else {
                    vec![
                        JsonValue::string("mcpace server sources --json"),
                        JsonValue::string("HTTP upstream forwarding is inventoried only until the remote connector is implemented"),
                    ]
                }),
            ),
        ])
    }
}

impl McpServerRemoveResult {
    pub fn to_json_value(&self) -> JsonValue {
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
            (
                "remainingServerCount",
                JsonValue::number(self.remaining_server_count),
            ),
        ])
    }
}

impl McpServerToggleResult {
    pub fn to_json_value(&self) -> JsonValue {
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
            ("enabled", JsonValue::bool(self.enabled)),
            (
                "previousEnabled",
                self.previous_enabled
                    .map(JsonValue::bool)
                    .unwrap_or(JsonValue::Null),
            ),
            ("serverCount", JsonValue::number(self.server_count)),
            (
                "suggestedNextCommands",
                JsonValue::array([
                    JsonValue::string(format!(
                        "mcpace server test {} --refresh --json",
                        self.normalized_name
                    )),
                    JsonValue::string("mcpace verify readiness --json"),
                ]),
            ),
        ])
    }
}

fn find_source_path_for_server(
    root_path: &Path,
    normalized_name: &str,
) -> Result<Option<PathBuf>, String> {
    let registry = load_mcp_server_registry(root_path)?;
    Ok(registry
        .servers
        .get(normalized_name)
        .map(|entry| PathBuf::from(&entry.source)))
}
