use super::process_config::manager_data_path;
#[cfg(unix)]
use super::stdio_runtime::spawn_stdio_server;
use super::*;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn temp_root() -> PathBuf {
    let unique = format!(
        "mcpace-upstream-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = env::temp_dir().join(unique);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn config_declared_sensitive_tools_require_explicit_policy_flags() {
    let server = UpstreamServerConfig {
        name: "custom-desktop".to_string(),
        enabled: true,
        disabled_reason: None,
        source_type: "stdio".to_string(),
        command: Some("tool".to_string()),
        args: Vec::new(),
        env: BTreeMap::new(),
        cwd: None,
        url: None,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        tool_policies: vec![
            ToolRiskPolicy {
                tools: vec!["Screenshot".to_string(), "Snapshot".to_string()],
                risk_class: Some("desktop-observation".to_string()),
                allow_argument: Some("allowDesktopObservation".to_string()),
                description: None,
            },
            ToolRiskPolicy {
                tools: vec!["Type".to_string(), "Shortcut".to_string()],
                risk_class: Some("desktop-control".to_string()),
                allow_argument: Some("allowDesktopControl".to_string()),
                description: None,
            },
            ToolRiskPolicy {
                tools: vec!["Power*".to_string()],
                risk_class: Some("system-control".to_string()),
                allow_argument: Some("allowSystemControl".to_string()),
                description: None,
            },
        ],
    };
    let ungated_server = UpstreamServerConfig {
        name: "ungated".to_string(),
        tool_policies: Vec::new(),
        ..server.clone()
    };

    assert!(validate_upstream_tool_policy(&server, "Wait", None).is_ok());
    assert!(validate_upstream_tool_policy(&ungated_server, "Type", None).is_ok());

    let blocked_screenshot = validate_upstream_tool_policy(&server, "Screenshot", None)
        .expect_err("screenshot should require observation opt-in");
    assert!(blocked_screenshot.contains("allowDesktopObservation=true"));
    assert!(blocked_screenshot.contains("desktop-observation"));

    let blocked_type = validate_upstream_tool_policy(&server, "Type", None)
        .expect_err("type should require desktop-control opt-in");
    assert!(blocked_type.contains("allowDesktopControl=true"));

    let blocked_powershell = validate_upstream_tool_policy(&server, "PowerShell", None)
        .expect_err("powershell should require system-control opt-in");
    assert!(blocked_powershell.contains("allowSystemControl=true"));

    let mut observation_args = BTreeSet::new();
    observation_args.insert("allowDesktopObservation".to_string());
    let observation = UpstreamLeaseContext {
        allow_arguments: observation_args,
        ..Default::default()
    };
    assert!(validate_upstream_tool_policy(&server, "Snapshot", Some(&observation)).is_ok());
    assert!(validate_upstream_tool_policy(&server, "Screenshot", Some(&observation)).is_ok());

    let mut desktop_risks = BTreeSet::new();
    desktop_risks.insert("desktop-control".to_string());
    let desktop_control = UpstreamLeaseContext {
        allowed_tool_risk_classes: desktop_risks,
        ..Default::default()
    };
    assert!(validate_upstream_tool_policy(&server, "Type", Some(&desktop_control)).is_ok());
    assert!(validate_upstream_tool_policy(&server, "Shortcut", Some(&desktop_control)).is_ok());

    let mut system_args = BTreeSet::new();
    system_args.insert("allowSystemControl".to_string());
    let system_control = UpstreamLeaseContext {
        allow_arguments: system_args,
        ..Default::default()
    };
    assert!(validate_upstream_tool_policy(&server, "PowerShell", Some(&system_control)).is_ok());

    let batch = [UpstreamToolCall {
        tool: "Type".to_string(),
        arguments: mcp::empty_object(),
    }];
    assert!(validate_upstream_batch_tool_policy(&server, &batch, None).is_err());
    assert!(validate_upstream_batch_tool_policy(&server, &batch, Some(&desktop_control)).is_ok());
}

#[test]
fn allow_policy_argument_collectors_normalize_shared_bridge_inputs() {
    let args = parse_str(
        r#"{
              "allowDesktopObservation": true,
              "allowDesktopControl": false,
              "allowSystemControl": null,
              "allowArguments": [" allowCustomRisk ", "", "allowFilesystemMutation"],
              "allowToolRiskClasses": ["Desktop-Control", " filesystem-mutation ", ""]
            }"#,
    )
    .unwrap();

    let allow_arguments = collect_allow_arguments(&args).unwrap();
    assert!(allow_arguments.contains("allowDesktopObservation"));
    assert!(allow_arguments.contains("allowCustomRisk"));
    assert!(allow_arguments.contains("allowFilesystemMutation"));
    assert!(!allow_arguments.contains("allowDesktopControl"));
    assert!(!allow_arguments.contains("allowSystemControl"));
    assert!(!allow_arguments.contains("allowToolRiskClasses"));

    let risk_classes = collect_allowed_tool_risk_classes(&args).unwrap();
    assert_eq!(
        risk_classes,
        BTreeSet::from([
            "desktop-control".to_string(),
            "filesystem-mutation".to_string()
        ])
    );

    let bad_allow = parse_str(r#"{"allowArguments":[true]}"#).unwrap();
    assert_eq!(
        collect_allow_arguments(&bad_allow).unwrap_err(),
        "allowArguments must be an array of strings"
    );

    let bad_risk = parse_str(r#"{"allowToolRiskClasses":["ok", 1]}"#).unwrap();
    assert_eq!(
        collect_allowed_tool_risk_classes(&bad_risk).unwrap_err(),
        "allowToolRiskClasses must be an array of strings"
    );
}

#[test]
fn server_fingerprint_does_not_embed_secret_env_values() {
    let mut env = BTreeMap::new();
    env.insert("API_TOKEN".to_string(), "secret-value".to_string());
    let server = UpstreamServerConfig {
        name: "secret-probe".to_string(),
        enabled: true,
        disabled_reason: None,
        source_type: "stdio".to_string(),
        command: Some("node".to_string()),
        args: Vec::new(),
        env,
        cwd: None,
        url: None,
        timeout_ms: 1_000,
        tool_policies: Vec::new(),
    };

    let fingerprint = server_fingerprint(&server);

    assert!(fingerprint.contains("API_TOKEN:"));
    assert!(!fingerprint.contains("secret-value"));
    assert!(fingerprint.contains("len12-hash"));
}

#[test]
fn env_var_names_accept_codex_local_object_entries_and_skip_remote_entries() {
    let value = parse_str(
        r#"[
              "PLAIN_TOKEN",
              { "name": "LOCAL_OBJECT_TOKEN", "source": "local" },
              { "name": "DEFAULT_LOCAL_TOKEN" },
              { "name": "REMOTE_ONLY_TOKEN", "source": "remote" },
              { "source": "local" },
              ""
            ]"#,
    )
    .unwrap();
    let names = env_var_names_from_array(value.as_array());

    assert_eq!(
        names,
        vec![
            "PLAIN_TOKEN".to_string(),
            "LOCAL_OBJECT_TOKEN".to_string(),
            "DEFAULT_LOCAL_TOKEN".to_string(),
        ]
    );
}

#[test]
fn stderr_suffix_redacts_sensitive_diagnostics_without_removing_context() {
    let (tx, rx) = mpsc::channel();
    tx.send(
        "startup failed TOKEN = abc123 Authorization: Bearer bearer-secret workspace=/tmp/work"
            .to_string(),
    )
    .unwrap();
    drop(tx);

    let suffix = stderr_suffix(&rx);

    assert!(suffix.contains("startup failed"));
    assert!(suffix.contains("workspace=/tmp/work"));
    assert!(suffix.contains(DIAGNOSTIC_REDACTION));
    assert!(!suffix.contains("abc123"));
    assert!(!suffix.contains("bearer-secret"));
}

#[test]
fn stderr_suffix_bounds_diagnostic_line_count_and_length() {
    let (tx, rx) = mpsc::channel();
    for index in 0..8 {
        tx.send(format!("line-{index}: {}", "x".repeat(512)))
            .unwrap();
    }
    drop(tx);

    let suffix = stderr_suffix(&rx);

    assert!(suffix.contains("line-0"));
    assert!(suffix.contains("line-5"));
    assert!(!suffix.contains("line-6"));
    assert!(suffix.contains("<truncated>"));
    assert!(suffix.len() < 2_400);
}

#[cfg(unix)]
#[test]
fn spawn_stdio_server_does_not_forward_unspecified_parent_environment() {
    let root = temp_root();
    env::set_var("MCPACE_PARENT_SECRET_DO_NOT_FORWARD", "secret-value");
    env::set_var("MCPACE_ALLOWED_TOKEN_TEST", "allowed-value");
    let mut explicit_env = BTreeMap::new();
    explicit_env.insert("EXPLICIT_TOKEN".to_string(), "explicit-value".to_string());
    explicit_env.insert(
        "MCPACE_ALLOWED_TOKEN_TEST".to_string(),
        env::var("MCPACE_ALLOWED_TOKEN_TEST").unwrap(),
    );
    let server = UpstreamServerConfig {
            name: "env-probe".to_string(),
            enabled: true,
            disabled_reason: None,
            source_type: "stdio".to_string(),
            command: Some("sh".to_string()),
            args: vec![
                "-c".to_string(),
                r#"printf '%s|%s|%s|%s\n' "$MCPACE_PARENT_SECRET_DO_NOT_FORWARD" "$MCPACE_ALLOWED_TOKEN_TEST" "$EXPLICIT_TOKEN" "$MCPACE_PRIMARY_WORKSPACE""#.to_string(),
            ],
            env: explicit_env,
            cwd: None,
            url: None,
            timeout_ms: 1_000,
            tool_policies: Vec::new(),
        };

    let running = spawn_stdio_server(&root, &server).expect("spawn env probe");
    let line = running
        .stdout_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("env probe output");
    let fields = line.split('|').collect::<Vec<_>>();

    assert_eq!(fields[0], "");
    assert_eq!(fields[1], "allowed-value");
    assert_eq!(fields[2], "explicit-value");
    assert!(fields[3].contains(&root.display().to_string()));

    env::remove_var("MCPACE_PARENT_SECRET_DO_NOT_FORWARD");
    env::remove_var("MCPACE_ALLOWED_TOKEN_TEST");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn load_servers_attaches_declarative_tool_policies_from_project_config() {
    let root = temp_root();
    fs::write(
        root.join("mcp_settings.json"),
        r#"{
  "mcpServers": {
    "custom": { "enabled": true, "type": "stdio", "command": "node", "args": ["server.js"] }
  }
}"#,
    )
    .unwrap();
    fs::write(
        root.join("mcpace.config.json"),
        r#"{
  "servers": {
    "custom": {
      "toolPolicies": [
        {
          "riskClass": "custom-risk",
          "allowArgument": "allowCustomRisk",
          "tools": ["danger_*"]
        }
      ]
    }
  }
}"#,
    )
    .unwrap();

    let servers = load_servers(&root).expect("servers");
    let server = servers.get("custom").expect("custom server");

    let blocked = validate_upstream_tool_policy(server, "danger_write", None)
        .expect_err("config policy should block matching tool");
    assert!(blocked.contains("custom-risk"));
    assert!(blocked.contains("allowCustomRisk=true"));

    let mut allowed_risks = BTreeSet::new();
    allowed_risks.insert("custom-risk".to_string());
    let context = UpstreamLeaseContext {
        allowed_tool_risk_classes: allowed_risks,
        ..Default::default()
    };
    assert!(validate_upstream_tool_policy(server, "danger_write", Some(&context)).is_ok());
    assert!(validate_upstream_tool_policy(server, "safe_read", None).is_ok());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn load_servers_applies_profile_and_platform_policy_from_project_config() {
    let root = temp_root();
    let unsupported_platform = if current_platform_alias() == "windows" {
        "linux"
    } else {
        "windows"
    };
    fs::write(
            root.join("mcp_settings.json"),
            r#"{
  "mcpServers": {
    "profile-blocked": { "enabled": true, "type": "stdio", "command": "node", "args": ["server.js"] },
    "platform-blocked": { "enabled": true, "type": "stdio", "command": "node", "args": ["server.js"] }
  }
}"#,
        )
        .unwrap();
    fs::write(
        root.join("mcpace.config.json"),
        format!(
            r#"{{
  "profiles": {{
    "runtime": {{
      "default": "safe",
      "profiles": {{
        "safe": {{ "serverOverrides": {{}} }}
      }}
    }}
  }},
  "servers": {{
    "profile-blocked": {{ "required": false, "defaultEnabled": false }},
    "platform-blocked": {{ "required": false, "defaultEnabled": true, "platforms": ["{}"] }}
  }}
}}"#,
            unsupported_platform
        ),
    )
    .unwrap();

    let servers = load_servers(&root).expect("servers");
    let profile_blocked = servers.get("profile-blocked").expect("profile-blocked");
    assert!(!profile_blocked.enabled);
    assert!(profile_blocked
        .disabled_reason
        .as_deref()
        .unwrap_or_default()
        .contains("runtime profile"));

    let platform_blocked = servers.get("platform-blocked").expect("platform-blocked");
    assert!(!platform_blocked.enabled);
    assert!(platform_blocked
        .disabled_reason
        .as_deref()
        .unwrap_or_default()
        .contains("current platform"));

    let error = ensure_callable_stdio(&root, platform_blocked).expect_err("platform disabled");
    assert!(error.contains("disabled"));
    assert!(error.contains("current platform"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn server_runtime_callable_blocks_missing_cwd_before_spawn() {
    let root = temp_root();
    let missing_cwd = root.join("missing-workspace");
    let server = UpstreamServerConfig {
        name: "missing-cwd-probe".to_string(),
        enabled: true,
        disabled_reason: None,
        source_type: "stdio".to_string(),
        command: Some("node".to_string()),
        args: Vec::new(),
        env: BTreeMap::new(),
        cwd: Some(missing_cwd.clone()),
        url: None,
        timeout_ms: 1_000,
        tool_policies: Vec::new(),
    };

    let (callable, resolved, error) = server_runtime_callable(&root, &server);

    assert!(!callable);
    assert!(resolved.is_none());
    let error = error.expect("missing cwd error");
    assert!(error.contains("configured cwd"));
    assert!(error.contains(&missing_cwd.display().to_string()));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn relative_stdio_command_paths_resolve_against_configured_cwd() {
    let root = temp_root();
    let workspace = root.join("workspace");
    let bin_dir = workspace.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let executable = bin_dir.join("server-probe");
    fs::write(&executable, "#!/bin/sh\n").unwrap();

    let resolved = resolve_command_for_cwd("./bin/server-probe", &workspace)
        .expect("resolve relative command from cwd");

    assert_eq!(resolved, executable.canonicalize().unwrap());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn bare_launch_manager_commands_spawn_by_basename() {
    let resolved = if cfg!(windows) {
        PathBuf::from(r"C:\tools\bunx.exe")
    } else {
        PathBuf::from("/usr/local/bin/bunx")
    };
    assert_eq!(
        spawn_program_for_command("bunx", &resolved),
        PathBuf::from(resolved.file_name().unwrap())
    );
    #[cfg(windows)]
    assert_eq!(
        spawn_program_for_command("bunx", &PathBuf::from(r"C:\tools\bunx.EXE")),
        PathBuf::from("bunx.exe")
    );
    assert_eq!(
        spawn_program_for_command("./bin/server", &resolved),
        resolved
    );
}

#[test]
fn load_servers_accepts_source_only_standard_mcp_shape() {
    let root = temp_root();
    fs::create_dir_all(root.join("workspace")).unwrap();
    env::set_var("MCPACE_TEST_FORWARDED_ENV", "forwarded-value");
    fs::write(
        root.join("mcp_settings.json"),
        r#"{
  "mcpServers": {
    "Server From Settings": {
      "command": "node",
      "args": ["${MCPACE_TEST_ARG:-fallback-arg}"],
      "cwd": "${MCPACE_PRIMARY_WORKSPACE}/workspace",
      "env": { "STATIC_ROOT": "${MCPACE_PRIMARY_WORKSPACE}" },
      "env_vars": ["MCPACE_TEST_FORWARDED_ENV"]
    }
  }
}"#,
    )
    .unwrap();

    let servers = load_servers(&root).expect("source-only server config");
    let server = servers
        .get("Server From Settings")
        .expect("server from settings");
    assert!(server.enabled);
    assert_eq!(server.source_type, "stdio");
    assert_eq!(server.command.as_deref(), Some("node"));
    assert_eq!(server.args, vec!["fallback-arg".to_string()]);
    assert_eq!(
        server
            .env
            .get("MCPACE_TEST_FORWARDED_ENV")
            .map(String::as_str),
        Some("forwarded-value")
    );
    assert!(server
        .env
        .get("STATIC_ROOT")
        .map(|value| value.contains(&root.display().to_string()))
        .unwrap_or(false));
    assert!(server.cwd.as_ref().unwrap().ends_with("workspace"));

    env::remove_var("MCPACE_TEST_FORWARDED_ENV");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn load_servers_normalizes_public_sse_legacy_type_to_internal_runtime_type() {
    let root = temp_root();
    fs::write(
        root.join("mcp_settings.json"),
        r#"{
  "mcpServers": {
    "Remote SSE": {
      "enabled": true,
      "type": "sse-legacy",
      "url": "http://127.0.0.1:39023/sse"
    }
  }
}"#,
    )
    .unwrap();

    let servers = load_servers(&root).expect("sse legacy source config");
    let server = servers.get("Remote SSE").expect("remote SSE server");
    assert_eq!(server.source_type, "legacy-sse");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn expands_workspace_and_fallback_placeholders() {
    let root = PathBuf::from(r"C:\workspace\project");
    let expanded = expand_template(
        "${MCPACE_PRIMARY_WORKSPACE}|${MCPACE_MANAGER_DATA}|${MISSING_TEST_ENV:-fallback}",
        &root,
    );
    assert!(expanded.contains(r"C:\workspace\project"));
    assert!(expanded.contains(&manager_data_path(&root).display().to_string()));
    assert!(expanded.ends_with("fallback"));
}

#[test]
fn child_process_paths_strip_windows_extended_prefixes() {
    let drive_root = PathBuf::from(r"\\?\C:\workspace\project");
    assert_eq!(
        expand_template("${MCPACE_PRIMARY_WORKSPACE}", &drive_root),
        r"C:\workspace\project"
    );

    let unc_root = PathBuf::from(r"\\?\UNC\server\share\project");
    assert_eq!(
        expand_template("${MCPACE_PRIMARY_WORKSPACE}", &unc_root),
        r"\\server\share\project"
    );
}

#[test]
fn inventory_marks_stdio_and_plain_http_callable() {
    let root = temp_root();
    let command = std::env::current_exe()
        .unwrap()
        .display()
        .to_string()
        .replace('\\', "\\\\");
    fs::write(
            root.join("mcp_settings.json"),
            r#"{
  "mcpServers": {
    "memory": { "enabled": true, "type": "stdio", "command": "__COMMAND__", "args": ["-y", "server"] },
    "remote-demo": { "enabled": true, "type": "http", "url": "http://127.0.0.1:39022/mcp" },
    "off": { "enabled": false, "type": "stdio", "command": "uvx", "args": [] }
  }
}"#
            .replace("__COMMAND__", &command),
        )
        .unwrap();

    let inventory = configured_inventory(&root).expect("inventory");
    let servers = json_helpers::array_at_path(&inventory, &["servers"]).unwrap();
    let memory = servers
        .iter()
        .find(|item| json_helpers::string_at_path(item, &["name"]) == Some("memory"))
        .unwrap();
    let remote = servers
        .iter()
        .find(|item| json_helpers::string_at_path(item, &["name"]) == Some("remote-demo"))
        .unwrap();
    assert_eq!(
        json_helpers::string_at_path(memory, &["status"]),
        Some("callable-stdio")
    );
    assert_eq!(
        json_helpers::string_at_path(remote, &["status"]),
        Some("callable-http")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn plain_http_upstream_lists_and_calls_tools() {
    let root = temp_root();
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock HTTP MCP server");
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut handled = 0usize;
        while handled < 6 && std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    handled += 1;
                    serve_mock_http_mcp_request(&mut stream);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
        handled
    });

    fs::write(
        root.join("mcp_settings.json"),
        format!(
            r#"{{
  "mcpServers": {{
    "remote": {{ "enabled": true, "type": "http", "url": "http://127.0.0.1:{}/mcp" }}
  }}
}}"#,
            port
        ),
    )
    .unwrap();
    fs::write(root.join("mcpace.config.json"), r#"{ "version": "0.5.9" }"#).unwrap();

    let listed = list_tools(&root, Some("remote"), Some(5_000), true).expect("list HTTP tools");
    assert_eq!(
        json_helpers::value_at_path(&listed, &["toolCount"]).and_then(JsonValue::as_i64),
        Some(1)
    );
    let called =
        call_tool(&root, "remote", "ok", &empty_object(), Some(5_000)).expect("call HTTP tool");
    assert_eq!(json_helpers::bool_at_path(&called, &["ok"]), Some(true));
    let handled = handle.join().unwrap();
    assert_eq!(handled, 6);
    let _ = fs::remove_dir_all(root);
}

fn serve_mock_http_mcp_request(stream: &mut std::net::TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut raw = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        let Ok(read) = stream.read(&mut buffer) else {
            return;
        };
        if read == 0 {
            return;
        }
        raw.extend_from_slice(&buffer[..read]);
        let text = String::from_utf8_lossy(&raw);
        let Some((headers, body)) = text.split_once("\r\n\r\n") else {
            continue;
        };
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if body.len() < content_length {
            continue;
        }
        let request = parse_str(&body[..content_length]).unwrap();
        let method = json_helpers::string_at_path(&request, &["method"]).unwrap_or_default();
        let id = json_helpers::value_at_path(&request, &["id"])
            .cloned()
            .unwrap_or(JsonValue::Null);
        let (status, body, extra_header) = match method {
            "initialize" => (
                "200 OK",
                mcp::result(
                    id,
                    JsonValue::object([
                        (
                            "protocolVersion",
                            JsonValue::string(mcp::CURRENT_PROTOCOL_VERSION),
                        ),
                        (
                            "capabilities",
                            JsonValue::object([("tools", empty_object())]),
                        ),
                        (
                            "serverInfo",
                            JsonValue::object([
                                ("name", JsonValue::string("mock-http")),
                                ("version", JsonValue::string("0.0.0")),
                            ]),
                        ),
                    ]),
                )
                .to_compact_string(),
                "Mcp-Session-Id: sess-test\r\n",
            ),
            "notifications/initialized" => ("202 Accepted", String::new(), ""),
            "tools/list" => (
                "200 OK",
                mcp::result(
                    id,
                    JsonValue::object([(
                        "tools",
                        JsonValue::array([JsonValue::object([
                            ("name", JsonValue::string("ok")),
                            ("description", JsonValue::string("ok")),
                            (
                                "inputSchema",
                                JsonValue::object([
                                    ("type", JsonValue::string("object")),
                                    ("properties", empty_object()),
                                ]),
                            ),
                        ])]),
                    )]),
                )
                .to_compact_string(),
                "",
            ),
            "tools/call" => (
                "200 OK",
                mcp::result(
                    id,
                    JsonValue::object([(
                        "content",
                        JsonValue::array([JsonValue::object([
                            ("type", JsonValue::string("text")),
                            ("text", JsonValue::string("called")),
                        ])]),
                    )]),
                )
                .to_compact_string(),
                "",
            ),
            _ => (
                "404 Not Found",
                mcp::error(id, -32601, "not found", None).to_compact_string(),
                "",
            ),
        };
        let response = format!(
            "HTTP/1.1 {}\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            status,
            extra_header,
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }
}

#[test]
fn surface_manifest_is_explicit_about_wrapper_projection() {
    let root = temp_root();
    let command = std::env::current_exe()
        .unwrap()
        .display()
        .to_string()
        .replace('\\', "\\\\");
    fs::write(
            root.join("mcp_settings.json"),
            r#"{
  "mcpServers": {
    "memory": { "enabled": true, "type": "stdio", "command": "__COMMAND__", "args": ["-y", "server"] }
  }
}"#
            .replace("__COMMAND__", &command),
        )
        .unwrap();

    let manifest = surface_manifest(
        &root,
        "streamable-http",
        vec!["surface_manifest".to_string(), "upstream_call".to_string()],
        false,
        None,
        false,
    )
    .expect("surface manifest");
    assert_eq!(json_helpers::bool_at_path(&manifest, &["ok"]), Some(true));
    assert_eq!(
        json_helpers::bool_at_path(
            &manifest,
            &["upstreamTools", "directTopLevelProjection", "enabled"]
        ),
        Some(true)
    );
    assert_eq!(
        json_helpers::value_at_path(&manifest, &["topLevelTools", "count"])
            .and_then(JsonValue::as_i64),
        Some(2)
    );
    assert_eq!(
        json_helpers::bool_at_path(&manifest, &["upstreamTools", "liveCatalogIncluded"]),
        Some(false)
    );
    assert_eq!(
        json_helpers::string_at_path(&manifest, &["configurationModel", "name"]),
        Some("bring-your-own-mcp-servers")
    );
    assert_eq!(
        json_helpers::string_at_path(&manifest, &["configurationModel", "serverSourceOfTruth"]),
        Some("mcp_settings.json.mcpServers")
    );
    assert_eq!(
        json_helpers::bool_at_path(
            &manifest,
            &[
                "configurationModel",
                "packagedDefaults",
                "upstreamServersEnabled"
            ]
        ),
        Some(false)
    );
    assert_eq!(
        json_helpers::bool_at_path(
            &manifest,
            &[
                "configurationModel",
                "packagedDefaults",
                "requiresHardcodedServerNames"
            ]
        ),
        Some(false)
    );
    assert_eq!(
        json_helpers::bool_at_path(&manifest, &["configurationModel", "arbitraryServerNames"]),
        Some(true)
    );
    assert_eq!(
        json_helpers::bool_at_path(
            &manifest,
            &["configurationModel", "requiresRecompileForNewServers"]
        ),
        Some(false)
    );
    assert_eq!(
        json_helpers::bool_at_path(
            &manifest,
            &["configurationModel", "installsUpstreamPackages"]
        ),
        Some(false)
    );
    assert_eq!(
        json_helpers::bool_at_path(
            &manifest,
            &["configurationModel", "httpUpstreamForwardingImplemented"]
        ),
        Some(true)
    );
    assert_eq!(
        json_helpers::bool_at_path(
            &manifest,
            &["configurationModel", "httpsUpstreamForwardingImplemented"]
        ),
        Some(false)
    );
    assert!(json_helpers::string_at_path(&manifest, &["summary"])
        .unwrap_or_default()
        .contains("disguised as native MCPace tools"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inventory_and_probe_report_missing_future_server_commands() {
    let root = temp_root();
    fs::write(
        root.join("mcp_settings.json"),
        r#"{
  "mcpServers": {
    "future-tool": {
      "enabled": true,
      "type": "stdio",
      "command": "definitely-missing-mcpace-test-binary",
      "args": []
    }
  }
}"#,
    )
    .unwrap();

    let inventory = configured_inventory(&root).expect("inventory");
    let servers = json_helpers::array_at_path(&inventory, &["servers"]).unwrap();
    let future = servers
        .iter()
        .find(|item| json_helpers::string_at_path(item, &["name"]) == Some("future-tool"))
        .unwrap();
    assert_eq!(
        json_helpers::bool_at_path(future, &["runtimeCallable"]),
        Some(false)
    );
    assert_eq!(
        json_helpers::string_at_path(future, &["status"]),
        Some("blocked-command-not-found")
    );

    let probe = probe_servers(&root, Some("future-tool"), Some(1_000), false).expect("probe");
    assert_eq!(json_helpers::bool_at_path(&probe, &["ok"]), Some(false));
    assert_eq!(
        json_helpers::value_at_path(&probe, &["failedCount"]).and_then(JsonValue::as_i64),
        Some(1)
    );
    assert_eq!(
        json_helpers::value_at_path(&probe, &["cacheHitCount"]).and_then(JsonValue::as_i64),
        Some(0)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn probe_reuses_successful_tool_list_cache_for_callable_stdio_servers() {
    let root = temp_root();
    let command = std::env::current_exe()
        .unwrap()
        .display()
        .to_string()
        .replace('\\', "\\\\");
    fs::write(
        root.join("mcp_settings.json"),
        r#"{
  "mcpServers": {
    "cached-probe": { "enabled": true, "type": "stdio", "command": "__COMMAND__", "args": [] }
  }
}"#
        .replace("__COMMAND__", &command),
    )
    .unwrap();
    let servers = load_servers(&root).expect("servers");
    let server = servers.get("cached-probe").unwrap();
    write_cached_tools(
        tool_list_cache_key(&root, server),
        JsonValue::array([JsonValue::object([(
            "name",
            JsonValue::string("cached_probe_tool"),
        )])]),
    );

    let probe =
        probe_servers(&root, Some("cached-probe"), Some(1_000), false).expect("cached probe");

    assert_eq!(json_helpers::bool_at_path(&probe, &["ok"]), Some(true));
    assert_eq!(
        json_helpers::value_at_path(&probe, &["cacheHitCount"]).and_then(JsonValue::as_i64),
        Some(1)
    );
    let results = json_helpers::array_at_path(&probe, &["results"]).unwrap();
    assert_eq!(
        json_helpers::bool_at_path(&results[0], &["cacheHit"]),
        Some(true)
    );
    let tool_names = json_helpers::array_at_path(&results[0], &["toolNames"]).unwrap();
    assert_eq!(tool_names[0].as_str(), Some("cached_probe_tool"));
    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn stdio_request_cleanup_terminates_windows_descendant_processes() {
    if std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }

    let root = temp_root();
    let child_path = root.join("child-mcp.js");
    let launcher_path = root.join("launcher.js");
    let pid_path = root.join("child.pid");

    fs::write(
        &child_path,
        r#"
const fs = require('fs');
const readline = require('readline');
fs.writeFileSync(process.argv[2], String(process.pid));
setInterval(() => {}, 1000);
const rl = readline.createInterface({ input: process.stdin });
rl.on('line', (line) => {
  const msg = JSON.parse(line);
  if (msg.method === 'initialize') {
    console.log(JSON.stringify({ jsonrpc: '2.0', id: msg.id, result: { protocolVersion: '2025-06-18', capabilities: { tools: {} }, serverInfo: { name: 'child', version: '0.0.0' } } }));
  } else if (msg.method === 'tools/list') {
    console.log(JSON.stringify({ jsonrpc: '2.0', id: msg.id, result: { tools: [{ name: 'ping', description: 'ping', inputSchema: { type: 'object', properties: {} } }] } }));
  }
});
"#,
    )
    .unwrap();
    fs::write(
        &launcher_path,
        r#"
const { spawn } = require('child_process');
spawn(process.execPath, [process.argv[2], process.argv[3]], { stdio: 'inherit', windowsHide: true });
setInterval(() => {}, 1000);
"#,
    )
    .unwrap();

    let server = UpstreamServerConfig {
        name: "windows-tree".to_string(),
        enabled: true,
        disabled_reason: None,
        source_type: "stdio".to_string(),
        command: Some("node".to_string()),
        args: vec![
            launcher_path.display().to_string(),
            child_path.display().to_string(),
            pid_path.display().to_string(),
        ],
        env: BTreeMap::new(),
        cwd: Some(root.clone()),
        url: None,
        timeout_ms: 5_000,
        tool_policies: Vec::new(),
    };

    let result = run_stdio_request(
        &root,
        &server,
        "tools/list",
        None,
        Duration::from_secs(5),
        None,
    )
    .expect("tools/list through descendant process");
    let tools = json_helpers::array_at_path(&result, &["tools"]).expect("tools");
    assert_eq!(tools.len(), 1);

    let pid = fs::read_to_string(&pid_path)
        .expect("child pid")
        .trim()
        .parse::<u32>()
        .expect("numeric child pid");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while windows_pid_exists(pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !windows_pid_exists(pid),
        "descendant node process {} should be killed with the upstream tree",
        pid
    );
    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
fn windows_pid_exists(pid: u32) -> bool {
    let output = match std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/FO", "CSV", "/NH"])
        .output()
    {
        Ok(value) => value,
        Err(_) => return false,
    };
    String::from_utf8_lossy(&output.stdout).contains(&format!("\"{}\"", pid))
}

#[cfg(unix)]
#[test]
fn stdio_request_cleanup_terminates_unix_descendant_processes() {
    if std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }

    let root = temp_root();
    let child_path = root.join("child-mcp.js");
    let launcher_path = root.join("launcher.js");
    let pid_path = root.join("child.pid");

    fs::write(
        &child_path,
        r#"
const fs = require('fs');
const readline = require('readline');
fs.writeFileSync(process.argv[2], String(process.pid));
setInterval(() => {}, 1000);
const rl = readline.createInterface({ input: process.stdin });
rl.on('line', (line) => {
  const msg = JSON.parse(line);
  if (msg.method === 'initialize') {
    console.log(JSON.stringify({ jsonrpc: '2.0', id: msg.id, result: { protocolVersion: '2025-06-18', capabilities: { tools: {} }, serverInfo: { name: 'child', version: '0.0.0' } } }));
  } else if (msg.method === 'tools/list') {
    console.log(JSON.stringify({ jsonrpc: '2.0', id: msg.id, result: { tools: [{ name: 'ping', description: 'ping', inputSchema: { type: 'object', properties: {} } }] } }));
  }
});
"#,
    )
    .unwrap();
    fs::write(
        &launcher_path,
        r#"
const { spawn } = require('child_process');
spawn(process.execPath, [process.argv[2], process.argv[3]], { stdio: 'inherit' });
setInterval(() => {}, 1000);
"#,
    )
    .unwrap();

    let server = UpstreamServerConfig {
        name: "unix-tree".to_string(),
        enabled: true,
        disabled_reason: None,
        source_type: "stdio".to_string(),
        command: Some("node".to_string()),
        args: vec![
            launcher_path.display().to_string(),
            child_path.display().to_string(),
            pid_path.display().to_string(),
        ],
        env: BTreeMap::new(),
        cwd: Some(root.clone()),
        url: None,
        timeout_ms: 5_000,
        tool_policies: Vec::new(),
    };

    let result = run_stdio_request(
        &root,
        &server,
        "tools/list",
        None,
        Duration::from_secs(5),
        None,
    )
    .expect("tools/list through descendant process");
    let tools = json_helpers::array_at_path(&result, &["tools"]).expect("tools");
    assert_eq!(tools.len(), 1);

    let pid = fs::read_to_string(&pid_path)
        .expect("child pid")
        .trim()
        .parse::<u32>()
        .expect("numeric child pid");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while unix_pid_exists(pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !unix_pid_exists(pid),
        "descendant node process {} should be killed with the upstream process group",
        pid
    );
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
fn unix_pid_exists(pid: u32) -> bool {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    unsafe { kill(pid, 0) == 0 }
}

#[test]
fn catalog_reports_arbitrary_configured_server_names_without_whitelist() {
    let root = temp_root();
    fs::write(
        root.join("mcp_settings.json"),
        r#"{
  "mcpServers": {
    "alpha-telemetry": {
      "enabled": true,
      "type": "stdio",
      "command": "definitely-missing-mcpace-test-binary",
      "args": []
    },
    "zeta-ops": {
      "enabled": false,
      "type": "stdio",
      "command": "also-missing-mcpace-test-binary",
      "args": []
    }
  }
}"#,
    )
    .unwrap();

    let catalog = catalog_tools(&root, None, Some(1_000), false).expect("catalog");
    assert_eq!(
        json_helpers::string_at_path(&catalog, &["mode"]),
        Some("catalog")
    );
    let servers = json_helpers::array_at_path(&catalog, &["servers"]).unwrap();
    assert!(servers
        .iter()
        .any(|item| json_helpers::string_at_path(item, &["name"]) == Some("alpha-telemetry")));
    assert!(servers
        .iter()
        .any(|item| json_helpers::string_at_path(item, &["name"]) == Some("zeta-ops")));

    let selected = catalog_tools(&root, Some("ALPHA-TELEMETRY"), Some(1_000), false)
        .expect("selected catalog");
    let selected_servers = json_helpers::array_at_path(&selected, &["servers"]).unwrap();
    assert_eq!(selected_servers.len(), 1);
    assert_eq!(
        json_helpers::string_at_path(&selected_servers[0], &["name"]),
        Some("alpha-telemetry")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn bounded_server_task_runner_preserves_input_order() {
    fn server(name: &str) -> UpstreamServerConfig {
        UpstreamServerConfig {
            name: name.to_string(),
            enabled: true,
            disabled_reason: None,
            source_type: "stdio".to_string(),
            command: Some("tool".to_string()),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            url: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            tool_policies: Vec::new(),
        }
    }

    let root = temp_root();
    let results = run_server_tasks(
        &root,
        vec![server("zeta"), server("alpha"), server("middle")],
        None,
        |_root, server, _timeout| JsonValue::object([("name", JsonValue::string(&server.name))]),
    );

    let names = results
        .iter()
        .filter_map(|item| json_helpers::string_at_path(item, &["name"]))
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["zeta", "alpha", "middle"]);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn bounded_server_task_runner_reports_worker_panics() {
    fn server(name: &str) -> UpstreamServerConfig {
        UpstreamServerConfig {
            name: name.to_string(),
            enabled: true,
            disabled_reason: None,
            source_type: "stdio".to_string(),
            command: Some("tool".to_string()),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            url: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            tool_policies: Vec::new(),
        }
    }

    let root = temp_root();
    let results = run_server_tasks(
        &root,
        vec![server("ok"), server("panic")],
        None,
        |_root, server, _timeout| {
            if server.name == "panic" {
                panic!("intentional task panic");
            }
            JsonValue::object([
                ("name", JsonValue::string(&server.name)),
                ("ok", JsonValue::bool(true)),
            ])
        },
    );

    assert_eq!(json_helpers::bool_at_path(&results[0], &["ok"]), Some(true));
    assert_eq!(
        json_helpers::string_at_path(&results[1], &["name"]),
        Some("panic")
    );
    assert_eq!(
        json_helpers::string_at_path(&results[1], &["status"]),
        Some("worker-panicked")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tool_list_cache_key_tracks_settings_metadata() {
    let root = temp_root();
    let settings_path = root.join("mcp_settings.json");
    fs::write(
        &settings_path,
        r#"{
  "mcpServers": {
    "alpha": { "enabled": true, "type": "stdio", "command": "node", "args": ["a"] }
  }
}"#,
    )
    .unwrap();
    let servers = load_servers(&root).expect("servers");
    let key_before = tool_list_cache_key(&root, servers.get("alpha").unwrap());

    fs::write(
        &settings_path,
        r#"{
  "mcpServers": {
    "alpha": { "enabled": true, "type": "stdio", "command": "node", "args": ["a", "b"] }
  }
}"#,
    )
    .unwrap();
    let servers = load_servers(&root).expect("updated servers");
    let key_after = tool_list_cache_key(&root, servers.get("alpha").unwrap());

    assert_ne!(key_before, key_after);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tool_list_cache_returns_fresh_entries_and_drops_stale_entries() {
    let root = temp_root();
    let key = ToolListCacheKey {
        root_path: root.display().to_string(),
        server_name: "alpha-cache".to_string(),
        settings_modified_ms: 1,
        settings_len: 2,
        server_fingerprint: "fingerprint".to_string(),
    };
    let tools = JsonValue::array([JsonValue::object([(
        "name",
        JsonValue::string("cached_tool"),
    )])]);

    let cache = TOOL_LIST_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    cache.lock().unwrap().insert(
        key.clone(),
        CachedToolList {
            stored_at: Instant::now(),
            tools: tools.clone(),
        },
    );
    assert_eq!(read_cached_tools(&key), Some(tools));

    cache.lock().unwrap().insert(
        key.clone(),
        CachedToolList {
            stored_at: Instant::now() - TOOL_LIST_CACHE_TTL - Duration::from_millis(1),
            tools: JsonValue::array([JsonValue::string("stale")]),
        },
    );
    assert_eq!(read_cached_tools(&key), None);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tool_list_cache_persists_to_disk_and_invalidates_when_settings_change() {
    let root = temp_root();
    let settings_path = root.join("mcp_settings.json");
    fs::write(
        &settings_path,
        r#"{
  "mcpServers": {
    "alpha": { "enabled": true, "type": "stdio", "command": "node", "args": ["a"] }
  }
}"#,
    )
    .unwrap();
    let servers = load_servers(&root).expect("servers");
    let key_before = tool_list_cache_key(&root, servers.get("alpha").unwrap());
    let tools = JsonValue::array([JsonValue::object([(
        "name",
        JsonValue::string("persisted_tool"),
    )])]);

    write_cached_tools(key_before.clone(), tools.clone());
    let cache = TOOL_LIST_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    cache.lock().unwrap().remove(&key_before);

    assert_eq!(read_cached_tools(&key_before), Some(tools));

    fs::write(
        &settings_path,
        r#"{
  "mcpServers": {
    "alpha": { "enabled": true, "type": "stdio", "command": "node", "args": ["a", "changed"] }
  }
}"#,
    )
    .unwrap();
    let servers = load_servers(&root).expect("updated servers");
    let key_after = tool_list_cache_key(&root, servers.get("alpha").unwrap());
    cache.lock().unwrap().remove(&key_before);

    assert_ne!(key_before, key_after);
    assert_eq!(read_cached_tools(&key_after), None);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tool_list_cache_prunes_oldest_entries_to_bound_memory() {
    let mut cache = BTreeMap::new();
    let now = Instant::now();
    for index in 0..(TOOL_LIST_CACHE_MAX_ENTRIES + 5) {
        cache.insert(
            ToolListCacheKey {
                root_path: format!("test-prune-root-{}", std::process::id()),
                server_name: format!("server-{index:03}"),
                settings_modified_ms: index as u128,
                settings_len: index as u64,
                server_fingerprint: format!("fingerprint-{index:03}"),
            },
            CachedToolList {
                stored_at: now + Duration::from_millis(index as u64),
                tools: JsonValue::array([JsonValue::string(format!("tool-{index}"))]),
            },
        );
    }

    prune_tool_list_cache(&mut cache);

    assert_eq!(cache.len(), TOOL_LIST_CACHE_MAX_ENTRIES);
    assert!(cache
        .keys()
        .all(|key| key.server_name.as_str() >= "server-005"));
}

#[test]
fn flat_catalog_tools_include_server_and_upstream_call_arguments() {
    let results = vec![JsonValue::object([
        ("name", JsonValue::string("alpha")),
        ("ok", JsonValue::bool(true)),
        ("cacheHit", JsonValue::bool(true)),
        (
            "tools",
            JsonValue::array([JsonValue::object([
                ("name", JsonValue::string("read_item")),
                ("title", JsonValue::string("Read item")),
                ("description", JsonValue::string("Read one item.")),
            ])]),
        ),
    ])];

    let tools = flatten_catalog_tools(&results);

    assert_eq!(tools.len(), 1);
    let tool = &tools[0];
    assert_eq!(
        json_helpers::string_at_path(tool, &["server"]),
        Some("alpha")
    );
    assert_eq!(
        json_helpers::string_at_path(tool, &["qualifiedName"]),
        Some("alpha.read_item")
    );
    assert_eq!(
        json_helpers::string_at_path(tool, &["call", "tool"]),
        Some("upstream_call")
    );
    assert_eq!(
        json_helpers::string_at_path(tool, &["call", "arguments", "server"]),
        Some("alpha")
    );
    assert_eq!(
        json_helpers::string_at_path(tool, &["call", "arguments", "tool"]),
        Some("read_item")
    );
}

#[test]
fn catalog_cache_counts_ignore_failed_servers() {
    let results = vec![
        JsonValue::object([
            ("name", JsonValue::string("hit")),
            ("ok", JsonValue::bool(true)),
            ("cacheHit", JsonValue::bool(true)),
        ]),
        JsonValue::object([
            ("name", JsonValue::string("miss")),
            ("ok", JsonValue::bool(true)),
            ("cacheHit", JsonValue::bool(false)),
        ]),
        JsonValue::object([
            ("name", JsonValue::string("failed")),
            ("ok", JsonValue::bool(false)),
            ("cacheHit", JsonValue::bool(false)),
        ]),
    ];

    assert_eq!(catalog_cache_counts(&results), (1, 1));
}

#[test]
fn tool_policy_audit_flags_unprotected_mutating_tools_without_enforcing_heuristics() {
    let server = UpstreamServerConfig {
        name: "audit-fixture".to_string(),
        enabled: true,
        disabled_reason: None,
        source_type: "stdio".to_string(),
        command: Some("tool".to_string()),
        args: Vec::new(),
        env: BTreeMap::new(),
        cwd: None,
        url: None,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        tool_policies: Vec::new(),
    };
    let audit = audit_tool(
        &server,
        &JsonValue::object([
            ("name", JsonValue::string("delete_file")),
            (
                "annotations",
                JsonValue::object([
                    ("destructiveHint", JsonValue::bool(true)),
                    ("readOnlyHint", JsonValue::bool(false)),
                ]),
            ),
        ]),
    );

    assert!(audit.has_annotations);
    assert!(audit.has_advisory_risk);
    assert!(audit.guard_recommended);
    assert!(!audit.policy_covered);
    assert_eq!(
        json_helpers::string_at_path(&audit.value, &["policyStatus"]),
        Some("unprotected-guard-recommended")
    );
    assert!(
        json_helpers::array_at_path(&audit.value, &["advisoryRiskClasses"])
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some("mutation"))
    );
}

#[test]
fn tool_policy_audit_reports_declarative_policy_coverage() {
    let server = UpstreamServerConfig {
        name: "audit-fixture".to_string(),
        enabled: true,
        disabled_reason: None,
        source_type: "stdio".to_string(),
        command: Some("tool".to_string()),
        args: Vec::new(),
        env: BTreeMap::new(),
        cwd: None,
        url: None,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        tool_policies: vec![ToolRiskPolicy {
            tools: vec!["write_*".to_string()],
            risk_class: Some("filesystem-mutation".to_string()),
            allow_argument: Some("allowFilesystemMutation".to_string()),
            description: Some("writes project files".to_string()),
        }],
    };
    let audit = audit_tool(
        &server,
        &JsonValue::object([("name", JsonValue::string("write_file"))]),
    );

    assert!(audit.has_advisory_risk);
    assert!(audit.guard_recommended);
    assert!(audit.policy_covered);
    assert!(!audit.review_recommended);
    assert_eq!(
        json_helpers::string_at_path(&audit.value, &["policyStatus"]),
        Some("covered-advisory-risk")
    );
    let policies = json_helpers::array_at_path(&audit.value, &["matchingPolicies"]).unwrap();
    assert_eq!(policies.len(), 1);
    assert_eq!(
        json_helpers::string_at_path(&policies[0], &["allowArgument"]),
        Some("allowFilesystemMutation")
    );
}

#[test]
fn policy_suggestions_group_unprotected_guarded_tools_by_generated_risk_class() {
    let audit = JsonValue::object([(
        "servers",
        JsonValue::array([JsonValue::object([
            ("name", JsonValue::string("alpha-tools")),
            ("ok", JsonValue::bool(true)),
            (
                "tools",
                JsonValue::array([
                    JsonValue::object([
                        ("name", JsonValue::string("delete_item")),
                        ("guardRecommended", JsonValue::bool(true)),
                        ("policyCovered", JsonValue::bool(false)),
                        (
                            "policyStatus",
                            JsonValue::string("unprotected-guard-recommended"),
                        ),
                        (
                            "advisoryRiskClasses",
                            JsonValue::array([JsonValue::string("mutation")]),
                        ),
                        (
                            "advisorySignals",
                            JsonValue::array([JsonValue::string("name-token:delete")]),
                        ),
                    ]),
                    JsonValue::object([
                        ("name", JsonValue::string("write_item")),
                        ("guardRecommended", JsonValue::bool(true)),
                        ("policyCovered", JsonValue::bool(false)),
                        (
                            "policyStatus",
                            JsonValue::string("unprotected-guard-recommended"),
                        ),
                        (
                            "advisoryRiskClasses",
                            JsonValue::array([JsonValue::string("not-readonly")]),
                        ),
                        (
                            "advisorySignals",
                            JsonValue::array([JsonValue::string("mcp.readOnlyHint=false")]),
                        ),
                    ]),
                ]),
            ),
        ])]),
    )]);

    let report = policy_suggestion_report(&audit);
    assert_eq!(
        json_helpers::value_at_path(&report, &["suggestedPolicyCount"]).and_then(JsonValue::as_i64),
        Some(1)
    );
    assert_eq!(
        json_helpers::value_at_path(&report, &["suggestedToolCount"]).and_then(JsonValue::as_i64),
        Some(2)
    );
    let suggestions = json_helpers::array_at_path(&report, &["suggestions"]).unwrap();
    let suggestion = &suggestions[0];
    assert_eq!(
        json_helpers::string_at_path(suggestion, &["server"]),
        Some("alpha-tools")
    );
    assert_eq!(
        json_helpers::string_at_path(suggestion, &["policy", "riskClass"]),
        Some("alpha-tools-mutation")
    );
    assert_eq!(
        json_helpers::string_at_path(suggestion, &["policy", "allowArgument"]),
        Some("allowAlphaToolsMutation")
    );
    assert_eq!(
        json_helpers::string_at_path(suggestion, &["confidence"]),
        Some("high")
    );
    let tools = json_helpers::array_at_path(suggestion, &["policy", "tools"]).unwrap();
    assert_eq!(tools.len(), 2);
}

#[test]
fn policy_suggestions_keep_interaction_control_as_stable_cross_server_risk_class() {
    let audit = JsonValue::object([(
        "servers",
        JsonValue::array([JsonValue::object([
            ("name", JsonValue::string("interactive")),
            ("ok", JsonValue::bool(true)),
            (
                "tools",
                JsonValue::array([JsonValue::object([
                    ("name", JsonValue::string("page_navigate")),
                    ("guardRecommended", JsonValue::bool(true)),
                    ("policyCovered", JsonValue::bool(false)),
                    (
                        "policyStatus",
                        JsonValue::string("unprotected-guard-recommended"),
                    ),
                    (
                        "advisoryRiskClasses",
                        JsonValue::array([JsonValue::string("interaction-control")]),
                    ),
                    (
                        "advisorySignals",
                        JsonValue::array([JsonValue::string("name-token:navigate")]),
                    ),
                ])]),
            ),
        ])]),
    )]);

    let report = policy_suggestion_report(&audit);
    let suggestions = json_helpers::array_at_path(&report, &["suggestions"]).unwrap();
    assert_eq!(
        json_helpers::string_at_path(&suggestions[0], &["policy", "riskClass"]),
        Some("interaction-control")
    );
    assert_eq!(
        json_helpers::string_at_path(&suggestions[0], &["policy", "allowArgument"]),
        Some("allowInteractionControl")
    );
}

#[test]
fn tool_summary_uses_upstream_name_title_and_description() {
    let summary = tool_summary(&JsonValue::object([
        ("name", JsonValue::string("alpha_tool")),
        ("title", JsonValue::string("Alpha tool")),
        ("description", JsonValue::string("Short alpha description")),
        (
            "inputSchema",
            JsonValue::object([("type", JsonValue::string("object"))]),
        ),
    ]));

    assert_eq!(
        json_helpers::string_at_path(&summary, &["name"]),
        Some("alpha_tool")
    );
    assert_eq!(
        json_helpers::string_at_path(&summary, &["title"]),
        Some("Alpha tool")
    );
    assert_eq!(
        json_helpers::string_at_path(&summary, &["description"]),
        Some("Short alpha description")
    );
    assert!(json_helpers::value_at_path(&summary, &["inputSchema"]).is_none());
}
