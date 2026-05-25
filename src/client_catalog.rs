use crate::json::JsonValue;
use crate::json_helpers;
use crate::text_utils;
use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientInstallKind {
    TomlMcpServersManagedTable,
    JsonMcpServers(JsonMcpServerShape),
    YamlMcpServersManagedSection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonMcpServerShape {
    pub url_field: &'static str,
    pub include_type_http: bool,
    pub include_tools_star: bool,
    pub include_disabled_false: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientInstallSupport {
    pub kind: ClientInstallKind,
    pub preferred_scope: &'static str,
    pub preferred_config_path: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct ClientTarget {
    pub id: &'static str,
    pub family_id: &'static str,
    pub display_name: &'static str,
    pub aliases: &'static [&'static str],
    pub maturity: &'static str,
    pub surface_class: &'static str,
    pub surface_kind: &'static str,
    pub proof_tier: &'static str,
    pub config_format: &'static str,
    pub config_paths: &'static [&'static str],
    pub config_precedence: &'static [&'static str],
    pub native_scopes: &'static [&'static str],
    pub supported_ingresses: &'static [&'static str],
    pub documented_features: &'static [&'static str],
    pub documented_constraints: &'static [&'static str],
    pub notes: &'static [&'static str],
    pub install_support: Option<ClientInstallSupport>,
}

mod builtin;

pub use self::builtin::CLIENT_TARGETS;

#[derive(Clone, Debug)]
pub struct ClientRegistry {
    pub targets: Vec<ClientTargetRecord>,
    pub sources: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientInstallKindRecord {
    TomlMcpServersManagedTable,
    JsonMcpServers(JsonMcpServerShapeRecord),
    YamlMcpServersManagedSection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonMcpServerShapeRecord {
    pub url_field: String,
    pub include_type_http: bool,
    pub include_tools_star: bool,
    pub include_disabled_false: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientInstallSupportRecord {
    pub kind: ClientInstallKindRecord,
    pub preferred_scope: String,
    pub preferred_config_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientTargetRecord {
    pub id: String,
    pub family_id: String,
    pub display_name: String,
    pub aliases: Vec<String>,
    pub maturity: String,
    pub surface_class: String,
    pub surface_kind: String,
    pub proof_tier: String,
    pub config_format: String,
    pub config_paths: Vec<String>,
    pub config_precedence: Vec<String>,
    pub native_scopes: Vec<String>,
    pub supported_ingresses: Vec<String>,
    pub documented_features: Vec<String>,
    pub documented_constraints: Vec<String>,
    pub notes: Vec<String>,
    pub install_support: Option<ClientInstallSupportRecord>,
    pub source: String,
}

pub fn load_registry(root_path: Option<&Path>) -> Result<ClientRegistry, String> {
    let mut targets = BTreeMap::<String, ClientTargetRecord>::new();
    let mut sources = vec!["builtin-client-catalog".to_string()];
    let mut warnings = Vec::new();

    for target in CLIENT_TARGETS {
        targets.insert(
            normalize(target.id),
            ClientTargetRecord::from_static(target),
        );
    }

    let mut catalog_paths = Vec::new();
    if let Some(root_path) = root_path {
        let config_path = root_path.join("mcpace.config.json");
        if config_path.is_file() {
            match json_helpers::read_json_file(&config_path) {
                Ok(config) => {
                    for (path_expr, source_key) in [
                        (
                            json_helpers::array_at_path(&config, &["clientCatalog", "paths"]),
                            "clientCatalog.paths",
                        ),
                        (
                            json_helpers::array_at_path(&config, &["client", "catalogPaths"]),
                            "client.catalogPaths",
                        ),
                    ] {
                        for item in json_helpers::strings_from_array(path_expr) {
                            catalog_paths.push(resolve_catalog_path(root_path, &item));
                            sources.push(format!("{}:{}", source_key, item));
                        }
                    }
                    for (path_expr, source_key) in [
                        (
                            json_helpers::array_at_path(&config, &["clientCatalog", "targets"]),
                            "clientCatalog.targets",
                        ),
                        (
                            json_helpers::array_at_path(&config, &["client", "catalog", "targets"]),
                            "client.catalog.targets",
                        ),
                    ] {
                        if let Some(items) = path_expr {
                            merge_targets(
                                &mut targets,
                                parse_targets_from_array(items, source_key, &mut warnings),
                                &mut warnings,
                            );
                            sources.push(source_key.to_string());
                        }
                    }
                }
                Err(error) => warnings.push(format!(
                    "client catalog extension could not read '{}': {}",
                    config_path.display(),
                    error
                )),
            }
        }
    }

    if let Ok(env_paths) = env::var("MCPACE_CLIENT_CATALOG") {
        for env_path in env::split_paths(&env_paths) {
            catalog_paths.push(env_path.clone());
            sources.push(format!("env:MCPACE_CLIENT_CATALOG:{}", env_path.display()));
        }
    }

    for catalog_path in catalog_paths {
        match load_targets_from_file(&catalog_path) {
            Ok(items) => merge_targets(&mut targets, items, &mut warnings),
            Err(error) => warnings.push(error),
        }
    }

    Ok(ClientRegistry {
        targets: targets.into_values().collect(),
        sources: text_utils::sorted_unique(sources),
        warnings: text_utils::sorted_unique(warnings),
    })
}

pub fn find_in_targets<'a>(
    targets: &'a [ClientTargetRecord],
    id: &str,
) -> Option<&'a ClientTargetRecord> {
    let normalized = normalize(id);
    targets.iter().find(|target| {
        normalize(&target.id) == normalized
            || target
                .aliases
                .iter()
                .any(|alias| normalize(alias) == normalized)
    })
}

pub fn client_install_support_summary_for_targets(targets: &[ClientTargetRecord]) -> String {
    let values = targets
        .iter()
        .filter(|target| target.supports_client_install())
        .map(|target| target.id.clone())
        .collect::<Vec<_>>();
    join_human_list(&values)
}

impl ClientTargetRecord {
    fn from_static(target: &ClientTarget) -> Self {
        ClientTargetRecord {
            id: target.id.to_string(),
            family_id: target.family_id.to_string(),
            display_name: target.display_name.to_string(),
            aliases: target
                .aliases
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            maturity: target.maturity.to_string(),
            surface_class: target.surface_class.to_string(),
            surface_kind: target.surface_kind.to_string(),
            proof_tier: target.proof_tier.to_string(),
            config_format: target.config_format.to_string(),
            config_paths: target
                .config_paths
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            config_precedence: target
                .config_precedence
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            native_scopes: target
                .native_scopes
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            supported_ingresses: target
                .supported_ingresses
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            documented_features: target
                .documented_features
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            documented_constraints: target
                .documented_constraints
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            notes: target
                .notes
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            install_support: target
                .install_support
                .map(ClientInstallSupportRecord::from_static),
            source: "builtin".to_string(),
        }
    }

    fn from_json(
        value: &JsonValue,
        fallback_id: Option<&str>,
        source: &str,
        warnings: &mut Vec<String>,
    ) -> Option<Self> {
        let object = value.as_object()?;
        let id = string_field(object, "id")
            .or_else(|| fallback_id.map(str::to_string))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(id) = id else {
            warnings.push(format!(
                "{} contains a client target without an id; skipping",
                source
            ));
            return None;
        };
        let install_support = object
            .get("installSupport")
            .and_then(|value| ClientInstallSupportRecord::from_json(value, source, warnings));
        Some(ClientTargetRecord {
            family_id: string_field(object, "familyId").unwrap_or_else(|| id.clone()),
            display_name: string_field(object, "displayName").unwrap_or_else(|| id.clone()),
            aliases: array_field(object, "aliases"),
            maturity: string_field(object, "maturity").unwrap_or_else(|| "external".to_string()),
            surface_class: string_field(object, "surfaceClass")
                .unwrap_or_else(|| "custom".to_string()),
            surface_kind: string_field(object, "surfaceKind")
                .unwrap_or_else(|| "custom-client".to_string()),
            proof_tier: string_field(object, "proofTier").unwrap_or_else(|| "external".to_string()),
            config_format: string_field(object, "configFormat")
                .unwrap_or_else(|| "host-defined".to_string()),
            config_paths: array_field(object, "configPaths"),
            config_precedence: default_array_field(object, "configPrecedence", &["host-defined"]),
            native_scopes: default_array_field(object, "nativeScopes", &["host-defined"]),
            supported_ingresses: default_array_field(object, "supportedIngresses", &["stdio"]),
            documented_features: array_field(object, "documentedFeatures"),
            documented_constraints: array_field(object, "documentedConstraints"),
            notes: array_field(object, "notes"),
            install_support,
            source: source.to_string(),
            id,
        })
    }

    pub fn supports_ingress(&self, ingress: &str) -> bool {
        let normalized = normalize_transport(ingress);
        self.supported_ingresses
            .iter()
            .any(|value| normalize_transport(value) == normalized)
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        let normalized = normalize(feature);
        self.documented_features
            .iter()
            .any(|value| normalize(value) == normalized)
    }

    pub fn has_constraint(&self, constraint: &str) -> bool {
        let normalized = normalize(constraint);
        self.documented_constraints
            .iter()
            .any(|value| normalize(value) == normalized)
    }

    pub fn proof_tier(&self) -> &str {
        &self.proof_tier
    }

    pub fn install_support(&self) -> Option<ClientInstallSupportRecord> {
        self.install_support.clone()
    }

    pub fn supports_client_install(&self) -> bool {
        self.install_support.is_some()
    }

    pub fn preferred_install_scope(&self) -> Option<&str> {
        self.install_support
            .as_ref()
            .map(|value| value.preferred_scope.as_str())
    }

    pub fn preferred_install_config_path(&self) -> Option<&str> {
        self.install_support
            .as_ref()
            .map(|value| value.preferred_config_path.as_str())
            .or_else(|| self.config_paths.first().map(String::as_str))
    }

    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("id", JsonValue::string(self.id.clone())),
            ("familyId", JsonValue::string(self.family_id.clone())),
            ("displayName", JsonValue::string(self.display_name.clone())),
            (
                "aliases",
                JsonValue::array(self.aliases.iter().cloned().map(JsonValue::string)),
            ),
            ("maturity", JsonValue::string(self.maturity.clone())),
            (
                "surfaceClass",
                JsonValue::string(self.surface_class.clone()),
            ),
            ("surfaceKind", JsonValue::string(self.surface_kind.clone())),
            ("proofTier", JsonValue::string(self.proof_tier.clone())),
            (
                "configFormat",
                JsonValue::string(self.config_format.clone()),
            ),
            (
                "configPaths",
                JsonValue::array(self.config_paths.iter().cloned().map(JsonValue::string)),
            ),
            (
                "configPrecedence",
                JsonValue::array(
                    self.config_precedence
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "nativeScopes",
                JsonValue::array(self.native_scopes.iter().cloned().map(JsonValue::string)),
            ),
            (
                "supportedIngresses",
                JsonValue::array(
                    self.supported_ingresses
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "documentedFeatures",
                JsonValue::array(
                    self.documented_features
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "documentedConstraints",
                JsonValue::array(
                    self.documented_constraints
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "notes",
                JsonValue::array(self.notes.iter().cloned().map(JsonValue::string)),
            ),
            ("source", JsonValue::string(self.source.clone())),
            (
                "installSupported",
                JsonValue::bool(self.supports_client_install()),
            ),
            (
                "installSupport",
                match self.install_support() {
                    Some(value) => value.to_json_value(),
                    None => JsonValue::Null,
                },
            ),
        ])
    }
}

impl ClientInstallSupportRecord {
    fn from_static(value: ClientInstallSupport) -> Self {
        ClientInstallSupportRecord {
            kind: ClientInstallKindRecord::from_static(value.kind),
            preferred_scope: value.preferred_scope.to_string(),
            preferred_config_path: value.preferred_config_path.to_string(),
        }
    }

    fn from_json(value: &JsonValue, source: &str, warnings: &mut Vec<String>) -> Option<Self> {
        let object = value.as_object()?;
        let kind = string_field(object, "kind").unwrap_or_else(|| "json-mcp-servers".to_string());
        let parsed_kind = ClientInstallKindRecord::from_kind_name(&kind, object, source, warnings)?;
        Some(ClientInstallSupportRecord {
            kind: parsed_kind,
            preferred_scope: string_field(object, "preferredScope")
                .unwrap_or_else(|| "user".to_string()),
            preferred_config_path: string_field(object, "preferredConfigPath")
                .unwrap_or_else(|| "client config".to_string()),
        })
    }

    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("kind", JsonValue::string(self.kind.as_str())),
            (
                "preferredScope",
                JsonValue::string(self.preferred_scope.clone()),
            ),
            (
                "preferredConfigPath",
                JsonValue::string(self.preferred_config_path.clone()),
            ),
            (
                "jsonServerShape",
                match &self.kind {
                    ClientInstallKindRecord::JsonMcpServers(shape) => shape.to_json_value(),
                    _ => JsonValue::Null,
                },
            ),
        ])
    }
}

impl ClientInstallKindRecord {
    fn from_static(kind: ClientInstallKind) -> Self {
        match kind {
            ClientInstallKind::TomlMcpServersManagedTable => {
                ClientInstallKindRecord::TomlMcpServersManagedTable
            }
            ClientInstallKind::JsonMcpServers(shape) => ClientInstallKindRecord::JsonMcpServers(
                JsonMcpServerShapeRecord::from_static(shape),
            ),
            ClientInstallKind::YamlMcpServersManagedSection => {
                ClientInstallKindRecord::YamlMcpServersManagedSection
            }
        }
    }

    fn from_kind_name(
        kind: &str,
        object: &BTreeMap<String, JsonValue>,
        source: &str,
        warnings: &mut Vec<String>,
    ) -> Option<Self> {
        match normalize(kind).as_str() {
            "toml-mcp-servers-managed-table" | "toml" => {
                Some(ClientInstallKindRecord::TomlMcpServersManagedTable)
            }
            "yaml-mcp-servers-managed-section" | "yaml" => {
                Some(ClientInstallKindRecord::YamlMcpServersManagedSection)
            }
            "json-mcp-servers" | "json" => Some(ClientInstallKindRecord::JsonMcpServers(
                JsonMcpServerShapeRecord::from_json(object.get("jsonServerShape"))
                    .unwrap_or_default(),
            )),
            other => {
                warnings.push(format!("{} uses unknown client install kind '{}'; install patching is disabled for that target", source, other));
                None
            }
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ClientInstallKindRecord::TomlMcpServersManagedTable => "toml-mcp-servers-managed-table",
            ClientInstallKindRecord::JsonMcpServers(_) => "json-mcp-servers",
            ClientInstallKindRecord::YamlMcpServersManagedSection => {
                "yaml-mcp-servers-managed-section"
            }
        }
    }
}

impl JsonMcpServerShapeRecord {
    fn from_static(shape: JsonMcpServerShape) -> Self {
        JsonMcpServerShapeRecord {
            url_field: shape.url_field.to_string(),
            include_type_http: shape.include_type_http,
            include_tools_star: shape.include_tools_star,
            include_disabled_false: shape.include_disabled_false,
        }
    }

    fn from_json(value: Option<&JsonValue>) -> Option<Self> {
        let object = value?.as_object()?;
        Some(JsonMcpServerShapeRecord {
            url_field: string_field(object, "urlField").unwrap_or_else(|| "url".to_string()),
            include_type_http: bool_field(object, "includeTypeHttp", true),
            include_tools_star: bool_field(object, "includeToolsStar", false),
            include_disabled_false: bool_field(object, "includeDisabledFalse", false),
        })
    }

    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("urlField", JsonValue::string(self.url_field.clone())),
            ("includeTypeHttp", JsonValue::bool(self.include_type_http)),
            ("includeToolsStar", JsonValue::bool(self.include_tools_star)),
            (
                "includeDisabledFalse",
                JsonValue::bool(self.include_disabled_false),
            ),
        ])
    }
}

impl Default for JsonMcpServerShapeRecord {
    fn default() -> Self {
        JsonMcpServerShapeRecord {
            url_field: "url".to_string(),
            include_type_http: true,
            include_tools_star: false,
            include_disabled_false: false,
        }
    }
}

fn load_targets_from_file(path: &Path) -> Result<Vec<ClientTargetRecord>, String> {
    let json = json_helpers::read_json_file(path)?;
    let source = format!("file:{}", path.display());
    let mut warnings = Vec::new();
    let targets = match json.as_array() {
        Some(items) => parse_targets_from_array(items, &source, &mut warnings),
        None => match json.get("targets").and_then(JsonValue::as_array) {
            Some(items) => parse_targets_from_array(items, &source, &mut warnings),
            None => parse_targets_from_object(&json, &source, &mut warnings),
        },
    };
    if targets.is_empty() && warnings.is_empty() {
        return Err(format!(
            "client catalog '{}' did not contain any targets",
            path.display()
        ));
    }
    Ok(targets)
}

fn parse_targets_from_array(
    items: &[JsonValue],
    source: &str,
    warnings: &mut Vec<String>,
) -> Vec<ClientTargetRecord> {
    items
        .iter()
        .filter_map(|item| ClientTargetRecord::from_json(item, None, source, warnings))
        .collect()
}

fn parse_targets_from_object(
    value: &JsonValue,
    source: &str,
    warnings: &mut Vec<String>,
) -> Vec<ClientTargetRecord> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    object
        .iter()
        .filter_map(|(id, item)| ClientTargetRecord::from_json(item, Some(id), source, warnings))
        .collect()
}

fn merge_targets(
    targets: &mut BTreeMap<String, ClientTargetRecord>,
    items: Vec<ClientTargetRecord>,
    warnings: &mut Vec<String>,
) {
    for target in items {
        let key = normalize(&target.id);
        if let Some(previous) = targets.insert(key, target.clone()) {
            warnings.push(format!(
                "client target '{}' from {} replaced previous definition from {}",
                target.id, target.source, previous.source
            ));
        }
    }
}

fn resolve_catalog_path(root_path: &Path, raw: &str) -> PathBuf {
    let expanded = expand_home(raw);
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        path
    } else {
        root_path.join(path)
    }
}

fn expand_home(raw: &str) -> String {
    if raw == "~" || raw.starts_with("~/") || raw.starts_with("~\\") {
        if let Ok(home) = env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
            let suffix = raw.trim_start_matches('~').trim_start_matches(['/', '\\']);
            return PathBuf::from(home).join(suffix).display().to_string();
        }
    }
    raw.to_string()
}

fn string_field(object: &BTreeMap<String, JsonValue>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(JsonValue::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn bool_field(object: &BTreeMap<String, JsonValue>, key: &str, default: bool) -> bool {
    object
        .get(key)
        .and_then(JsonValue::as_bool)
        .unwrap_or(default)
}

fn array_field(object: &BTreeMap<String, JsonValue>, key: &str) -> Vec<String> {
    json_helpers::strings_from_array(object.get(key).and_then(JsonValue::as_array))
}

fn default_array_field(
    object: &BTreeMap<String, JsonValue>,
    key: &str,
    fallback: &[&str],
) -> Vec<String> {
    let values = array_field(object, key);
    if values.is_empty() {
        fallback.iter().map(|value| (*value).to_string()).collect()
    } else {
        values
    }
}

pub fn find(id: &str) -> Option<&'static ClientTarget> {
    let normalized = normalize(id);
    CLIENT_TARGETS.iter().find(|target| {
        target.id == normalized
            || target
                .aliases
                .iter()
                .any(|alias| normalize(alias) == normalized)
    })
}

pub fn normalize(value: &str) -> String {
    text_utils::normalize_flag(value)
}

pub fn client_install_support_summary() -> String {
    let values = CLIENT_TARGETS
        .iter()
        .filter(|target| target.supports_client_install())
        .map(|target| target.id.to_string())
        .collect::<Vec<_>>();
    join_human_list(&values)
}

impl ClientTarget {
    pub fn supports_ingress(&self, ingress: &str) -> bool {
        let normalized = normalize_transport(ingress);
        self.supported_ingresses
            .iter()
            .any(|value| normalize_transport(value) == normalized)
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        let normalized = normalize(feature);
        self.documented_features
            .iter()
            .any(|value| normalize(value) == normalized)
    }

    pub fn has_constraint(&self, constraint: &str) -> bool {
        let normalized = normalize(constraint);
        self.documented_constraints
            .iter()
            .any(|value| normalize(value) == normalized)
    }

    pub fn proof_tier(&self) -> &'static str {
        self.proof_tier
    }

    pub fn install_support(&self) -> Option<ClientInstallSupport> {
        self.install_support
    }

    pub fn supports_client_install(&self) -> bool {
        self.install_support.is_some()
    }

    pub fn preferred_install_scope(&self) -> Option<&'static str> {
        self.install_support.map(|value| value.preferred_scope)
    }

    pub fn preferred_install_config_path(&self) -> Option<&'static str> {
        self.install_support
            .map(|value| value.preferred_config_path)
            .or_else(|| self.config_paths.first().copied())
    }

    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("id", JsonValue::string(self.id)),
            ("familyId", JsonValue::string(self.family_id)),
            ("displayName", JsonValue::string(self.display_name)),
            (
                "aliases",
                JsonValue::array(self.aliases.iter().copied().map(JsonValue::string)),
            ),
            ("maturity", JsonValue::string(self.maturity)),
            ("surfaceClass", JsonValue::string(self.surface_class)),
            ("surfaceKind", JsonValue::string(self.surface_kind)),
            ("proofTier", JsonValue::string(self.proof_tier)),
            ("configFormat", JsonValue::string(self.config_format)),
            (
                "configPaths",
                JsonValue::array(self.config_paths.iter().copied().map(JsonValue::string)),
            ),
            (
                "configPrecedence",
                JsonValue::array(
                    self.config_precedence
                        .iter()
                        .copied()
                        .map(JsonValue::string),
                ),
            ),
            (
                "nativeScopes",
                JsonValue::array(self.native_scopes.iter().copied().map(JsonValue::string)),
            ),
            (
                "supportedIngresses",
                JsonValue::array(
                    self.supported_ingresses
                        .iter()
                        .copied()
                        .map(JsonValue::string),
                ),
            ),
            (
                "documentedFeatures",
                JsonValue::array(
                    self.documented_features
                        .iter()
                        .copied()
                        .map(JsonValue::string),
                ),
            ),
            (
                "documentedConstraints",
                JsonValue::array(
                    self.documented_constraints
                        .iter()
                        .copied()
                        .map(JsonValue::string),
                ),
            ),
            (
                "notes",
                JsonValue::array(self.notes.iter().copied().map(JsonValue::string)),
            ),
            (
                "installSupported",
                JsonValue::bool(self.supports_client_install()),
            ),
            (
                "installSupport",
                match self.install_support() {
                    Some(value) => value.to_json_value(),
                    None => JsonValue::Null,
                },
            ),
        ])
    }
}

impl ClientInstallSupport {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("kind", JsonValue::string(self.kind.as_str())),
            ("preferredScope", JsonValue::string(self.preferred_scope)),
            (
                "preferredConfigPath",
                JsonValue::string(self.preferred_config_path),
            ),
            (
                "jsonServerShape",
                match self.kind {
                    ClientInstallKind::JsonMcpServers(shape) => shape.to_json_value(),
                    _ => JsonValue::Null,
                },
            ),
        ])
    }
}

impl ClientInstallKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClientInstallKind::TomlMcpServersManagedTable => "toml-mcp-servers-managed-table",
            ClientInstallKind::JsonMcpServers(_) => "json-mcp-servers",
            ClientInstallKind::YamlMcpServersManagedSection => "yaml-mcp-servers-managed-section",
        }
    }
}

impl JsonMcpServerShape {
    pub fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("urlField", JsonValue::string(self.url_field)),
            ("includeTypeHttp", JsonValue::bool(self.include_type_http)),
            ("includeToolsStar", JsonValue::bool(self.include_tools_star)),
            (
                "includeDisabledFalse",
                JsonValue::bool(self.include_disabled_false),
            ),
        ])
    }
}

fn normalize_transport(value: &str) -> String {
    match normalize(value).as_str() {
        "http" | "streamable-http" | "streamable_http" => "streamable-http".to_string(),
        "stdio" | "local-stdio" => "stdio".to_string(),
        "sse" => "sse".to_string(),
        other => other.to_string(),
    }
}

fn join_human_list(values: &[String]) -> String {
    match values {
        [] => "none".to_string(),
        [one] => one.clone(),
        [first, second] => format!("{} and {}", first, second),
        _ => {
            let mut joined = values[..values.len() - 1].join(", ");
            joined.push_str(", and ");
            joined.push_str(values.last().map(String::as_str).unwrap_or(""));
            joined
        }
    }
}
