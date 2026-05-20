use super::args::ParsedArgs;
use crate::client_catalog::{self, ClientTargetRecord};
use crate::json::JsonValue;
use crate::mcp_sources;
use crate::runtimepaths::{self, ServeEndpoint};
use crate::server::ServerRecord;
use crate::{server, verify};
use std::path::Path;

#[derive(Clone, Debug)]
pub(super) struct ConnectReport {
    pub(super) root_path: String,
    pub(super) endpoint: ConnectEndpoint,
    pub(super) selected_client: Option<ConnectClient>,
    pub(super) upstream: ConnectUpstreamSummary,
    pub(super) readiness: ConnectReadiness,
    pub(super) next_steps: Vec<String>,
    pub(super) blockers: Vec<String>,
    pub(super) warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct ConnectEndpoint {
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) mcp_path: String,
    pub(super) health_path: String,
    pub(super) local_mcp_url: String,
    pub(super) advertised_mcp_url: String,
    pub(super) health_url: String,
    pub(super) public_url_configured: bool,
}

#[derive(Clone, Debug)]
pub(super) struct ConnectClient {
    pub(super) id: String,
    pub(super) display_name: String,
    pub(super) surface_class: String,
    pub(super) proof_tier: String,
    pub(super) supports_local_http: bool,
    pub(super) supports_install: bool,
    pub(super) install_scope: Option<String>,
    pub(super) install_path: Option<String>,
    pub(super) selection_reason: String,
}

#[derive(Clone, Debug)]
pub(super) struct ConnectUpstreamSummary {
    pub(super) configured_count: usize,
    pub(super) source_count: usize,
    pub(super) effective_enabled_count: usize,
    pub(super) stdio_count: usize,
    pub(super) http_inventory_count: usize,
    pub(super) selected_server: Option<String>,
    pub(super) names: Vec<String>,
    pub(super) source_paths: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct ConnectReadiness {
    pub(super) read_only_ready: bool,
    pub(super) runtime_ready: bool,
    pub(super) missing_runtime_prerequisites: Vec<String>,
    pub(super) missing_required_commands: Vec<String>,
    pub(super) missing_profile_commands: Vec<String>,
}

pub(super) fn build_report(root_path: &Path, parsed: &ParsedArgs) -> Result<ConnectReport, String> {
    let endpoint =
        ConnectEndpoint::from_endpoint(runtimepaths::resolve_serve_endpoint(Some(root_path)));
    let source_report = mcp_sources::load_mcp_source_report(root_path)?;
    let server_records = server::load_server_records(root_path)?;
    let client_registry = client_catalog::load_registry(Some(root_path))?;
    let readiness_report = verify::collect_readiness(root_path)?;

    let selected_client = select_client(&client_registry.targets, parsed.client_id.as_deref());
    let upstream = summarize_upstreams(
        &source_report,
        &server_records,
        parsed.server_name.as_deref(),
    );
    let readiness = ConnectReadiness {
        read_only_ready: readiness_report.ready_for_read_only_ops,
        runtime_ready: readiness_report.ready_for_runtime_ops,
        missing_runtime_prerequisites: readiness_report.missing_runtime_prerequisites,
        missing_required_commands: readiness_report.missing_required_commands,
        missing_profile_commands: readiness_report.missing_profile_commands,
    };

    let mut warnings = source_report.registry.warnings.clone();
    warnings.extend(client_registry.warnings.clone());
    warnings.sort();
    warnings.dedup();

    let blockers = collect_blockers(&selected_client, &upstream, &readiness);
    let next_steps = build_next_steps(root_path, &selected_client, &upstream, &readiness);

    Ok(ConnectReport {
        root_path: root_path.display().to_string(),
        endpoint,
        selected_client,
        upstream,
        readiness,
        next_steps,
        blockers,
        warnings,
    })
}

impl ConnectEndpoint {
    fn from_endpoint(endpoint: ServeEndpoint) -> Self {
        let local_mcp_url = endpoint.bind_mcp_url();
        let advertised_mcp_url = endpoint.mcp_url();
        let health_url = endpoint.health_url();
        Self {
            public_url_configured: endpoint.public_mcp_url.is_some(),
            host: endpoint.host,
            port: endpoint.port,
            mcp_path: endpoint.mcp_path,
            health_path: endpoint.health_path,
            local_mcp_url,
            advertised_mcp_url,
            health_url,
        }
    }
}

impl ConnectReport {
    pub(super) fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("schema", JsonValue::string("mcpace.connectReport.v1")),
            ("rootPath", JsonValue::string(self.root_path.clone())),
            ("endpoint", self.endpoint.to_json_value()),
            (
                "selectedClient",
                self.selected_client
                    .as_ref()
                    .map(ConnectClient::to_json_value)
                    .unwrap_or(JsonValue::Null),
            ),
            ("upstream", self.upstream.to_json_value()),
            ("readiness", self.readiness.to_json_value()),
            (
                "nextSteps",
                JsonValue::array(self.next_steps.iter().cloned().map(JsonValue::string)),
            ),
            (
                "blockers",
                JsonValue::array(self.blockers.iter().cloned().map(JsonValue::string)),
            ),
            (
                "warnings",
                JsonValue::array(self.warnings.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

impl ConnectEndpoint {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("host", JsonValue::string(self.host.clone())),
            ("port", JsonValue::number(self.port)),
            ("mcpPath", JsonValue::string(self.mcp_path.clone())),
            ("healthPath", JsonValue::string(self.health_path.clone())),
            ("localMcpUrl", JsonValue::string(self.local_mcp_url.clone())),
            (
                "advertisedMcpUrl",
                JsonValue::string(self.advertised_mcp_url.clone()),
            ),
            ("healthUrl", JsonValue::string(self.health_url.clone())),
            (
                "publicUrlConfigured",
                JsonValue::bool(self.public_url_configured),
            ),
        ])
    }
}

impl ConnectClient {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("id", JsonValue::string(self.id.clone())),
            ("displayName", JsonValue::string(self.display_name.clone())),
            (
                "surfaceClass",
                JsonValue::string(self.surface_class.clone()),
            ),
            ("proofTier", JsonValue::string(self.proof_tier.clone())),
            (
                "supportsLocalHttp",
                JsonValue::bool(self.supports_local_http),
            ),
            ("supportsInstall", JsonValue::bool(self.supports_install)),
            (
                "installScope",
                self.install_scope
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "installPath",
                self.install_path
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "selectionReason",
                JsonValue::string(self.selection_reason.clone()),
            ),
        ])
    }
}

impl ConnectUpstreamSummary {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("configuredCount", JsonValue::number(self.configured_count)),
            ("sourceCount", JsonValue::number(self.source_count)),
            (
                "effectiveEnabledCount",
                JsonValue::number(self.effective_enabled_count),
            ),
            ("stdioCount", JsonValue::number(self.stdio_count)),
            (
                "httpInventoryCount",
                JsonValue::number(self.http_inventory_count),
            ),
            (
                "selectedServer",
                self.selected_server
                    .as_ref()
                    .map(|value| JsonValue::string(value.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "names",
                JsonValue::array(self.names.iter().cloned().map(JsonValue::string)),
            ),
            (
                "sourcePaths",
                JsonValue::array(self.source_paths.iter().cloned().map(JsonValue::string)),
            ),
        ])
    }
}

impl ConnectReadiness {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("readOnlyReady", JsonValue::bool(self.read_only_ready)),
            ("runtimeReady", JsonValue::bool(self.runtime_ready)),
            (
                "missingRuntimePrerequisites",
                JsonValue::array(
                    self.missing_runtime_prerequisites
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "missingRequiredCommands",
                JsonValue::array(
                    self.missing_required_commands
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
            (
                "missingProfileCommands",
                JsonValue::array(
                    self.missing_profile_commands
                        .iter()
                        .cloned()
                        .map(JsonValue::string),
                ),
            ),
        ])
    }
}

fn select_client(targets: &[ClientTargetRecord], requested: Option<&str>) -> Option<ConnectClient> {
    if let Some(requested) = requested {
        return client_catalog::find_in_targets(targets, requested).map(|target| {
            connect_client_from_target(target, format!("requested by --client {}", requested))
        });
    }

    targets
        .iter()
        .max_by_key(|target| client_score(target))
        .map(|target| {
            connect_client_from_target(
                target,
                "best local installable HTTP-capable target".to_string(),
            )
        })
}

fn client_score(target: &ClientTargetRecord) -> i32 {
    let mut score = 0;
    if client_catalog::normalize(&target.surface_class) == "local" {
        score += 1000;
    }
    if target.supports_ingress("streamable-http") {
        score += 300;
    }
    if target.supports_client_install() {
        score += 200;
    }
    if client_catalog::normalize(target.proof_tier()) == "tier-1" {
        score += 100;
    }
    if target.has_feature("tools") {
        score += 20;
    }
    if target.has_constraint("public-http-only") {
        score -= 500;
    }
    score
}

fn connect_client_from_target(
    target: &ClientTargetRecord,
    selection_reason: String,
) -> ConnectClient {
    ConnectClient {
        id: target.id.clone(),
        display_name: target.display_name.clone(),
        surface_class: target.surface_class.clone(),
        proof_tier: target.proof_tier.clone(),
        supports_local_http: target.supports_ingress("streamable-http")
            && !target.has_constraint("public-http-only"),
        supports_install: target.supports_client_install(),
        install_scope: target.preferred_install_scope().map(str::to_string),
        install_path: target.preferred_install_config_path().map(str::to_string),
        selection_reason,
    }
}

fn summarize_upstreams(
    source_report: &mcp_sources::McpSourceReport,
    server_records: &[ServerRecord],
    requested_server: Option<&str>,
) -> ConnectUpstreamSummary {
    let mut names = source_report
        .registry
        .servers
        .values()
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup();

    let effective_enabled_count = server_records
        .iter()
        .filter(|record| record.effective_enabled)
        .count();
    let stdio_count = server_records
        .iter()
        .filter(|record| record.source_enabled && record.source_type == "stdio")
        .count();
    let http_inventory_count = server_records
        .iter()
        .filter(|record| record.source_enabled && record.source_type == "http")
        .count();

    let selected_server = requested_server
        .and_then(|name| find_server_name(server_records, name))
        .or_else(|| {
            server_records
                .iter()
                .find(|record| record.source_enabled && record.source_type == "stdio")
                .map(|record| record.name.clone())
        })
        .or_else(|| {
            server_records
                .iter()
                .find(|record| record.source_enabled)
                .map(|record| record.name.clone())
        });

    ConnectUpstreamSummary {
        configured_count: source_report.registry.servers.len(),
        source_count: source_report.registry.sources.len(),
        effective_enabled_count,
        stdio_count,
        http_inventory_count,
        selected_server,
        names,
        source_paths: source_report.registry.sources.clone(),
    }
}

fn find_server_name(server_records: &[ServerRecord], requested: &str) -> Option<String> {
    let requested = requested.trim().to_ascii_lowercase();
    server_records
        .iter()
        .find(|record| record.name.to_ascii_lowercase() == requested)
        .map(|record| record.name.clone())
}

fn collect_blockers(
    selected_client: &Option<ConnectClient>,
    upstream: &ConnectUpstreamSummary,
    readiness: &ConnectReadiness,
) -> Vec<String> {
    let mut blockers = Vec::new();
    if selected_client.is_none() {
        blockers.push("No client target could be selected; run 'mcpace client list'.".to_string());
    }
    if upstream.configured_count == 0 {
        blockers.push("No upstream MCP servers are configured yet.".to_string());
    }
    if upstream.stdio_count == 0 && upstream.http_inventory_count > 0 {
        blockers.push("Only remote HTTP MCP entries are configured; MCPace currently inventories them but does not forward remote HTTP upstreams yet.".to_string());
    }
    if !readiness.missing_runtime_prerequisites.is_empty() {
        blockers.push(format!(
            "Missing runtime prerequisites: {}.",
            readiness.missing_runtime_prerequisites.join(", ")
        ));
    }
    let mut missing_commands = readiness.missing_required_commands.clone();
    missing_commands.extend(readiness.missing_profile_commands.clone());
    missing_commands.sort();
    missing_commands.dedup();
    if !missing_commands.is_empty() {
        blockers.push(format!(
            "Missing upstream commands: {}.",
            missing_commands.join(", ")
        ));
    }
    blockers
}

fn build_next_steps(
    root_path: &Path,
    selected_client: &Option<ConnectClient>,
    upstream: &ConnectUpstreamSummary,
    readiness: &ConnectReadiness,
) -> Vec<String> {
    let root = root_path.display();
    let mut steps = Vec::new();

    if upstream.configured_count == 0 {
        steps.push("Run the home setup without adding upstream servers: mcpace up  (then add a server only when you choose: mcpace install <package|url|command> --dry-run)".to_string());
        steps.push("For local filesystem or repo tools, add them explicitly with scoped paths and then probe before wiring more clients.".to_string());
        steps.push("Or import an existing MCP client config: mcpace server import --from <mcp-settings.json> --dry-run".to_string());
        steps.push(
            "For a fully custom server, use: mcpace server add <name> --command <cmd> --arg <arg>"
                .to_string(),
        );
        steps.push("Then run: mcpace server sources --json".to_string());
    }

    if let Some(server) = &upstream.selected_server {
        steps.push(format!(
            "Smoke the selected upstream: mcpace server test {} --refresh --root {}",
            shell_token(server),
            shell_token(&root.to_string())
        ));
    } else if upstream.configured_count > 0 {
        steps.push(format!(
            "Smoke all callable stdio upstreams: mcpace server test --refresh --root {}",
            shell_token(&root.to_string())
        ));
    }

    steps.push(format!(
        "Start the local MCPace endpoint: mcpace serve --root {}",
        shell_token(&root.to_string())
    ));

    if let Some(client) = selected_client {
        steps.push(format!(
            "Inspect the client contract: mcpace client export {} --root {}",
            shell_token(&client.id),
            shell_token(&root.to_string())
        ));
        if client.supports_install {
            steps.push(format!(
                "Preview the client config patch: mcpace client install {} --dry-run --diff --root {}",
                shell_token(&client.id),
                shell_token(&root.to_string())
            ));
        }
    } else {
        steps.push("Choose a client target: mcpace client list".to_string());
    }

    if !readiness.runtime_ready {
        steps.push(format!(
            "Check blockers: mcpace verify readiness --json --root {}",
            shell_token(&root.to_string())
        ));
    }

    steps
}

fn shell_token(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '\\'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
