use super::{
    empty_object, http_boundary, http_headers, http_session, http_tool_definitions,
    http_tool_definitions_for_protocol, http_tool_names, now_ms, run_http_tool,
    write_empty_response, write_empty_response_with_headers, write_json_response,
    write_json_response_with_owned_headers, write_text_response, DashboardConfig, HttpRequest,
    McpHttpResponse, ServeSurface,
};
use crate::adapter;
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use crate::tool_result::{self, ToolResultOptions};
use std::net::TcpStream;

pub(super) fn handle_mcp_http_route(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<(), String> {
    match request.method.as_str() {
        "GET" => {
            if http_boundary::accepts(request, "text/event-stream") {
                write_empty_response_with_headers(
                    stream,
                    "405 Method Not Allowed",
                    &[("Allow", "POST")],
                )?;
                return Ok(());
            }
            let payload = JsonValue::object([
                ("ok", JsonValue::bool(true)),
                (
                    "surface",
                    JsonValue::string(match config.surface {
                        ServeSurface::Dashboard => "dashboard-http",
                        ServeSurface::UnifiedServe => "unified-serve-http",
                    }),
                ),
                (
                    "message",
                    JsonValue::string(
                        "Use HTTP POST with a single JSON-RPC request body at this endpoint.",
                    ),
                ),
            ]);
            write_json_response(stream, "200 OK", &payload)?;
        }
        "DELETE" => {
            let close_result = config
                .http_session_store
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .close_from_request(request, now_ms());
            match close_result {
                Ok(_) => write_empty_response(stream, "202 Accepted")?,
                Err(error) => write_json_error_response(
                    stream,
                    error.http_status(),
                    "mcp_session_error",
                    &error.message,
                )?,
            }
        }
        "POST" => {
            let response = match handle_mcp_http_request(request, config) {
                Ok(value) => value,
                Err(error) => McpHttpResponse::JsonStatus(
                    "400 Bad Request",
                    mcp_error_response(JsonValue::Null, -32700, error),
                ),
            };
            match response {
                McpHttpResponse::Json(payload) => write_json_response(stream, "200 OK", &payload)?,
                McpHttpResponse::JsonWithHeaders(payload, headers) => {
                    write_json_response_with_owned_headers(stream, "200 OK", &payload, &headers)?
                }
                McpHttpResponse::JsonStatus(status, payload) => {
                    write_json_response(stream, status, &payload)?
                }
                McpHttpResponse::Accepted => write_empty_response(stream, "202 Accepted")?,
            }
        }
        _ => write_text_response(
            stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            "Not found",
        )?,
    }

    Ok(())
}

pub(super) fn write_json_error_response(
    stream: &mut TcpStream,
    status: &str,
    code: &str,
    message: &str,
) -> Result<(), String> {
    write_json_response(
        stream,
        status,
        &JsonValue::object([
            ("ok", JsonValue::bool(false)),
            ("generatedAtMs", JsonValue::number(now_ms())),
            (
                "error",
                JsonValue::object([
                    ("code", JsonValue::string(code)),
                    ("message", JsonValue::string(message)),
                ]),
            ),
        ]),
    )
}

fn handle_mcp_http_request(
    request: &HttpRequest,
    config: &DashboardConfig,
) -> Result<McpHttpResponse, String> {
    if !http_boundary::accepts_streamable_http_post(request) {
        return Ok(McpHttpResponse::JsonStatus(
            "400 Bad Request",
            mcp_error_response(
                JsonValue::Null,
                mcp::ERROR_INVALID_REQUEST,
                "missing required Accept header entries: application/json and text/event-stream",
            ),
        ));
    }
    if !http_boundary::content_type_is(request, "application/json") {
        return Ok(McpHttpResponse::JsonStatus(
            "400 Bad Request",
            mcp_error_response(
                JsonValue::Null,
                mcp::ERROR_INVALID_REQUEST,
                "missing required Content-Type header: application/json",
            ),
        ));
    }
    let body_text = std::str::from_utf8(&request.body)
        .map_err(|error| format!("invalid UTF-8 request body: {}", error))?;
    let message =
        parse_str(body_text.trim()).map_err(|error| format!("invalid JSON-RPC body: {}", error))?;
    let request_id = json_helpers::value_at_path(&message, &["id"]).cloned();
    let method_value = json_helpers::value_at_path(&message, &["method"]);

    let id = request_id.clone().unwrap_or(JsonValue::Null);
    let method = match method_value {
        Some(JsonValue::String(value)) => value.as_str(),
        Some(_) => {
            return Ok(McpHttpResponse::JsonStatus(
                "400 Bad Request",
                mcp_error_response(
                    id,
                    mcp::ERROR_INVALID_REQUEST,
                    "JSON-RPC method must be a string",
                ),
            ));
        }
        None => {
            if json_helpers::value_at_path(&message, &["result"]).is_some()
                || json_helpers::value_at_path(&message, &["error"]).is_some()
            {
                if let Err(response) = touch_mcp_session_for_request(request, config, id.clone()) {
                    return Ok(response);
                }
                return Ok(McpHttpResponse::Accepted);
            }
            return Err("missing JSON-RPC method".to_string());
        }
    };
    if let Err(error) = mcp::validate_request_envelope(&message) {
        return Ok(McpHttpResponse::JsonStatus(
            "400 Bad Request",
            mcp_error_response(id, mcp::ERROR_INVALID_REQUEST, error),
        ));
    }
    if request_id.is_some() && mcp::method_is_notification(method) {
        return Ok(McpHttpResponse::JsonStatus(
            "400 Bad Request",
            mcp_error_response(
                id,
                mcp::ERROR_INVALID_REQUEST,
                "MCP notifications must not include a JSON-RPC id",
            ),
        ));
    }
    match http_boundary::request_header_string_unique(Some(request), "mcp-protocol-version") {
        Ok(Some(protocol_header)) if !mcp::is_supported_protocol_version(&protocol_header) => {
            return Ok(McpHttpResponse::JsonStatus(
                "400 Bad Request",
                mcp_error_response(
                    id,
                    mcp::ERROR_INVALID_REQUEST,
                    format!(
                        "unsupported MCP-Protocol-Version header: {}",
                        protocol_header
                    ),
                ),
            ));
        }
        Ok(_) => {}
        Err(error) => {
            return Ok(McpHttpResponse::JsonStatus(
                "400 Bad Request",
                mcp_error_response(id, mcp::ERROR_INVALID_REQUEST, error),
            ));
        }
    }
    if let Err(error) = http_headers::validate_mcp_standard_headers(request, &message, method) {
        return Ok(McpHttpResponse::JsonStatus(
            "400 Bad Request",
            mcp_error_response(id, mcp::ERROR_HEADER_MISMATCH, error),
        ));
    }

    let mut session_protocol: Option<String> = None;

    if method != "initialize" {
        if method == "notifications/initialized" && request_id.is_none() {
            if let Err(response) = mark_mcp_session_initialized(request, config, id.clone()) {
                return Ok(response);
            }
            return Ok(McpHttpResponse::Accepted);
        }

        let session = match touch_mcp_session_for_request(request, config, id.clone()) {
            Ok(value) => value,
            Err(response) => return Ok(response),
        };
        session_protocol = Some(session.protocol_version.clone());
        if request_id.is_none() {
            return Ok(McpHttpResponse::Accepted);
        }
        if let Some(id_key) = request_id.as_ref().and_then(mcp::request_id_key) {
            if let Err(response) = track_mcp_request_id(config, &session.id, &id_key, id.clone()) {
                return Ok(response);
            }
        }
        if method != "ping" && !session.initialized {
            return Ok(McpHttpResponse::JsonStatus(
                "400 Bad Request",
                mcp_error_response(
                    id,
                    mcp::ERROR_NOT_INITIALIZED,
                    "MCP HTTP session is initialized but not ready; send notifications/initialized before normal operations",
                ),
            ));
        }
    } else if request_id.is_none() {
        return Ok(McpHttpResponse::Accepted);
    }

    match method {
        "initialize" => {
            let requested = json_helpers::string_at_path(&message, &["params", "protocolVersion"])
                .unwrap_or(mcp::CURRENT_PROTOCOL_VERSION);
            let negotiated = mcp::negotiate_protocol_version(requested);

            let session_id =
                match http_session::generated_mcp_http_session_id(request, &id, negotiated) {
                    Ok(value) => value,
                    Err(error) => {
                        let message = format!(
                            "failed to generate cryptographically secure MCP HTTP session id: {}",
                            error
                        );
                        return Ok(McpHttpResponse::JsonStatus(
                            "500 Internal Server Error",
                            mcp_error_response(id, mcp::ERROR_INTERNAL, message),
                        ));
                    }
                };
            let client_name =
                json_helpers::string_at_path(&message, &["params", "clientInfo", "name"])
                    .map(ToOwned::to_owned);
            let client_version =
                json_helpers::string_at_path(&message, &["params", "clientInfo", "version"])
                    .map(ToOwned::to_owned);
            config
                .http_session_store
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .create_or_replace(
                    session_id.clone(),
                    negotiated,
                    client_name,
                    client_version,
                    request_id.as_ref().and_then(mcp::request_id_key),
                    now_ms(),
                );
            Ok(McpHttpResponse::JsonWithHeaders(
                JsonValue::object([
                    ("jsonrpc", JsonValue::string("2.0")),
                    ("id", id),
                    (
                        "result",
                        JsonValue::object([
                            ("protocolVersion", JsonValue::string(negotiated)),
                            ("capabilities", adapter::adapter_capabilities()),
                            (
                                "serverInfo",
                                JsonValue::object([
                                    ("name", JsonValue::string("mcpace")),
                                    ("version", JsonValue::string(env!("CARGO_PKG_VERSION"))),
                                ]),
                            ),
                            (
                                "instructions",
                                JsonValue::string(adapter::adapter_instructions()),
                            ),
                        ]),
                    ),
                ]),
                vec![
                    ("Mcp-Session-Id".to_string(), session_id),
                    ("MCP-Protocol-Version".to_string(), negotiated.to_string()),
                ],
            ))
        }
        "ping" => Ok(McpHttpResponse::Json(JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("id", id),
            ("result", empty_object()),
        ]))),
        "tools/list" => {
            let cursor = json_helpers::string_at_path(&message, &["params", "cursor"]);
            let protocol = match http_boundary::request_header_string_unique(
                Some(request),
                "mcp-protocol-version",
            ) {
                Ok(protocol_header) => protocol_header
                    .or_else(|| session_protocol.clone())
                    .unwrap_or_else(|| mcp::STREAMABLE_HTTP_DEFAULT_PROTOCOL_VERSION.to_string()),
                Err(error) => {
                    return Ok(McpHttpResponse::JsonStatus(
                        "400 Bad Request",
                        mcp_error_response(id, mcp::ERROR_INVALID_REQUEST, error),
                    ));
                }
            };
            let protocol_params =
                JsonValue::object([("protocolVersion", JsonValue::string(protocol.clone()))]);
            let result = adapter::tool_list_result(
                &config.root_path,
                http_tool_definitions_for_protocol(Some(protocol.as_str())),
                Some(&protocol_params),
                cursor,
            );
            Ok(McpHttpResponse::Json(JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", id),
                ("result", result),
            ])))
        }
        "prompts/list" => {
            let cursor = json_helpers::string_at_path(&message, &["params", "cursor"]);
            Ok(McpHttpResponse::Json(JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", id),
                (
                    "result",
                    adapter::list_prompts(&config.root_path, None, cursor),
                ),
            ])))
        }
        "prompts/get" => {
            let name = json_helpers::string_at_path(&message, &["params", "name"]);
            let args = match mcp::params_arguments_object_or_empty(&message, "prompts/get") {
                Ok(value) => value,
                Err(error) => {
                    return Ok(McpHttpResponse::Json(mcp_error_response(
                        id,
                        mcp::ERROR_INVALID_PARAMS,
                        error,
                    )));
                }
            };
            let result = match name {
                Some(name) => match adapter::get_prompt(&config.root_path, name, args, None) {
                    Ok(value) => JsonValue::object([
                        ("jsonrpc", JsonValue::string("2.0")),
                        ("id", id),
                        ("result", value),
                    ]),
                    Err(error) => JsonValue::object([
                        ("jsonrpc", JsonValue::string("2.0")),
                        ("id", id),
                        (
                            "error",
                            JsonValue::object([
                                ("code", JsonValue::number(-32602)),
                                ("message", JsonValue::string(error)),
                            ]),
                        ),
                    ]),
                },
                None => JsonValue::object([
                    ("jsonrpc", JsonValue::string("2.0")),
                    ("id", id),
                    (
                        "error",
                        JsonValue::object([
                            ("code", JsonValue::number(-32602)),
                            (
                                "message",
                                JsonValue::string("prompts/get requires a prompt name"),
                            ),
                        ]),
                    ),
                ]),
            };
            Ok(McpHttpResponse::Json(result))
        }
        "resources/list" => {
            let cursor = json_helpers::string_at_path(&message, &["params", "cursor"]);
            Ok(McpHttpResponse::Json(JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", id),
                (
                    "result",
                    adapter::list_resources(&config.root_path, None, cursor),
                ),
            ])))
        }
        "resources/templates/list" => {
            let cursor = json_helpers::string_at_path(&message, &["params", "cursor"]);
            Ok(McpHttpResponse::Json(JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", id),
                (
                    "result",
                    adapter::list_resource_templates(&config.root_path, None, cursor),
                ),
            ])))
        }
        "resources/read" => {
            let uri = json_helpers::string_at_path(&message, &["params", "uri"]);
            let result = match uri {
                Some(uri) => match adapter::read_resource(&config.root_path, uri, None) {
                    Ok(value) => JsonValue::object([
                        ("jsonrpc", JsonValue::string("2.0")),
                        ("id", id),
                        ("result", value),
                    ]),
                    Err(error) => JsonValue::object([
                        ("jsonrpc", JsonValue::string("2.0")),
                        ("id", id),
                        (
                            "error",
                            JsonValue::object([
                                ("code", JsonValue::number(-32602)),
                                ("message", JsonValue::string(error)),
                            ]),
                        ),
                    ]),
                },
                None => JsonValue::object([
                    ("jsonrpc", JsonValue::string("2.0")),
                    ("id", id),
                    (
                        "error",
                        JsonValue::object([
                            ("code", JsonValue::number(-32602)),
                            (
                                "message",
                                JsonValue::string("resources/read requires a uri"),
                            ),
                        ]),
                    ),
                ]),
            };
            Ok(McpHttpResponse::Json(result))
        }
        "tools/call" => {
            let Some(tool_name) = json_helpers::string_at_path(&message, &["params", "name"])
            else {
                return Ok(McpHttpResponse::Json(mcp_error_response(
                    id,
                    mcp::ERROR_INVALID_PARAMS,
                    "tools/call requires a tool name",
                )));
            };
            let args = match mcp::tool_call_arguments_or_empty(&message) {
                Ok(value) => value,
                Err(error) => {
                    return Ok(McpHttpResponse::Json(mcp_error_response(
                        id,
                        mcp::ERROR_INVALID_PARAMS,
                        error,
                    )));
                }
            };
            let projected_call = tool_name.starts_with("u_");
            if !projected_call
                && !http_tool_names()
                    .iter()
                    .any(|name| name.as_str() == tool_name)
            {
                return Ok(McpHttpResponse::Json(mcp_error_response(
                    id,
                    mcp::ERROR_INVALID_PARAMS,
                    format!("Unknown tool: {}", tool_name),
                )));
            }
            let option_arguments = if projected_call {
                adapter::projected_adapter_control_arguments(&args)
            } else {
                args.clone()
            };
            let result_options = match tool_result::options_from_arguments(&option_arguments) {
                Ok(options) => options,
                Err(error) => {
                    return Ok(mcp_tool_result(
                        id,
                        JsonValue::object([
                            ("ok", JsonValue::bool(false)),
                            ("tool", JsonValue::string(tool_name)),
                            ("error", JsonValue::string(error)),
                        ]),
                        true,
                        ToolResultOptions::default(),
                    ));
                }
            };
            let (structured, is_error) =
                match run_http_tool(config, tool_name, &args, Some(request)) {
                    Ok(value) => (value, false),
                    Err(error) => (
                        JsonValue::object([
                            ("ok", JsonValue::bool(false)),
                            ("tool", JsonValue::string(tool_name)),
                            ("error", JsonValue::string(error)),
                            (
                                "supportedTools",
                                JsonValue::array(http_tool_definitions().into_iter().filter_map(
                                    |tool| {
                                        tool.get("name")
                                            .and_then(JsonValue::as_str)
                                            .map(JsonValue::string)
                                    },
                                )),
                            ),
                        ]),
                        true,
                    ),
                };
            if !is_error
                && (matches!(tool_name, "upstream_call" | "upstream_batch") || projected_call)
            {
                Ok(mcp_tool_result_payload(
                    id,
                    tool_result::upstream_tool_result_payload(structured, false, result_options),
                ))
            } else {
                Ok(mcp_tool_result(id, structured, is_error, result_options))
            }
        }
        _ => Ok(McpHttpResponse::Json(JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("id", id),
            (
                "error",
                JsonValue::object([
                    ("code", JsonValue::number(-32601)),
                    (
                        "message",
                        JsonValue::string(format!("unsupported MCP method '{}'", method)),
                    ),
                ]),
            ),
        ]))),
    }
}

fn touch_mcp_session_for_request(
    request: &HttpRequest,
    config: &DashboardConfig,
    id: JsonValue,
) -> Result<http_session::McpHttpSession, McpHttpResponse> {
    let result = config
        .http_session_store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .touch_from_request(request, now_ms());
    match result {
        Ok(session) => Ok(session),
        Err(error) => {
            let code = match error.kind {
                http_session::McpHttpSessionErrorKind::Missing
                | http_session::McpHttpSessionErrorKind::Invalid
                | http_session::McpHttpSessionErrorKind::ProtocolMismatch
                | http_session::McpHttpSessionErrorKind::DuplicateRequestId => {
                    mcp::ERROR_INVALID_REQUEST
                }
                http_session::McpHttpSessionErrorKind::Unknown
                | http_session::McpHttpSessionErrorKind::Expired => mcp::ERROR_NOT_INITIALIZED,
            };
            Err(McpHttpResponse::JsonStatus(
                error.http_status(),
                mcp_error_response(id, code, error.message),
            ))
        }
    }
}

fn mark_mcp_session_initialized(
    request: &HttpRequest,
    config: &DashboardConfig,
    id: JsonValue,
) -> Result<http_session::McpHttpSession, McpHttpResponse> {
    let result = config
        .http_session_store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .mark_initialized_from_request(request, now_ms());
    match result {
        Ok(session) => Ok(session),
        Err(error) => Err(session_error_response(error, id)),
    }
}

fn track_mcp_request_id(
    config: &DashboardConfig,
    session_id: &str,
    request_id_key: &str,
    id: JsonValue,
) -> Result<http_session::McpHttpSession, McpHttpResponse> {
    let result = config
        .http_session_store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .track_request_id(session_id, request_id_key);
    match result {
        Ok(session) => Ok(session),
        Err(error) => Err(session_error_response(error, id)),
    }
}

fn session_error_response(
    error: http_session::McpHttpSessionError,
    id: JsonValue,
) -> McpHttpResponse {
    let code = match error.kind {
        http_session::McpHttpSessionErrorKind::Missing
        | http_session::McpHttpSessionErrorKind::Invalid
        | http_session::McpHttpSessionErrorKind::ProtocolMismatch
        | http_session::McpHttpSessionErrorKind::DuplicateRequestId => mcp::ERROR_INVALID_REQUEST,
        http_session::McpHttpSessionErrorKind::Unknown
        | http_session::McpHttpSessionErrorKind::Expired => mcp::ERROR_NOT_INITIALIZED,
    };
    McpHttpResponse::JsonStatus(
        error.http_status(),
        mcp_error_response(id, code, error.message),
    )
}

fn mcp_tool_result(
    id: JsonValue,
    structured: JsonValue,
    is_error: bool,
    options: ToolResultOptions,
) -> McpHttpResponse {
    mcp_tool_result_payload(
        id,
        tool_result::tool_result_payload(structured, is_error, options),
    )
}

fn mcp_tool_result_payload(id: JsonValue, payload: JsonValue) -> McpHttpResponse {
    McpHttpResponse::Json(JsonValue::object([
        ("jsonrpc", JsonValue::string("2.0")),
        ("id", id),
        ("result", payload),
    ]))
}

fn mcp_error_response(id: JsonValue, code: i64, message: impl Into<String>) -> JsonValue {
    JsonValue::object([
        ("jsonrpc", JsonValue::string("2.0")),
        ("id", id),
        (
            "error",
            JsonValue::object([
                ("code", JsonValue::number(code)),
                ("message", JsonValue::string(message.into())),
            ]),
        ),
    ])
}
