use super::args::ParsedArgs;
use super::context::resolve_context;
use super::metadata::load_metadata;
use super::plan::build_plan;
use super::render::{count_static, join_count_map, join_static_or_none, write_text_plan};
use crate::client_catalog::{ClientTarget, CLIENT_TARGETS};
use crate::doctor;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::server;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

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

    let context = resolve_context(&parsed, &metadata);
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
    let context = resolve_context(&parsed, &metadata);
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

fn read_client_key_name(root_path: &Path) -> Option<String> {
    let config_path = root_path.join("mcpace.config.json");
    let config = json_helpers::read_json_file(&config_path).ok()?;
    json_helpers::string_at_path(&config, &["client", "keyName"])
        .map(|value| value.trim().to_string())
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

impl ClientExportPreview {
    fn from_plan(target: &ClientTarget, plan: &super::model::ClientPlan) -> Self {
        let adapter_key_name = plan
            .configured_client_key_name
            .clone()
            .unwrap_or_else(|| format!("{}-adapter", target.family_id));
        let export_mode = resolve_export_mode(target, plan);

        let mut blockers = match export_mode.as_str() {
            "local-stdio-launcher" => vec![
                "The live stdio forwarding path is not implemented yet in this repo; the current stdio-shim is bootstrap-only, so MCPace cannot honestly emit a connectable local launcher config block today.".to_string(),
            ],
            "local-streamable-http" => vec![
                "The grouped local Streamable HTTP ingress is not implemented yet in this repo, so MCPace cannot honestly emit a live localhost MCP URL today.".to_string(),
            ],
            "public-http-connector" => vec![
                "This client surface needs a public HTTP MCP endpoint or relay, and MCPace does not ship that lane yet in this repo.".to_string(),
            ],
            _ => vec![
                "MCPace does not yet have a verified ingress lane for this client surface, so export stays preview-only.".to_string(),
            ],
        };
        blockers.extend(plan.warnings.iter().cloned());
        blockers.sort();
        blockers.dedup();

        let next_actions = match export_mode.as_str() {
            "local-stdio-launcher" => vec![
                "Promote bootstrap-only mcpace stdio-shim into live MCP stdio forwarding so local clients can launch one stable MCPace adapter process.".to_string(),
                "Promote client export from preview-only to a real config patcher once the shim is verified.".to_string(),
            ],
            "local-streamable-http" => vec![
                "Ship local Streamable HTTP ingress with session handling and localhost-only defaults.".to_string(),
                "Promote client export from preview-only to a real config patcher once the HTTP lane is verified.".to_string(),
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
            mode: "preview-only".to_string(),
            can_connect_today: false,
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
        map.insert("writesConfig".to_string(), JsonValue::bool(self.writes_config));
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
        let _ = writeln!(stdout, "Client export preview only");
        let _ = writeln!(stdout, "Client target: {} ({})", self.client_target_id, self.display_name);
        let _ = writeln!(stdout, "Adapter key: {}", self.adapter_key_name);
        let _ = writeln!(stdout, "Config format: {}", self.config_format);
        let _ = writeln!(stdout, "Config paths: {}", join_or_none(&self.config_paths));
        let _ = writeln!(stdout, "Config precedence: {}", join_or_none(&self.config_precedence));
        let _ = writeln!(stdout, "Native scopes: {}", join_or_none(&self.native_scopes));
        let _ = writeln!(stdout, "Preferred ingress: {}", self.preferred_ingress);
        let _ = writeln!(stdout, "Export mode: {}", self.export_mode);
        let _ = writeln!(stdout, "Entrypoint mode: {}", self.entrypoint_mode);
        let _ = writeln!(stdout, "Launcher command: {}", self.launcher_command);
        let _ = writeln!(stdout, "Can connect today: {}", yes_no(self.can_connect_today));
        let _ = writeln!(stdout, "Writes config: {}", yes_no(self.writes_config));
        self.adapter_contract.write_text(stdout);
        let _ = writeln!(stdout, "Blockers: {}", join_or_none(&self.blockers));
        let _ = writeln!(stdout, "Warnings: {}", join_or_none(&self.warnings));
        let _ = writeln!(stdout, "Next actions: {}", join_or_none(&self.next_actions));
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
    if plan.preferred_ingress == "streamable-http" {
        return "local-streamable-http".to_string();
    }
    "local-stdio-launcher".to_string()
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
            url_template: Some("http://127.0.0.1:<mcpace-port>/mcp".to_string()),
            metadata_carrier: "localhost HTTP request metadata plus session headers".to_string(),
            session_model: "sticky local HTTP session keyed by client/session/project".to_string(),
            notes: vec![
                "Use one localhost MCPace URL so the client never needs direct knowledge of upstream MCP servers.".to_string(),
                "Origin checks and localhost-only binding belong to the future HTTP ingress, not the client patcher.".to_string(),
            ],
        },
        _ => AdapterContractPreview {
            kind: "stdio-launcher".to_string(),
            command: Some(plan.launcher_command.clone()),
            args: vec!["stdio-shim".to_string()],
            url_template: None,
            metadata_carrier:
                "MCP initialize params, roots, cwd, and optional _meta context hints".to_string(),
            session_model: "sticky stdio lease derived from external session id or planned fallback".to_string(),
            notes: vec![
                format!(
                    "{} should see one MCPace launcher entry instead of one config block per upstream MCP server.",
                    target.display_name
                ),
                "The future stdio shim should reuse planner context resolution so clients do not need per-surface routing logic.".to_string(),
            ],
        },
    }
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
