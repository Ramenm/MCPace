use super::{
    empty_object, http_boundary, http_headers, http_session, http_tool_definitions,
    http_tool_definitions_for_request, now_ms, reject_forbidden_origin, run_http_tool,
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
            if reject_forbidden_origin(stream, request)? {
                return Ok(());
            }
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
            if reject_forbidden_origin(stream, request)? {
                return Ok(());
            }
            write_empty_response(stream, "202 Accepted")?;
        }
        "POST" => {
            if reject_forbidden_origin(stream, request)? {
                return Ok(());
            }
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
    http_boundary::validate_origin(request)?;
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
    let body_text = std::str::from_utf8(&request.body)
        .map_err(|error| format!("invalid UTF-8 request body: {}", error))?;
    let message =
        parse_str(body_text.trim()).map_err(|error| format!("invalid JSON-RPC body: {}", error))?;
    let id = json_helpers::value_at_path(&message, &["id"]).cloned();
    let method = json_helpers::string_at_path(&message, &["method"]);

    if id.is_none() {
        if method.is_some() || json_helpers::value_at_path(&message, &["result"]).is_some() {
            return Ok(McpHttpResponse::Accepted);
        }
        if json_helpers::value_at_path(&message, &["error"]).is_some() {
            return Ok(McpHttpResponse::Accepted);
        }
    }

    let id = id.unwrap_or(JsonValue::Null);
    let method = method.ok_or_else(|| "missing JSON-RPC method".to_string())?;
    if let Some(protocol_header) =
        http_boundary::request_header_string(Some(request), "mcp-protocol-version")
    {
        let protocol_header = protocol_header.trim();
        if !protocol_header.is_empty() && !mcp::is_supported_protocol_version(protocol_header) {
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
    }
    if let Err(error) = http_headers::validate_mcp_standard_headers(request, &message, method) {
        return Ok(McpHttpResponse::JsonStatus(
            "400 Bad Request",
            mcp_error_response(id, mcp::ERROR_HEADER_MISMATCH, error),
        ));
    }

    match method {
        "initialize" => {
            let requested = json_helpers::string_at_path(&message, &["params", "protocolVersion"])
                .unwrap_or(mcp::CURRENT_PROTOCOL_VERSION);
            let negotiated = mcp::negotiate_protocol_version(requested);

            let session_id = http_boundary::request_header_string(Some(request), "mcp-session-id")
                .and_then(|value| http_session::normalize_mcp_http_session_id(&value))
                .unwrap_or_else(|| {
                    http_session::generated_mcp_http_session_id(request, &id, negotiated)
                });
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
            let protocol =
                http_boundary::request_header_string(Some(request), "mcp-protocol-version")
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| mcp::STREAMABLE_HTTP_DEFAULT_PROTOCOL_VERSION.to_string());
            let protocol_params =
                JsonValue::object([("protocolVersion", JsonValue::string(protocol))]);
            let result = adapter::tool_list_result(
                &config.root_path,
                http_tool_definitions_for_request(request),
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
            let args = json_helpers::value_at_path(&message, &["params", "arguments"])
                .cloned()
                .unwrap_or_else(empty_object);
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
            let tool_name = json_helpers::string_at_path(&message, &["params", "name"])
                .ok_or_else(|| "tools/call requires a tool name".to_string())?;
            let args = json_helpers::value_at_path(&message, &["params", "arguments"])
                .cloned()
                .unwrap_or_else(empty_object);
            let projected_call = tool_name.starts_with("u_");
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
