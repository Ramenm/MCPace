use super::diagnostics::stderr_suffix;
use super::lease_runtime::runtime_lease_lost_error;
use super::process_config::{
    child_process_path, manager_data_path, resolve_command_for_cwd, spawn_program_for_command,
    validate_stdio_cwd,
};
use super::{
    batch_tool_call_error, empty_object, negotiated_protocol_version, validate_tool_call_result,
    ToolListPagination, UpstreamServerConfig, UpstreamToolCall, INITIALIZE_ID, METHOD_ID,
};
use crate::json::{parse_str, JsonValue};
use crate::json_helpers;
use crate::mcp_protocol as mcp;
use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::thread;
use std::time::{Duration, Instant};

const MAX_STDIO_RESPONSE_LINE_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDERR_LINE_BYTES: usize = 16 * 1024;
const STDOUT_CHANNEL_CAPACITY: usize = 8;
const STDERR_CHANNEL_CAPACITY: usize = 64;
const STDIN_CHANNEL_CAPACITY: usize = 1;
const STDIN_WAIT_SLICE: Duration = Duration::from_millis(25);

#[derive(Debug)]
pub(super) enum StdioOutput {
    Line(String),
    Error(String),
}

struct StdioWriteRequest {
    payload: Vec<u8>,
    result_tx: SyncSender<StdioRuntimeResult<()>>,
}

pub(super) struct RunningServer {
    pub(super) child: Child,
    stdin_tx: Option<SyncSender<StdioWriteRequest>>,
    pub(super) stdout_rx: Receiver<StdioOutput>,
    pub(super) stderr_rx: Receiver<String>,
}

struct PendingChild {
    child: Option<Child>,
}

impl PendingChild {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn child_mut(&mut self) -> Option<&mut Child> {
        self.child.as_mut()
    }

    fn disarm(mut self) -> Option<Child> {
        self.child.take()
    }
}

impl Drop for PendingChild {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            terminate_child_process(child);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum StdioRuntimeError {
    Message(String),
}

pub(super) type StdioRuntimeResult<T> = Result<T, StdioRuntimeError>;

impl fmt::Display for StdioRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(error) => write!(formatter, "{}", error),
        }
    }
}

impl std::error::Error for StdioRuntimeError {}

impl From<String> for StdioRuntimeError {
    fn from(error: String) -> Self {
        Self::Message(error)
    }
}

impl From<&str> for StdioRuntimeError {
    fn from(error: &str) -> Self {
        Self::Message(error.to_string())
    }
}

impl From<StdioRuntimeError> for String {
    fn from(error: StdioRuntimeError) -> Self {
        error.to_string()
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stdin_tx.take();
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
) -> StdioRuntimeResult<JsonValue> {
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
    write_jsonrpc(
        &running,
        JsonValue::object(request_entries),
        deadline,
        &server.name,
        method,
        lease_lost,
    )?;
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

pub(super) fn run_stdio_tools_list(
    root_path: &Path,
    server: &UpstreamServerConfig,
    timeout: Duration,
    lease_lost: Option<&AtomicBool>,
) -> StdioRuntimeResult<JsonValue> {
    let mut running = spawn_stdio_server(root_path, server)?;
    let deadline = Instant::now() + timeout;
    initialize_running_server(&mut running, server, deadline, lease_lost)?;

    let mut pagination = ToolListPagination::new();
    let mut cursor: Option<String> = None;
    let mut page = 0usize;
    loop {
        let request_id = METHOD_ID.saturating_add(page as i64);
        let mut entries = vec![
            ("jsonrpc", JsonValue::string("2.0")),
            ("id", JsonValue::number(request_id)),
            ("method", JsonValue::string("tools/list")),
        ];
        if let Some(cursor) = cursor.as_ref() {
            entries.push((
                "params",
                JsonValue::object([("cursor", JsonValue::string(cursor.clone()))]),
            ));
        }
        write_jsonrpc(
            &running,
            JsonValue::object(entries),
            deadline,
            &server.name,
            "tools/list",
            lease_lost,
        )?;
        let page_result = read_response(
            &running.stdout_rx,
            &running.stderr_rx,
            request_id,
            deadline,
            &server.name,
            "tools/list",
            lease_lost,
        )?;
        cursor = pagination
            .add_page(&server.name, &page_result)
            .map_err(StdioRuntimeError::from)?;
        page = page.saturating_add(1);
        if cursor.is_none() {
            return Ok(pagination.finish());
        }
    }
}

pub(super) fn run_stdio_tool_calls(
    root_path: &Path,
    server: &UpstreamServerConfig,
    calls: &[UpstreamToolCall],
    timeout: Duration,
    lease_lost: Option<&AtomicBool>,
) -> StdioRuntimeResult<Vec<JsonValue>> {
    let mut running = spawn_stdio_server(root_path, server)?;
    let deadline = Instant::now() + timeout;

    initialize_running_server(&mut running, server, deadline, lease_lost)?;

    let mut results = Vec::new();
    for (index, call) in calls.iter().enumerate() {
        let request_id = METHOD_ID + index as i64;
        write_jsonrpc(
            &running,
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
            deadline,
            &server.name,
            "tools/call",
            lease_lost,
        )
        .map_err(|error| {
            StdioRuntimeError::from(batch_tool_call_error(
                &server.name,
                index,
                calls.len(),
                error,
            ))
        })?;
        let result = read_response(
            &running.stdout_rx,
            &running.stderr_rx,
            request_id,
            deadline,
            &server.name,
            "tools/call",
            lease_lost,
        )
        .map_err(|error| {
            StdioRuntimeError::from(batch_tool_call_error(
                &server.name,
                index,
                calls.len(),
                error,
            ))
        })?;
        let upstream_is_error = validate_tool_call_result(&server.name, &call.tool, &result)
            .map_err(|error| {
                StdioRuntimeError::from(batch_tool_call_error(
                    &server.name,
                    index,
                    calls.len(),
                    error,
                ))
            })?;
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
) -> StdioRuntimeResult<()> {
    write_jsonrpc(
        running,
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
        deadline,
        &server.name,
        "initialize",
        lease_lost,
    )?;
    let initialize_result = read_response(
        &running.stdout_rx,
        &running.stderr_rx,
        INITIALIZE_ID,
        deadline,
        &server.name,
        "initialize",
        lease_lost,
    )?;
    negotiated_protocol_version(&server.name, &initialize_result)?;

    write_jsonrpc(
        running,
        JsonValue::object([
            ("jsonrpc", JsonValue::string("2.0")),
            ("method", JsonValue::string("notifications/initialized")),
        ]),
        deadline,
        &server.name,
        "notifications/initialized",
        lease_lost,
    )
}

pub(super) fn spawn_stdio_server(
    root_path: &Path,
    server: &UpstreamServerConfig,
) -> StdioRuntimeResult<RunningServer> {
    let command_name = server.command.as_deref().unwrap_or_default();
    let cwd = server.cwd.as_deref().unwrap_or(root_path);
    if let Some(cwd_error) = validate_stdio_cwd(cwd, &server.name) {
        return Err(cwd_error.into());
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

    let child = command.spawn().map_err(|error| {
        format!(
            "failed to start upstream server '{}' with '{}': {}",
            server.name,
            program.display(),
            error
        )
    })?;
    let mut pending_child = PendingChild::new(child);
    let stdin = pending_child
        .child_mut()
        .ok_or_else(|| format!("upstream server '{}' child was unavailable", server.name))?
        .stdin
        .take()
        .ok_or_else(|| format!("upstream server '{}' stdin was unavailable", server.name))?;
    let stdout = pending_child
        .child_mut()
        .ok_or_else(|| format!("upstream server '{}' child was unavailable", server.name))?
        .stdout
        .take()
        .ok_or_else(|| format!("upstream server '{}' stdout was unavailable", server.name))?;
    let stderr = pending_child
        .child_mut()
        .ok_or_else(|| format!("upstream server '{}' child was unavailable", server.name))?
        .stderr
        .take()
        .ok_or_else(|| format!("upstream server '{}' stderr was unavailable", server.name))?;

    let (stdin_tx, stdin_rx) = mpsc::sync_channel(STDIN_CHANNEL_CAPACITY);
    thread::Builder::new()
        .name("mcpace-upstream-stdin".to_string())
        .spawn(move || run_stdin_writer(stdin, stdin_rx))
        .map_err(|error| {
            format!(
                "failed to start stdin writer for upstream server '{}': {}",
                server.name, error
            )
        })?;

    let (stdout_tx, stdout_rx) = mpsc::sync_channel(STDOUT_CHANNEL_CAPACITY);
    thread::Builder::new()
        .name("mcpace-upstream-stdout".to_string())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_bounded_line(&mut reader, MAX_STDIO_RESPONSE_LINE_BYTES) {
                    Ok(Some(line)) => {
                        if stdout_tx.send(StdioOutput::Line(line)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = stdout_tx.send(StdioOutput::Error(error.to_string()));
                        break;
                    }
                }
            }
        })
        .map_err(|error| {
            format!(
                "failed to start stdout reader for upstream server '{}': {}",
                server.name, error
            )
        })?;

    let (stderr_tx, stderr_rx) = mpsc::sync_channel(STDERR_CHANNEL_CAPACITY);
    thread::Builder::new()
        .name("mcpace-upstream-stderr".to_string())
        .spawn(move || {
            let mut reader = BufReader::new(stderr);
            loop {
                match read_bounded_line(&mut reader, MAX_STDERR_LINE_BYTES) {
                    Ok(Some(line)) => {
                        // Diagnostics must never backpressure or deadlock a noisy server.
                        if matches!(
                            stderr_tx.try_send(line),
                            Err(mpsc::TrySendError::Disconnected(_))
                        ) {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = stderr_tx.try_send(error.to_string());
                        break;
                    }
                }
            }
        })
        .map_err(|error| {
            format!(
                "failed to start stderr reader for upstream server '{}': {}",
                server.name, error
            )
        })?;

    let child = pending_child
        .disarm()
        .ok_or_else(|| format!("upstream server '{}' child was unavailable", server.name))?;
    Ok(RunningServer {
        child,
        stdin_tx: Some(stdin_tx),
        stdout_rx,
        stderr_rx,
    })
}

pub(super) fn read_bounded_line(
    reader: &mut impl BufRead,
    max_bytes: usize,
) -> StdioRuntimeResult<Option<String>> {
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf().map_err(|error| {
            StdioRuntimeError::from(format!("failed to read upstream process output: {}", error))
        })?;
        if available.is_empty() {
            if bytes.is_empty() {
                return Ok(None);
            }
            break;
        }
        let take = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(available.len());
        if bytes.len().saturating_add(take) > max_bytes {
            return Err(StdioRuntimeError::from(format!(
                "upstream process output line exceeds {} bytes",
                max_bytes
            )));
        }
        bytes.extend_from_slice(&available[..take]);
        reader.consume(take);
        if bytes.last() == Some(&b'\n') {
            break;
        }
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| StdioRuntimeError::from("upstream process output is not valid UTF-8"))
}

fn run_stdin_writer(mut stdin: impl Write, requests: Receiver<StdioWriteRequest>) {
    while let Ok(request) = requests.recv() {
        let result = stdin
            .write_all(&request.payload)
            .and_then(|_| stdin.flush())
            .map_err(|error| {
                StdioRuntimeError::from(format!(
                    "failed to write upstream JSON-RPC message: {}",
                    error
                ))
            });
        let failed = result.is_err();
        let _ = request.result_tx.send(result);
        if failed {
            break;
        }
    }
}

fn write_jsonrpc_interruption(
    running: &RunningServer,
    deadline: Instant,
    server_name: &str,
    method: &str,
    lease_lost: Option<&AtomicBool>,
) -> Option<StdioRuntimeError> {
    if lease_lost
        .map(|value| value.load(Ordering::SeqCst))
        .unwrap_or(false)
    {
        return Some(runtime_lease_lost_error(server_name, method, &running.stderr_rx).into());
    }
    if Instant::now() >= deadline {
        return Some(
            format!(
                "timed out writing upstream server '{}' request for {}{}",
                server_name,
                method,
                stderr_suffix(&running.stderr_rx)
            )
            .into(),
        );
    }
    None
}

pub(super) fn write_jsonrpc(
    running: &RunningServer,
    message: JsonValue,
    deadline: Instant,
    server_name: &str,
    method: &str,
    lease_lost: Option<&AtomicBool>,
) -> StdioRuntimeResult<()> {
    let sender = running.stdin_tx.as_ref().ok_or_else(|| {
        StdioRuntimeError::from(format!(
            "upstream server '{}' stdin writer is unavailable",
            server_name
        ))
    })?;
    let mut payload = message.to_compact_string().into_bytes();
    payload.push(b'\n');
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let mut pending = StdioWriteRequest { payload, result_tx };

    loop {
        if let Some(error) =
            write_jsonrpc_interruption(running, deadline, server_name, method, lease_lost)
        {
            return Err(error);
        }
        match sender.try_send(pending) {
            Ok(()) => break,
            Err(mpsc::TrySendError::Full(request)) => {
                pending = request;
                let remaining = deadline.saturating_duration_since(Instant::now());
                thread::sleep(remaining.min(STDIN_WAIT_SLICE));
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                return Err(format!(
                    "upstream server '{}' stdin writer closed before {}{}",
                    server_name,
                    method,
                    stderr_suffix(&running.stderr_rx)
                )
                .into());
            }
        }
    }

    loop {
        if let Some(error) =
            write_jsonrpc_interruption(running, deadline, server_name, method, lease_lost)
        {
            return Err(error);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        match result_rx.recv_timeout(remaining.min(STDIN_WAIT_SLICE)) {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(error)) => return Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(format!(
                    "upstream server '{}' stdin writer stopped before {}{}",
                    server_name,
                    method,
                    stderr_suffix(&running.stderr_rx)
                )
                .into());
            }
        }
    }
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
    stdout_rx: &Receiver<StdioOutput>,
    stderr_rx: &Receiver<String>,
    expected_id: i64,
    deadline: Instant,
    server_name: &str,
    method: &str,
    lease_lost: Option<&AtomicBool>,
) -> StdioRuntimeResult<JsonValue> {
    loop {
        if lease_lost
            .map(|value| value.load(Ordering::SeqCst))
            .unwrap_or(false)
        {
            return Err(runtime_lease_lost_error(server_name, method, stderr_rx).into());
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(format!(
                "timed out waiting for upstream server '{}' response to {}.{}{}",
                server_name,
                method,
                format_expected_id(expected_id),
                stderr_suffix(stderr_rx)
            )
            .into());
        }
        let remaining = deadline.saturating_duration_since(now);
        match stdout_rx.recv_timeout(remaining.min(Duration::from_millis(250))) {
            Ok(StdioOutput::Error(error)) => {
                return Err(format!(
                    "upstream server '{}' produced invalid output while responding to {}: {}{}",
                    server_name,
                    method,
                    error,
                    stderr_suffix(stderr_rx)
                )
                .into());
            }
            Ok(StdioOutput::Line(line)) => {
                if lease_lost
                    .map(|value| value.load(Ordering::SeqCst))
                    .unwrap_or(false)
                {
                    return Err(runtime_lease_lost_error(server_name, method, stderr_rx).into());
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
                    return Err(runtime_lease_lost_error(server_name, method, stderr_rx).into());
                }
                if let Err(error) = mcp::validate_response_envelope(&message, expected_id) {
                    return Err(format!(
                        "upstream server '{}' returned a malformed JSON-RPC response for {}: {}{}",
                        server_name,
                        method,
                        error,
                        stderr_suffix(stderr_rx)
                    )
                    .into());
                }
                if let Some(error) = json_helpers::value_at_path(&message, &["error"]) {
                    return Err(format!(
                        "upstream server '{}' returned JSON-RPC error for {}: {}{}",
                        server_name,
                        method,
                        error.to_compact_string(),
                        stderr_suffix(stderr_rx)
                    )
                    .into());
                }
                return Ok(json_helpers::value_at_path(&message, &["result"])
                    .cloned()
                    .expect("validated JSON-RPC result"));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(format!(
                    "upstream server '{}' closed stdout before responding to {}{}",
                    server_name,
                    method,
                    stderr_suffix(stderr_rx)
                )
                .into());
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
    let Ok(mut taskkill_child) = command.spawn() else {
        return;
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match taskkill_child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) if Instant::now() >= deadline => break,
            Ok(None) => thread::sleep(Duration::from_millis(25)),
        }
    }
    let _ = taskkill_child.kill();
    let force_deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < force_deadline {
        match taskkill_child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => thread::sleep(Duration::from_millis(25)),
        }
    }
}
