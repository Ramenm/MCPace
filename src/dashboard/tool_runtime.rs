use super::{
    empty_object, http_boundary, http_tool_names, run_json_command, run_json_command_vec,
    runtime_diagnostics, DashboardConfig, HttpRequest,
};
use crate::adapter;
use crate::json::JsonValue;
use crate::json_helpers;
use crate::upstream;
use std::collections::BTreeSet;

pub(super) fn run_http_tool(
    config: &DashboardConfig,
    name: &str,
    args: &JsonValue,
    request: Option<&HttpRequest>,
) -> Result<JsonValue, String> {
    let root_path = &config.root_path;
    if name.starts_with("u_") {
        let reserved = http_tool_names().into_iter().collect::<BTreeSet<_>>();
        let target = adapter::resolve_projected_tool(
            root_path,
            name,
            &reserved,
            &adapter::ToolExposureOptions::for_call_resolution(),
        )?;
        if let Some(target) = target {
            let control_arguments = adapter::projected_adapter_control_arguments(args);
            let context = http_upstream_lease_context(&control_arguments, request)?;
            let upstream_arguments = adapter::strip_projected_adapter_arguments(args);
            let timeout_ms = json_helpers::value_at_path(&control_arguments, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            return upstream::call_tool_with_pooled_context(
                root_path,
                &target.server,
                &target.tool,
                &upstream_arguments,
                timeout_ms,
                Some(&context),
                &config.upstream_session_pool,
            );
        }
    }
    match name {
        "doctor" => run_json_command(root_path, &["doctor", "--json"]),
        "hub_status" => run_json_command(root_path, &["hub", "status", "--json"]),
        "hub_up" => run_json_command(root_path, &["hub", "up", "--json"]),
        "hub_down" => run_json_command(root_path, &["hub", "down", "--json"]),
        "hub_repair" => run_json_command(root_path, &["hub", "repair", "--json"]),
        "hub_logs" => {
            let tail = json_helpers::value_at_path(args, &["tail"])
                .and_then(JsonValue::as_i64)
                .unwrap_or(20);
            run_json_command_vec(
                root_path,
                vec![
                    "hub".to_string(),
                    "logs".to_string(),
                    "--json".to_string(),
                    "--tail".to_string(),
                    tail.to_string(),
                ],
            )
        }
        "runtime_leases" => run_json_command(root_path, &["hub", "lease", "list", "--json"]),
        "server_list" => run_json_command(root_path, &["server", "list", "--json"]),
        "server_capabilities" => {
            let name = required_http_string(args, "name")?;
            run_json_command_vec(
                root_path,
                vec![
                    "server".to_string(),
                    "capabilities".to_string(),
                    "--json".to_string(),
                    "--name".to_string(),
                    name,
                ],
            )
        }
        "runtime_diagnostics" => runtime_diagnostics(config).map_err(String::from),
        "adapter_profile" => {
            let include_live_catalog =
                json_helpers::bool_at_path(args, &["includeLiveCatalog"]).unwrap_or(false);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            let context = http_upstream_lease_context(args, request)?;
            let visible_tools = adapter::visible_tool_names(&http_tool_names(), context.metadata.as_ref());
            adapter::adapter_profile(
                root_path,
                context.metadata.as_ref(),
                context.transport.as_deref().unwrap_or("streamable-http"),
                &visible_tools,
                include_live_catalog,
                timeout_ms,
                refresh,
            )
            .map_err(String::from)
        }
        "adapter_route" => {
            let include_live_catalog =
                json_helpers::bool_at_path(args, &["includeLiveCatalog"]).unwrap_or(false);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            let calls = json_helpers::value_at_path(args, &["calls"]);
            adapter::adapter_route_plan(root_path, calls, include_live_catalog, timeout_ms, refresh)
        }
        "upstream_search" => {
            let server = json_helpers::string_at_path(args, &["server"]);
            let query = json_helpers::string_at_path(args, &["query"]);
            let limit = json_helpers::value_at_path(args, &["limit"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as usize)
                .unwrap_or(20);
            let include_schema =
                json_helpers::bool_at_path(args, &["includeSchema"]).unwrap_or(false);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            adapter::upstream_search(
                root_path,
                server,
                query,
                limit,
                include_schema,
                timeout_ms,
                refresh,
            )
        }
        "surface_manifest" => {
            let include_live_catalog =
                json_helpers::bool_at_path(args, &["includeLiveCatalog"]).unwrap_or(false);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            upstream::surface_manifest(
                root_path,
                "streamable-http",
                http_tool_names(),
                include_live_catalog,
                timeout_ms,
                refresh,
            )
            .map_err(String::from)
        }
        "upstream_tools" => {
            let server = json_helpers::string_at_path(args, &["server"]);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            upstream::list_tools(root_path, server, timeout_ms, refresh)
        }
        "upstream_probe" => {
            let server = json_helpers::string_at_path(args, &["server"]);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            upstream::probe_servers(root_path, server, timeout_ms, refresh).map_err(String::from)
        }
        "upstream_catalog" => {
            let server = json_helpers::string_at_path(args, &["server"]);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            upstream::catalog_tools(root_path, server, timeout_ms, refresh).map_err(String::from)
        }
        "upstream_policy_audit" => {
            let server = json_helpers::string_at_path(args, &["server"]);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            upstream::audit_tool_policies(root_path, server, timeout_ms, refresh).map_err(String::from)
        }
        "upstream_policy_suggest" => {
            let server = json_helpers::string_at_path(args, &["server"]);
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let refresh = json_helpers::bool_at_path(args, &["refresh"]).unwrap_or(false);
            upstream::suggest_tool_policies(root_path, server, timeout_ms, refresh).map_err(String::from)
        }
        "upstream_call" => {
            let server = json_helpers::string_at_path(args, &["server"])
                .ok_or_else(|| "upstream_call requires a 'server' string".to_string())?;
            let tool = json_helpers::string_at_path(args, &["tool"])
                .ok_or_else(|| "upstream_call requires a 'tool' string".to_string())?;
            let arguments = optional_object_arg(args, "arguments")?;
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let context = http_upstream_lease_context(args, request)?;
            upstream::call_tool_with_pooled_context(
                root_path,
                server,
                tool,
                &arguments,
                timeout_ms,
                Some(&context),
                &config.upstream_session_pool,
            )
        }
        "upstream_batch" => {
            let server = json_helpers::string_at_path(args, &["server"])
                .ok_or_else(|| "upstream_batch requires a 'server' string".to_string())?;
            let raw_calls = json_helpers::array_at_path(args, &["calls"])
                .ok_or_else(|| "upstream_batch requires a 'calls' array".to_string())?;
            let mut calls = Vec::new();
            for (index, raw_call) in raw_calls.iter().enumerate() {
                calls.push(parse_http_upstream_batch_call(raw_call, index)?);
            }
            let timeout_ms = json_helpers::value_at_path(args, &["timeoutMs"])
                .and_then(JsonValue::as_i64)
                .filter(|value| *value > 0)
                .map(|value| value as u64);
            let context = http_upstream_lease_context(args, request)?;
            upstream::call_tools_with_pooled_context(
                root_path,
                server,
                &calls,
                timeout_ms,
                Some(&context),
                &config.upstream_session_pool,
            )
        }
        "client_list" => run_json_command(root_path, &["client", "list", "--json"]),
        "client_plan" => run_json_command_vec(
            root_path,
            build_http_client_args("plan", args, request)?,
        ),
        "client_export" => run_json_command_vec(
            root_path,
            build_http_client_args("export", args, request)?,
        ),
        other => Err(format!(
            "unsupported MCPace HTTP tool '{}'. This HTTP endpoint exposes MCPace management tools plus adapter_profile/upstream_search and stdio upstream access through surface_manifest/upstream_catalog/upstream_probe/upstream_policy_audit/upstream_policy_suggest/upstream_tools/upstream_call/upstream_batch. In auto/native exposure mode, upstream tools may also appear as projected u_<server>_<tool>_<hash> names in tools/list; call adapter_profile for the current routing plan, upstream_search for concise discovery, upstream_tools for one server's full schemas, then upstream_call or upstream_batch when brokered routing is better. Call runtime_diagnostics for exact status.",
            other
        )),
    }
}

fn build_http_client_args(
    action: &str,
    args: &JsonValue,
    request: Option<&HttpRequest>,
) -> Result<Vec<String>, String> {
    let context = http_upstream_lease_context(args, request)?;
    let client_id = context
        .client_id
        .clone()
        .unwrap_or_else(|| "local-http".to_string());
    let mut command = vec![
        "client".to_string(),
        action.to_string(),
        "--json".to_string(),
    ];
    if action == "export" {
        command.push(client_id);
    } else {
        command.push("--client-id".to_string());
        command.push(client_id);
    }
    push_optional_http_arg(&mut command, "--session-id", context.session_id);
    push_optional_http_arg(&mut command, "--project-root", context.project_root);
    push_optional_http_arg(&mut command, "--transport", context.transport);
    if let Some(metadata) = context.metadata {
        command.push("--metadata-json".to_string());
        command.push(metadata.to_compact_string());
    }
    Ok(command)
}

fn push_optional_http_arg(args: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(value) = value {
        args.push(flag.to_string());
        args.push(value);
    }
}

fn required_http_string(args: &JsonValue, key: &str) -> Result<String, String> {
    optional_http_string(args, key)?.ok_or_else(|| format!("{} is required", key))
}

fn parse_http_upstream_batch_call(
    raw_call: &JsonValue,
    index: usize,
) -> Result<upstream::UpstreamToolCall, String> {
    if let Some(items) = raw_call.as_array() {
        if items.is_empty() || items.len() > 2 {
            return Err(format!(
                "upstream_batch calls[{}] tuple form must be [tool] or [tool, arguments]",
                index
            ));
        }
        let tool = items[0]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "upstream_batch calls[{}][0] must be a non-empty tool string",
                    index
                )
            })?
            .to_string();
        let arguments = match items.get(1) {
            Some(JsonValue::Object(_)) => items[1].clone(),
            Some(JsonValue::Null) | None => empty_object(),
            Some(_) => {
                return Err(format!(
                    "upstream_batch calls[{}][1] must be a JSON object when present",
                    index
                ));
            }
        };
        return Ok(upstream::UpstreamToolCall { tool, arguments });
    }

    let tool = json_helpers::string_at_path(raw_call, &["tool"])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "upstream_batch calls[{}] requires a non-empty 'tool'",
                index
            )
        })?
        .to_string();
    let arguments = optional_object_arg(raw_call, "arguments")?;
    Ok(upstream::UpstreamToolCall { tool, arguments })
}

fn optional_object_arg(arguments: &JsonValue, key: &str) -> Result<JsonValue, String> {
    match json_helpers::value_at_path(arguments, &[key]) {
        Some(value @ JsonValue::Object(_)) => Ok(value.clone()),
        Some(JsonValue::Null) | None => Ok(empty_object()),
        Some(_) => Err(format!("{} must be a JSON object", key)),
    }
}

pub(super) fn http_upstream_lease_context(
    args: &JsonValue,
    request: Option<&HttpRequest>,
) -> Result<upstream::UpstreamLeaseContext, String> {
    Ok(upstream::UpstreamLeaseContext {
        client_id: Some(
            optional_http_string(args, "clientId")?
                .or_else(|| first_http_metadata_string(args, CLIENT_ID_METADATA_PATHS))
                .or_else(|| http_boundary::request_header_string(request, "x-mcp-client-id"))
                .or_else(|| http_boundary::request_header_string(request, "x-mcpace-client-id"))
                .or_else(|| http_boundary::request_header_string(request, "x-codex-client-id"))
                .unwrap_or_else(|| "local-http".to_string()),
        ),
        session_id: optional_http_string(args, "sessionId")?
            .or_else(|| first_http_metadata_string(args, SESSION_ID_METADATA_PATHS))
            .or_else(|| http_boundary::request_header_string(request, "mcp-session-id"))
            .or_else(|| http_boundary::request_header_string(request, "x-mcp-session-id"))
            .or_else(|| http_boundary::request_header_string(request, "x-mcpace-session-id"))
            .or_else(|| http_boundary::request_header_string(request, "x-mcpace-conversation-id"))
            .or_else(|| http_boundary::request_header_string(request, "x-mcpace-chat-id"))
            .or_else(|| http_boundary::request_header_string(request, "x-codex-session-id"))
            .or_else(|| http_boundary::request_header_string(request, "x-codex-conversation-id")),
        project_root: optional_http_string(args, "projectRoot")?
            .or_else(|| first_http_metadata_string(args, PROJECT_ROOT_METADATA_PATHS))
            .or_else(|| http_boundary::request_header_string(request, "x-mcpace-project-root"))
            .or_else(|| http_boundary::request_header_string(request, "x-mcpace-workspace-root"))
            .or_else(|| http_boundary::request_header_string(request, "x-codex-project-root")),
        transport: Some(
            optional_http_string(args, "transport")?
                .or_else(|| first_http_metadata_string(args, TRANSPORT_METADATA_PATHS))
                .unwrap_or_else(|| "streamable-http".to_string()),
        ),
        metadata: json_helpers::value_at_path(args, &["metadata"]).cloned(),
        ttl_ms: json_helpers::value_at_path(args, &["ttlMs"])
            .and_then(JsonValue::as_i64)
            .filter(|value| *value > 0)
            .map(|value| value as u128),
        allow_arguments: http_allow_arguments(args)?,
        allowed_tool_risk_classes: http_allowed_tool_risk_classes(args)?,
    })
}

const CLIENT_ID_METADATA_PATHS: &[&[&str]] = &[
    &["metadata", "client", "id"],
    &["metadata", "clientId"],
    &["metadata", "clientProfileId"],
    &["metadata", "context", "clientId"],
];

const SESSION_ID_METADATA_PATHS: &[&[&str]] = &[
    &["metadata", "session", "id"],
    &["metadata", "sessionId"],
    &["metadata", "externalSessionId"],
    &["metadata", "conversationId"],
    &["metadata", "context", "sessionId"],
    &["metadata", "context", "externalSessionId"],
    &["metadata", "headers", "Mcp-Session-Id"],
    &["metadata", "headers", "mcp-session-id"],
];

const PROJECT_ROOT_METADATA_PATHS: &[&[&str]] = &[
    &["metadata", "projectRoot"],
    &["metadata", "workspaceRoot"],
    &["metadata", "workspace", "root"],
    &["metadata", "context", "projectRoot"],
    &["metadata", "context", "cwd"],
    &["metadata", "cwd"],
];

const TRANSPORT_METADATA_PATHS: &[&[&str]] = &[
    &["metadata", "transport"],
    &["metadata", "ingress"],
    &["metadata", "context", "transport"],
];

fn first_http_metadata_string(args: &JsonValue, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        json_helpers::string_at_path(args, path)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn optional_http_string(args: &JsonValue, key: &str) -> Result<Option<String>, String> {
    match json_helpers::value_at_path(args, &[key]) {
        Some(JsonValue::String(value)) => Ok(Some(value.clone())),
        Some(JsonValue::Null) | None => Ok(None),
        Some(_) => Err(format!("{} must be a string when provided", key)),
    }
}

fn http_allow_arguments(args: &JsonValue) -> Result<BTreeSet<String>, String> {
    upstream::collect_allow_arguments(args).map_err(|error| format!("{} when provided", error))
}

fn http_allowed_tool_risk_classes(args: &JsonValue) -> Result<BTreeSet<String>, String> {
    upstream::collect_allowed_tool_risk_classes(args)
        .map_err(|error| format!("{} when provided", error))
}
