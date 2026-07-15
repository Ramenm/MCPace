#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
use serde::Deserialize;
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::fmt;
#[cfg(windows)]
use std::fs::{self, File, OpenOptions};
#[cfg(windows)]
use std::io::Write;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::{Command, Stdio};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
#[cfg(windows)]
const PLAN_SCHEMA: &str = "mcpace.windowsAutostartPlan.v1";
#[cfg(windows)]
const MCPACE_EXE_NAME: &str = "mcpace.exe";
#[cfg(windows)]
const ROOT_MARKER_FILE: &str = "mcpace.config.json";
#[cfg(windows)]
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

#[cfg(windows)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutostartPlan {
    schema: String,
    target_app_path: String,
    target_args: Vec<String>,
    root_path: Option<String>,
}

#[cfg(windows)]
#[derive(Debug)]
struct Invocation {
    program: PathBuf,
    args: Vec<OsString>,
    root_path: Option<PathBuf>,
    source: String,
}

#[cfg(windows)]
#[derive(Debug)]
struct LauncherError {
    message: String,
}

#[cfg(windows)]
impl LauncherError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[cfg(windows)]
impl fmt::Display for LauncherError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

#[cfg(windows)]
type LauncherResult<T> = std::result::Result<T, LauncherError>;

#[cfg(windows)]
fn main() {
    std::process::exit(run_windows());
}

#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}

#[cfg(windows)]
fn run_windows() -> i32 {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let invocation = match resolve_invocation(args) {
        Ok(value) => value,
        Err(error) => {
            let log_dir = fallback_log_dir();
            let _ = write_launcher_log(&log_dir, &error.to_string());
            return 2;
        }
    };

    let root = invocation
        .root_path
        .clone()
        .or_else(|| arg_value(&invocation.args, "--root").map(PathBuf::from));
    let log_dir = root
        .as_deref()
        .map(agent_log_dir_for_root)
        .unwrap_or_else(fallback_log_dir);
    let _ = fs::create_dir_all(&log_dir);

    if !invocation.program.is_file() {
        let _ = write_launcher_log(
            &log_dir,
            &format!(
                "target MCPace binary does not exist: {}",
                invocation.program.display()
            ),
        );
        return 1;
    }

    let stdout = open_log_file(&log_dir.join("agent-stdout.log"));
    let stderr = open_log_file(&log_dir.join("agent-stderr.log"));

    let mut command = Command::new(&invocation.program);
    command.args(&invocation.args);
    if let Some(root) = root.as_deref() {
        command.current_dir(root);
    }
    command
        .stdin(Stdio::null())
        .stdout(stdout.map(Stdio::from).unwrap_or_else(|_| Stdio::null()))
        .stderr(stderr.map(Stdio::from).unwrap_or_else(|_| Stdio::null()))
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);

    match command.spawn() {
        Ok(child) => {
            let _ = write_launcher_log(
                &log_dir,
                &format!(
                    "started hidden MCPace Agent pid={} source={} target={} argsCount={}",
                    child.id(),
                    invocation.source,
                    invocation.program.display(),
                    invocation.args.len()
                ),
            );
            0
        }
        Err(error) => {
            let _ = write_launcher_log(
                &log_dir,
                &format!(
                    "failed to start hidden MCPace Agent source={} target={}: {}",
                    invocation.source,
                    invocation.program.display(),
                    error
                ),
            );
            1
        }
    }
}

#[cfg(windows)]
fn resolve_invocation(mut args: Vec<OsString>) -> LauncherResult<Invocation> {
    if args.is_empty() || os_arg_eq(&args[0], "--from-login") {
        return invocation_from_plan(&default_plan_path());
    }
    if os_arg_eq(&args[0], "--autostart-plan") {
        let Some(path) = args.get(1).map(PathBuf::from) else {
            return Err(LauncherError::new("missing value after --autostart-plan"));
        };
        return invocation_from_plan(&path);
    }

    let program = PathBuf::from(args.remove(0));
    let args_as_strings = args
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    validate_invocation_target(&program)?;
    validate_agent_args(&args_as_strings)?;
    Ok(Invocation {
        program,
        args,
        root_path: None,
        source: "explicit-argv".to_string(),
    })
}

#[cfg(windows)]
fn invocation_from_plan(path: &Path) -> LauncherResult<Invocation> {
    let plan = load_plan(path)?;
    if plan.schema != PLAN_SCHEMA {
        return Err(LauncherError::new(format!(
            "unsupported MCPace autostart plan schema '{}' in {}; expected '{}'",
            plan.schema,
            path.display(),
            PLAN_SCHEMA
        )));
    }
    if plan.target_app_path.trim().is_empty() {
        return Err(LauncherError::new(format!(
            "MCPace autostart plan has an empty targetAppPath: {}",
            path.display()
        )));
    }
    let program = PathBuf::from(&plan.target_app_path);
    validate_invocation_target(&program)?;
    validate_agent_args(&plan.target_args)?;
    let root_path = plan
        .root_path
        .map(PathBuf::from)
        .or_else(|| arg_value_string(&plan.target_args, "--root").map(PathBuf::from));
    if let Some(root) = root_path.as_deref() {
        validate_agent_root(root)?;
    }
    Ok(Invocation {
        program,
        args: plan.target_args.into_iter().map(OsString::from).collect(),
        root_path,
        source: format!("plan:{}", path.display()),
    })
}

#[cfg(windows)]
fn validate_invocation_target(program: &Path) -> LauncherResult<()> {
    let Some(file_name) = program.file_name().and_then(|value| value.to_str()) else {
        return Err(LauncherError::new(
            "MCPace autostart target is missing a file name",
        ));
    };
    if !file_name.eq_ignore_ascii_case(MCPACE_EXE_NAME) {
        return Err(LauncherError::new(format!(
            "MCPace autostart launcher refuses to start unexpected target '{}'; expected {}",
            file_name, MCPACE_EXE_NAME
        )));
    }

    let launcher_exe = std::env::current_exe().map_err(|error| {
        LauncherError::new(format!("failed to resolve launcher path: {}", error))
    })?;
    let launcher_dir = launcher_exe
        .parent()
        .ok_or_else(|| LauncherError::new("launcher executable has no parent directory"))?;
    let target_dir = program
        .parent()
        .ok_or_else(|| LauncherError::new("MCPace autostart target has no parent directory"))?;
    let launcher_dir = fs::canonicalize(launcher_dir).map_err(|error| {
        LauncherError::new(format!(
            "failed to canonicalize launcher directory {}: {}",
            launcher_dir.display(),
            error
        ))
    })?;
    let target_dir = fs::canonicalize(target_dir).map_err(|error| {
        LauncherError::new(format!(
            "failed to canonicalize target directory {}: {}",
            target_dir.display(),
            error
        ))
    })?;
    if launcher_dir != target_dir {
        return Err(LauncherError::new(format!(
            "MCPace autostart launcher refuses target outside its install directory: target={}, launcherDir={}",
            program.display(),
            launcher_dir.display()
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_agent_args(args: &[String]) -> LauncherResult<()> {
    if args.len() < 3 || args[0] != "agent" || args[1] != "run" || args[2] != "--autostart" {
        return Err(LauncherError::new(
            "MCPace autostart launcher only starts `mcpace agent run --autostart`",
        ));
    }
    if args
        .iter()
        .any(|arg| arg.contains('\0') || arg.contains('\r') || arg.contains('\n'))
    {
        return Err(LauncherError::new(
            "MCPace autostart plan contains a disallowed control character",
        ));
    }
    if arg_value_string(args, "--root").is_none() {
        return Err(LauncherError::new(
            "MCPace autostart plan must forward an explicit --root",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_agent_root(root: &Path) -> LauncherResult<()> {
    if root.join(ROOT_MARKER_FILE).is_file() {
        Ok(())
    } else {
        Err(LauncherError::new(format!(
            "MCPace autostart root does not contain {}: {}",
            ROOT_MARKER_FILE,
            root.display()
        )))
    }
}

#[cfg(windows)]
fn arg_value_string(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|items| items[0].eq_ignore_ascii_case(flag))
        .map(|items| items[1].clone())
}

#[cfg(windows)]
fn load_plan(path: &Path) -> LauncherResult<AutostartPlan> {
    let data = fs::read(path).map_err(|error| {
        LauncherError::new(format!(
            "failed to read MCPace autostart plan {}: {}",
            path.display(),
            error
        ))
    })?;
    serde_json::from_slice(&data).map_err(|error| {
        LauncherError::new(format!(
            "failed to parse MCPace autostart plan {}: {}",
            path.display(),
            error
        ))
    })
}

#[cfg(windows)]
fn arg_value(args: &[OsString], flag: &str) -> Option<OsString> {
    args.windows(2)
        .find(|items| os_arg_eq(&items[0], flag))
        .map(|items| items[1].clone())
}

#[cfg(windows)]
fn os_arg_eq(value: &OsString, expected: &str) -> bool {
    value.to_string_lossy().eq_ignore_ascii_case(expected)
}

#[cfg(windows)]
fn agent_log_dir_for_root(root: &Path) -> PathBuf {
    root.join("data").join("runtime").join("agent")
}

#[cfg(windows)]
fn default_plan_path() -> PathBuf {
    fallback_log_dir().join("autostart-plan.json")
}

#[cfg(windows)]
fn fallback_log_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TEMP").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("MCPace")
        .join("agent")
}

#[cfg(windows)]
fn open_log_file(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    rotate_log_if_needed(path)?;
    OpenOptions::new().create(true).append(true).open(path)
}

#[cfg(windows)]
fn rotate_log_if_needed(path: &Path) -> std::io::Result<()> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    if metadata.len() <= MAX_LOG_BYTES {
        return Ok(());
    }
    let rotated = path.with_extension(match path.extension().and_then(|value| value.to_str()) {
        Some(extension) if !extension.is_empty() => format!("{}.1", extension),
        _ => "1".to_string(),
    });
    let _ = fs::remove_file(&rotated);
    fs::rename(path, rotated)
}

#[cfg(windows)]
fn write_launcher_log(log_dir: &Path, message: &str) -> std::io::Result<()> {
    let mut file = open_log_file(&log_dir.join("launcher.log"))?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    writeln!(file, "{} pid={} {}", now_ms, std::process::id(), message)
}
