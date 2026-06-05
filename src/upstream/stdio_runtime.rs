use super::diagnostics::stderr_suffix;
use super::lease_runtime::runtime_lease_lost_error;
use super::process_config::{
    child_process_path, manager_data_path, resolve_command_for_cwd, spawn_program_for_command,
    validate_stdio_cwd,
};
use super::{empty_object, UpstreamServerConfig, UpstreamToolCall, INITIALIZE_ID, METHOD_ID};
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use std::collections::BTreeMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};
use std::thread;
use std::time::{Duration, Instant};

pub(super) struct RunningServer {
    pub(super) child: Child,
    pub(super) stdin: Box<dyn Write + Send>,
    pub(super) stdout_rx: Receiver<String>,
    pub(super) stderr_rx: Receiver<String>,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = std::mem::replace(&mut self.stdin, Box::new(std::io::sink()));
        terminate_child_process(&mut self.child);
    }
}

impl RunningServer {
    pub(super) fn has_exited(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(Some(_)) | Err(_) => true,
            Ok(None) => false,
        }
    }
}

pub(super) fn run_stdio_request(
    root_path: &Path,
    server: &UpstreamServerConfig,
    method: &str,
    params: Option<JsonValue>,
    timeout: Duration,
    lease_lost: Option<&AtomicBool>,
) -> Result<JsonValue, String> {
    let mut running = spawn_stdio_server(root_path, server)?;
    let deadline = Instant::now() + timeout;

    initialize_running_server(&mut running, server, deadline, lease_lost)?;

    let mut request_entries = vec![
        ("jsonrpc", JsonValue::string("2.0")),
        ("id", JsonValue::number(METHOD_ID)),
        ("method", JsonValue::string(method)),
    ];
    if let Some(params) = params {
        request_entries.push(("params", params));
    }
    write_jsonrpc(&mut running.stdin, JsonValue::object(request_entries))?;
    read_response(
        &running.stdout_rx,
        &running.stderr_rx,
        METHOD_ID,
        deadline,
        &server.name,
        method,
        lease_lost,
    )
}

pub(super) fn run_stdio_tool_calls(
    root_path: &Path,
    server: &UpstreamServerConfig,
    calls: &[UpstreamToolCall],
    timeout: Duration,
    lease_lost: Option<&AtomicBool>,
) -> Result<Vec<JsonValue>, String> {
    let mut running = spawn_stdio_server(root_path, server)?;
    let deadline = Instant::now() + timeout;

    initialize_running_server(&mut running, server, deadline, lease_lost)?;

    let mut results = Vec::new();
    for (index, call) in calls.iter().enumerate() {
        let request_id = METHOD_ID + index as i64;
        write_jsonrpc(
            &mut running.stdin,
            JsonValue::object([
                ("jsonrpc", JsonValue::string("2.0")),
                ("id", JsonValue::number(request_id)),
                ("method", JsonValue::string("tools/call")),
                (
                    "params",
                    JsonValue::object([
                        ("name", JsonValue::string(call.tool.clone())),
                        ("arguments", call.arguments.clone()),
                    ]),
                ),
            ]),
        )?;
        let result = read_response(
            &running.stdout_rx,
            &running.stderr_rx,
            request_id,
            deadline,
            &server.name,
            "tools/call",
            lease_lost,
        )?;
        let upstream_is_error = json_helpers::bool_at_path(&result, &["isError"]).unwrap_or(false);
        let upstream_ok = !upstream_is_error;
        results.push(JsonValue::object([
            ("index", JsonValue::number(index)),
            ("ok", JsonValue::bool(upstream_ok)),
            ("upstreamOk", JsonValue::bool(upstream_ok)),
            ("upstreamIsError", JsonValue::bool(upstream_is_error)),
            ("tool", JsonValue::string(call.tool.clone())),
            ("upstreamResult", result),
        ]));
    }

    Ok(results)
}

pub(super) fn initialize_running_server(
    running: &mut RunningServer,
    server: &UpstreamServerConfig,
    deadline: Instant,
    lease_lost: Option<&AtomicBool>,
) -> Result<(), String> {
    write_jsonrpc(
        &mut running.stdin,
        JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("id", JsonValue::number(INITIALIZE_ID)),
            ("method", JsonValue::string("initialize")),
            (
                "params",
                JsonValue::object([
                    (
                        "protocolVersion",
                        JsonValue::string(mcp::CURRENT_PROTOCOL_VERSION),
                    ),
                    ("capabilities", empty_object()),
                    (
                        "clientInfo",
                        JsonValue::object([
                            ("name", JsonValue::string("mcpace-upstream-bridge")),
                            ("version", JsonValue::string(env!("CARGO_PKG_VERSION"))),
                        ]),
                    ),
                ]),
            ),
        ]),
    )?;
    let _initialize_result = read_response(
        &running.stdout_rx,
        &running.stderr_rx,
        INITIALIZE_ID,
        deadline,
        &server.name,
        "initialize",
        lease_lost,
    )?;

    write_jsonrpc(
        &mut running.stdin,
        JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("method", JsonValue::string("notifications/initialized")),
        ]),
    )
}

pub(super) fn spawn_stdio_server(
    root_path: &Path,
    server: &UpstreamServerConfig,
) -> Result<RunningServer, String> {
    let command_name = server.command.as_deref().unwrap_or_default();
    let cwd = server.cwd.as_deref().unwrap_or(root_path);
    if let Some(cwd_error) = validate_stdio_cwd(cwd, &server.name) {
        return Err(cwd_error);
    }
    let program = resolve_command_for_cwd(command_name, cwd).map_err(|error| {
        format!(
            "failed to resolve command '{}' for upstream server '{}': {}",
            command_name, server.name, error
        )
    })?;

    let spawn_program = spawn_program_for_command(command_name, &program);
    let mut command = Command::new(&spawn_program);
    command.env_clear();
    for (key, value) in default_child_process_environment() {
        command.env(key, value);
    }
    command
        .args(&server.args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("MCPACE_PRIMARY_WORKSPACE", child_process_path(root_path))
        .env(
            "MCPACE_MANAGER_DATA",
            child_process_path(&manager_data_path(root_path)),
        );
    for (key, value) in &server.env {
        command.env(key, value);
    }
    #[cfg(unix)]
    crate::process_detach::configure_unix_process_group(&mut command);
    #[cfg(windows)]
    crate::windows_process::configure_no_window(&mut command);

    let mut child = command.spawn().map_err(|error| {
        format!(
            "failed to start upstream server '{}' with '{}': {}",
            server.name,
            program.display(),
            error
        )
    })?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("upstream server '{}' stdin was unavailable", server.name))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("upstream server '{}' stdout was unavailable", server.name))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("upstream server '{}' stderr was unavailable", server.name))?;

    let (stdout_tx, stdout_rx) = mpsc::channel();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if stdout_tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let (stderr_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if stderr_tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(RunningServer {
        child,
        stdin: Box::new(stdin),
        stdout_rx,
        stderr_rx,
    })
}

pub(super) fn write_jsonrpc(stdin: &mut dyn Write, message: JsonValue) -> Result<(), String> {
    writeln!(stdin, "{}", message.to_compact_string())
        .map_err(|error| format!("failed to write upstream JSON-RPC message: {}", error))?;
    stdin
        .flush()
        .map_err(|error| format!("failed to flush upstream JSON-RPC message: {}", error))
}

fn default_child_process_environment() -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    let names: &[&str] = if cfg!(windows) {
        &[
            "Path",
            "PATHEXT",
            "SystemRoot",
            "ComSpec",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramData",
            "HOMEDRIVE",
            "HOMEPATH",
            "USERNAME",
        ]
    } else {
        &[
            "PATH", "HOME", "TMPDIR", "TEMP", "TMP", "USER", "LOGNAME", "SHELL",
        ]
    };

    for name in names {
        if let Ok(value) = env::var(name) {
            values.insert((*name).to_string(), value);
        }
    }
    if cfg!(windows) && !values.contains_key("Path") {
        if let Ok(value) = env::var("PATH") {
            values.insert("Path".to_string(), value);
        }
    }
    values
}

pub(super) fn read_response(
    stdout_rx: &Receiver<String>,
    stderr_rx: &Receiver<String>,
    expected_id: i64,
    deadline: Instant,
    server_name: &str,
    method: &str,
    lease_lost: Option<&AtomicBool>,
) -> Result<JsonValue, String> {
    loop {
        if lease_lost
            .map(|value| value.load(Ordering::SeqCst))
            .unwrap_or(false)
        {
            return Err(runtime_lease_lost_error(server_name, method, stderr_rx));
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(format!(
                "timed out waiting for upstream server '{}' response to {}.{}{}",
                server_name,
                method,
                format_expected_id(expected_id),
                stderr_suffix(stderr_rx)
            ));
        }
        let remaining = deadline.saturating_duration_since(now);
        match stdout_rx.recv_timeout(remaining.min(Duration::from_millis(250))) {
            Ok(line) => {
                if lease_lost
                    .map(|value| value.load(Ordering::SeqCst))
                    .unwrap_or(false)
                {
                    return Err(runtime_lease_lost_error(server_name, method, stderr_rx));
                }
                let trimmed = line.trim();
                if trimmed.is_empty() || !trimmed.starts_with('{') {
                    continue;
                }
                let message = match parse_str(trimmed) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let id_matches = json_helpers::value_at_path(&message, &["id"])
                    .and_then(JsonValue::as_i64)
                    .map(|id| id == expected_id)
                    .unwrap_or(false);
                if !id_matches {
                    continue;
                }
                if lease_lost
                    .map(|value| value.load(Ordering::SeqCst))
                    .unwrap_or(false)
                {
                    return Err(runtime_lease_lost_error(server_name, method, stderr_rx));
                }
                if let Some(error) = json_helpers::value_at_path(&message, &["error"]) {
                    return Err(format!(
                        "upstream server '{}' returned JSON-RPC error for {}: {}{}",
                        server_name,
                        method,
                        error.to_compact_string(),
                        stderr_suffix(stderr_rx)
                    ));
                }
                return json_helpers::value_at_path(&message, &["result"])
                    .cloned()
                    .ok_or_else(|| {
                        format!(
                            "upstream server '{}' response to {} did not contain result{}",
                            server_name,
                            method,
                            stderr_suffix(stderr_rx)
                        )
                    });
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(format!(
                    "upstream server '{}' closed stdout before responding to {}{}",
                    server_name,
                    method,
                    stderr_suffix(stderr_rx)
                ));
            }
        }
    }
}

fn format_expected_id(expected_id: i64) -> String {
    format!(" (id {})", expected_id)
}

fn terminate_child_process(child: &mut Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }

    #[cfg(windows)]
    kill_windows_process_tree(child.id());
    #[cfg(unix)]
    crate::process_detach::kill_unix_process_group(child.id(), 15);

    let _ = child.kill();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) if Instant::now() >= deadline => break,
            Ok(None) => thread::sleep(Duration::from_millis(25)),
        }
    }

    #[cfg(unix)]
    crate::process_detach::kill_unix_process_group(child.id(), 9);
    let _ = child.kill();
    let force_deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < force_deadline {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => thread::sleep(Duration::from_millis(25)),
        }
    }
}

#[cfg(windows)]
fn kill_windows_process_tree(pid: u32) {
    let system_root = env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let taskkill = Path::new(&system_root)
        .join("System32")
        .join("taskkill.exe");
    let program = if taskkill.exists() {
        taskkill
    } else {
        Path::new("taskkill.exe").to_path_buf()
    };
    let mut command = Command::new(program);
    command
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::windows_process::configure_no_window(&mut command);
    let _ = command.status();
}
