use super::http_tool_names;
use super::overview::{run_json_commands_parallel, runtime_status_json, take_parallel_result};
use super::DashboardConfig;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::upstream;
use std::fmt;
use std::thread;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DashboardDiagnosticsError {
    message: String,
}

impl fmt::Display for DashboardDiagnosticsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DashboardDiagnosticsError {}

impl From<String> for DashboardDiagnosticsError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<DashboardDiagnosticsError> for String {
    fn from(error: DashboardDiagnosticsError) -> Self {
        error.to_string()
    }
}

type DashboardDiagnosticsResult<T> = Result<T, DashboardDiagnosticsError>;

pub(super) fn runtime_diagnostics(
    config: &DashboardConfig,
) -> DashboardDiagnosticsResult<JsonValue> {
    let root_path = &config.root_path;
    let inventory_root = root_path.to_path_buf();
    let inventory_handle = thread::spawn(move || upstream::configured_inventory(&inventory_root));
    let mut command_results = run_json_commands_parallel(
        root_path,
        vec![
            ("doctor", vec!["doctor", "--json"]),
            ("hub", vec!["hub", "status", "--json"]),
            (
                "serverCapabilities",
                vec!["server", "capabilities", "--json"],
            ),
        ],
    )?;
    let doctor = take_parallel_result(&mut command_results, "doctor")?;
    let hub_status = take_parallel_result(&mut command_results, "hub")?;
    let server_capabilities = take_parallel_result(&mut command_results, "serverCapabilities")?;
    let upstream_inventory = match inventory_handle.join() {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => JsonValue::object([
            ("ok", JsonValue::bool(false)),
            ("error", JsonValue::string(error)),
            ("servers", JsonValue::array([])),
        ]),
        Err(_) => JsonValue::object([
            ("ok", JsonValue::bool(false)),
            (
                "error",
                JsonValue::string("upstream inventory worker panicked"),
            ),
            ("servers", JsonValue::array([])),
        ]),
    };
    let server_items = server_capabilities.as_array().unwrap_or(&[]);
    let server_diagnostics = server_items
        .iter()
        .map(server_runtime_diagnostic)
        .collect::<Vec<_>>();
    let effective_enabled_count = server_items
        .iter()
        .filter(|server| json_helpers::bool_at_path(server, &["effectiveEnabled"]).unwrap_or(false))
        .count();
    let exposed_tools = http_tool_names();

    Ok(JsonValue::object([
        ("ok", JsonValue::bool(true)),
        (
            "surface",
            JsonValue::string("mcpace-management-http-mcp"),
        ),
        (
            "summary",
            JsonValue::string(
                "MCPace HTTP MCP is reachable. This build exposes management tools plus dynamic adapter_profile/upstream_search and explicit stdio/plain HTTP upstream access through surface_manifest/upstream_catalog/upstream_probe/upstream_policy_audit/upstream_policy_suggest/upstream_tools/upstream_call/upstream_batch; in auto/native exposure mode, upstream tools may also be advertised as projected u_<server>_<tool>_<hash> names when the live catalog fits the token budget.",
            ),
        ),
        ("doctor", doctor),
        ("hub", hub_status),
        ("runtime", runtime_status_json(config)),
        (
            "upstreamForwarding",
            JsonValue::object([
                ("implemented", JsonValue::bool(true)),
                ("stdioBridgeImplemented", JsonValue::bool(true)),
                (
                    "callableConfiguredStdioServerCount",
                    json_helpers::value_at_path(
                        &upstream_inventory,
                        &["callableConfiguredStdioServerCount"],
                    )
                    .cloned()
                    .unwrap_or_else(|| JsonValue::number(0)),
                ),
                (
                    "reason",
                    JsonValue::string(
                        "MCPace forwards resolvable configured stdio and Streamable HTTP/HTTPS upstreams through upstream_tools/upstream_call and uses upstream_batch for same-server stateful sequences. HTTPS uses platform certificate verification and configured authentication headers; plain HTTP is loopback-only. upstream_catalog lists concise tool descriptions, upstream_probe checks configured servers, upstream_policy_audit compares MCP annotations with declarative toolPolicies, and upstream_policy_suggest generates reviewable policy candidates without hardcoded server names. Legacy HTTP+SSE and custom transports remain explicit blocked diagnostics until bridged or upgraded.",
                    ),
                ),
            ]),
        ),
        (
            "surfaceContract",
            JsonValue::object([
                (
                    "nativeTopLevelClaim",
                    JsonValue::string(
                        "tools/list returns adapter management tools plus budgeted projected upstream tools when MCPACE_TOOL_EXPOSURE allows them.",
                    ),
                ),
                (
                    "upstreamProjection",
                    JsonValue::string(
                        "Configured upstream tools can be exposed as stable u_<server>_<tool>_<hash> names when the live catalog fits the token budget; broker discovery remains available through upstream_search/upstream_catalog/upstream_tools.",
                    ),
                ),
                ("directTopLevelProjectionEnabled", JsonValue::bool(true)),
            ]),
        ),
        ("upstreamInventory", upstream_inventory),
        (
            "managementTools",
            JsonValue::object([
                ("count", JsonValue::number(exposed_tools.len())),
                (
                    "names",
                    JsonValue::array(exposed_tools.into_iter().map(JsonValue::string)),
                ),
            ]),
        ),
        (
            "configuredServers",
            JsonValue::object([
                ("count", JsonValue::number(server_items.len())),
                ("effectiveEnabledCount", JsonValue::number(effective_enabled_count)),
                ("items", JsonValue::array(server_diagnostics)),
            ]),
        ),
        (
            "nextSafeAction",
            JsonValue::string(
                "Use adapter_profile to see whether the current tools/list projected upstream tools natively or fell back to broker mode. Use upstream_search/upstream_catalog/upstream_tools for discovery, upstream_call for stateless calls, and upstream_batch for stateful same-server sequences.",
            ),
        ),
    ]))
}

fn server_runtime_diagnostic(server: &JsonValue) -> JsonValue {
    let name = json_helpers::string_at_path(server, &["name"]).unwrap_or("unknown");
    let kind = json_helpers::string_at_path(server, &["kind"]).unwrap_or("unknown");
    let source_type = json_helpers::string_at_path(server, &["sourceType"]).unwrap_or("");
    let effective_enabled =
        json_helpers::bool_at_path(server, &["effectiveEnabled"]).unwrap_or(false);
    let required = json_helpers::bool_at_path(server, &["required"]).unwrap_or(false);
    let auto_start = json_helpers::bool_at_path(server, &["autoStart"]).unwrap_or(false);
    let source_url = json_helpers::string_at_path(server, &["sourceUrl"]).unwrap_or("");
    let source_command = json_helpers::string_at_path(server, &["sourceCommand"])
        .unwrap_or("")
        .trim();
    let runtime_callable = effective_enabled
        && ((source_type == "stdio" && !source_command.is_empty())
            || (source_type == "http"
                && crate::upstream::http_upstream_url_is_callable(source_url)));
    let (status, reason) = if !effective_enabled {
        (
            "disabled",
            "server is disabled by source/profile/default configuration",
        )
    } else if source_type == "http" && runtime_callable {
        (
            "callable-http-bridge",
            "enabled Streamable HTTP/HTTPS upstream can be listed with upstream_tools and called with upstream_call; HTTPS uses platform certificate verification and plain HTTP is loopback-only",
        )
    } else if source_type == "stdio" && runtime_callable {
        (
            "callable-stdio-bridge",
            "enabled stdio upstream can be listed with upstream_tools and called with upstream_call",
        )
    } else if kind == "host-bridge" {
        (
            "blocked-preview-host-bridge",
            "host-bridge policy is configured, but MCPace does not currently launch or proxy non-stdio bridges through this HTTP adapter",
        )
    } else if kind == "container-stdio" {
        (
            "blocked-nonstdio-or-missing-command",
            "this entry is not currently callable through the stdio bridge; check upstreamInventory for command/source details",
        )
    } else if source_type == "http" {
        (
            "blocked-http-upstream",
            "HTTP upstream is configured but not callable; verify the URL, required headers, and loopback-only rule for plain HTTP",
        )
    } else if kind == "external-http" || kind == "remote-http" {
        (
            "blocked-preview-http-upstream",
            "external/remote HTTP policy metadata has no callable source URL; add a Streamable HTTPS endpoint or a stdio adapter",
        )
    } else {
        (
            "blocked-preview-unknown-upstream",
            "this upstream kind is configured as inventory only and is not exposed as a callable MCP tool by this HTTP adapter",
        )
    };

    JsonValue::object([
        ("name", JsonValue::string(name)),
        ("kind", JsonValue::string(kind)),
        ("effectiveEnabled", JsonValue::bool(effective_enabled)),
        ("required", JsonValue::bool(required)),
        ("autoStart", JsonValue::bool(auto_start)),
        ("runtimeCallable", JsonValue::bool(runtime_callable)),
        ("exposedAsMcpTool", JsonValue::bool(false)),
        ("status", JsonValue::string(status)),
        ("reason", JsonValue::string(reason)),
        ("sourceType", JsonValue::string(source_type)),
        (
            "healthUrl",
            JsonValue::string(json_helpers::string_at_path(server, &["healthUrl"]).unwrap_or("")),
        ),
        (
            "requiredCommands",
            json_helpers::value_at_path(server, &["requiredCommands"])
                .cloned()
                .unwrap_or_else(|| JsonValue::array([])),
        ),
    ])
}
