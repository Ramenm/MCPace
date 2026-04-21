use super::args::ParsedArgs;
use super::context::resolve_context;
use super::metadata::load_metadata;
use super::plan::build_plan;
use super::render::{count_static, join_count_map, join_static_or_none, write_text_plan};
use crate::client_catalog::{ClientTarget, CLIENT_TARGETS};
use crate::doctor;
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::server;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const LOCAL_MCP_URL: &str = "http://127.0.0.1:39022/mcp";
const CLIENT_INSTALL_SUPPORTED_TARGETS: &[&str] = &[
    "codex",
    "claude-code",
    "cursor-local",
    "kiro-ide",
    "kiro-cli",
    "gemini-cli",
    "github-copilot-cli",
    "windsurf",
    "hermes-agent",
];

pub(super) fn supports_client_install(target_id: &str) -> bool {
    CLIENT_INSTALL_SUPPORTED_TARGETS.contains(&target_id)
}

pub(super) fn client_install_support_summary() -> String {
    "Codex, Claude Code, Cursor, Kiro, Gemini CLI, Windsurf, GitHub Copilot CLI, and Hermes Agent"
        .to_string()
}

pub(super) fn run_list(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let config_version = root_path.as_deref().and_then(doctor::read_config_version);
    let configured_client_key_name = root_path.as_deref().and_then(read_client_key_name);

    let family_counts = count_static(CLIENT_TARGETS.iter().map(|target| target.family_id));
    let surface_class_counts =
        count_static(CLIENT_TARGETS.iter().map(|target| target.surface_class));

    if parsed.json_output {
        let json = JsonValue::object([
            (
                "configVersion",
                match config_version {
                    Some(ref value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "configuredClientKeyName",
                match configured_client_key_name {
                    Some(ref value) => JsonValue::string(value.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "familyCounts",
                JsonValue::object(
                    family_counts
                        .iter()
                        .map(|(key, value)| (key.clone(), JsonValue::number(*value))),
                ),
            ),
            (
                "surfaceClassCounts",
                JsonValue::object(
                    surface_class_counts
                        .iter()
                        .map(|(key, value)| (key.clone(), JsonValue::number(*value))),
                ),
            ),
            (
                "targets",
                JsonValue::array(CLIENT_TARGETS.iter().map(ClientTarget::to_json_value)),
            ),
        ]);
        let _ = writeln!(stdout, "{}", json.to_pretty_string());
        return 0;
    }

    let _ = writeln!(stdout, "Known client targets: {}", CLIENT_TARGETS.len());
    let _ = writeln!(
        stdout,
        "Configured adapter key name: {}",
        configured_client_key_name.as_deref().unwrap_or("none")
    );
    let _ = writeln!(stdout, "Families: {}", join_count_map(&family_counts));
    let _ = writeln!(
        stdout,
        "Surface classes: {}",
        join_count_map(&surface_class_counts)
    );
    for target in CLIENT_TARGETS {
        let _ = writeln!(
            stdout,
            "- {} [{} / {} / {}] format={} ingress={} scopes={}",
            target.id,
            target.maturity,
            target.surface_class,
            target.surface_kind,
            target.config_format,
            join_static_or_none(target.supported_ingresses),
            join_static_or_none(target.native_scopes)
        );
        let _ = writeln!(
            stdout,
            "    family={} paths={}",
            target.family_id,
            join_static_or_none(target.config_paths)
        );
        let _ = writeln!(
            stdout,
            "    precedence={}",
            join_static_or_none(target.config_precedence)
        );
        let _ = writeln!(
            stdout,
            "    features={}",
            join_static_or_none(target.documented_features)
        );
        let _ = writeln!(
            stdout,
            "    constraints={}",
            join_static_or_none(target.documented_constraints)
        );
        let _ = writeln!(stdout, "    notes={}", join_static_or_none(target.notes));
    }
    0
}

pub(super) fn run_plan(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let server_records = match server::load_server_records(&root_path) {
        Ok(records) => records,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let metadata = match load_metadata(&parsed) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 2;
        }
    };

    let mut context = resolve_context(&parsed, &metadata);
    if let Some(client_target) = crate::client_catalog::find(&context.client_id) {
        prefer_local_http_when_supported(&parsed, &mut context, client_target, "serve-default");
    }
    let plan = build_plan(
        root_path.display().to_string(),
        doctor::read_config_version(&root_path),
        read_client_key_name(&root_path),
        context,
        &server_records,
    );

    if parsed.json_output {
        let _ = writeln!(stdout, "{}", plan.to_json_value().to_pretty_string());
        return 0;
    }

    write_text_plan(&plan, stdout);
    0
}

pub(super) fn run_export(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let metadata = match load_metadata(&parsed) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 2;
        }
    };
    let mut context = resolve_context(&parsed, &metadata);
    if context.project_root.is_none() {
        context.project_root = Some(sanitize_path_for_display(&canonicalize_or_original(
            &root_path,
        )));
        context.project_root_source = "export-root".to_string();
    }
    let client_target = match crate::client_catalog::find(&context.client_id) {
        Some(value) => value,
        None => {
            let _ = writeln!(
                stderr,
                "unknown client target '{}'; use 'mcpace client list' to inspect supported surfaces",
                context.client_id
            );
            return 2;
        }
    };
    prefer_local_http_when_supported(&parsed, &mut context, client_target, "serve-default");

    let server_records = match server::load_server_records(&root_path) {
        Ok(records) => records,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let plan = build_plan(
        root_path.display().to_string(),
        doctor::read_config_version(&root_path),
        read_client_key_name(&root_path),
        context,
        &server_records,
    );

    let preview = ClientExportPreview::from_plan(client_target, &plan);
    if parsed.json_output {
        let _ = writeln!(stdout, "{}", preview.to_json_value().to_pretty_string());
        return 0;
    }

    preview.write_text(stdout);
    0
}

pub(super) fn run_install(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let metadata = match load_metadata(&parsed) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 2;
        }
    };
    let mut context = resolve_context(&parsed, &metadata);
    if context.project_root.is_none() {
        context.project_root = Some(sanitize_path_for_display(&canonicalize_or_original(
            &root_path,
        )));
        context.project_root_source = "install-root".to_string();
    }
    let client_target = match crate::client_catalog::find(&context.client_id) {
        Some(value) => value,
        None => {
            let _ = writeln!(
                stderr,
                "unknown client target '{}'; use 'mcpace client list' to inspect supported surfaces",
                context.client_id
            );
            return 2;
        }
    };
    prefer_local_http_when_supported(&parsed, &mut context, client_target, "serve-default");

    let server_records = match server::load_server_records(&root_path) {
        Ok(records) => records,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let plan = build_plan(
        root_path.display().to_string(),
        doctor::read_config_version(&root_path),
        read_client_key_name(&root_path),
        context,
        &server_records,
    );

    let install = match ClientInstallPlan::from_plan(&root_path, client_target, &plan) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let result = match install.write() {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    if parsed.json_output {
        let _ = writeln!(stdout, "{}", result.to_json_value().to_pretty_string());
        return 0;
    }

    result.write_text(stdout);
    0
}

fn read_client_key_name(root_path: &Path) -> Option<String> {
    let config_path = root_path.join("mcpace.config.json");
    let config = json_helpers::read_json_file(&config_path).ok()?;
    json_helpers::string_at_path(&config, &["client", "keyName"])
        .map(|value| value.trim().to_string())
}

fn prefer_local_http_when_supported(
    parsed: &ParsedArgs,
    context: &mut super::model::ResolvedContext,
    client_target: &ClientTarget,
    source: &str,
) {
    if parsed.transport.is_none()
        && !client_target.has_constraint("public-http-only")
        && client_target.supports_ingress("streamable-http")
    {
        context.preferred_ingress = "streamable-http".to_string();
        context.preferred_ingress_source = source.to_string();
    }
}

struct ClientExportPreview {
    client_target_id: String,
    display_name: String,
    adapter_key_name: String,
    config_format: String,
    config_paths: Vec<String>,
    config_precedence: Vec<String>,
    native_scopes: Vec<String>,
    preferred_ingress: String,
    export_mode: String,
    entrypoint_mode: String,
    launcher_command: String,
    root_path: String,
    mode: String,
    can_connect_today: bool,
    writes_config: bool,
    adapter_contract: AdapterContractPreview,
    blockers: Vec<String>,
    warnings: Vec<String>,
    next_actions: Vec<String>,
}

struct AdapterContractPreview {
    kind: String,
    command: Option<String>,
    args: Vec<String>,
    url_template: Option<String>,
    metadata_carrier: String,
    session_model: String,
    notes: Vec<String>,
}

enum ClientInstallConfig {
    CodexToml,
    JsonMcpServers { server_config: JsonValue },
    HermesYaml,
}

struct ClientInstallPlan {
    client_target_id: String,
    display_name: String,
    adapter_key_name: String,
    config_path: PathBuf,
    server_url: String,
    config: ClientInstallConfig,
    warnings: Vec<String>,
}

struct ClientInstallResult {
    client_target_id: String,
    display_name: String,
    adapter_key_name: String,
    config_path: String,
    transport: String,
    server_url: String,
    changed: bool,
    replaced_existing_block: bool,
    created_config_dir: bool,
    created_config_file: bool,
    warnings: Vec<String>,
}

impl ClientExportPreview {
    fn from_plan(target: &ClientTarget, plan: &super::model::ClientPlan) -> Self {
        let adapter_key_name = plan
            .configured_client_key_name
            .clone()
            .unwrap_or_else(|| format!("{}-adapter", target.family_id));
        let export_mode = resolve_export_mode(target, plan);

        let blockers = match export_mode.as_str() {
            "local-stdio-launcher" | "local-streamable-http" => Vec::new(),
            "public-http-connector" => vec![
                "This client surface needs a public HTTP MCP endpoint or relay, and MCPace does not ship that lane yet in this repo.".to_string(),
            ],
            _ => vec![
                "MCPace does not yet have a verified ingress lane for this client surface, so export stays preview-only.".to_string(),
            ],
        };

        let next_actions = match export_mode.as_str() {
            "local-streamable-http" if supports_client_install(target.id) => {
                vec![
                format!(
                    "Run 'mcpace client install {} --root <path>' to patch the MCPace entry in {} automatically.",
                    target.id,
                    preferred_install_config_path(target)
                ),
                format!(
                    "Keep one MCPace server running on port 39022 so {} always points at the same localhost MCP URL.",
                    target.display_name
                ),
            ]
            }
            "local-streamable-http" => vec![
                "Run 'mcpace serve --port 39022' and point this client at the one MCPace URL.".to_string(),
                "Keep export as the source of truth for the HTTP MCPace contract until that client gets a dedicated config patcher.".to_string(),
            ],
            "local-stdio-launcher" => vec![
                "This client surface still needs the stdio MCPace fallback today.".to_string(),
                "Keep stdio surfaces as internal compatibility lanes while HTTP-first clients move to the one-port MCPace server.".to_string(),
            ],
            "public-http-connector" => vec![
                "Ship a public HTTP / relay lane for cloud connectors before claiming this surface works through MCPace.".to_string(),
                "Add credential bootstrap and real hosted compatibility traces for this connector class.".to_string(),
            ],
            _ => vec![
                "Add a verified ingress lane for this client surface before turning preview output into a real config patch.".to_string(),
            ],
        };

        ClientExportPreview {
            client_target_id: target.id.to_string(),
            display_name: target.display_name.to_string(),
            adapter_key_name,
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
            preferred_ingress: plan.preferred_ingress.clone(),
            export_mode: export_mode.clone(),
            entrypoint_mode: plan.entrypoint_mode.clone(),
            launcher_command: plan.launcher_command.clone(),
            root_path: plan.root_path.clone(),
            mode: if matches!(
                export_mode.as_str(),
                "local-stdio-launcher" | "local-streamable-http"
            ) {
                "connectable-preview".to_string()
            } else {
                "preview-only".to_string()
            },
            can_connect_today: matches!(
                export_mode.as_str(),
                "local-stdio-launcher" | "local-streamable-http"
            ),
            writes_config: false,
            adapter_contract: build_adapter_contract(target, plan, &export_mode),
            blockers,
            warnings: plan.warnings.clone(),
            next_actions,
        }
    }

    fn to_json_value(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert("mode".to_string(), JsonValue::string(self.mode.clone()));
        map.insert(
            "clientTargetId".to_string(),
            JsonValue::string(self.client_target_id.clone()),
        );
        map.insert(
            "displayName".to_string(),
            JsonValue::string(self.display_name.clone()),
        );
        map.insert(
            "adapterKeyName".to_string(),
            JsonValue::string(self.adapter_key_name.clone()),
        );
        map.insert(
            "configFormat".to_string(),
            JsonValue::string(self.config_format.clone()),
        );
        map.insert(
            "configPaths".to_string(),
            JsonValue::array(self.config_paths.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "configPrecedence".to_string(),
            JsonValue::array(
                self.config_precedence
                    .iter()
                    .cloned()
                    .map(JsonValue::string),
            ),
        );
        map.insert(
            "nativeScopes".to_string(),
            JsonValue::array(self.native_scopes.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "preferredIngress".to_string(),
            JsonValue::string(self.preferred_ingress.clone()),
        );
        map.insert(
            "exportMode".to_string(),
            JsonValue::string(self.export_mode.clone()),
        );
        map.insert(
            "entrypointMode".to_string(),
            JsonValue::string(self.entrypoint_mode.clone()),
        );
        map.insert(
            "launcherCommand".to_string(),
            JsonValue::string(self.launcher_command.clone()),
        );
        map.insert(
            "rootPath".to_string(),
            JsonValue::string(self.root_path.clone()),
        );
        map.insert(
            "canConnectToday".to_string(),
            JsonValue::bool(self.can_connect_today),
        );
        map.insert(
            "writesConfig".to_string(),
            JsonValue::bool(self.writes_config),
        );
        map.insert(
            "adapterContract".to_string(),
            self.adapter_contract.to_json_value(),
        );
        map.insert(
            "blockers".to_string(),
            JsonValue::array(self.blockers.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "warnings".to_string(),
            JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
        );
        map.insert(
            "nextActions".to_string(),
            JsonValue::array(self.next_actions.iter().cloned().map(JsonValue::string)),
        );
        JsonValue::Object(map)
    }

    fn write_text(&self, stdout: &mut dyn Write) {
        let _ = writeln!(
            stdout,
            "Client export {}",
            if self.can_connect_today {
                "connectable preview"
            } else {
                "preview only"
            }
        );
        let _ = writeln!(
            stdout,
            "Client target: {} ({})",
            self.client_target_id, self.display_name
        );
        let _ = writeln!(stdout, "Adapter key: {}", self.adapter_key_name);
        let _ = writeln!(stdout, "Config format: {}", self.config_format);
        let _ = writeln!(stdout, "Config paths: {}", join_or_none(&self.config_paths));
        let _ = writeln!(
            stdout,
            "Config precedence: {}",
            join_or_none(&self.config_precedence)
        );
        let _ = writeln!(
            stdout,
            "Native scopes: {}",
            join_or_none(&self.native_scopes)
        );
        let _ = writeln!(stdout, "Preferred ingress: {}", self.preferred_ingress);
        let _ = writeln!(stdout, "Export mode: {}", self.export_mode);
        let _ = writeln!(stdout, "Entrypoint mode: {}", self.entrypoint_mode);
        let _ = writeln!(stdout, "Launcher command: {}", self.launcher_command);
        let _ = writeln!(
            stdout,
            "Can connect today: {}",
            yes_no(self.can_connect_today)
        );
        let _ = writeln!(stdout, "Writes config: {}", yes_no(self.writes_config));
        self.adapter_contract.write_text(stdout);
        let _ = writeln!(stdout, "Blockers: {}", join_or_none(&self.blockers));
        let _ = writeln!(stdout, "Warnings: {}", join_or_none(&self.warnings));
        let _ = writeln!(stdout, "Next actions: {}", join_or_none(&self.next_actions));
    }
}

impl ClientInstallPlan {
    fn from_plan(
        root_path: &Path,
        target: &ClientTarget,
        plan: &super::model::ClientPlan,
    ) -> Result<Self, String> {
        let canonical_root = canonicalize_or_original(root_path);
        let adapter_key_name = plan
            .configured_client_key_name
            .clone()
            .unwrap_or_else(|| "MCPace".to_string());
        let (config_path, config) = match target.id {
            "codex" => (
                canonical_root.join(".codex").join("config.toml"),
                ClientInstallConfig::CodexToml,
            ),
            "claude-code" => (
                canonical_root.join(".mcp.json"),
                ClientInstallConfig::JsonMcpServers {
                    server_config: JsonValue::object([
                        ("type", JsonValue::string("http")),
                        ("url", JsonValue::string(LOCAL_MCP_URL)),
                    ]),
                },
            ),
            "cursor-local" => (
                canonical_root.join(".cursor").join("mcp.json"),
                ClientInstallConfig::JsonMcpServers {
                    server_config: JsonValue::object([("url", JsonValue::string(LOCAL_MCP_URL))]),
                },
            ),
            "kiro-ide" | "kiro-cli" => (
                canonical_root
                    .join(".kiro")
                    .join("settings")
                    .join("mcp.json"),
                ClientInstallConfig::JsonMcpServers {
                    server_config: JsonValue::object([
                        ("url", JsonValue::string(LOCAL_MCP_URL)),
                        ("disabled", JsonValue::bool(false)),
                    ]),
                },
            ),
            "gemini-cli" => (
                canonical_root.join(".gemini").join("settings.json"),
                ClientInstallConfig::JsonMcpServers {
                    server_config: JsonValue::object([(
                        "httpUrl",
                        JsonValue::string(LOCAL_MCP_URL),
                    )]),
                },
            ),
            "github-copilot-cli" => (
                resolve_user_install_path(".copilot", "mcp-config.json")?,
                ClientInstallConfig::JsonMcpServers {
                    server_config: JsonValue::object([
                        ("type", JsonValue::string("http")),
                        ("url", JsonValue::string(LOCAL_MCP_URL)),
                        ("tools", JsonValue::array([JsonValue::string("*")])),
                    ]),
                },
            ),
            "windsurf" => (
                resolve_user_install_path(".codeium\\windsurf", "mcp_config.json")?,
                ClientInstallConfig::JsonMcpServers {
                    server_config: JsonValue::object([(
                        "serverUrl",
                        JsonValue::string(LOCAL_MCP_URL),
                    )]),
                },
            ),
            "hermes-agent" => (
                resolve_user_install_path(".hermes", "config.yaml")?,
                ClientInstallConfig::HermesYaml,
            ),
            other => {
                return Err(format!(
                    "client install currently supports {}; '{}' remains manual for now",
                    client_install_support_summary(),
                    other,
                ))
            }
        };

        Ok(Self {
            client_target_id: target.id.to_string(),
            display_name: target.display_name.to_string(),
            adapter_key_name,
            config_path,
            server_url: LOCAL_MCP_URL.to_string(),
            config,
            warnings: install_warnings_from_plan(plan),
        })
    }

    fn write(&self) -> Result<ClientInstallResult, String> {
        let config_dir = self
            .config_path
            .parent()
            .ok_or_else(|| "failed to resolve the target client config directory".to_string())?;
        let created_config_dir = if config_dir.is_dir() {
            false
        } else {
            fs::create_dir_all(config_dir).map_err(|error| {
                format!(
                    "failed to create client config directory '{}': {}",
                    config_dir.display(),
                    error
                )
            })?;
            true
        };

        let existing = match fs::read_to_string(&self.config_path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(format!(
                    "failed to read client config '{}': {}",
                    self.config_path.display(),
                    error
                ))
            }
        };
        let created_config_file = existing.is_empty() && !self.config_path.is_file();
        let update = match &self.config {
            ClientInstallConfig::CodexToml => {
                let newline = detect_newline(&existing);
                let managed_block =
                    build_codex_managed_block(&self.adapter_key_name, &self.server_url, newline);
                upsert_codex_managed_block(
                    &existing,
                    &self.adapter_key_name,
                    &managed_block,
                    &self.config_path,
                )?
            }
            ClientInstallConfig::JsonMcpServers { server_config } => upsert_json_mcp_server(
                &existing,
                &self.adapter_key_name,
                server_config.clone(),
                &self.config_path,
            )?,
            ClientInstallConfig::HermesYaml => upsert_hermes_mcp_server(
                &existing,
                &self.adapter_key_name,
                &self.server_url,
                &self.config_path,
            )?,
        };

        let changed = update.contents != existing;
        if changed {
            fs::write(&self.config_path, update.contents).map_err(|error| {
                format!(
                    "failed to write client config '{}': {}",
                    self.config_path.display(),
                    error
                )
            })?;
        }

        Ok(ClientInstallResult {
            client_target_id: self.client_target_id.clone(),
            display_name: self.display_name.clone(),
            adapter_key_name: self.adapter_key_name.clone(),
            config_path: sanitize_path_for_display(&self.config_path),
            transport: "streamable-http".to_string(),
            server_url: self.server_url.clone(),
            changed,
            replaced_existing_block: update.replaced_existing_block,
            created_config_dir,
            created_config_file,
            warnings: self.warnings.clone(),
        })
    }
}

impl ClientInstallResult {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("mode", JsonValue::string("installed")),
            (
                "clientTargetId",
                JsonValue::string(self.client_target_id.clone()),
            ),
            ("displayName", JsonValue::string(self.display_name.clone())),
            (
                "adapterKeyName",
                JsonValue::string(self.adapter_key_name.clone()),
            ),
            ("configPath", JsonValue::string(self.config_path.clone())),
            ("transport", JsonValue::string(self.transport.clone())),
            ("url", JsonValue::string(self.server_url.clone())),
            ("writesConfig", JsonValue::bool(true)),
            ("changed", JsonValue::bool(self.changed)),
            (
                "replacedExistingBlock",
                JsonValue::bool(self.replaced_existing_block),
            ),
            ("createdConfigDir", JsonValue::bool(self.created_config_dir)),
            (
                "createdConfigFile",
                JsonValue::bool(self.created_config_file),
            ),
            (
                "warnings",
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }

    fn write_text(&self, stdout: &mut dyn Write) {
        let _ = writeln!(stdout, "Client install complete");
        let _ = writeln!(
            stdout,
            "Client target: {} ({})",
            self.client_target_id, self.display_name
        );
        let _ = writeln!(stdout, "Adapter key: {}", self.adapter_key_name);
        let _ = writeln!(stdout, "Config path: {}", self.config_path);
        let _ = writeln!(stdout, "Transport: {}", self.transport);
        let _ = writeln!(stdout, "URL: {}", self.server_url);
        let _ = writeln!(stdout, "Changed config: {}", yes_no(self.changed));
        let _ = writeln!(
            stdout,
            "Replaced existing block: {}",
            yes_no(self.replaced_existing_block)
        );
        let _ = writeln!(
            stdout,
            "Created config directory: {}",
            yes_no(self.created_config_dir)
        );
        let _ = writeln!(
            stdout,
            "Created config file: {}",
            yes_no(self.created_config_file)
        );
        let _ = writeln!(stdout, "Warnings: {}", join_or_none(&self.warnings));
    }
}

impl AdapterContractPreview {
    fn to_json_value(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert("type".to_string(), JsonValue::string(self.kind.clone()));
        match &self.command {
            Some(value) => {
                map.insert("command".to_string(), JsonValue::string(value.clone()));
            }
            None => {
                map.insert("command".to_string(), JsonValue::Null);
            }
        }
        map.insert(
            "args".to_string(),
            JsonValue::array(self.args.iter().cloned().map(JsonValue::string)),
        );
        match &self.url_template {
            Some(value) => {
                map.insert("urlTemplate".to_string(), JsonValue::string(value.clone()));
            }
            None => {
                map.insert("urlTemplate".to_string(), JsonValue::Null);
            }
        }
        map.insert(
            "metadataCarrier".to_string(),
            JsonValue::string(self.metadata_carrier.clone()),
        );
        map.insert(
            "sessionModel".to_string(),
            JsonValue::string(self.session_model.clone()),
        );
        map.insert(
            "notes".to_string(),
            JsonValue::array(self.notes.iter().cloned().map(JsonValue::string)),
        );
        JsonValue::Object(map)
    }

    fn write_text(&self, stdout: &mut dyn Write) {
        let _ = writeln!(stdout, "Adapter contract type: {}", self.kind);
        let _ = writeln!(
            stdout,
            "Adapter command: {}",
            self.command.as_deref().unwrap_or("none")
        );
        let _ = writeln!(stdout, "Adapter args: {}", join_or_none(&self.args));
        let _ = writeln!(
            stdout,
            "Adapter URL template: {}",
            self.url_template.as_deref().unwrap_or("none")
        );
        let _ = writeln!(stdout, "Metadata carrier: {}", self.metadata_carrier);
        let _ = writeln!(stdout, "Session model: {}", self.session_model);
        let _ = writeln!(stdout, "Adapter notes: {}", join_or_none(&self.notes));
    }
}

fn resolve_export_mode(target: &ClientTarget, plan: &super::model::ClientPlan) -> String {
    if target.has_constraint("public-http-only") || target.surface_class == "cloud" {
        return "public-http-connector".to_string();
    }
    if plan.preferred_ingress == "streamable-http" || target.supports_ingress("streamable-http") {
        return "local-streamable-http".to_string();
    }
    "local-stdio-launcher".to_string()
}

fn preferred_install_config_path(target: &ClientTarget) -> &str {
    target
        .config_paths
        .iter()
        .find(|path| path.starts_with('.'))
        .or_else(|| target.config_paths.first())
        .copied()
        .unwrap_or("client config")
}

fn build_adapter_contract(
    target: &ClientTarget,
    plan: &super::model::ClientPlan,
    export_mode: &str,
) -> AdapterContractPreview {
    match export_mode {
        "public-http-connector" => AdapterContractPreview {
            kind: "public-http-connector".to_string(),
            command: None,
            args: Vec::new(),
            url_template: Some("https://YOUR-MCPACE-RELAY/mcp".to_string()),
            metadata_carrier: "public HTTP request metadata plus MCP session headers".to_string(),
            session_model: "HTTP session with Mcp-Session-Id and relay-owned auth context".to_string(),
            notes: vec![
                format!(
                    "{} only reaches public HTTP MCP servers, so MCPace needs a relay/public ingress instead of a local launcher.",
                    target.display_name
                ),
                "The cloud/API connector path should keep one visible MCPace URL even when upstream servers stay mixed behind the runtime.".to_string(),
            ],
        },
        "local-streamable-http" => AdapterContractPreview {
            kind: "local-streamable-http".to_string(),
            command: None,
            args: Vec::new(),
            url_template: Some(LOCAL_MCP_URL.to_string()),
            metadata_carrier: "localhost HTTP request metadata plus session headers".to_string(),
            session_model: "sticky local HTTP session keyed by client/session/project".to_string(),
            notes: vec![
                "Use one localhost MCPace URL so the client never needs direct knowledge of upstream MCP servers.".to_string(),
                "Run one MCPace server on port 39022 and let every HTTP-capable local client point at that same endpoint.".to_string(),
            ],
        },
        _ => AdapterContractPreview {
            kind: "stdio-launcher".to_string(),
            command: Some(plan.launcher_command.clone()),
            args: vec![
                "mcp-server".to_string(),
                "--root".to_string(),
                sanitize_launcher_root_path(&plan.root_path),
                "--client-id".to_string(),
                target.id.to_string(),
            ],
            url_template: None,
            metadata_carrier:
                "MCP initialize params, roots, cwd, and optional _meta context hints".to_string(),
            session_model: "sticky stdio lease derived from external session id or planned fallback".to_string(),
            notes: vec![
                format!(
                    "{} should see one MCPace launcher entry instead of one config block per upstream MCP server.",
                    target.display_name
                ),
                "The live MCP server lets local clients connect to MCPace through one stable stdio command.".to_string(),
            ],
        },
    }
}

fn sanitize_launcher_root_path(root_path: &str) -> String {
    root_path
        .strip_prefix(r"\\?\")
        .unwrap_or(root_path)
        .to_string()
}

fn sanitize_path_for_display(path: &Path) -> String {
    sanitize_launcher_root_path(&path.display().to_string())
}

fn canonicalize_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn resolve_user_install_path(relative_dir: &str, file_name: &str) -> Result<PathBuf, String> {
    let home = user_home_dir().ok_or_else(|| {
        "failed to resolve the current user's home directory for user-scoped client config"
            .to_string()
    })?;
    let mut path = home;
    for segment in relative_dir.split(['/', '\\']) {
        if !segment.is_empty() {
            path.push(segment);
        }
    }
    path.push(file_name);
    Ok(path)
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn detect_newline(existing: &str) -> &'static str {
    if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn empty_json_object() -> JsonValue {
    JsonValue::object::<String, Vec<(String, JsonValue)>>(Vec::new())
}

fn build_codex_managed_block(adapter_key_name: &str, server_url: &str, newline: &str) -> String {
    let lines = vec![
        format!("# BEGIN MCPACE MANAGED BLOCK: {}", adapter_key_name),
        "# This block is managed by `mcpace client install codex`.".to_string(),
        format!("[mcp_servers.{}]", format_toml_table_key(adapter_key_name)),
        format!("url = {}", toml_basic_string(server_url)),
        "enabled = true".to_string(),
        "startup_timeout_sec = 20".to_string(),
        format!("# END MCPACE MANAGED BLOCK: {}", adapter_key_name),
        String::new(),
    ];
    lines.join(newline)
}

fn build_hermes_entry_block(adapter_key_name: &str, server_url: &str, newline: &str) -> String {
    let lines = vec![
        format!("  # BEGIN MCPACE MANAGED BLOCK: {}", adapter_key_name),
        format!("  {}:", adapter_key_name),
        format!("    url: {}", yaml_double_quoted_string(server_url)),
        "    enabled: true".to_string(),
        "    timeout: 120".to_string(),
        "    connect_timeout: 60".to_string(),
        format!("  # END MCPACE MANAGED BLOCK: {}", adapter_key_name),
        String::new(),
    ];
    lines.join(newline)
}

struct CodexConfigUpdate {
    contents: String,
    replaced_existing_block: bool,
}

fn upsert_json_mcp_server(
    existing: &str,
    adapter_key_name: &str,
    server_config: JsonValue,
    config_path: &Path,
) -> Result<CodexConfigUpdate, String> {
    let mut root = if existing.trim().is_empty() {
        JsonValue::object::<String, Vec<(String, JsonValue)>>(Vec::new())
    } else {
        parse_str(existing).map_err(|error| {
            format!(
                "failed to parse JSON client config '{}': {}",
                config_path.display(),
                error
            )
        })?
    };

    let root_object = match &mut root {
        JsonValue::Object(map) => map,
        _ => {
            return Err(format!(
                "JSON client config '{}' must be a top-level object",
                config_path.display()
            ))
        }
    };

    let servers_value = root_object
        .entry("mcpServers".to_string())
        .or_insert_with(empty_json_object);
    let servers_object = match servers_value {
        JsonValue::Object(map) => map,
        _ => {
            return Err(format!(
                "JSON client config '{}' has a non-object mcpServers field",
                config_path.display()
            ))
        }
    };

    let replaced_existing_block = servers_object.contains_key(adapter_key_name);
    servers_object.insert(adapter_key_name.to_string(), server_config);

    Ok(CodexConfigUpdate {
        contents: root.to_pretty_string(),
        replaced_existing_block,
    })
}

fn upsert_codex_managed_block(
    existing: &str,
    adapter_key_name: &str,
    managed_block: &str,
    config_path: &Path,
) -> Result<CodexConfigUpdate, String> {
    if let Some((start, end)) = find_managed_block(existing, adapter_key_name, config_path)? {
        let mut updated = String::new();
        updated.push_str(&existing[..start]);
        updated.push_str(managed_block);
        updated.push_str(&existing[end..]);
        return Ok(CodexConfigUpdate {
            contents: updated,
            replaced_existing_block: true,
        });
    }

    if let Some((start, end)) = find_codex_table_block(existing, adapter_key_name) {
        let mut updated = String::new();
        updated.push_str(&existing[..start]);
        updated.push_str(managed_block);
        updated.push_str(&existing[end..]);
        return Ok(CodexConfigUpdate {
            contents: updated,
            replaced_existing_block: true,
        });
    }

    let newline = detect_newline(existing);
    let mut updated = existing.to_string();
    if !updated.is_empty() {
        if !updated.ends_with('\n') {
            updated.push_str(newline);
        }
        if !updated.ends_with(&(newline.to_string() + newline)) {
            updated.push_str(newline);
        }
    }
    updated.push_str(managed_block);
    Ok(CodexConfigUpdate {
        contents: updated,
        replaced_existing_block: false,
    })
}

fn upsert_hermes_mcp_server(
    existing: &str,
    adapter_key_name: &str,
    server_url: &str,
    config_path: &Path,
) -> Result<CodexConfigUpdate, String> {
    let newline = detect_newline(existing);
    let entry_block = build_hermes_entry_block(adapter_key_name, server_url, newline);

    if let Some((start, end)) = find_managed_block(existing, adapter_key_name, config_path)? {
        let mut updated = String::new();
        updated.push_str(&existing[..start]);
        updated.push_str(&entry_block);
        updated.push_str(&existing[end..]);
        return Ok(CodexConfigUpdate {
            contents: updated,
            replaced_existing_block: true,
        });
    }

    if let Some((section_start, section_body_start, section_end)) =
        find_yaml_top_level_section(existing, "mcp_servers")
    {
        if let Some((entry_start, entry_end)) =
            find_yaml_section_entry(existing, section_body_start, section_end, adapter_key_name)
        {
            let mut updated = String::new();
            updated.push_str(&existing[..entry_start]);
            updated.push_str(&entry_block);
            updated.push_str(&existing[entry_end..]);
            return Ok(CodexConfigUpdate {
                contents: updated,
                replaced_existing_block: true,
            });
        }

        let mut updated = String::new();
        updated.push_str(&existing[..section_end]);
        if section_end > section_start && !existing[..section_end].ends_with('\n') {
            updated.push_str(newline);
        }
        updated.push_str(&entry_block);
        updated.push_str(&existing[section_end..]);
        return Ok(CodexConfigUpdate {
            contents: updated,
            replaced_existing_block: false,
        });
    }

    let mut updated = existing.to_string();
    if !updated.is_empty() {
        if !updated.ends_with('\n') {
            updated.push_str(newline);
        }
        if !updated.ends_with(&(newline.to_string() + newline)) {
            updated.push_str(newline);
        }
    }
    updated.push_str("mcp_servers:");
    updated.push_str(newline);
    updated.push_str(&entry_block);
    Ok(CodexConfigUpdate {
        contents: updated,
        replaced_existing_block: false,
    })
}

fn find_managed_block(
    existing: &str,
    adapter_key_name: &str,
    config_path: &Path,
) -> Result<Option<(usize, usize)>, String> {
    let begin_marker = format!("# BEGIN MCPACE MANAGED BLOCK: {}", adapter_key_name);
    let end_marker = format!("# END MCPACE MANAGED BLOCK: {}", adapter_key_name);
    let Some(marker_start) = existing.find(&begin_marker) else {
        return Ok(None);
    };
    let start = existing[..marker_start]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let Some(relative_end) = existing[marker_start..].find(&end_marker) else {
        return Err(format!(
            "Client config '{}' contains an MCPace begin marker without a matching end marker for '{}'",
            config_path.display(),
            adapter_key_name
        ));
    };

    let mut end = marker_start + relative_end + end_marker.len();
    if existing[end..].starts_with("\r\n") {
        end += 2;
    } else if existing[end..].starts_with('\n') {
        end += 1;
    }
    Ok(Some((start, end)))
}

fn find_codex_table_block(existing: &str, adapter_key_name: &str) -> Option<(usize, usize)> {
    let candidates = table_header_candidates(adapter_key_name);
    let mut start = None;
    let mut offset = 0usize;

    for line in existing.split_inclusive('\n') {
        let trimmed = trim_toml_line(line);
        if start.is_none() {
            if candidates.iter().any(|candidate| trimmed == candidate) {
                start = Some(offset);
            }
        } else if looks_like_toml_table_header(trimmed) {
            return Some((start.unwrap_or_default(), offset));
        }
        offset += line.len();
    }

    start.map(|value| (value, existing.len()))
}

fn table_header_candidates(adapter_key_name: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    if is_bare_toml_key(adapter_key_name) {
        candidates.push(format!("[mcp_servers.{}]", adapter_key_name));
    }
    candidates.push(format!(
        "[mcp_servers.{}]",
        toml_basic_string(adapter_key_name)
    ));
    candidates
}

fn trim_toml_line(line: &str) -> &str {
    let trimmed = line.trim();
    match trimmed.find('#') {
        Some(index) => trimmed[..index].trim_end(),
        None => trimmed,
    }
}

fn looks_like_toml_table_header(trimmed_line: &str) -> bool {
    trimmed_line.starts_with('[') && trimmed_line.ends_with(']')
}

fn find_yaml_top_level_section(existing: &str, key: &str) -> Option<(usize, usize, usize)> {
    let mut start = None;
    let mut body_start = 0usize;
    let mut offset = 0usize;

    for line in existing.split_inclusive('\n') {
        if let Some((indent, line_key)) = parse_yaml_mapping_key(line) {
            if start.is_none() {
                if indent == 0 && line_key == key {
                    start = Some(offset);
                    body_start = offset + line.len();
                }
            } else if indent == 0 {
                return Some((start.unwrap_or_default(), body_start, offset));
            }
        }
        offset += line.len();
    }

    start.map(|value| (value, body_start, existing.len()))
}

fn find_yaml_section_entry(
    existing: &str,
    section_body_start: usize,
    section_end: usize,
    adapter_key_name: &str,
) -> Option<(usize, usize)> {
    let section = &existing[section_body_start..section_end];
    let mut start = None;
    let mut entry_indent = 0usize;
    let mut offset = 0usize;

    for line in section.split_inclusive('\n') {
        if let Some((indent, line_key)) = parse_yaml_mapping_key(line) {
            if start.is_none() {
                if indent > 0 && line_key == adapter_key_name {
                    start = Some(section_body_start + offset);
                    entry_indent = indent;
                }
            } else if indent == entry_indent {
                return Some((start.unwrap_or_default(), section_body_start + offset));
            }
        }
        offset += line.len();
    }

    start.map(|value| (value, section_end))
}

fn parse_yaml_mapping_key(line: &str) -> Option<(usize, String)> {
    let without_newline = line.trim_end_matches(['\r', '\n']);
    if without_newline.trim().is_empty() {
        return None;
    }
    let indent = without_newline.len() - without_newline.trim_start().len();
    let content = without_newline.trim_start();
    if content.starts_with('#') || content.starts_with('-') {
        return None;
    }
    let colon_index = content.find(':')?;
    let key = content[..colon_index].trim();
    if key.is_empty() {
        return None;
    }
    Some((indent, trim_yaml_key_quotes(key).to_string()))
}

fn trim_yaml_key_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|trimmed| trimmed.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|trimmed| trimmed.strip_suffix('\''))
        })
        .unwrap_or(value)
}

fn format_toml_table_key(value: &str) -> String {
    if is_bare_toml_key(value) {
        value.to_string()
    } else {
        toml_basic_string(value)
    }
}

fn is_bare_toml_key(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

fn toml_basic_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

fn yaml_double_quoted_string(value: &str) -> String {
    toml_basic_string(value)
}

fn install_warnings_from_plan(plan: &super::model::ClientPlan) -> Vec<String> {
    let mut warnings = plan
        .warnings
        .iter()
        .filter(|warning| {
            warning.contains("At least one routed server uses stdio")
                || warning.contains("Client surface")
                || warning.contains("public HTTP")
                || warning.contains("cannot consume MCPace")
        })
        .cloned()
        .collect::<Vec<_>>();
    warnings.sort();
    warnings.dedup();
    warnings
}

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join("; ")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
