use crate::json::JsonValue;
use crate::json_helpers;
use crate::mcp_sources::{self, McpServerWriteOptions, McpServerWriteResult};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};

const DEFAULT_PRESET_CATALOG_PATH: &str = "presets/mcp-servers.json";
const ENV_MCP_PRESETS: &str = "MCPACE_MCP_PRESETS";

#[derive(Clone, Debug, Default)]
pub struct McpPresetCatalog {
    pub version: String,
    pub description: String,
    pub sources: Vec<String>,
    pub warnings: Vec<String>,
    pub presets: Vec<McpPreset>,
    pub starter_name: String,
    pub starter_description: String,
    pub starter_presets: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct McpPreset {
    pub id: String,
    pub default_name: String,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub command: String,
    pub args: Vec<String>,
    pub path_mode: String,
    pub trust_level: String,
    pub source: String,
    pub notes: Vec<String>,
    pub policy: Option<JsonValue>,
    pub review: Option<JsonValue>,
}

#[derive(Clone, Debug, Default)]
pub struct McpPresetInstallOptions {
    pub preset_id: String,
    pub name_override: Option<String>,
    pub paths: Vec<String>,
    pub extra_args: Vec<String>,
    pub env: Vec<String>,
    pub settings_path: Option<PathBuf>,
    pub dry_run: bool,
    pub force: bool,
}

#[derive(Clone, Debug)]
pub struct McpPresetInstallResult {
    pub preset: McpPreset,
    pub paths: Vec<String>,
    pub write: McpServerWriteResult,
}

#[derive(Clone, Debug)]
pub struct McpPresetStarterResult {
    pub name: String,
    pub description: String,
    pub dry_run: bool,
    pub installed: Vec<McpPresetInstallResult>,
}

#[allow(dead_code)]
pub fn preset_catalog_paths(root_path: &Path) -> Vec<PathBuf> {
    let mut warnings = Vec::new();
    collect_preset_catalog_paths(root_path, &mut warnings)
}

pub fn load_preset_catalog(root_path: &Path) -> Result<McpPresetCatalog, String> {
    let mut warnings = Vec::new();
    let paths = collect_preset_catalog_paths(root_path, &mut warnings);
    let mut sources = Vec::new();
    let mut presets_by_id = BTreeMap::new();
    let mut version = String::new();
    let mut descriptions = Vec::new();
    let mut starter_name = "default".to_string();
    let mut starter_description = String::new();
    let mut starter_presets = Vec::new();

    for path in paths {
        if !path.is_file() {
            warnings.push(format!(
                "MCP preset catalog '{}' does not exist; skipping",
                path.display()
            ));
            continue;
        }
        let value = json_helpers::read_json_file(&path).map_err(|error| {
            format!(
                "failed to load MCP preset catalog '{}': {}",
                path.display(),
                error
            )
        })?;
        let parsed = parse_preset_catalog(&value)?;
        if !version.is_empty() && version != parsed.version {
            warnings.push(format!(
                "MCP preset catalog '{}' uses version '{}' after version '{}'",
                path.display(),
                parsed.version,
                version
            ));
        }
        if version.is_empty() {
            version = parsed.version.clone();
        }
        if !parsed.description.is_empty() {
            descriptions.push(parsed.description.clone());
        }
        if !parsed.starter_presets.is_empty() {
            starter_name = parsed.starter_name.clone();
            starter_description = parsed.starter_description.clone();
            starter_presets = parsed.starter_presets.clone();
        }
        for preset in parsed.presets {
            if presets_by_id.insert(preset.id.clone(), preset).is_some() {
                warnings.push(format!(
                    "duplicate MCP preset id in '{}' overrides an earlier preset",
                    path.display()
                ));
            }
        }
        sources.push(path.display().to_string());
    }

    if sources.is_empty() {
        return Err(format!(
            "no MCP preset catalogs found; expected {} or configure mcpace.config.json mcpPresets.includePaths",
            DEFAULT_PRESET_CATALOG_PATH
        ));
    }
    if version.is_empty() {
        version = "1".to_string();
    }
    warnings.sort();
    warnings.dedup();
    sources.sort();
    sources.dedup();

    Ok(McpPresetCatalog {
        version,
        description: descriptions.join(" | "),
        sources,
        warnings,
        presets: presets_by_id.into_values().collect(),
        starter_name,
        starter_description,
        starter_presets,
    })
}

pub fn install_preset(
    root_path: &Path,
    options: McpPresetInstallOptions,
) -> Result<McpPresetInstallResult, String> {
    let catalog = load_preset_catalog(root_path)?;
    let preset_id = normalize_preset_id(&options.preset_id);
    if preset_id.is_empty() {
        return Err("server install requires a preset id; run 'mcpace server presets'".to_string());
    }
    let Some(preset) = catalog
        .presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .cloned()
    else {
        return Err(format!(
            "unknown MCP preset '{}'; run 'mcpace server presets' to see available presets",
            options.preset_id
        ));
    };

    let paths = materialize_preset_paths(root_path, &preset, &options.paths)?;
    let mut args = preset.args.clone();
    match preset.path_mode.as_str() {
        "none" => {}
        "append" => args.extend(paths.iter().cloned()),
        "repository-flag" => {
            let Some(repository) = paths.first() else {
                return Err(format!(
                    "MCP preset '{}' requires a repository path",
                    preset.id
                ));
            };
            args.push("--repository".to_string());
            args.push(repository.clone());
        }
        other => {
            return Err(format!(
            "MCP preset '{}' has unsupported pathMode '{}'; use none, append, or repository-flag",
            preset.id, other
        ))
        }
    }
    args.extend(options.extra_args.iter().cloned());
    let name = options
        .name_override
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&preset.default_name)
        .to_string();

    let write = mcp_sources::write_mcp_server_entry(
        root_path,
        McpServerWriteOptions {
            name,
            server_type: Some(preset.kind.clone()),
            command: Some(preset.command.clone()),
            args,
            url: None,
            env: options.env,
            headers: Vec::new(),
            settings_path: options.settings_path,
            enabled: true,
            dry_run: options.dry_run,
            force: options.force,
        },
    )?;
    write_preset_policy_overlay(root_path, &preset, &write, options.dry_run, options.force)?;

    Ok(McpPresetInstallResult {
        preset,
        paths,
        write,
    })
}

pub fn install_starter(
    root_path: &Path,
    paths: &[String],
    settings_path: Option<PathBuf>,
    dry_run: bool,
    force: bool,
) -> Result<McpPresetStarterResult, String> {
    let catalog = load_preset_catalog(root_path)?;
    if catalog.starter_presets.is_empty() {
        return Err("MCP preset catalog does not define a starter preset list".to_string());
    }

    let mut installed = Vec::new();
    for preset_id in &catalog.starter_presets {
        installed.push(install_preset(
            root_path,
            McpPresetInstallOptions {
                preset_id: preset_id.clone(),
                name_override: None,
                paths: paths.to_vec(),
                extra_args: Vec::new(),
                env: Vec::new(),
                settings_path: settings_path.clone(),
                dry_run,
                force,
            },
        )?);
    }

    Ok(McpPresetStarterResult {
        name: catalog.starter_name,
        description: catalog.starter_description,
        dry_run,
        installed,
    })
}

fn write_preset_policy_overlay(
    root_path: &Path,
    preset: &McpPreset,
    write: &McpServerWriteResult,
    dry_run: bool,
    force: bool,
) -> Result<(), String> {
    let Some(policy) = preset.policy.clone() else {
        return Ok(());
    };
    let JsonValue::Object(_) = &policy else {
        return Err(format!(
            "MCP preset '{}' policy must be a JSON object",
            preset.id
        ));
    };
    if dry_run {
        return Ok(());
    }

    let config_path = root_path.join("mcpace.config.json");
    let mut config = if config_path.is_file() {
        json_helpers::read_json_file(&config_path)?
    } else {
        JsonValue::object([(String::from("servers"), JsonValue::Object(BTreeMap::new()))])
    };
    let JsonValue::Object(root_object) = &mut config else {
        return Err(format!(
            "MCPace config '{}' must contain a JSON object",
            config_path.display()
        ));
    };
    if !root_object.contains_key("servers") {
        root_object.insert("servers".to_string(), JsonValue::Object(BTreeMap::new()));
    }
    let Some(JsonValue::Object(servers)) = root_object.get_mut("servers") else {
        return Err(format!(
            "MCPace config '{}' has non-object servers",
            config_path.display()
        ));
    };

    if servers.contains_key(&write.name) && !force {
        return Err(format!(
            "server '{}' already has a policy overlay in {}; rerun with --force to replace it",
            write.name,
            config_path.display()
        ));
    }

    let entry = JsonValue::object([
        ("kind", JsonValue::string(format!("preset-{}", preset.id))),
        ("defaultEnabled", JsonValue::bool(true)),
        (
            "transportPreference",
            JsonValue::string(preset.kind.clone()),
        ),
        (
            "supportedTransports",
            JsonValue::array(vec![JsonValue::string(preset.kind.clone())]),
        ),
        (
            "requiredCommands",
            JsonValue::array(vec![JsonValue::string(preset.command.clone())]),
        ),
        ("policy", policy),
        ("review", preset.review.clone().unwrap_or(JsonValue::Null)),
        (
            "installer",
            JsonValue::object([
                ("installTarget", JsonValue::string("mcp_settings")),
                ("installMethod", JsonValue::string("preset")),
                ("installPackage", JsonValue::string(preset.id.clone())),
                (
                    "verifyCommand",
                    JsonValue::string(format!(
                        "mcpace server test {} --refresh",
                        write.normalized_name
                    )),
                ),
            ]),
        ),
    ]);
    servers.insert(write.name.clone(), entry);

    let mut serialized = config.to_pretty_string();
    serialized.push('\n');
    crate::runtimepaths::write_text_atomic(&config_path, &serialized)
}

fn collect_preset_catalog_paths(root_path: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = BTreeSet::new();
    let config_path = root_path.join("mcpace.config.json");

    if config_path.is_file() {
        match json_helpers::read_json_file(&config_path) {
            Ok(value) => {
                for raw in json_helpers::strings_from_array(json_helpers::array_at_path(
                    &value,
                    &["mcpPresets", "includePaths"],
                )) {
                    push_unique_path(&mut paths, &mut seen, resolve_under_root(root_path, raw));
                }
            }
            Err(error) => warnings.push(format!(
                "failed to read MCP preset catalog config '{}': {}",
                config_path.display(),
                error
            )),
        }
    }

    if paths.is_empty() {
        push_unique_path(
            &mut paths,
            &mut seen,
            root_path.join(DEFAULT_PRESET_CATALOG_PATH),
        );
    }

    if let Ok(raw_paths) = env::var(ENV_MCP_PRESETS) {
        for path in env::split_paths(&raw_paths) {
            push_unique_path(&mut paths, &mut seen, resolve_under_root(root_path, path));
        }
    }

    paths
}

fn push_unique_path(paths: &mut Vec<PathBuf>, seen: &mut BTreeSet<PathBuf>, path: PathBuf) {
    if seen.insert(path.clone()) {
        paths.push(path);
    }
}

fn resolve_under_root<P: Into<PathBuf>>(root_path: &Path, path: P) -> PathBuf {
    let path = path.into();
    if path.is_absolute() {
        path
    } else {
        root_path.join(path)
    }
}

fn parse_preset_catalog(value: &JsonValue) -> Result<McpPresetCatalog, String> {
    let version = string_field(value, "version").unwrap_or_else(|| "1".to_string());
    let description = string_field(value, "description").unwrap_or_default();
    let presets_value = value
        .get("presets")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| "MCP preset catalog requires a presets array".to_string())?;

    let mut presets = Vec::new();
    for item in presets_value {
        presets.push(parse_preset(item)?);
    }

    let starter = value.get("starter");
    let starter_name = starter
        .and_then(|value| string_field(value, "name"))
        .unwrap_or_else(|| "default".to_string());
    let starter_description = starter
        .and_then(|value| string_field(value, "description"))
        .unwrap_or_default();
    let starter_presets = starter
        .and_then(|value| value.get("presets"))
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(normalize_preset_id)
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(McpPresetCatalog {
        version,
        description,
        sources: Vec::new(),
        warnings: Vec::new(),
        presets,
        starter_name,
        starter_description,
        starter_presets,
    })
}

fn parse_preset(value: &JsonValue) -> Result<McpPreset, String> {
    let id = normalize_preset_id(
        &string_field(value, "id").ok_or_else(|| "MCP preset requires id".to_string())?,
    );
    if id.is_empty() {
        return Err("MCP preset id must contain a usable ASCII token".to_string());
    }
    let default_name = string_field(value, "defaultName").unwrap_or_else(|| id.clone());
    let title = string_field(value, "title").unwrap_or_else(|| default_name.clone());
    let description = string_field(value, "description").unwrap_or_default();
    let kind = string_field(value, "kind").unwrap_or_else(|| "stdio".to_string());
    if kind != "stdio" {
        return Err(format!(
            "MCP preset '{}' uses unsupported kind '{}'; only stdio presets install natively today",
            id, kind
        ));
    }
    let command = string_field(value, "command")
        .filter(|command| !command.trim().is_empty())
        .ok_or_else(|| format!("MCP preset '{}' requires a command", id))?;
    let args = value
        .get("args")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let path_mode = string_field(value, "pathMode").unwrap_or_else(|| "none".to_string());
    if !matches!(path_mode.as_str(), "none" | "append" | "repository-flag") {
        return Err(format!(
            "MCP preset '{}' has unsupported pathMode '{}'; use none, append, or repository-flag",
            id, path_mode
        ));
    }
    let trust_level =
        string_field(value, "trustLevel").unwrap_or_else(|| "review-required".to_string());
    let source = string_field(value, "source").unwrap_or_default();
    let notes = value
        .get("notes")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(McpPreset {
        id,
        default_name,
        title,
        description,
        kind,
        command,
        args,
        path_mode,
        trust_level,
        source,
        notes,
        policy: value.get("policy").cloned(),
        review: value.get("review").cloned(),
    })
}

fn materialize_preset_paths(
    root_path: &Path,
    preset: &McpPreset,
    raw_paths: &[String],
) -> Result<Vec<String>, String> {
    match preset.path_mode.as_str() {
        "none" => Ok(Vec::new()),
        "append" => materialize_any_paths(root_path, raw_paths),
        "repository-flag" => {
            let mut paths = materialize_any_paths(root_path, raw_paths)?;
            if paths.len() > 1 {
                return Err(format!(
                    "MCP preset '{}' accepts only one --path repository value",
                    preset.id
                ));
            }
            if paths.is_empty() {
                paths.push(canonical_or_original(root_path));
            }
            Ok(paths)
        }
        other => Err(format!(
            "MCP preset '{}' has unsupported pathMode '{}'; use none, append, or repository-flag",
            preset.id, other
        )),
    }
}

fn materialize_any_paths(root_path: &Path, raw_paths: &[String]) -> Result<Vec<String>, String> {
    if raw_paths.is_empty() {
        return Ok(vec![canonical_or_original(root_path)]);
    }
    raw_paths
        .iter()
        .map(|path| normalize_user_path(root_path, path))
        .collect::<Result<Vec<_>, _>>()
}

fn normalize_user_path(root_path: &Path, raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("server install --path requires a non-empty path".to_string());
    }
    if trimmed.contains('\0') || trimmed.contains('\r') || trimmed.contains('\n') {
        return Err("server install --path cannot contain NUL or newlines".to_string());
    }
    let path = PathBuf::from(trimmed);
    let absolute = if path.is_absolute() {
        path
    } else {
        root_path.join(path)
    };
    Ok(canonical_or_original(&absolute))
}

fn canonical_or_original(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn normalize_preset_id(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn string_field(value: &JsonValue, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl McpPresetCatalog {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("schema", JsonValue::string("mcpace.mcpPresetCatalog.v1")),
            ("version", JsonValue::string(self.version.clone())),
            ("description", JsonValue::string(self.description.clone())),
            (
                "sources",
                JsonValue::array(self.sources.iter().cloned().map(JsonValue::string)),
            ),
            (
                "warnings",
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            ),
            (
                "starter",
                JsonValue::object([
                    ("name", JsonValue::string(self.starter_name.clone())),
                    (
                        "description",
                        JsonValue::string(self.starter_description.clone()),
                    ),
                    (
                        "presets",
                        JsonValue::array(
                            self.starter_presets.iter().cloned().map(JsonValue::string),
                        ),
                    ),
                ]),
            ),
            (
                "presets",
                JsonValue::array(self.presets.iter().map(McpPreset::to_json_value)),
            ),
        ])
    }
}

impl McpPreset {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("id", JsonValue::string(self.id.clone())),
            ("defaultName", JsonValue::string(self.default_name.clone())),
            ("title", JsonValue::string(self.title.clone())),
            ("description", JsonValue::string(self.description.clone())),
            ("kind", JsonValue::string(self.kind.clone())),
            ("command", JsonValue::string(self.command.clone())),
            (
                "args",
                JsonValue::array(self.args.iter().cloned().map(JsonValue::string)),
            ),
            ("pathMode", JsonValue::string(self.path_mode.clone())),
            ("trustLevel", JsonValue::string(self.trust_level.clone())),
            ("source", JsonValue::string(self.source.clone())),
            (
                "notes",
                JsonValue::array(self.notes.iter().cloned().map(JsonValue::string)),
            ),
            ("policy", self.policy.clone().unwrap_or(JsonValue::Null)),
            ("review", self.review.clone().unwrap_or(JsonValue::Null)),
        ])
    }
}

impl McpPresetInstallResult {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("schema", JsonValue::string("mcpace.mcpPresetInstall.v1")),
            ("preset", self.preset.to_json_value()),
            (
                "paths",
                JsonValue::array(self.paths.iter().cloned().map(JsonValue::string)),
            ),
            ("write", self.write.to_json_value()),
        ])
    }
}

impl McpPresetStarterResult {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("schema", JsonValue::string("mcpace.mcpStarterInstall.v1")),
            ("name", JsonValue::string(self.name.clone())),
            ("description", JsonValue::string(self.description.clone())),
            ("dryRun", JsonValue::bool(self.dry_run)),
            (
                "installed",
                JsonValue::array(
                    self.installed
                        .iter()
                        .map(McpPresetInstallResult::to_json_value),
                ),
            ),
        ])
    }
}
