use crate::dashboard;
use crate::json::{parse_str, JsonValue};
use crate::runtimepaths;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Default)]
struct ParsedArgs {
    action: Option<String>,
    help: bool,
    json_output: bool,
    root_override: Option<PathBuf>,
    host: Option<String>,
    port: Option<u16>,
    passthrough: Vec<String>,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct ServeState {
    root_path: String,
    state_root: String,
    host: String,
    port: u16,
    url: String,
    pid: u32,
    started_at_ms: u128,
    runner_path: String,
    stdout_log_path: String,
    stderr_log_path: String,
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

    match parsed.action.as_deref() {
        Some("start") => run_start(parsed, default_root, stdout, stderr),
        Some("stop") => run_stop(parsed, default_root, stdout, stderr),
        Some("status") => run_status(parsed, default_root, stdout, stderr),
        _ => dashboard::run_serve(&parsed.passthrough, default_root, stdout, stderr),
    }
}

fn parse_args(args: &[String]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "start" | "stop" | "status" => {
                if parsed.action.is_some() {
                    parsed.error = Some("serve accepts only one action".to_string());
                    return parsed;
                }
                parsed.action = Some(args[index].to_string());
                index += 1;
            }
            "--json" | "-json" => {
                parsed.json_output = true;
                parsed.passthrough.push(args[index].clone());
                index += 1;
            }
            "--root" | "-root" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("serve requires a path after --root".to_string());
                    return parsed;
                };
                parsed.root_override = Some(PathBuf::from(value));
                parsed.passthrough.push(args[index].clone());
                parsed.passthrough.push(value.clone());
                index += 2;
            }
            "--host" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("serve requires a value after --host".to_string());
                    return parsed;
                };
                parsed.host = Some(value.to_string());
                parsed.passthrough.push(args[index].clone());
                parsed.passthrough.push(value.clone());
                index += 2;
            }
            "--port" => {
                let Some(value) = args.get(index + 1) else {
                    parsed.error = Some("serve requires a value after --port".to_string());
                    return parsed;
                };
                match value.parse::<u16>() {
                    Ok(port) => parsed.port = Some(port),
                    Err(_) => {
                        parsed.error = Some("serve --port must be a valid u16".to_string());
                        return parsed;
                    }
                }
                parsed.passthrough.push(args[index].clone());
                parsed.passthrough.push(value.clone());
                index += 2;
            }
            "-h" | "--help" | "-?" => {
                parsed.help = true;
                return parsed;
            }
            other => {
                parsed.passthrough.push(other.to_string());
                index += 1;
            }
        }
    }

    parsed
}

fn write_help(stdout: &mut dyn Write) {
    let _ = writeln!(
        stdout,
        "Usage: mcpace serve [start|stop|status] [--json] [--root <path>] [--host <addr>] [--port <n>]"
    );
    let _ = writeln!(stdout, "");
    let _ = writeln!(stdout, "Public serve surface:");
    let _ = writeln!(
        stdout,
        "  mcpace serve [--root <path>] [--host <addr>] [--port <n>]"
    );
    let _ = writeln!(
        stdout,
        "  mcpace serve start [--json] [--root <path>] [--host <addr>] [--port <n>]"
    );
    let _ = writeln!(stdout, "  mcpace serve stop [--json] [--root <path>]");
    let _ = writeln!(stdout, "  mcpace serve status [--json] [--root <path>]");
    let _ = writeln!(stdout, "");
    let _ = writeln!(stdout, "serve is the public one-port MCPace surface. The default local MCP endpoint is http://127.0.0.1:39022/mcp.");
}

fn run_start(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };

    let host = parsed.host.unwrap_or_else(|| "127.0.0.1".to_string());
    let port = parsed.port.unwrap_or(39022);
    let canonical_root = canonicalize_or_original(&root_path);
    let state_root = runtimepaths::resolve_state_root(&canonical_root);
    if let Err(error) = runtimepaths::ensure_runtime_dir(&state_root) {
        let _ = writeln!(stderr, "{}", error);
        return 1;
    }
    if let Err(error) = runtimepaths::ensure_serve_dir(&state_root) {
        let _ = writeln!(stderr, "{}", error);
        return 1;
    }
    if let Err(error) = runtimepaths::ensure_runtime_bin_dir(&state_root) {
        let _ = writeln!(stderr, "{}", error);
        return 1;
    }

    if let Ok(status) = collect_status(&canonical_root, Some(host.clone()), Some(port)) {
        if status.status == "running" {
            return write_status_response(&status, parsed.json_output, stdout);
        }
    }

    let current_exe = match resolve_runner_source() {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "failed to resolve mcpace binary path: {}", error);
            return 1;
        }
    };
    let runner_path = runtimepaths::serve_runner_path(&state_root);
    if let Err(error) = fs::copy(&current_exe, &runner_path) {
        let _ = writeln!(
            stderr,
            "failed to copy mcpace serve runner to '{}': {}",
            runner_path.display(),
            error
        );
        return 1;
    }

    let stdout_log_path = runtimepaths::serve_stdout_log_path(&state_root);
    let stderr_log_path = runtimepaths::serve_stderr_log_path(&state_root);
    let stdout_file = match File::create(&stdout_log_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to open serve stdout log '{}': {}",
                stdout_log_path.display(),
                error
            );
            return 1;
        }
    };
    let stderr_file = match File::create(&stderr_log_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to open serve stderr log '{}': {}",
                stderr_log_path.display(),
                error
            );
            return 1;
        }
    };

    let child = match spawn_background(
        &runner_path,
        &canonical_root,
        &host,
        port,
        stdout_file,
        stderr_file,
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };

    let state = ServeState {
        root_path: sanitize_display(&canonical_root),
        state_root: sanitize_display(&state_root),
        host: host.clone(),
        port,
        url: format!("http://{}:{}/mcp", host, port),
        pid: child.id(),
        started_at_ms: now_ms(),
        runner_path: sanitize_display(&runner_path),
        stdout_log_path: sanitize_display(&stdout_log_path),
        stderr_log_path: sanitize_display(&stderr_log_path),
    };
    if let Err(error) = write_state(&runtimepaths::serve_state_path(&state_root), &state) {
        let _ = writeln!(stderr, "{}", error);
        return 1;
    }

    if let Err(error) = wait_for_health(&host, port, 60, Duration::from_millis(100)) {
        let _ = kill_process(state.pid);
        let _ = fs::remove_file(runtimepaths::serve_state_path(&state_root));
        let _ = writeln!(stderr, "{}", error);
        return 1;
    }

    let status = match collect_status(&canonical_root, Some(host), Some(port)) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    write_status_response(&status, parsed.json_output, stdout)
}

fn run_stop(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };
    let canonical_root = canonicalize_or_original(&root_path);
    let state_root = runtimepaths::resolve_state_root(&canonical_root);
    let state_path = runtimepaths::serve_state_path(&state_root);
    let existing = read_state(&state_path).ok();
    if let Some(state) = &existing {
        let _ = kill_process(state.pid);
        for _ in 0..40 {
            if !health_check(&state.host, state.port).unwrap_or(false) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    let _ = fs::remove_file(&state_path);

    let status = match collect_status(&canonical_root, None, None) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    write_status_response(&status, parsed.json_output, stdout)
}

fn run_status(
    parsed: ParsedArgs,
    default_root: Option<PathBuf>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let root_path = parsed.root_override.or(default_root);
    let Some(root_path) = root_path else {
        let _ = writeln!(stderr, "mcpace root not found; expected mcpace.config.json");
        return 1;
    };
    let canonical_root = canonicalize_or_original(&root_path);
    let status = match collect_status(&canonical_root, parsed.host, parsed.port) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    write_status_response(&status, parsed.json_output, stdout)
}

#[derive(Debug)]
struct ServeStatus {
    root_path: String,
    state_root: String,
    host: String,
    port: u16,
    url: String,
    status: String,
    pid: Option<u32>,
    started_at_ms: Option<u128>,
    stdout_log_path: String,
    stderr_log_path: String,
    warnings: Vec<String>,
}

fn collect_status(
    root_path: &Path,
    host_override: Option<String>,
    port_override: Option<u16>,
) -> Result<ServeStatus, String> {
    let state_root = runtimepaths::resolve_state_root(root_path);
    let state_path = runtimepaths::serve_state_path(&state_root);
    let state = read_state(&state_path).ok();
    let should_probe = state.is_some() || host_override.is_some() || port_override.is_some();
    let host = host_override
        .or_else(|| state.as_ref().map(|value| value.host.clone()))
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = port_override
        .or_else(|| state.as_ref().map(|value| value.port))
        .unwrap_or(39022);
    let running = should_probe && health_check(&host, port).unwrap_or(false);
    let mut warnings = Vec::new();
    let status = if running {
        "running".to_string()
    } else if state.is_some() {
        warnings.push(
            "serve state exists but the MCPace HTTP endpoint did not answer the local health probe"
                .to_string(),
        );
        "stopped".to_string()
    } else {
        "stopped".to_string()
    };

    Ok(ServeStatus {
        root_path: sanitize_display(root_path),
        state_root: sanitize_display(&state_root),
        host: host.clone(),
        port,
        url: format!("http://{}:{}/mcp", host, port),
        status,
        pid: state.as_ref().map(|value| value.pid).filter(|_| running),
        started_at_ms: state
            .as_ref()
            .map(|value| value.started_at_ms)
            .filter(|_| running),
        stdout_log_path: state
            .as_ref()
            .map(|value| value.stdout_log_path.clone())
            .unwrap_or_else(|| sanitize_display(&runtimepaths::serve_stdout_log_path(&state_root))),
        stderr_log_path: state
            .as_ref()
            .map(|value| value.stderr_log_path.clone())
            .unwrap_or_else(|| sanitize_display(&runtimepaths::serve_stderr_log_path(&state_root))),
        warnings,
    })
}

fn write_status_response(status: &ServeStatus, json_output: bool, stdout: &mut dyn Write) -> i32 {
    if json_output {
        let mut map = BTreeMap::new();
        map.insert(
            "rootPath".to_string(),
            JsonValue::string(status.root_path.clone()),
        );
        map.insert(
            "stateRoot".to_string(),
            JsonValue::string(status.state_root.clone()),
        );
        map.insert("host".to_string(), JsonValue::string(status.host.clone()));
        map.insert("port".to_string(), JsonValue::number(status.port));
        map.insert("url".to_string(), JsonValue::string(status.url.clone()));
        map.insert(
            "status".to_string(),
            JsonValue::string(status.status.clone()),
        );
        map.insert(
            "pid".to_string(),
            match status.pid {
                Some(value) => JsonValue::number(value),
                None => JsonValue::Null,
            },
        );
        map.insert(
            "startedAtMs".to_string(),
            match status.started_at_ms {
                Some(value) => JsonValue::number(value),
                None => JsonValue::Null,
            },
        );
        map.insert(
            "stdoutLogPath".to_string(),
            JsonValue::string(status.stdout_log_path.clone()),
        );
        map.insert(
            "stderrLogPath".to_string(),
            JsonValue::string(status.stderr_log_path.clone()),
        );
        map.insert(
            "warnings".to_string(),
            JsonValue::array(status.warnings.iter().cloned().map(JsonValue::string)),
        );
        let _ = writeln!(stdout, "{}", JsonValue::Object(map).to_pretty_string());
        return 0;
    }

    let _ = writeln!(stdout, "Serve status: {}", status.status);
    let _ = writeln!(stdout, "URL: {}", status.url);
    let _ = writeln!(stdout, "Host: {}", status.host);
    let _ = writeln!(stdout, "Port: {}", status.port);
    let _ = writeln!(stdout, "Root path: {}", status.root_path);
    let _ = writeln!(stdout, "State root: {}", status.state_root);
    let _ = writeln!(stdout, "Stdout log: {}", status.stdout_log_path);
    let _ = writeln!(stdout, "Stderr log: {}", status.stderr_log_path);
    if let Some(pid) = status.pid {
        let _ = writeln!(stdout, "PID: {}", pid);
    }
    if let Some(started_at_ms) = status.started_at_ms {
        let _ = writeln!(stdout, "Started at ms: {}", started_at_ms);
    }
    if !status.warnings.is_empty() {
        let _ = writeln!(stdout, "Warnings: {}", status.warnings.join(" | "));
    }
    0
}

fn write_state(path: &Path, state: &ServeState) -> Result<(), String> {
    let payload = JsonValue::object([
        ("rootPath", JsonValue::string(state.root_path.clone())),
        ("stateRoot", JsonValue::string(state.state_root.clone())),
        ("host", JsonValue::string(state.host.clone())),
        ("port", JsonValue::number(state.port)),
        ("url", JsonValue::string(state.url.clone())),
        ("pid", JsonValue::number(state.pid)),
        ("startedAtMs", JsonValue::number(state.started_at_ms)),
        ("runnerPath", JsonValue::string(state.runner_path.clone())),
        (
            "stdoutLogPath",
            JsonValue::string(state.stdout_log_path.clone()),
        ),
        (
            "stderrLogPath",
            JsonValue::string(state.stderr_log_path.clone()),
        ),
    ])
    .to_pretty_string();
    write_atomic(path, payload)
}

fn read_state(path: &Path) -> Result<ServeState, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read serve state '{}': {}", path.display(), error))?;
    let json = parse_str(&raw).map_err(|error| {
        format!(
            "failed to parse serve state '{}': {}",
            path.display(),
            error
        )
    })?;
    let host = json
        .get("host")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| format!("serve state '{}' is missing host", path.display()))?
        .to_string();
    let port = json
        .get("port")
        .and_then(JsonValue::as_i64)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| format!("serve state '{}' is missing port", path.display()))?;
    let pid = json
        .get("pid")
        .and_then(JsonValue::as_i64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("serve state '{}' is missing pid", path.display()))?;
    let started_at_ms = json
        .get("startedAtMs")
        .and_then(JsonValue::as_i64)
        .and_then(|value| u128::try_from(value).ok())
        .ok_or_else(|| format!("serve state '{}' is missing startedAtMs", path.display()))?;

    Ok(ServeState {
        root_path: json
            .get("rootPath")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        state_root: json
            .get("stateRoot")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        host: host.clone(),
        port,
        url: json
            .get("url")
            .and_then(JsonValue::as_str)
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("http://{}:{}/mcp", host, port)),
        pid,
        started_at_ms,
        runner_path: json
            .get("runnerPath")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        stdout_log_path: json
            .get("stdoutLogPath")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        stderr_log_path: json
            .get("stderrLogPath")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

fn health_check(host: &str, port: u16) -> Result<bool, String> {
    let mut addrs = (host, port).to_socket_addrs().map_err(|error| {
        format!(
            "failed to resolve serve address {}:{}: {}",
            host, port, error
        )
    })?;
    let Some(addr) = addrs.next() else {
        return Ok(false);
    };
    match TcpStream::connect_timeout(&addr, Duration::from_millis(300)) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

fn wait_for_health(host: &str, port: u16, attempts: usize, delay: Duration) -> Result<(), String> {
    for _ in 0..attempts {
        if health_check(host, port).unwrap_or(false) {
            return Ok(());
        }
        thread::sleep(delay);
    }
    Err(format!(
        "serve did not become healthy on http://{}:{}/healthz in time",
        host, port
    ))
}

fn canonicalize_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn resolve_runner_source() -> Result<PathBuf, std::io::Error> {
    let current = std::env::current_exe()?;
    if current
        .file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.starts_with("mcpace"))
        .unwrap_or(false)
        && !current
            .parent()
            .and_then(|parent| parent.file_name())
            .map(|value| value == "deps")
            .unwrap_or(false)
    {
        return Ok(current);
    }

    let fallback = current
        .parent()
        .and_then(|parent| parent.parent())
        .map(|parent| {
            parent.join(if cfg!(windows) {
                "mcpace.exe"
            } else {
                "mcpace"
            })
        });
    match fallback {
        Some(path) if path.is_file() => Ok(path),
        _ => Ok(current),
    }
}

fn sanitize_display(path: &Path) -> String {
    let rendered = path.display().to_string();
    rendered
        .strip_prefix(r"\\?\")
        .unwrap_or(&rendered)
        .to_string()
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn write_atomic(path: &Path, contents: String) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {}", parent.display(), error))?;
    }
    let temp_path = path.with_extension(format!("tmp-{}-{}", std::process::id(), now_ms()));
    fs::write(&temp_path, contents)
        .map_err(|error| format!("failed to write {}: {}", temp_path.display(), error))?;
    #[cfg(windows)]
    {
        let _ = fs::remove_file(path);
    }
    fs::rename(&temp_path, path).map_err(|error| {
        format!(
            "failed to move {} to {}: {}",
            temp_path.display(),
            path.display(),
            error
        )
    })?;
    Ok(())
}

fn spawn_background(
    runner_path: &Path,
    root_path: &Path,
    host: &str,
    port: u16,
    stdout_file: File,
    stderr_file: File,
) -> Result<std::process::Child, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        const CREATE_NO_WINDOW: u32 = 0x08000000;

        return Command::new(runner_path)
            .arg("serve")
            .arg("--root")
            .arg(root_path)
            .arg("--host")
            .arg(host)
            .arg("--port")
            .arg(port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| format!("failed to start MCPace serve runtime: {}", error));
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};

        unsafe extern "C" {
            fn setsid() -> i32;
        }

        let mut command = Command::new(runner_path);
        command
            .arg("serve")
            .arg("--root")
            .arg(root_path)
            .arg("--host")
            .arg(host)
            .arg("--port")
            .arg(port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file));

        unsafe {
            command.pre_exec(|| {
                if setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        return command
            .spawn()
            .map_err(|error| format!("failed to start MCPace serve runtime: {}", error));
    }

    #[allow(unreachable_code)]
    Err("background serve launch is not implemented for this platform".to_string())
}

fn kill_process(pid: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        let output = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output()
            .map_err(|error| format!("failed to stop serve process {}: {}", pid, error))?;
        if output.status.success() {
            return Ok(());
        }
        let stderr_text = String::from_utf8_lossy(&output.stderr);
        if stderr_text.contains("not found") || stderr_text.contains("не найден") {
            return Ok(());
        }
        return Err(format!(
            "failed to stop serve process {}: {}",
            pid,
            stderr_text.trim()
        ));
    }

    #[cfg(unix)]
    {
        let output = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output()
            .map_err(|error| format!("failed to stop serve process {}: {}", pid, error))?;
        if output.status.success() {
            return Ok(());
        }
        let stderr_text = String::from_utf8_lossy(&output.stderr);
        if stderr_text.contains("No such process") {
            return Ok(());
        }
        return Err(format!(
            "failed to stop serve process {}: {}",
            pid,
            stderr_text.trim()
        ));
    }

    #[allow(unreachable_code)]
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn temp_root() -> PathBuf {
        let unique = format!("mcpace-serve-test-{}-{}", std::process::id(), now_ms());
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_minimal_config(root: &Path) {
        fs::write(
            root.join("mcpace.config.json"),
            r#"{
  "version": "0.3.0",
  "profiles": {
    "runtime": {
      "default": "safe",
      "profiles": {
        "safe": { "description": "Safe", "serverOverrides": {} }
      }
    }
  },
  "servers": {}
}"#,
        )
        .unwrap();
    }

    fn free_port() -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }

    #[test]
    fn serve_start_status_stop_round_trip() {
        let root = temp_root();
        write_minimal_config(&root);
        let port = free_port();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let start = run(
            &[
                "start".to_string(),
                "--json".to_string(),
                "--root".to_string(),
                root.display().to_string(),
                "--port".to_string(),
                port.to_string(),
            ],
            None,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(start, 0, "stderr: {}", String::from_utf8_lossy(&stderr));
        let start_text = String::from_utf8(stdout.clone()).unwrap();
        assert!(
            start_text.contains(r#""status": "running""#),
            "stdout: {}",
            start_text
        );
        assert!(health_check("127.0.0.1", port).unwrap_or(false));

        let mut status_stdout = Vec::new();
        let mut status_stderr = Vec::new();
        let status = run(
            &[
                "status".to_string(),
                "--json".to_string(),
                "--root".to_string(),
                root.display().to_string(),
            ],
            None,
            &mut status_stdout,
            &mut status_stderr,
        );
        assert_eq!(
            status,
            0,
            "stderr: {}",
            String::from_utf8_lossy(&status_stderr)
        );
        let status_text = String::from_utf8(status_stdout).unwrap();
        assert!(
            status_text.contains(r#""status": "running""#),
            "stdout: {}",
            status_text
        );
        assert!(
            status_text.contains(&format!(r#""port": {}"#, port)),
            "stdout: {}",
            status_text
        );

        let mut stop_stdout = Vec::new();
        let mut stop_stderr = Vec::new();
        let stop = run(
            &[
                "stop".to_string(),
                "--json".to_string(),
                "--root".to_string(),
                root.display().to_string(),
            ],
            None,
            &mut stop_stdout,
            &mut stop_stderr,
        );
        assert_eq!(stop, 0, "stderr: {}", String::from_utf8_lossy(&stop_stderr));
        let stop_text = String::from_utf8(stop_stdout).unwrap();
        assert!(
            stop_text.contains(r#""status": "stopped""#),
            "stdout: {}",
            stop_text
        );

        let _ = fs::remove_dir_all(root);
    }
}
