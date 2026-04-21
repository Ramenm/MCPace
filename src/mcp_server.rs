use crate::app;
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

const CURRENT_PROTOCOL_VERSION: &str = "2025-11-25";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const SERVER_NAME: &str = "mcpace";

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
                let response = jsonrpc_error(
                    JsonValue::Null,
                    -32700,
                    "Invalid JSON message",
                    Some(JsonValue::object([("details", JsonValue::string(error))])),
                );
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
                continue;
            }
        };

        let Some(method) = json_helpers::string_at_path(&message, &["method"]) else {
            continue;
        };
        let id = request_id(&message);

        match method {
            "initialize" => {
                initialize_seen = true;
                initialize_params = json_helpers::value_at_path(&message, &["params"]).cloned();
                let startup_notes = bootstrap_hub_runtime(&config, stderr);
                let requested =
                    json_helpers::string_at_path(&message, &["params", "protocolVersion"])
                        .unwrap_or(CURRENT_PROTOCOL_VERSION);
                let negotiated = negotiate_protocol_version(requested);
                let response = jsonrpc_result(
                    id,
                    JsonValue::object([
                        ("protocolVersion", JsonValue::string(negotiated.to_string())),
                        (
                            "capabilities",
                            JsonValue::object([("tools", empty_object())]),
                        ),
                        (
                            "serverInfo",
                            JsonValue::object([
                                ("name", JsonValue::string(SERVER_NAME)),
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
                let response = jsonrpc_result(id, empty_object());
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            "tools/list" => {
                if !initialize_seen {
                    let response = jsonrpc_error(id, -32002, "Server not initialized", None);
                    if write_message(stdout, &response).is_err() {
                        return 1;
                    }
                    continue;
                }

                let tools = JsonValue::array(TOOL_SPECS.iter().map(|tool| tool_definition(tool)));
                let response = jsonrpc_result(id, JsonValue::object([("tools", tools)]));
                if write_message(stdout, &response).is_err() {
                    return 1;
                }
            }
            "tools/call" => {
                if !initialize_seen {
                    let response = jsonrpc_error(id, -32002, "Server not initialized", None);
                    if write_message(stdout, &response).is_err() {
                        return 1;
                    }
                    continue;
                }

                let name = match json_helpers::string_at_path(&message, &["params", "name"]) {
                    Some(value) => value,
                    None => {
                        let response =
                            jsonrpc_error(id, -32602, "tools/call requires a tool name", None);
                        if write_message(stdout, &response).is_err() {
                            return 1;
                        }
                        continue;
                    }
                };
                let arguments = json_helpers::value_at_path(&message, &["params", "arguments"])
                    .cloned()
                    .unwrap_or_else(empty_object);

                let result =
                    match execute_tool(&config, name, &arguments, initialize_params.as_ref()) {
                        Ok(value) => value,
                        Err(ToolCallError::InvalidParams(message)) => {
                            jsonrpc_error(id, -32602, &message, None)
                        }
                        Err(ToolCallError::UnknownTool(message)) => {
                            jsonrpc_error(id, -32601, &message, None)
                        }
                        Err(ToolCallError::Execution(message)) => {
                            jsonrpc_result(id, tool_error_result(message))
                        }
                    };
                if write_message(stdout, &result).is_err() {
                    return 1;
                }
            }
            _ => {
                let response = jsonrpc_error(
                    id,
                    -32601,
                    &format!("unsupported MCP method '{}'", method),
                    None,
                );
                if write_message(stdout, &response).is_err() {
                    return 1;
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

fn negotiate_protocol_version(requested: &str) -> &'static str {
    if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
        return SUPPORTED_PROTOCOL_VERSIONS
            .iter()
            .find(|candidate| **candidate == requested)
            .copied()
            .unwrap_or(CURRENT_PROTOCOL_VERSION);
    }
    CURRENT_PROTOCOL_VERSION
}

fn request_id(message: &JsonValue) -> JsonValue {
    json_helpers::value_at_path(message, &["id"])
        .cloned()
        .unwrap_or(JsonValue::Null)
}

fn write_message(stdout: &mut dyn Write, message: &JsonValue) -> io::Result<()> {
    writeln!(stdout, "{}", message.to_compact_string())?;
    stdout.flush()
}

fn jsonrpc_result(id: JsonValue, result: JsonValue) -> JsonValue {
    JsonValue::object([
        ("jsonrpc", JsonValue::string("2.0")),
        ("id", id),
        ("result", result),
    ])
}

fn jsonrpc_error(id: JsonValue, code: i64, message: &str, data: Option<JsonValue>) -> JsonValue {
    let error_value = match data {
        Some(value) => JsonValue::object([
            ("code", JsonValue::number(code)),
            ("message", JsonValue::string(message)),
            ("data", value),
        ]),
        None => JsonValue::object([
            ("code", JsonValue::number(code)),
            ("message", JsonValue::string(message)),
        ]),
    };

    JsonValue::object([
        ("jsonrpc", JsonValue::string("2.0")),
        ("id", id),
        ("error", error_value),
    ])
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
                _ => JsonValue::object([
                    ("type", JsonValue::string("object")),
                    ("properties", empty_object()),
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
        _ => {
            return Err(ToolCallError::UnknownTool(format!(
                "unknown MCPace tool '{}'",
                tool_name
            )))
        }
    };

    Ok(tool_success_result(result))
}

fn run_json_command(config: &ServerConfig, args: &[&str]) -> Result<JsonValue, ToolCallError> {
    run_json_command_vec(
        config,
        args.iter().map(|value| (*value).to_string()).collect(),
    )
}

fn run_json_command_vec(
    config: &ServerConfig,
    mut args: Vec<String>,
) -> Result<JsonValue, ToolCallError> {
    args.push("--root".to_string());
    args.push(config.root_path.display().to_string());

    let mut stdout_buffer = Vec::new();
    let mut stderr_buffer = Vec::new();
    let exit_code = app::run(args, &mut stdout_buffer, &mut stderr_buffer);
    if exit_code != 0 {
        let stderr_text = String::from_utf8(stderr_buffer).unwrap_or_default();
        let stdout_text = String::from_utf8(stdout_buffer).unwrap_or_default();
        let details = if !stderr_text.trim().is_empty() {
            stderr_text.trim().to_string()
        } else if !stdout_text.trim().is_empty() {
            stdout_text.trim().to_string()
        } else {
            format!("command failed with exit code {}", exit_code)
        };
        return Err(ToolCallError::Execution(details));
    }

    let stdout_text = String::from_utf8(stdout_buffer)
        .map_err(|error| ToolCallError::Execution(format!("non-UTF8 command output: {}", error)))?;
    parse_str(stdout_text.trim()).map_err(|error| {
        ToolCallError::Execution(format!("command produced invalid JSON: {}", error))
    })
}

fn build_client_args(
    action: &str,
    config: &ServerConfig,
    arguments: &JsonValue,
    initialize_params: Option<&JsonValue>,
) -> Result<Vec<String>, ToolCallError> {
    let client_id = optional_string_argument(arguments, "clientId")?
        .or_else(|| Some(config.client_id.clone()))
        .unwrap_or_else(|| "generic-stdio".to_string());
    let session_id =
        optional_string_argument(arguments, "sessionId")?.or_else(|| config.session_id.clone());
    let project_root =
        optional_string_argument(arguments, "projectRoot")?.or_else(|| config.project_root.clone());
    let transport = optional_string_argument(arguments, "transport")?
        .or_else(|| Some(config.transport.clone()));
    let metadata =
        optional_value_argument(arguments, "metadata").or_else(|| initialize_params.cloned());

    let mut args = vec![
        "client".to_string(),
        action.to_string(),
        "--json".to_string(),
    ];
    if action == "export" {
        args.push(client_id.clone());
    } else {
        args.push("--client-id".to_string());
        args.push(client_id.clone());
    }
    if let Some(value) = session_id {
        args.push("--session-id".to_string());
        args.push(value);
    }
    if let Some(value) = project_root {
        args.push("--project-root".to_string());
        args.push(value);
    }
    if let Some(value) = transport {
        args.push("--transport".to_string());
        args.push(value);
    }
    if let Some(value) = metadata {
        args.push("--metadata-json".to_string());
        args.push(value.to_compact_string());
    }
    Ok(args)
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

fn empty_object() -> JsonValue {
    JsonValue::Object(BTreeMap::new())
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
