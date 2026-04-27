use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use crate::{app, upstream};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[derive(Debug, Default)]
struct ParsedArgs {
    help: bool,
    root_override: Option<PathBuf>,
    client_id: Option<String>,
    session_id: Option<String>,
    project_root: Option<String>,
    transport: Option<String>,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct ServerConfig {
    root_path: PathBuf,
    client_id: String,
    session_id: Option<String>,
    project_root: Option<String>,
    transport: String,
}

#[derive(Clone, Copy, Debug)]
struct ToolSpec {
    name: &'static str,
    title: &'static str,
    description: &'static str,
}

const TOOL_SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "doctor",
        title: "Inspect MCPace readiness",
        description: "Return the native MCPace doctor report for this root.",
    },
    ToolSpec {
        name: "hub_status",
        title: "Inspect hub status",
        description: "Return the local hub status report without changing state.",
    },
    ToolSpec {
        name: "hub_up",
        title: "Start the hub",
        description: "Start the local MCPace hub runtime for this root.",
    },
    ToolSpec {
        name: "hub_down",
        title: "Stop the hub",
        description: "Stop the local MCPace hub runtime for this root.",
    },
    ToolSpec {
        name: "hub_logs",
        title: "Read hub logs",
        description: "Return recent MCPace hub log events.",
    },
    ToolSpec {
        name: "runtime_leases",
        title: "List runtime leases",
        description: "Return the current MCPace runtime lease store after pruning expired leases.",
    },
    ToolSpec {
        name: "runtime_acquire",
        title: "Acquire a runtime lease",
        description:
            "Acquire the scheduler lease for one configured server before routing work to it.",
    },
    ToolSpec {
        name: "runtime_renew",
        title: "Renew a runtime lease",
        description: "Extend a previously acquired MCPace scheduler lease before it expires.",
    },
    ToolSpec {
        name: "runtime_release",
        title: "Release a runtime lease",
        description: "Release a previously acquired MCPace scheduler lease.",
    },
    ToolSpec {
        name: "server_list",
        title: "List configured servers",
        description: "Return the grouped MCPace server inventory for this root.",
    },
    ToolSpec {
        name: "server_capabilities",
        title: "Inspect one server",
        description: "Return grouped capability details for one configured server.",
    },
    ToolSpec {
        name: "client_list",
        title: "List known client targets",
        description: "Return the documented client surface catalog.",
    },
    ToolSpec {
        name: "client_plan",
        title: "Build a client routing plan",
        description: "Resolve routing, session, and server arbitration for a client.",
    },
    ToolSpec {
        name: "client_export",
        title: "Build a client connection contract",
        description: "Return the client launcher contract for a target surface.",
    },
    ToolSpec {
        name: "upstream_tools",
        title: "List one upstream server's tools",
        description: "List callable tools for one configured stdio upstream MCP server; omit server for fast inventory only.",
    },
    ToolSpec {
        name: "upstream_catalog",
        title: "Catalog configured upstream tools",
        description: "Discover configured upstream MCP tools with concise flat server-qualified descriptions and upstream_call arguments without hardcoded server names.",
    },
    ToolSpec {
        name: "upstream_probe",
        title: "Probe configured upstream servers",
        description: "Probe configured upstream MCP servers without hardcoded server names; uses the short successful tools/list cache unless refresh=true is supplied.",
    },
    ToolSpec {
        name: "upstream_call",
        title: "Call one upstream tool",
        description: "Call a tool on a configured stdio upstream MCP server.",
    },
    ToolSpec {
        name: "upstream_batch",
        title: "Call upstream tools in one session",
        description: "Call multiple tools on one configured stdio upstream MCP server in a single state-preserving session.",
    },
    ToolSpec {
        name: "browser_status",
        title: "Explain browser bridge status",
        description: "Explain browser MCP bridge status from the configured upstream inventory.",
    },
];

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
        let method = match json_helpers::string_at_path(&message, &["method"]) {
            Some(value) => value,
            None => {
                if let Some(id) = request_id {
                    let response = mcp::error(
                        id,
                        mcp::ERROR_INVALID_REQUEST,
                        "JSON-RPC request requires a method",
                        None,
                    );
                    if write_message(stdout, &response).is_err() {
                        return 1;
                    }
                }
                continue;
            }
        };

        match method {
            "initialize" => {
                let Some(id) = request_id else {
                    continue;
                };
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
                        (
                            "capabilities",
                            JsonValue::object([("tools", mcp::empty_object())]),
                        ),
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

                let tools = JsonValue::array(TOOL_SPECS.iter().map(|tool| tool_definition(tool)));
                let response = mcp::result(id, JsonValue::object([("tools", tools)]));
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
                let arguments = json_helpers::value_at_path(&message, &["params", "arguments"])
                    .cloned()
                    .unwrap_or_else(mcp::empty_object);

                let response =
                    match execute_tool(&config, name, &arguments, initialize_params.as_ref()) {
                        Ok(value) => mcp::result(id, value),
                        Err(ToolCallError::InvalidParams(message)) => {
                            mcp::error(id, mcp::ERROR_INVALID_PARAMS, &message, None)
                        }
                        Err(ToolCallError::UnknownTool(message)) => {
                            mcp::error(id, mcp::ERROR_METHOD_NOT_FOUND, &message, None)
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

fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("mcp-server requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                index += 2;
            }
            "--client-id" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("mcp-server requires a value after --client-id".to_string());
                    return parsed;
                };
                parsed.client_id = Some(value.to_string());
                index += 2;
            }
            "--session-id" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("mcp-server requires a value after --session-id".to_string());
                    return parsed;
                };
                parsed.session_id = Some(value.to_string());
                index += 2;
            }
            "--project-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("mcp-server requires a value after --project-root".to_string());
                    return parsed;
                };
                parsed.project_root = Some(value.to_string());
                index += 2;
            }
            "--transport" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error =
                        Some("mcp-server requires a value after --transport".to_string());
                    return parsed;
                };
                parsed.transport = Some(value.to_string());
                index += 2;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            other => {
                parsed.error = Some(format!("unsupported mcp-server argument: {}", other));
                return parsed;
            }
        }
    }

    parsed
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace mcp-server [--root <path>] [--client-id <id>] \
         [--session-id <id>] [--project-root <path>] \
         [--transport <stdio|streamable-http>]"
    );
    let _ = writeln!(stdout, "");
    let _ = writeln!(
        stdout,
        "mcp-server starts a live MCP stdio server for local clients."
    );
    let _ = writeln!(
        stdout,
        "It speaks newline-delimited JSON-RPC over stdin/stdout and exposes a \
         focused MCPace management tool catalog."
    );
}

fn write_message(stdout: &mut dyn Write, message: &JsonValue) -> io::Result<()> {
    writeln!(stdout, "{}", message.to_compact_string())?;
    stdout.flush()
}

fn tool_definition(tool: &ToolSpec) -> JsonValue {
    JsonValue::object([
        ("name", JsonValue::string(tool.name)),
        ("title", JsonValue::string(tool.title)),
        ("description", JsonValue::string(tool.description)),
        (
            "inputSchema",
            match tool.name {
                "hub_logs" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([(
                            "tail",
                            JsonValue::object([
                                ("type", JsonValue::string("integer")),
                                (
                                    "description",
                                    JsonValue::string("Optional number of log lines to return."),
                                ),
                            ]),
                        )]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "runtime_acquire" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "server",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Configured MCPace server name to lease.",
                                        ),
                                    ),
                                ]),
                            ),
                            (
                                "clientId",
                                JsonValue::object([("type", JsonValue::string("string"))]),
                            ),
                            (
                                "sessionId",
                                JsonValue::object([("type", JsonValue::string("string"))]),
                            ),
                            (
                                "projectRoot",
                                JsonValue::object([("type", JsonValue::string("string"))]),
                            ),
                            (
                                "transport",
                                JsonValue::object([("type", JsonValue::string("string"))]),
                            ),
                            (
                                "ttlMs",
                                JsonValue::object([("type", JsonValue::string("integer"))]),
                            ),
                            (
                                "metadata",
                                JsonValue::object([("type", JsonValue::string("object"))]),
                            ),
                        ]),
                    ),
                    ("required", JsonValue::array([JsonValue::string("server")])),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "runtime_renew" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "leaseId",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string("Lease id returned by runtime_acquire."),
                                    ),
                                ]),
                            ),
                            (
                                "ttlMs",
                                JsonValue::object([
                                    ("type", JsonValue::string("integer")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional renewed lease TTL in milliseconds.",
                                        ),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    ("required", JsonValue::array([JsonValue::string("leaseId")])),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "runtime_release" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([(
                            "leaseId",
                            JsonValue::object([
                                ("type", JsonValue::string("string")),
                                (
                                    "description",
                                    JsonValue::string("Lease id returned by runtime_acquire."),
                                ),
                            ]),
                        )]),
                    ),
                    ("required", JsonValue::array([JsonValue::string("leaseId")])),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "server_capabilities" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([(
                            "name",
                            JsonValue::object([
                                ("type", JsonValue::string("string")),
                                (
                                    "description",
                                    JsonValue::string("Configured MCPace server name to inspect."),
                                ),
                            ]),
                        )]),
                    ),
                    ("required", JsonValue::array([JsonValue::string("name")])),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "client_plan" | "client_export" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "clientId",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional client target override. Defaults to the \
                                             server launch client id.",
                                        ),
                                    ),
                                ]),
                            ),
                            (
                                "sessionId",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string("Optional external session id override."),
                                    ),
                                ]),
                            ),
                            (
                                "projectRoot",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string("Optional project root override."),
                                    ),
                                ]),
                            ),
                            (
                                "transport",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional ingress override such as stdio or \
                                             streamable-http.",
                                        ),
                                    ),
                                ]),
                            ),
                            (
                                "metadata",
                                JsonValue::object([
                                    ("type", JsonValue::string("object")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional MCP metadata object forwarded as \
                                             metadata-json.",
                                        ),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "upstream_tools" | "upstream_catalog" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "server",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional configured upstream server name from mcp_settings.json.",
                                        ),
                                    ),
                                ]),
                            ),
                            (
                                "timeoutMs",
                                JsonValue::object([
                                    ("type", JsonValue::string("integer")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional per-server timeout from 1000 to 300000 ms.",
                                        ),
                                    ),
                                ]),
                            ),
                            (
                                "refresh",
                                JsonValue::object([
                                    ("type", JsonValue::string("boolean")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Bypass the short in-process tools/list cache and refresh from the upstream server.",
                                        ),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "upstream_probe" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "server",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional configured upstream server name from mcp_settings.json.",
                                        ),
                                    ),
                                ]),
                            ),
                            (
                                "timeoutMs",
                                JsonValue::object([
                                    ("type", JsonValue::string("integer")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional per-server timeout from 1000 to 300000 ms.",
                                        ),
                                    ),
                                ]),
                            ),
                            (
                                "refresh",
                                JsonValue::object([
                                    ("type", JsonValue::string("boolean")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Bypass the short successful tools/list cache and force a fresh upstream probe.",
                                        ),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "upstream_call" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "server",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string("Configured upstream server name."),
                                    ),
                                ]),
                            ),
                            (
                                "tool",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    ("description", JsonValue::string("Upstream tool name.")),
                                ]),
                            ),
                            (
                                "arguments",
                                JsonValue::object([
                                    ("type", JsonValue::string("object")),
                                    (
                                        "description",
                                        JsonValue::string("Arguments to pass to the upstream tool."),
                                    ),
                                    ("additionalProperties", JsonValue::bool(true)),
                                ]),
                            ),
                            (
                                "timeoutMs",
                                JsonValue::object([
                                    ("type", JsonValue::string("integer")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional per-call timeout from 1000 to 300000 ms.",
                                        ),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    (
                        "required",
                        JsonValue::array([JsonValue::string("server"), JsonValue::string("tool")]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                "upstream_batch" => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    (
                        "properties",
                        JsonValue::object([
                            (
                                "server",
                                JsonValue::object([
                                    ("type", JsonValue::string("string")),
                                    (
                                        "description",
                                        JsonValue::string("Configured upstream server name."),
                                    ),
                                ]),
                            ),
                            (
                                "calls",
                                JsonValue::object([
                                    ("type", JsonValue::string("array")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Ordered upstream calls to execute after one initialize handshake.",
                                        ),
                                    ),
                                    (
                                        "items",
                                        JsonValue::object([
                                            ("type", JsonValue::string("object")),
                                            (
                                                "properties",
                                                JsonValue::object([
                                                    (
                                                        "tool",
                                                        JsonValue::object([
                                                            ("type", JsonValue::string("string")),
                                                            (
                                                                "description",
                                                                JsonValue::string(
                                                                    "Upstream tool name.",
                                                                ),
                                                            ),
                                                        ]),
                                                    ),
                                                    (
                                                        "arguments",
                                                        JsonValue::object([
                                                            ("type", JsonValue::string("object")),
                                                            (
                                                                "description",
                                                                JsonValue::string(
                                                                    "Arguments to pass to the upstream tool.",
                                                                ),
                                                            ),
                                                            (
                                                                "additionalProperties",
                                                                JsonValue::bool(true),
                                                            ),
                                                        ]),
                                                    ),
                                                ]),
                                            ),
                                            (
                                                "required",
                                                JsonValue::array([JsonValue::string("tool")]),
                                            ),
                                            ("additionalProperties", JsonValue::bool(false)),
                                        ]),
                                    ),
                                ]),
                            ),
                            (
                                "timeoutMs",
                                JsonValue::object([
                                    ("type", JsonValue::string("integer")),
                                    (
                                        "description",
                                        JsonValue::string(
                                            "Optional total batch timeout from 1000 to 300000 ms.",
                                        ),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    (
                        "required",
                        JsonValue::array([JsonValue::string("server"), JsonValue::string("calls")]),
                    ),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
                _ => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    ("properties", mcp::empty_object()),
                    ("additionalProperties", JsonValue::bool(false)),
                ]),
            },
        ),
    ])
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
) -> Result<JsonValue, ToolCallError> {
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
        "upstream_call" => {
            let server = required_string_argument(arguments, "server")?;
            let tool = required_string_argument(arguments, "tool")?;
            let upstream_arguments =
                optional_value_argument(arguments, "arguments").unwrap_or_else(mcp::empty_object);
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            upstream::call_tool(
                &config.root_path,
                &server,
                &tool,
                &upstream_arguments,
                timeout_ms,
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
                let upstream_arguments = json_helpers::value_at_path(raw_call, &["arguments"])
                    .cloned()
                    .unwrap_or_else(mcp::empty_object);
                calls.push(upstream::UpstreamToolCall {
                    tool,
                    arguments: upstream_arguments,
                });
            }
            let timeout_ms = timeout_argument(arguments, "timeoutMs")?;
            upstream::call_tools(&config.root_path, &server, &calls, timeout_ms)
                .map_err(ToolCallError::Execution)?
        }
        "browser_status" => {
            upstream::browser_status(&config.root_path).map_err(ToolCallError::Execution)?
        }
        _ => {
            return Err(ToolCallError::UnknownTool(format!(
                "unknown MCPace tool '{}'",
                tool_name
            )))
        }
    };

    Ok(tool_success_result(result))
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

fn timeout_argument(arguments: &JsonValue, key: &str) -> Result<Option<u64>, ToolCallError> {
    Ok(integer_argument(arguments, key)?
        .filter(|value| *value > 0)
        .map(|value| value as u64))
}

fn optional_value_argument(arguments: &JsonValue, key: &str) -> Option<JsonValue> {
    json_helpers::value_at_path(arguments, &[key]).cloned()
}

fn tool_success_result(value: JsonValue) -> JsonValue {
    let text = value.to_pretty_string();
    JsonValue::object([
        (
            "content",
            JsonValue::array([JsonValue::object([
                ("type", JsonValue::string("text")),
                ("text", JsonValue::string(text)),
            ])]),
        ),
        ("structuredContent", value),
        ("isError", JsonValue::bool(false)),
    ])
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
    let base =
        "Use MCPace tools to inspect readiness, manage the hub, and build client routing/export contracts.";
    if startup_notes.is_empty() {
        return base.to_string();
    }

    format!("{} Startup notes: {}", base, startup_notes.join(" | "))
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
