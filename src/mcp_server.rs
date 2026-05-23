use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use crate::tool_result::{self, ToolResultOptions};
use crate::{adapter, app, upstream};
use std::collections::BTreeSet;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;
mod args;
mod tool_surface;
use self::args::{parse_args, write_help};
use self::tool_surface::{mcp_tool_names, tool_definition, TOOL_SPECS};
#[derive(Clone, Debug)]
struct ServerConfig {
    root_path: PathBuf,
    client_id: String,
    session_id: Option<String>,
    project_root: Option<String>,
    transport: String,
}
pub fn run(
    args: &[String],
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let parsed = parse_args(args);
    if let Some(error) = parsed.error {
        let _ = writeln!(stderr, "{}", error);
        return 2;
    }
    if parsed.help {
        write_help(stdout);
        return 0;
    }
    let root_path = parsed.root_override.clone().or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };
    let config = ServerConfig {
        root_path,
        client_id: parsed
            .client_id
            .unwrap_or_else(|| "generic-stdio".to_string()),
        session_id: parsed.session_id,
        project_root: parsed.project_root,
        transport: parsed.transport.unwrap_or_else(|| "stdio".to_string()),
    };

    serve(config, stdout, stderr)
}

fn serve(config: ServerConfig, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut line = String::new();
    let mut initialize_seen = false;
    let mut initialized_notification_seen = false;
    let mut initialize_params: Option<JsonValue> = None;
    let mut seen_request_ids = BTreeSet::new();
    let upstream_session_pool = Mutex::new(upstream::UpstreamSessionPool::default());

    loop {
        line.clear();
        match input.read_line(&mut line) {
            Ok(0) => return 0,
            Ok(_) => {}
            Err(error) => {
                let _ = writeln!(stderr, "failed to read MCP stdin: {}", error);
                return 1;
            }
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let message = match parse_str(trimmed) {
            Ok(value) => value,
            Err(error) => {
                let response = mcp::error(
                    JsonValue::Null,
                    mcp::ERROR_PARSE,
                    "Invalid JSON message",
                    Some(JsonValue::object([("details", JsonValue::string(error))])),
                );
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
                continue;
            }
        };

        let request_id = mcp::request_id(&message);
        if let Err(error) = mcp::validate_request_envelope(&message) {
            let response = mcp::error(
                mcp::request_id_or_null(&message),
                mcp::ERROR_INVALID_REQUEST,
                &error,
                None,
            );
            if write_message(stdout, &response).is_err() {
                return 1;
            }
            continue;
        }
        let method = json_helpers::string_at_path(&message, &["method"])
            .expect("validate_request_envelope checked method");
        if let Some(id) = request_id.as_ref() {
            if method_is_notification(method) {
                let response = mcp::error(
                    id.clone(),
                    mcp::ERROR_INVALID_REQUEST,
                    "MCP notifications must not include a JSON-RPC id",
                    None,
                );
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
                continue;
            }
            let Some(id_key) = mcp::request_id_key(id) else {
                let response = mcp::error(
                    id.clone(),
                    mcp::ERROR_INVALID_REQUEST,
                    "JSON-RPC request id must be a string or integer number",
                    None,
                );
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
                continue;
            };
            if !seen_request_ids.insert(id_key) {
                let response = mcp::error(
                    id.clone(),
                    mcp::ERROR_INVALID_REQUEST,
                    "JSON-RPC request id was already used in this MCP session",
                    None,
                );
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
                continue;
            }
        }

        if method != "initialize" && method != "ping" && !initialize_seen {
            if let Some(id) = request_id.clone() {
                let response = mcp::error(
                    id,
                    mcp::ERROR_NOT_INITIALIZED,
                    "Server not initialized; send initialize first",
                    None,
                );
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            continue;
        }

        if initialize_seen
            && !initialized_notification_seen
            && !matches!(method, "initialize" | "ping" | "notifications/initialized")
        {
            if let Some(id) = request_id.clone() {
                let response = mcp::error(
                    id,
                    mcp::ERROR_NOT_INITIALIZED,
                    "MCP session is initialized but not ready; send notifications/initialized before normal operations",
                    None,
                );
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            continue;
        }

        match method {
            "initialize" => {
                let Some(id) = request_id else {
                    continue;
                };
                if initialize_seen {
                    let response = mcp::error(
                        id,
                        mcp::ERROR_INVALID_REQUEST,
                        "MCP session is already initialized",
                        None,
                    );
                    if write_message(stdout, &response).is_err() {
                        return 1;
                    }
                    continue;
                }
                initialize_seen = true;
                initialize_params = json_helpers::value_at_path(&message, &["params"]).cloned();
                let startup_notes = bootstrap_hub_runtime(&config, stderr);
                let requested =
                    json_helpers::string_at_path(&message, &["params", "protocolVersion"])
                        .unwrap_or(mcp::CURRENT_PROTOCOL_VERSION);
                let negotiated = mcp::negotiate_protocol_version(requested);
                let response = mcp::result(
                    id,
                    JsonValue::object([
                        ("protocolVersion", JsonValue::string(negotiated.to_string())),
                        ("capabilities", adapter::adapter_capabilities()),
                        (
                            "serverInfo",
                            JsonValue::object([
                                ("name", JsonValue::string(mcp::SERVER_NAME)),
                                ("version", JsonValue::string(env!("CARGO_PKG_VERSION"))),
                            ]),
                        ),
                        (
                            "instructions",
                            JsonValue::string(instructions_text(&startup_notes)),
                        ),
                    ]),
                );
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            "notifications/initialized" => {
                initialized_notification_seen = true;
            }
            "notifications/cancelled" => {}
            "ping" => {
                let Some(id) = request_id else {
                    continue;
                };
                let response = mcp::result(id, mcp::empty_object());
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            "tools/list" => {
                let Some(id) = request_id else {
                    continue;
                };
                if !initialize_seen {
                    let response = mcp::error(
                        id,
                        mcp::ERROR_NOT_INITIALIZED,
                        "Server not initialized",
                        None,
                    );
                    if write_message(stdout, &response).is_err() {
                        return 1;
                    }
                    continue;
                }

                let surface_options =
                    adapter::tool_surface_options_from_initialize(initialize_params.as_ref());
                let base_tools = TOOL_SPECS
                    .iter()
                    .map(|tool| tool_definition(tool, surface_options))
                    .collect::<Vec<_>>();
                let cursor = json_helpers::string_at_path(&message, &["params", "cursor"]);
                let result = adapter::tool_list_result(
                    &config.root_path,
                    base_tools,
                    initialize_params.as_ref(),
                    cursor,
                );
                let response = mcp::result(id, result);
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            "prompts/list" => {
                let Some(id) = request_id else {
                    continue;
                };
                if !initialize_seen {
                    let response = mcp::error(
                        id,
                        mcp::ERROR_NOT_INITIALIZED,
                        "Server not initialized",
                        None,
                    );
                    if write_message(stdout, &response).is_err() {
                        return 1;
                    }
                    continue;
                }
                let cursor = json_helpers::string_at_path(&message, &["params", "cursor"]);
                let response =
                    mcp::result(id, adapter::list_prompts(&config.root_path, None, cursor));
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            "prompts/get" => {
                let Some(id) = request_id else {
                    continue;
                };
                if !initialize_seen {
                    let response = mcp::error(
                        id,
                        mcp::ERROR_NOT_INITIALIZED,
                        "Server not initialized",
                        None,
                    );
                    if write_message(stdout, &response).is_err() {
                        return 1;
                    }
                    continue;
                }
                let name = json_helpers::string_at_path(&message, &["params", "name"]);
                let args = json_helpers::value_at_path(&message, &["params", "arguments"])
                    .cloned()
                    .unwrap_or_else(mcp::empty_object);
                let response = match name {
                    Some(name) => match adapter::get_prompt(&config.root_path, name, args, None) {
                        Ok(value) => mcp::result(id, value),
                        Err(error) => mcp::error(id, mcp::ERROR_INVALID_PARAMS, &error, None),
                    },
                    None => mcp::error(
                        id,
                        mcp::ERROR_INVALID_PARAMS,
                        "prompts/get requires a prompt name",
                        None,
                    ),
                };
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            "resources/list" => {
                let Some(id) = request_id else {
                    continue;
                };
                if !initialize_seen {
                    let response = mcp::error(
                        id,
                        mcp::ERROR_NOT_INITIALIZED,
                        "Server not initialized",
                        None,
                    );
                    if write_message(stdout, &response).is_err() {
                        return 1;
                    }
                    continue;
                }
                let cursor = json_helpers::string_at_path(&message, &["params", "cursor"]);
                let response =
                    mcp::result(id, adapter::list_resources(&config.root_path, None, cursor));
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            "resources/templates/list" => {
                let Some(id) = request_id else {
                    continue;
                };
                if !initialize_seen {
                    let response = mcp::error(
                        id,
                        mcp::ERROR_NOT_INITIALIZED,
                        "Server not initialized",
                        None,
                    );
                    if write_message(stdout, &response).is_err() {
                        return 1;
                    }
                    continue;
                }
                let response = mcp::result(
                    id,
                    adapter::list_resource_templates(
                        &config.root_path,
                        None,
                        json_helpers::string_at_path(&message, &["params", "cursor"]),
                    ),
                );
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            "resources/read" => {
                let Some(id) = request_id else {
                    continue;
                };
                if !initialize_seen {
                    let response = mcp::error(
                        id,
                        mcp::ERROR_NOT_INITIALIZED,
                        "Server not initialized",
                        None,
                    );
                    if write_message(stdout, &response).is_err() {
                        return 1;
                    }
                    continue;
                }
                let uri = json_helpers::string_at_path(&message, &["params", "uri"]);
                let response = match uri {
                    Some(uri) => match adapter::read_resource(&config.root_path, uri, None) {
                        Ok(value) => mcp::result(id, value),
                        Err(error) => mcp::error(id, mcp::ERROR_INVALID_PARAMS, &error, None),
                    },
                    None => mcp::error(
                        id,
                        mcp::ERROR_INVALID_PARAMS,
                        "resources/read requires a uri",
                        None,
                    ),
                };
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            "tools/call" => {
                let Some(id) = request_id else {
                    continue;
                };
                if !initialize_seen {
                    let response = mcp::error(
                        id,
                        mcp::ERROR_NOT_INITIALIZED,
                        "Server not initialized",
                        None,
                    );
                    if write_message(stdout, &response).is_err() {
                        return 1;
                    }
                    continue;
                }

                let name = match json_helpers::string_at_path(&message, &["params", "name"]) {
                    Some(value) => value,
                    None => {
                        let response = mcp::error(
                            id,
                            mcp::ERROR_INVALID_PARAMS,
                            "tools/call requires a tool name",
                            None,
                        );
                        if write_message(stdout, &response).is_err() {
                            return 1;
                        }
                        continue;
                    }
                };
                let arguments = match mcp::tool_call_arguments_or_empty(&message) {
                    Ok(value) => value,
                    Err(error) => {
                        let response = mcp::error(id, mcp::ERROR_INVALID_PARAMS, &error, None);
                        if write_message(stdout, &response).is_err() {
                            return 1;
                        }
                        continue;
                    }
                };

                let response = match execute_tool(
                    &config,
                    name,
                    &arguments,
                    initialize_params.as_ref(),
                    &upstream_session_pool,
                ) {
                    Ok(value) => mcp::result(id, value),
                    Err(ToolCallError::InvalidParams(message)) => {
                        mcp::error(id, mcp::ERROR_INVALID_PARAMS, &message, None)
                    }
                    Err(ToolCallError::UnknownTool(message)) => {
                        mcp::error(id, mcp::ERROR_INVALID_PARAMS, &message, None)
                    }
                    Err(ToolCallError::Execution(message)) => {
                        mcp::result(id, tool_error_result(message))
                    }
                };
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            _ => {
                if let Some(id) = request_id {
                    let response = mcp::error(
                        id,
                        mcp::ERROR_METHOD_NOT_FOUND,
                        &format!("unsupported MCP method '{}'", method),
                        None,
                    );
                    if write_message(stdout, &response).is_err() {
                        return 1;
                    }
                }
            }
        }

        if initialized_notification_seen {
            let _ = stderr.flush();
        }
    }
}

fn write_message(stdout: &mut dyn Write, message: &JsonValue) -> io::Result<()> {
    writeln!(stdout, "{}", message.to_compact_string())?;
    stdout.flush()
}

fn method_is_notification(method: &str) -> bool {
    method.starts_with("notifications/")
}

enum ToolCallError {
    InvalidParams(String),
    UnknownTool(String),
    Execution(String),
}

fn execute_tool(
    config: &ServerConfig,
    tool_name: &str,
    arguments: &JsonValue,
    initialize_params: Option<&JsonValue>,
    upstream_session_pool: &Mutex<upstream::UpstreamSessionPool>,
) -> Result<JsonValue, ToolCallError> {
    if tool_name.starts_with("u_") {
        let reserved = mcp_tool_names().into_iter().collect::<BTreeSet<_>>();
        let target = adapter::resolve_projected_tool(
            &config.root_path,
            tool_name,
            &reserved,
            &adapter::ToolExposureOptions::for_call_resolution(),
        )
        .map_err(ToolCallError::Execution)?;
        if let Some(target) = target {
            let control_arguments = adapter::projected_adapter_control_arguments(arguments);
            let context = ForwardedContext::from_tool_arguments(
                config,
                &control_arguments,
                initialize_params,
            )?;
            let result_options = result_options_from_arguments(&control_arguments)?;
            let upstream_arguments = adapter::strip_projected_adapter_arguments(arguments);
            let ttl_ms = integer_argument(&control_arguments, "ttlMs")?;
            let result = upstream::call_tool_with_pooled_context(
                &config.root_path,
                &target.server,
                &target.tool,
                &upstream_arguments,
                timeout_argument(&control_arguments, "timeoutMs")?,
                Some(&context.upstream_lease_context(ttl_ms)),
                upstream_session_pool,
            )
            .map_err(ToolCallError::Execution)?;
            return Ok(tool_result::upstream_tool_result_payload(
                result,
                false,
                result_options,
            ));
        }
    }

    let result_options = result_options_from_arguments(arguments)?;
    let result = match tool_name {
        "doctor" => run_json_command(config, &["doctor", "--json"])?,
        "hub_status" => run_json_command(config, &["hub", "status", "--json"])?,
        "hub_up" => run_json_command(config, &["hub", "up", "--json"])?,
        "hub_down" => run_json_command(config, &["hub", "down", "--json"])?,
        "hub_logs" => {
            let mut args = vec!["hub".to_string(), "logs".to_string(), "--json".to_string()];
            if let Some(tail) = integer_argument(arguments, "tail")? {
                args.push("--tail".to_string());
                args.push(tail.to_string());
            }
            run_json_command_vec(config, args)?
        }
        "runtime_leases" => run_json_command(config, &["hub", "lease", "list", "--json"])?,
        "runtime_acquire" => {
            let server = required_string_argument(arguments, "server")?;
            run_json_command_vec_allow_structured_exit(
                config,
                build_runtime_acquire_args(config, arguments, initialize_params, server)?,
            )?
        }
        "runtime_renew" => {
            let lease_id = required_string_argument(arguments, "leaseId")?;
            let mut args = vec![
                "hub".to_string(),
                "lease".to_string(),
                "renew".to_string(),
                "--json".to_string(),
                "--lease-id".to_string(),
                lease_id,
            ];
            if let Some(ttl_ms) = integer_argument(arguments, "ttlMs")? {
                args.push("--ttl-ms".to_string());
                args.push(ttl_ms.to_string());
            }
            run_json_command_vec_allow_structured_exit(config, args)?
        }
        "runtime_release" => {
            let lease_id = required_string_argument(arguments, "leaseId")?;
            run_json_command_vec_allow_structured_exit(
                config,
                vec![
                    "hub".to_string(),
                    "lease".to_string(),
                    "release".to_string(),
                    "--json".to_string(),
                    "--lease-id".to_string(),
                    lease_id,
                ],
            )?
        }
        "server_list" => run_json_command(config, &["server", "list", "--json"])?,
        "server_capabilities" => {
            let name = required_string_argument(arguments, "name")?;
            run_json_command_vec(
                config,
                vec![
                    "server".to_string(),
                    "capabilities".to_string(),
                    "--json".to_string(),
                    "--name".to_string(),
                    name,
                ],
            )?
        }
        "client_list" => run_json_command(config, &["client", "list", "--json"])?,
        "client_plan" => run_json_command_vec(
            config,
            build_client_args("plan", config, arguments, initialize_params)?,
        )?,
        "client_export" => run_json_command_vec(
            config,
            build_client_args("export", config, arguments, initialize_params)?,
        )?,
        "adapter_profile" => {
            let include_live_catalog =
                bool_argument(arguments, "includeLiveCatalog")?.unwrap_or(false);
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            let refresh = bool_argument(arguments, "refresh")?.unwrap_or(false);
            let visible_tools = adapter::visible_tool_names(&mcp_tool_names(), initialize_params);
            adapter::adapter_profile(
                &config.root_path,
                initialize_params,
                &config.transport,
                &visible_tools,
                include_live_catalog,
                timeout_ms,
                refresh,
            )
            .map_err(ToolCallError::Execution)?
        }
        "adapter_route" => {
            let include_live_catalog =
                bool_argument(arguments, "includeLiveCatalog")?.unwrap_or(false);
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            let refresh = bool_argument(arguments, "refresh")?.unwrap_or(false);
            let calls = json_helpers::value_at_path(arguments, &["calls"]);
            adapter::adapter_route_plan(
                &config.root_path,
                calls,
                include_live_catalog,
                timeout_ms,
                refresh,
            )
            .map_err(ToolCallError::Execution)?
        }
        "upstream_search" => {
            let query = optional_string_argument(arguments, "query")?;
            let server = optional_string_argument(arguments, "server")?;
            let limit = integer_argument(arguments, "limit")?
                .filter(|value| *value > 0)
                .map(|value| value as usize)
                .unwrap_or(20);
            let include_schema = bool_argument(arguments, "includeSchema")?.unwrap_or(false);
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            let refresh = bool_argument(arguments, "refresh")?.unwrap_or(false);
            adapter::upstream_search(
                &config.root_path,
                server.as_deref(),
                query.as_deref(),
                limit,
                include_schema,
                timeout_ms,
                refresh,
            )
            .map_err(ToolCallError::Execution)?
        }
        "surface_manifest" => {
            let include_live_catalog =
                bool_argument(arguments, "includeLiveCatalog")?.unwrap_or(false);
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            let refresh = bool_argument(arguments, "refresh")?.unwrap_or(false);
            upstream::surface_manifest(
                &config.root_path,
                &config.transport,
                adapter::visible_tool_names(&mcp_tool_names(), initialize_params),
                include_live_catalog,
                timeout_ms,
                refresh,
            )
            .map_err(ToolCallError::Execution)?
        }
        "upstream_tools" => {
            let server = optional_string_argument(arguments, "server")?;
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            let refresh = bool_argument(arguments, "refresh")?.unwrap_or(false);
            upstream::list_tools(&config.root_path, server.as_deref(), timeout_ms, refresh)
                .map_err(ToolCallError::Execution)?
        }
        "upstream_catalog" => {
            let server = optional_string_argument(arguments, "server")?;
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            let refresh = bool_argument(arguments, "refresh")?.unwrap_or(false);
            upstream::catalog_tools(&config.root_path, server.as_deref(), timeout_ms, refresh)
                .map_err(ToolCallError::Execution)?
        }
        "upstream_probe" => {
            let server = optional_string_argument(arguments, "server")?;
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            let refresh = bool_argument(arguments, "refresh")?.unwrap_or(false);
            upstream::probe_servers(&config.root_path, server.as_deref(), timeout_ms, refresh)
                .map_err(ToolCallError::Execution)?
        }
        "upstream_policy_audit" => {
            let server = optional_string_argument(arguments, "server")?;
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            let refresh = bool_argument(arguments, "refresh")?.unwrap_or(false);
            upstream::audit_tool_policies(&config.root_path, server.as_deref(), timeout_ms, refresh)
                .map_err(ToolCallError::Execution)?
        }
        "upstream_policy_suggest" => {
            let server = optional_string_argument(arguments, "server")?;
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            let refresh = bool_argument(arguments, "refresh")?.unwrap_or(false);
            upstream::suggest_tool_policies(
                &config.root_path,
                server.as_deref(),
                timeout_ms,
                refresh,
            )
            .map_err(ToolCallError::Execution)?
        }
        "upstream_call" => {
            let server = required_string_argument(arguments, "server")?;
            let tool = required_string_argument(arguments, "tool")?;
            let upstream_arguments = optional_object_argument(arguments, "arguments")?;
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            let context =
                ForwardedContext::from_tool_arguments(config, arguments, initialize_params)?;
            upstream::call_tool_with_pooled_context(
                &config.root_path,
                &server,
                &tool,
                &upstream_arguments,
                timeout_ms,
                Some(&context.upstream_lease_context(integer_argument(arguments, "ttlMs")?)),
                upstream_session_pool,
            )
            .map_err(ToolCallError::Execution)?
        }
        "upstream_batch" => {
            let server = required_string_argument(arguments, "server")?;
            let raw_calls =
                json_helpers::array_at_path(arguments, &["calls"]).ok_or_else(|| {
                    ToolCallError::InvalidParams("'calls' must be an array".to_string())
                })?;
            let mut calls = Vec::new();
            for (index, raw_call) in raw_calls.iter().enumerate() {
                calls.push(parse_upstream_batch_call(raw_call, index)?);
            }
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            let context =
                ForwardedContext::from_tool_arguments(config, arguments, initialize_params)?;
            upstream::call_tools_with_pooled_context(
                &config.root_path,
                &server,
                &calls,
                timeout_ms,
                Some(&context.upstream_lease_context(integer_argument(arguments, "ttlMs")?)),
                upstream_session_pool,
            )
            .map_err(ToolCallError::Execution)?
        }
        _ => {
            return Err(ToolCallError::UnknownTool(format!(
                "unknown MCPace tool '{}'",
                tool_name
            )))
        }
    };

    if matches!(tool_name, "upstream_call" | "upstream_batch") {
        Ok(tool_result::upstream_tool_result_payload(
            result,
            false,
            result_options,
        ))
    } else {
        Ok(tool_success_result(result, result_options))
    }
}

fn parse_upstream_batch_call(
    raw_call: &JsonValue,
    index: usize,
) -> Result<upstream::UpstreamToolCall, ToolCallError> {
    if let Some(items) = raw_call.as_array() {
        if items.is_empty() || items.len() > 2 {
            return Err(ToolCallError::InvalidParams(format!(
                "calls[{}] tuple form must be [tool] or [tool, arguments]",
                index
            )));
        }
        let tool = items[0]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ToolCallError::InvalidParams(format!(
                    "calls[{}][0] must be a non-empty tool string",
                    index
                ))
            })?
            .to_string();
        let arguments = match items.get(1) {
            Some(JsonValue::Object(_)) => items[1].clone(),
            Some(JsonValue::Null) | None => mcp::empty_object(),
            Some(_) => {
                return Err(ToolCallError::InvalidParams(format!(
                    "calls[{}][1] must be a JSON object when present",
                    index
                )));
            }
        };
        return Ok(upstream::UpstreamToolCall { tool, arguments });
    }

    let tool = json_helpers::string_at_path(raw_call, &["tool"])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ToolCallError::InvalidParams(format!(
                "calls[{}].tool must be a non-empty string",
                index
            ))
        })?
        .to_string();
    let arguments = match json_helpers::value_at_path(raw_call, &["arguments"]) {
        Some(JsonValue::Object(_)) => json_helpers::value_at_path(raw_call, &["arguments"])
            .cloned()
            .expect("checked above"),
        Some(JsonValue::Null) | None => mcp::empty_object(),
        Some(_) => {
            return Err(ToolCallError::InvalidParams(format!(
                "upstream_batch calls[{}].arguments must be a JSON object",
                index
            )));
        }
    };
    Ok(upstream::UpstreamToolCall { tool, arguments })
}

#[derive(Clone, Copy)]
enum CommandOutputPolicy {
    RequireSuccessfulExit,
    AcceptStructuredStdout,
}

fn run_json_command(config: &ServerConfig, args: &[&str]) -> Result<JsonValue, ToolCallError> {
    run_json_command_vec(
        config,
        args.iter().map(|value| (*value).to_string()).collect(),
    )
}

fn run_json_command_vec(
    config: &ServerConfig,
    args: Vec<String>,
) -> Result<JsonValue, ToolCallError> {
    run_app_json_command(config, args, CommandOutputPolicy::RequireSuccessfulExit)
}

fn run_json_command_vec_allow_structured_exit(
    config: &ServerConfig,
    args: Vec<String>,
) -> Result<JsonValue, ToolCallError> {
    run_app_json_command(config, args, CommandOutputPolicy::AcceptStructuredStdout)
}

fn run_app_json_command(
    config: &ServerConfig,
    mut args: Vec<String>,
    output_policy: CommandOutputPolicy,
) -> Result<JsonValue, ToolCallError> {
    args.push("--root".to_string());
    args.push(config.root_path.display().to_string());

    let mut stdout_buffer = Vec::new();
    let mut stderr_buffer = Vec::new();
    let exit_code = app::run(args, &mut stdout_buffer, &mut stderr_buffer);
    let stdout_text = command_output_to_string(stdout_buffer)?;

    if matches!(output_policy, CommandOutputPolicy::AcceptStructuredStdout)
        && !stdout_text.trim().is_empty()
    {
        return parse_command_json(&stdout_text);
    }

    if exit_code != 0 {
        return Err(ToolCallError::Execution(command_failure_details(
            exit_code,
            &stdout_text,
            stderr_buffer,
        )));
    }

    if stdout_text.trim().is_empty() {
        return Err(ToolCallError::Execution(
            "command produced empty JSON output".to_string(),
        ));
    }

    parse_command_json(&stdout_text)
}

fn command_output_to_string(stdout_buffer: Vec<u8>) -> Result<String, ToolCallError> {
    String::from_utf8(stdout_buffer)
        .map_err(|error| ToolCallError::Execution(format!("non-UTF8 command output: {}", error)))
}

fn parse_command_json(stdout_text: &str) -> Result<JsonValue, ToolCallError> {
    parse_str(stdout_text.trim()).map_err(|error| {
        ToolCallError::Execution(format!("command produced invalid JSON: {}", error))
    })
}

fn command_failure_details(exit_code: i32, stdout_text: &str, stderr_buffer: Vec<u8>) -> String {
    let stderr_text = String::from_utf8(stderr_buffer).unwrap_or_default();
    if !stderr_text.trim().is_empty() {
        stderr_text.trim().to_string()
    } else if !stdout_text.trim().is_empty() {
        stdout_text.trim().to_string()
    } else {
        format!("command failed with exit code {}", exit_code)
    }
}

#[derive(Clone, Debug)]
struct ForwardedContext {
    client_id: String,
    session_id: Option<String>,
    project_root: Option<String>,
    transport: String,
    metadata: Option<JsonValue>,
    allow_arguments: BTreeSet<String>,
    allowed_tool_risk_classes: BTreeSet<String>,
}

impl ForwardedContext {
    fn from_tool_arguments(
        config: &ServerConfig,
        arguments: &JsonValue,
        initialize_params: Option<&JsonValue>,
    ) -> Result<Self, ToolCallError> {
        Ok(Self {
            client_id: optional_string_argument(arguments, "clientId")?
                .unwrap_or_else(|| config.client_id.clone()),
            session_id: optional_string_argument(arguments, "sessionId")?
                .or_else(|| config.session_id.clone()),
            project_root: optional_string_argument(arguments, "projectRoot")?
                .or_else(|| config.project_root.clone()),
            transport: optional_string_argument(arguments, "transport")?
                .unwrap_or_else(|| config.transport.clone()),
            metadata: optional_value_argument(arguments, "metadata")
                .or_else(|| initialize_params.cloned()),
            allow_arguments: allow_arguments(arguments)?,
            allowed_tool_risk_classes: allowed_tool_risk_classes_argument(arguments)?,
        })
    }

    fn append_optional_cli_args(self, args: &mut Vec<String>) {
        push_optional_arg(args, "--session-id", self.session_id);
        push_optional_arg(args, "--project-root", self.project_root);
        push_arg(args, "--transport", self.transport);
        if let Some(value) = self.metadata {
            push_arg(args, "--metadata-json", value.to_compact_string());
        }
    }

    fn upstream_lease_context(&self, ttl_ms: Option<i64>) -> upstream::UpstreamLeaseContext {
        upstream::UpstreamLeaseContext {
            client_id: Some(self.client_id.clone()),
            session_id: self.session_id.clone(),
            project_root: self.project_root.clone(),
            transport: Some(self.transport.clone()),
            metadata: self.metadata.clone(),
            ttl_ms: ttl_ms.filter(|value| *value > 0).map(|value| value as u128),
            allow_arguments: self.allow_arguments.clone(),
            allowed_tool_risk_classes: self.allowed_tool_risk_classes.clone(),
        }
    }
}

fn build_client_args(
    action: &str,
    config: &ServerConfig,
    arguments: &JsonValue,
    initialize_params: Option<&JsonValue>,
) -> Result<Vec<String>, ToolCallError> {
    let context = ForwardedContext::from_tool_arguments(config, arguments, initialize_params)?;
    let mut args = vec![
        "client".to_string(),
        action.to_string(),
        "--json".to_string(),
    ];
    if action == "export" {
        args.push(context.client_id.clone());
    } else {
        push_arg(&mut args, "--client-id", context.client_id.clone());
    }
    context.append_optional_cli_args(&mut args);
    Ok(args)
}

fn build_runtime_acquire_args(
    config: &ServerConfig,
    arguments: &JsonValue,
    initialize_params: Option<&JsonValue>,
    server: String,
) -> Result<Vec<String>, ToolCallError> {
    let context = ForwardedContext::from_tool_arguments(config, arguments, initialize_params)?;
    let mut args = vec![
        "hub".to_string(),
        "lease".to_string(),
        "acquire".to_string(),
        "--json".to_string(),
        "--server".to_string(),
        server,
    ];
    push_arg(&mut args, "--client-id", context.client_id.clone());
    if let Some(ttl_ms) = integer_argument(arguments, "ttlMs")? {
        push_arg(&mut args, "--ttl-ms", ttl_ms.to_string());
    }
    context.append_optional_cli_args(&mut args);
    Ok(args)
}

fn push_arg(args: &mut Vec<String>, flag: &str, value: String) {
    args.push(flag.to_string());
    args.push(value);
}

fn push_optional_arg(args: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(value) = value {
        push_arg(args, flag, value);
    }
}

fn required_string_argument(arguments: &JsonValue, key: &str) -> Result<String, ToolCallError> {
    optional_string_argument(arguments, key)?
        .ok_or_else(|| ToolCallError::InvalidParams(format!("'{}' is required", key)))
}

fn optional_string_argument(
    arguments: &JsonValue,
    key: &str,
) -> Result<Option<String>, ToolCallError> {
    match json_helpers::value_at_path(arguments, &[key]) {
        Some(JsonValue::String(value)) => Ok(Some(value.clone())),
        Some(JsonValue::Null) | None => Ok(None),
        Some(_) => Err(ToolCallError::InvalidParams(format!(
            "'{}' must be a string",
            key
        ))),
    }
}

fn optional_object_argument(arguments: &JsonValue, key: &str) -> Result<JsonValue, ToolCallError> {
    match json_helpers::value_at_path(arguments, &[key]) {
        Some(JsonValue::Object(_)) => Ok(json_helpers::value_at_path(arguments, &[key])
            .cloned()
            .expect("checked above")),
        Some(JsonValue::Null) | None => Ok(mcp::empty_object()),
        Some(_) => Err(ToolCallError::InvalidParams(format!(
            "'{}' must be a JSON object",
            key
        ))),
    }
}

fn integer_argument(arguments: &JsonValue, key: &str) -> Result<Option<i64>, ToolCallError> {
    match json_helpers::value_at_path(arguments, &[key]) {
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| ToolCallError::InvalidParams(format!("'{}' must be an integer", key))),
        None => Ok(None),
    }
}

fn bool_argument(arguments: &JsonValue, key: &str) -> Result<Option<bool>, ToolCallError> {
    match json_helpers::value_at_path(arguments, &[key]) {
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| ToolCallError::InvalidParams(format!("'{}' must be a boolean", key))),
        None => Ok(None),
    }
}

fn allow_arguments(arguments: &JsonValue) -> Result<BTreeSet<String>, ToolCallError> {
    upstream::collect_allow_arguments(arguments).map_err(ToolCallError::InvalidParams)
}

fn allowed_tool_risk_classes_argument(
    arguments: &JsonValue,
) -> Result<BTreeSet<String>, ToolCallError> {
    upstream::collect_allowed_tool_risk_classes(arguments).map_err(ToolCallError::InvalidParams)
}

fn timeout_argument(arguments: &JsonValue, key: &str) -> Result<Option<u64>, ToolCallError> {
    Ok(integer_argument(arguments, key)?
        .filter(|value| *value > 0)
        .map(|value| value as u64))
}

fn optional_value_argument(arguments: &JsonValue, key: &str) -> Option<JsonValue> {
    json_helpers::value_at_path(arguments, &[key]).cloned()
}

fn tool_success_result(value: JsonValue, options: ToolResultOptions) -> JsonValue {
    tool_result::tool_result_payload(value, false, options)
}

fn result_options_from_arguments(
    arguments: &JsonValue,
) -> Result<ToolResultOptions, ToolCallError> {
    tool_result::options_from_arguments(arguments).map_err(ToolCallError::InvalidParams)
}

fn tool_error_result(message: String) -> JsonValue {
    JsonValue::object([
        (
            "content",
            JsonValue::array([JsonValue::object([
                ("type", JsonValue::string("text")),
                ("text", JsonValue::string(message.clone())),
            ])]),
        ),
        (
            "structuredContent",
            JsonValue::object([("error", JsonValue::string(message))]),
        ),
        ("isError", JsonValue::bool(true)),
    ])
}

fn tool_error_text(error: &ToolCallError) -> String {
    match error {
        ToolCallError::InvalidParams(message)
        | ToolCallError::UnknownTool(message)
        | ToolCallError::Execution(message) => message.clone(),
    }
}

fn instructions_text(startup_notes: &[String]) -> String {
    let instructions = adapter::adapter_instructions();
    if startup_notes.is_empty() {
        instructions
    } else {
        let public_notes = startup_notes
            .iter()
            .map(|note| public_startup_note(note))
            .collect::<Vec<_>>();
        format!(
            "{}\n\nStartup notes:\n- {}",
            instructions,
            public_notes.join("\n- ")
        )
    }
}

fn public_startup_note(note: &str) -> String {
    let trimmed = note.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.contains("failed") && lower.contains("automatically") {
        return trimmed.to_string();
    }

    let summary = trimmed
        .split(':')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("MCPace startup diagnostic");
    format!(
        "{}; details withheld from initialize response; check local MCPace logs.",
        summary
    )
}

fn bootstrap_hub_runtime(config: &ServerConfig, stderr: &mut dyn Write) -> Vec<String> {
    let mut notes = Vec::new();

    let mut status_json = match run_json_command(config, &["hub", "status", "--json"]) {
        Ok(value) => value,
        Err(error) => {
            notes.push(format!(
                "hub status check failed: {}",
                tool_error_text(&error)
            ));
            return notes;
        }
    };

    let mut status = json_helpers::string_at_path(&status_json, &["status"])
        .unwrap_or("unknown")
        .to_string();

    if status == "stale" {
        match run_json_command(config, &["hub", "down", "--json"]) {
            Ok(_) => notes.push("stale hub state was cleaned up automatically".to_string()),
            Err(error) => {
                notes.push(format!(
                    "failed to clean stale hub state automatically: {}",
                    tool_error_text(&error)
                ));
                let _ = writeln!(stderr, "{}", notes.last().unwrap_or(&String::new()));
                return notes;
            }
        }
        match run_json_command(config, &["hub", "status", "--json"]) {
            Ok(value) => {
                status_json = value;
                status = json_helpers::string_at_path(&status_json, &["status"])
                    .unwrap_or("unknown")
                    .to_string();
            }
            Err(error) => {
                notes.push(format!(
                    "hub status refresh failed after stale cleanup: {}",
                    tool_error_text(&error)
                ));
                let _ = writeln!(stderr, "{}", notes.last().unwrap_or(&String::new()));
                return notes;
            }
        }
    }

    if status == "corrupt" {
        match run_json_command(config, &["hub", "repair", "--json"]) {
            Ok(_) => notes.push("corrupt hub state was repaired automatically".to_string()),
            Err(error) => {
                notes.push(format!(
                    "failed to repair corrupt hub state automatically: {}",
                    tool_error_text(&error)
                ));
                let _ = writeln!(stderr, "{}", notes.last().unwrap_or(&String::new()));
                return notes;
            }
        }
        match run_json_command(config, &["hub", "status", "--json"]) {
            Ok(value) => {
                status_json = value;
                status = json_helpers::string_at_path(&status_json, &["status"])
                    .unwrap_or("unknown")
                    .to_string();
            }
            Err(error) => {
                notes.push(format!(
                    "hub status refresh failed after repair: {}",
                    tool_error_text(&error)
                ));
                let _ = writeln!(stderr, "{}", notes.last().unwrap_or(&String::new()));
                return notes;
            }
        }
    }

    if !matches!(status.as_str(), "running" | "starting" | "stopping") {
        match run_json_command(config, &["hub", "up", "--json"]) {
            Ok(_) => notes.push("hub was started automatically for this MCP session".to_string()),
            Err(error) => {
                notes.push(format!(
                    "failed to start hub automatically: {}",
                    tool_error_text(&error)
                ));
                let _ = writeln!(stderr, "{}", notes.last().unwrap_or(&String::new()));
                return notes;
            }
        }
        match run_json_command(config, &["hub", "status", "--json"]) {
            Ok(value) => {
                status_json = value;
                status = json_helpers::string_at_path(&status_json, &["status"])
                    .unwrap_or("unknown")
                    .to_string();
            }
            Err(error) => {
                notes.push(format!(
                    "hub status refresh failed after startup: {}",
                    tool_error_text(&error)
                ));
                let _ = writeln!(stderr, "{}", notes.last().unwrap_or(&String::new()));
                return notes;
            }
        }
    }

    if !matches!(status.as_str(), "running" | "starting" | "stopping") {
        notes.push(format!(
            "hub is still not live after bootstrap attempt (status: {})",
            status
        ));
    } else if let Some(health) = json_helpers::string_at_path(&status_json, &["health"]) {
        if health != "healthy" {
            notes.push(format!("hub came up with health '{}'", health));
        }
    }

    for note in &notes {
        let _ = writeln!(stderr, "{}", note);
    }
    notes
}

#[cfg(test)]
mod tests;
