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
use std::thread;
#[cfg(windows)]
use std::time::{Duration, Instant};

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
const SUPERVISOR_PID_FILE: &str = "supervisor.pid";
#[cfg(windows)]
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
#[cfg(windows)]
const STABLE_RUN_RESET: Duration = Duration::from_secs(120);
#[cfg(windows)]
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(windows)]
const RESTART_DELAYS: [Duration; 7] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(16),
    Duration::from_secs(30),
    Duration::from_secs(60),
];

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
    let _supervisor_mutex = match root.as_deref() {
        Some(value) => match SupervisorMutex::acquire(value) {
            Ok(Some(supervisor_mutex)) => Some(supervisor_mutex),
            Ok(None) => {
                let _ = write_launcher_log(
                    &log_dir,
                    "another MCPace Agent supervisor already owns this root; exiting",
                );
                return 0;
            }
            Err(error) => {
                let _ = write_launcher_log(
                    &log_dir,
                    &format!(
                        "failed to acquire MCPace Agent supervisor ownership: {}",
                        error
                    ),
                );
                return 1;
            }
        },
        None => None,
    };
    let _registration = match root.as_deref() {
        Some(value) => match SupervisorRegistration::create(agent_supervisor_pid_path(value)) {
            Ok(registration) => Some(registration),
            Err(error) => {
                let _ = write_launcher_log(
                    &log_dir,
                    &format!("failed to register MCPace Agent supervisor: {}", error),
                );
                return 1;
            }
        },
        None => None,
    };
    let stop_marker = root.as_deref().map(agent_supervisor_stop_path);
    if stop_requested(stop_marker.as_deref()) {
        acknowledge_stop_request(stop_marker.as_deref());
        let _ = write_launcher_log(
            &log_dir,
            "MCPace Agent stop was requested during supervisor startup; supervisor is stopping",
        );
        return 0;
    }

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

    supervise_invocation(
        &invocation,
        root.as_deref(),
        &log_dir,
        stop_marker.as_deref(),
    )
}

#[cfg(windows)]
fn supervise_invocation(
    invocation: &Invocation,
    root: Option<&Path>,
    log_dir: &Path,
    stop_marker: Option<&Path>,
) -> i32 {
    let mut consecutive_failures = 0usize;
    loop {
        if stop_requested(stop_marker) {
            acknowledge_stop_request(stop_marker);
            let _ = write_launcher_log(
                log_dir,
                "MCPace Agent stop was requested; supervisor is stopping before spawn",
            );
            return 0;
        }
        let stdout = open_log_file(&log_dir.join("agent-stdout.log"));
        let stderr = open_log_file(&log_dir.join("agent-stderr.log"));
        let mut command = Command::new(&invocation.program);
        command.args(&invocation.args);
        if let Some(root) = root {
            command.current_dir(root);
        }
        command
            .stdin(Stdio::null())
            .stdout(stdout.map(Stdio::from).unwrap_or_else(|_| Stdio::null()))
            .stderr(stderr.map(Stdio::from).unwrap_or_else(|_| Stdio::null()))
            .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);

        let started = Instant::now();
        let outcome = match command.spawn() {
            Ok(mut child) => {
                let _ = write_launcher_log(
                    log_dir,
                    &format!(
                        "started supervised MCPace Agent pid={} source={} target={} argsCount={}",
                        child.id(),
                        invocation.source,
                        invocation.program.display(),
                        invocation.args.len()
                    ),
                );
                let waited = loop {
                    match child.try_wait() {
                        Ok(Some(status)) => break Ok(status),
                        Ok(None) => {}
                        Err(error) => break Err(error),
                    }
                    if stop_requested(stop_marker) {
                        if let Err(error) = child.kill() {
                            let _ = write_launcher_log(
                                log_dir,
                                &format!(
                                    "failed to terminate supervised MCPace Agent after stop request: {}",
                                    error
                                ),
                            );
                            return 1;
                        }
                        if let Err(error) = child.wait() {
                            let _ = write_launcher_log(
                                log_dir,
                                &format!(
                                    "failed to reap supervised MCPace Agent after stop request: {}",
                                    error
                                ),
                            );
                            return 1;
                        }
                        acknowledge_stop_request(stop_marker);
                        let _ = write_launcher_log(
                            log_dir,
                            "MCPace Agent stop was requested; supervisor terminated its owned child and is stopping",
                        );
                        return 0;
                    }
                    thread::sleep(STOP_POLL_INTERVAL);
                };
                match waited {
                    Ok(status) if status.success() => {
                        let _ = write_launcher_log(
                            log_dir,
                            "MCPace Agent exited successfully; supervisor is stopping",
                        );
                        return 0;
                    }
                    Ok(status) => format!("MCPace Agent exited with status {}", status),
                    Err(error) => format!("failed while waiting for MCPace Agent: {}", error),
                }
            }
            Err(error) => format!(
                "failed to start supervised MCPace Agent source={} target={}: {}",
                invocation.source,
                invocation.program.display(),
                error
            ),
        };

        if stop_requested(stop_marker) {
            acknowledge_stop_request(stop_marker);
            let _ = write_launcher_log(
                log_dir,
                "MCPace Agent stop was requested; supervisor is stopping",
            );
            return 0;
        }
        if started.elapsed() >= STABLE_RUN_RESET {
            consecutive_failures = 0;
        } else {
            consecutive_failures = consecutive_failures.saturating_add(1);
        }
        let delay = restart_delay(consecutive_failures);
        let _ = write_launcher_log(
            log_dir,
            &format!(
                "{}; restarting after {}ms (consecutiveFailures={})",
                outcome,
                delay.as_millis(),
                consecutive_failures
            ),
        );
        if !wait_for_restart(delay, stop_marker) {
            acknowledge_stop_request(stop_marker);
            let _ = write_launcher_log(
                log_dir,
                "MCPace Agent stop was requested during restart backoff; supervisor is stopping",
            );
            return 0;
        }
    }
}

#[cfg(windows)]
fn stop_requested(stop_marker: Option<&Path>) -> bool {
    stop_marker.is_some_and(Path::is_file)
}

#[cfg(windows)]
fn acknowledge_stop_request(stop_marker: Option<&Path>) {
    if let Some(path) = stop_marker {
        let _ = fs::remove_file(path);
    }
}

#[cfg(windows)]
fn wait_for_restart(delay: Duration, stop_marker: Option<&Path>) -> bool {
    let mut waited = Duration::ZERO;
    while waited < delay {
        if stop_requested(stop_marker) {
            return false;
        }
        let step = STOP_POLL_INTERVAL.min(delay - waited);
        thread::sleep(step);
        waited += step;
    }
    !stop_requested(stop_marker)
}

#[cfg(windows)]
fn restart_delay(consecutive_failures: usize) -> Duration {
    let index = consecutive_failures
        .saturating_sub(1)
        .min(RESTART_DELAYS.len() - 1);
    RESTART_DELAYS[index]
}

#[cfg(windows)]
struct SupervisorMutex {
    handle: *mut std::ffi::c_void,
}

#[cfg(windows)]
impl SupervisorMutex {
    fn acquire(root: &Path) -> std::io::Result<Option<Self>> {
        const ERROR_ALREADY_EXISTS: u32 = 183;

        #[link(name = "kernel32")]
        extern "system" {
            fn CreateMutexW(
                mutex_attributes: *mut std::ffi::c_void,
                initial_owner: i32,
                name: *const u16,
            ) -> *mut std::ffi::c_void;
            fn CloseHandle(object: *mut std::ffi::c_void) -> i32;
            fn GetLastError() -> u32;
        }

        let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in canonical_root.to_string_lossy().to_lowercase().as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        let name = format!(r"Local\MCPace.Agent.{hash:016x}");
        let wide = name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 0, wide.as_ptr()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Ok(None);
        }
        Ok(Some(Self { handle }))
    }
}

#[cfg(windows)]
impl Drop for SupervisorMutex {
    fn drop(&mut self) {
        #[link(name = "kernel32")]
        extern "system" {
            fn CloseHandle(object: *mut std::ffi::c_void) -> i32;
        }
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
struct SupervisorRegistration {
    path: PathBuf,
    pid: u32,
}

#[cfg(windows)]
impl SupervisorRegistration {
    fn create(path: PathBuf) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let pid = std::process::id();
        fs::write(&path, format!("{}\n", pid))?;
        Ok(Self { path, pid })
    }
}

#[cfg(windows)]
impl Drop for SupervisorRegistration {
    fn drop(&mut self) {
        let owns_registration = fs::read_to_string(&self.path)
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
            == Some(self.pid);
        if owns_registration {
            let _ = fs::remove_file(&self.path);
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
fn agent_supervisor_stop_path(root: &Path) -> PathBuf {
    agent_log_dir_for_root(root).join("stop-requested")
}

#[cfg(windows)]
fn agent_supervisor_pid_path(root: &Path) -> PathBuf {
    agent_log_dir_for_root(root).join(SUPERVISOR_PID_FILE)
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn restart_backoff_is_bounded_and_monotonic() {
        assert_eq!(restart_delay(0), Duration::from_secs(1));
        assert_eq!(restart_delay(1), Duration::from_secs(1));
        assert_eq!(restart_delay(2), Duration::from_secs(2));
        assert_eq!(restart_delay(6), Duration::from_secs(30));
        assert_eq!(restart_delay(7), Duration::from_secs(60));
        assert_eq!(restart_delay(100), Duration::from_secs(60));
    }

    #[test]
    fn stop_request_interrupts_backoff_before_another_spawn() {
        let root =
            std::env::temp_dir().join(format!("mcpace-launcher-stop-test-{}", std::process::id()));
        let marker = agent_supervisor_stop_path(&root);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, "stop\n").unwrap();

        assert!(stop_requested(Some(&marker)));
        assert!(!wait_for_restart(Duration::from_secs(1), Some(&marker)));
        acknowledge_stop_request(Some(&marker));
        assert!(!marker.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn supervisor_mutex_allows_only_one_owner_per_root() {
        let root =
            std::env::temp_dir().join(format!("mcpace-launcher-mutex-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let first = SupervisorMutex::acquire(&root).unwrap().unwrap();
        assert!(SupervisorMutex::acquire(&root).unwrap().is_none());
        drop(first);
        assert!(SupervisorMutex::acquire(&root).unwrap().is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn supervisor_registration_removes_only_its_own_pid_file() {
        let root = std::env::temp_dir().join(format!(
            "mcpace-launcher-registration-test-{}",
            std::process::id()
        ));
        let path = agent_supervisor_pid_path(&root);
        let registration = SupervisorRegistration::create(path.clone()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().trim(),
            std::process::id().to_string()
        );
        std::fs::write(&path, "1\n").unwrap();
        drop(registration);
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(root);
    }
}
