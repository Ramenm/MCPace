use crate::json::JsonValue;
use crate::json_helpers;
use std::env;
use std::fmt;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static ATOMIC_WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimePathError {
    Io {
        operation: &'static str,
        path: PathBuf,
        reason: String,
    },
    NotRealDirectory {
        path: PathBuf,
    },
    MissingFileName {
        purpose: String,
        path: PathBuf,
    },
    Locked {
        purpose: String,
        target: PathBuf,
        lock_path: PathBuf,
    },
    TemporaryFileExhausted {
        target: PathBuf,
        last_error: Option<String>,
    },
    AtomicReplaceFailed {
        temp_path: PathBuf,
        target: PathBuf,
        reason: String,
    },
}

pub type RuntimePathResult<T> = Result<T, RuntimePathError>;

impl RuntimePathError {
    fn io(operation: &'static str, path: &Path, error: impl fmt::Display) -> Self {
        Self::Io {
            operation,
            path: path.to_path_buf(),
            reason: error.to_string(),
        }
    }
}

impl fmt::Display for RuntimePathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, path, reason } => {
                write!(formatter, "failed to {} {}: {}", operation, path.display(), reason)
            }
            Self::NotRealDirectory { path } => {
                write!(formatter, "runtime path is not a real directory: {}", path.display())
            }
            Self::MissingFileName { purpose, path } => {
                write!(formatter, "{} lock target '{}' has no file name", purpose, path.display())
            }
            Self::Locked { purpose, target, lock_path } => write!(
                formatter,
                "{} target '{}' is locked by another MCPace process; retry after that operation finishes or remove stale lock '{}' only after verifying no MCPace process is active",
                purpose,
                target.display(),
                lock_path.display()
            ),
            Self::TemporaryFileExhausted { target, last_error } => write!(
                formatter,
                "failed to create a unique temporary file next to {}{}",
                target.display(),
                last_error
                    .as_ref()
                    .map(|error| format!(": {}", error))
                    .unwrap_or_default()
            ),
            Self::AtomicReplaceFailed { temp_path, target, reason } => write!(
                formatter,
                "failed to move {} to {}: {}",
                temp_path.display(),
                target.display(),
                reason
            ),
        }
    }
}

impl std::error::Error for RuntimePathError {}

impl From<RuntimePathError> for String {
    fn from(error: RuntimePathError) -> Self {
        error.to_string()
    }
}

pub fn write_text_atomic(path: &Path, contents: &str) -> RuntimePathResult<()> {
    write_text_atomic_with_mode(path, contents, None)
}

pub fn write_private_text_atomic(path: &Path, contents: &str) -> RuntimePathResult<()> {
    write_text_atomic_with_mode(path, contents, Some(0o600))
}

fn write_text_atomic_with_mode(
    path: &Path,
    contents: &str,
    requested_unix_mode: Option<u32>,
) -> RuntimePathResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| RuntimePathError::io("create", parent, error))?;

    #[cfg(unix)]
    let target_mode = requested_unix_mode
        .or_else(|| existing_file_mode(path))
        .unwrap_or(0o644);
    #[cfg(not(unix))]
    let _ = requested_unix_mode;

    let mut last_temp_error = None;
    for _ in 0..100 {
        let temp_path = atomic_temp_path(parent, path);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);

        let mut file = match options.open(&temp_path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                last_temp_error = Some(error);
                continue;
            }
            Err(error) => {
                return Err(RuntimePathError::io(
                    "create temporary file",
                    &temp_path,
                    error,
                ));
            }
        };

        let write_result = (|| -> RuntimePathResult<()> {
            file.write_all(contents.as_bytes())
                .map_err(|error| RuntimePathError::io("write", &temp_path, error))?;
            file.sync_all()
                .map_err(|error| RuntimePathError::io("fsync", &temp_path, error))?;
            Ok(())
        })();
        drop(file);

        if let Err(error) = write_result {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }

        match replace_file_atomic(&temp_path, path) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    fs::set_permissions(path, fs::Permissions::from_mode(target_mode))
                        .map_err(|error| RuntimePathError::io("set permissions on", path, error))?;
                    if let Ok(file) = fs::File::open(path) {
                        let _ = file.sync_all();
                    }
                }
                fsync_parent_dir_best_effort(parent);
                return Ok(());
            }
            Err(error) => {
                let _ = fs::remove_file(&temp_path);
                return Err(RuntimePathError::AtomicReplaceFailed {
                    temp_path: temp_path.to_path_buf(),
                    target: path.to_path_buf(),
                    reason: error.to_string(),
                });
            }
        }
    }

    Err(RuntimePathError::TemporaryFileExhausted {
        target: path.to_path_buf(),
        last_error: last_temp_error.map(|error| error.to_string()),
    })
}

pub struct ExclusiveFileLockGuard {
    path: PathBuf,
    _file: fs::File,
}

impl Drop for ExclusiveFileLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn acquire_exclusive_file_lock(
    target_path: &Path,
    purpose: &str,
) -> RuntimePathResult<ExclusiveFileLockGuard> {
    let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| RuntimePathError::io("create lock directory", parent, error))?;
    let parent_metadata = fs::symlink_metadata(parent)
        .map_err(|error| RuntimePathError::io("inspect lock directory", parent, error))?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err(RuntimePathError::NotRealDirectory {
            path: parent.to_path_buf(),
        });
    }

    let file_name = target_path
        .file_name()
        .ok_or_else(|| RuntimePathError::MissingFileName {
            purpose: purpose.to_string(),
            path: target_path.to_path_buf(),
        })?
        .to_string_lossy();
    let lock_path = parent.join(format!(".{}.mcpace.lock", file_name));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = match options.open(&lock_path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(RuntimePathError::Locked {
                purpose: purpose.to_string(),
                target: target_path.to_path_buf(),
                lock_path: lock_path.clone(),
            });
        }
        Err(error) => {
            return Err(RuntimePathError::io("acquire lock", &lock_path, error));
        }
    };

    let now_ms = unix_time_ms();
    let _ = writeln!(
        file,
        "pid={} timeMs={} purpose={} target={}",
        std::process::id(),
        now_ms,
        purpose,
        target_path.display()
    );
    let _ = file.sync_all();

    Ok(ExclusiveFileLockGuard {
        path: lock_path,
        _file: file,
    })
}

pub fn acquire_exclusive_file_locks(
    target_paths: &[PathBuf],
    purpose: &str,
) -> RuntimePathResult<Vec<ExclusiveFileLockGuard>> {
    let mut paths = target_paths.to_vec();
    paths.sort();
    paths.dedup();
    let mut locks = Vec::new();
    for path in paths {
        locks.push(acquire_exclusive_file_lock(&path, purpose)?);
    }
    Ok(locks)
}

fn atomic_temp_path(parent: &Path, path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "mcpace-runtime-file".to_string());
    parent.join(format!(
        ".{}.tmp-{}-{}-{}",
        file_name,
        std::process::id(),
        unix_time_ms(),
        ATOMIC_WRITE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

#[cfg(unix)]
fn existing_file_mode(path: &Path) -> Option<u32> {
    fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions().mode() & 0o777)
        .filter(|mode| *mode != 0)
}

#[cfg(unix)]
fn fsync_parent_dir_best_effort(parent: &Path) {
    if let Ok(file) = fs::File::open(parent) {
        let _ = file.sync_all();
    }
}

#[cfg(not(unix))]
fn fsync_parent_dir_best_effort(_parent: &Path) {}

fn replace_file_atomic(temp_path: &Path, path: &Path) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        replace_file_atomic_windows(temp_path, path)
    }
    #[cfg(not(windows))]
    {
        fs::rename(temp_path, path)
    }
}

#[cfg(windows)]
fn replace_file_atomic_windows(temp_path: &Path, path: &Path) -> Result<(), std::io::Error> {
    let mut last_error = None;
    for attempt in 0..=20 {
        match fs::rename(temp_path, path)
            .or_else(|_| replace_existing_file_windows(temp_path, path))
        {
            Ok(()) => return Ok(()),
            Err(error)
                if attempt < 20
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Other
                    ) =>
            {
                last_error = Some(error);
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| std::io::Error::other("atomic replace failed")))
}

#[cfg(windows)]
fn replace_existing_file_windows(temp_path: &Path, path: &Path) -> Result<(), std::io::Error> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    type Bool = i32;
    type Dword = u32;
    type Lpcwstr = *const u16;

    const MOVEFILE_REPLACE_EXISTING: Dword = 0x1;
    const MOVEFILE_WRITE_THROUGH: Dword = 0x8;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing_file_name: Lpcwstr, new_file_name: Lpcwstr, flags: Dword) -> Bool;
    }

    fn wide_null(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    let from = wide_null(temp_path.as_os_str());
    let to = wide_null(path.as_os_str());
    let moved = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(crate) fn unix_time_ms() -> u128 {
    unix_time_ms_checked().unwrap_or(0)
}

pub(crate) fn unix_time_ms_checked() -> Option<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

pub(crate) fn strip_windows_extended_path_prefix(value: &str) -> String {
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{}", rest);
    }
    value.strip_prefix(r"\\?\").unwrap_or(value).to_string()
}

pub fn canonicalize_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn user_home_dir() -> Option<PathBuf> {
    platform_user_home_dir().map(PathBuf::from)
}

pub(crate) fn resolve_user_config_path_expression(expression: &str) -> Option<PathBuf> {
    let normalized = expression.trim().replace('\\', "/");
    if normalized.is_empty()
        || normalized.contains('*')
        || normalized.contains('<')
        || normalized.contains('>')
    {
        return None;
    }

    let (base, relative) = if cfg!(windows) {
        if let Some(relative) = normalized.strip_prefix("~/AppData/Roaming/") {
            let app_data = env::var_os("APPDATA").map(PathBuf::from);
            let base = match app_data.filter(|path| path.is_absolute()) {
                Some(path) => path,
                None => user_home_dir()?.join("AppData").join("Roaming"),
            };
            (base, relative)
        } else if let Some(relative) = normalized.strip_prefix("~/") {
            (user_home_dir()?, relative)
        } else {
            let absolute = PathBuf::from(expression.trim());
            return absolute.is_absolute().then_some(absolute);
        }
    } else if let Some(relative) = normalized.strip_prefix("~/.config/") {
        let xdg = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute());
        let base = match xdg {
            Some(path) => path,
            None => user_home_dir()?.join(".config"),
        };
        (base, relative)
    } else if let Some(relative) = normalized.strip_prefix("~/") {
        (user_home_dir()?, relative)
    } else {
        let absolute = PathBuf::from(expression.trim());
        return absolute.is_absolute().then_some(absolute);
    };

    join_safe_relative_path(base, relative)
}

fn join_safe_relative_path(mut base: PathBuf, relative: &str) -> Option<PathBuf> {
    for segment in relative.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return None;
        }
        base.push(segment);
    }
    Some(base)
}

#[cfg(windows)]
fn platform_user_home_dir() -> Option<std::ffi::OsString> {
    env::var_os("USERPROFILE").or_else(|| {
        match (env::var_os("HOMEDRIVE"), env::var_os("HOMEPATH")) {
            (Some(drive), Some(path)) => {
                let mut value = drive;
                value.push(path);
                Some(value)
            }
            _ => env::var_os("HOME"),
        }
    })
}

#[cfg(not(windows))]
fn platform_user_home_dir() -> Option<std::ffi::OsString> {
    env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))
}

pub const DEFAULT_LOCAL_HOST: &str = "127.0.0.1";
pub const DEFAULT_LOCAL_MCP_PORT: u16 = 39022;
pub const DEFAULT_LOCAL_MCP_PATH: &str = "/mcp";
pub const DEFAULT_LOCAL_HEALTH_PATH: &str = "/healthz";
pub const PUBLIC_MCP_RELAY_PLACEHOLDER_URL: &str = "https://YOUR-MCPACE-RELAY/mcp";

const ENV_PUBLIC_MCP_URL: &str = "MCPACE_PUBLIC_MCP_URL";
const ENV_SERVE_HOST: &str = "MCPACE_SERVE_HOST";
const ENV_SERVE_PORT: &str = "MCPACE_SERVE_PORT";
const ENV_SERVE_MCP_PATH: &str = "MCPACE_SERVE_PATH";
const ENV_SERVE_HEALTH_PATH: &str = "MCPACE_HEALTH_PATH";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServeEndpoint {
    pub host: String,
    pub port: u16,
    pub mcp_path: String,
    pub health_path: String,
    pub public_mcp_url: Option<String>,
}

impl ServeEndpoint {
    pub fn mcp_url(&self) -> String {
        self.public_mcp_url
            .clone()
            .unwrap_or_else(|| http_url(&self.host, self.port, &self.mcp_path))
    }

    pub fn bind_mcp_url(&self) -> String {
        http_url(&self.host, self.port, &self.mcp_path)
    }

    pub fn health_url(&self) -> String {
        http_url(&self.host, self.port, &self.health_path)
    }
}

impl Default for ServeEndpoint {
    fn default() -> Self {
        Self {
            host: DEFAULT_LOCAL_HOST.to_string(),
            port: DEFAULT_LOCAL_MCP_PORT,
            mcp_path: DEFAULT_LOCAL_MCP_PATH.to_string(),
            health_path: DEFAULT_LOCAL_HEALTH_PATH.to_string(),
            public_mcp_url: None,
        }
    }
}

pub fn default_local_mcp_url() -> String {
    ServeEndpoint::default().mcp_url()
}

pub fn default_local_health_url() -> String {
    ServeEndpoint::default().health_url()
}

pub fn public_mcp_url_or_placeholder(root_path: Option<&Path>) -> String {
    resolve_serve_endpoint(root_path)
        .public_mcp_url
        .unwrap_or_else(|| PUBLIC_MCP_RELAY_PLACEHOLDER_URL.to_string())
}

pub fn configured_mcp_url(root_path: &Path) -> String {
    resolve_serve_endpoint(Some(root_path)).mcp_url()
}

pub fn configured_bind_mcp_url(root_path: &Path) -> String {
    resolve_serve_endpoint(Some(root_path)).bind_mcp_url()
}

pub fn configured_health_url(root_path: &Path) -> String {
    resolve_serve_endpoint(Some(root_path)).health_url()
}

pub fn http_url(host: &str, port: u16, path: &str) -> String {
    let host = normalize_url_host(host);
    let path = normalize_http_path(path, DEFAULT_LOCAL_MCP_PATH);
    format!("http://{}:{}{}", host, port, path)
}

pub fn normalize_http_path(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || !trimmed.starts_with('/')
        || trimmed.contains('?')
        || trimmed.contains('#')
        || trimmed
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn resolve_serve_endpoint(root_path: Option<&Path>) -> ServeEndpoint {
    let mut endpoint = ServeEndpoint::default();

    if let Some(root_path) = root_path {
        if let Ok(config) = json_helpers::read_json_file(&root_path.join("mcpace.config.json")) {
            if let Some(value) = string_at(&config, &["serve", "host"]) {
                endpoint.host = value;
            }
            if let Some(value) =
                u16_at(&config, &["serve", "port"]).or_else(|| u16_at(&config, &["ports", "serve"]))
            {
                endpoint.port = value;
            }
            if let Some(value) = string_at(&config, &["serve", "mcpPath"]) {
                endpoint.mcp_path = normalize_http_path(&value, DEFAULT_LOCAL_MCP_PATH);
            }
            if let Some(value) = string_at(&config, &["serve", "path"]) {
                endpoint.mcp_path = normalize_http_path(&value, DEFAULT_LOCAL_MCP_PATH);
            }
            if let Some(value) = string_at(&config, &["serve", "healthPath"]) {
                endpoint.health_path = normalize_http_path(&value, DEFAULT_LOCAL_HEALTH_PATH);
            }
            if let Some(value) = string_at(&config, &["serve", "publicUrl"]) {
                endpoint.public_mcp_url = normalize_public_url(&value);
            }
        }
    }

    if let Ok(value) = env::var(ENV_SERVE_HOST) {
        let value = value.trim();
        if !value.is_empty() {
            endpoint.host = value.to_string();
        }
    }
    if let Ok(value) = env::var(ENV_SERVE_PORT) {
        if let Ok(port) = value.trim().parse::<u16>() {
            endpoint.port = port;
        }
    }
    if let Ok(value) = env::var(ENV_SERVE_MCP_PATH) {
        endpoint.mcp_path = normalize_http_path(&value, DEFAULT_LOCAL_MCP_PATH);
    }
    if let Ok(value) = env::var(ENV_SERVE_HEALTH_PATH) {
        endpoint.health_path = normalize_http_path(&value, DEFAULT_LOCAL_HEALTH_PATH);
    }
    if let Ok(value) = env::var(ENV_PUBLIC_MCP_URL) {
        endpoint.public_mcp_url = normalize_public_url(&value);
    }

    endpoint
}

pub fn resolve_state_root(root_path: &Path) -> PathBuf {
    let env_override = env::var_os("MCPACE_STATE_ROOT").map(PathBuf::from);
    absolutize_or_root(root_path, env_override)
}

pub fn absolutize_or_root(root_path: &Path, candidate: Option<PathBuf>) -> PathBuf {
    match candidate {
        Some(path) if !path.as_os_str().is_empty() => absolutize(root_path, path),
        _ => root_path.to_path_buf(),
    }
}

fn ensure_private_dir(path: &Path) -> RuntimePathResult<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| RuntimePathError::io("create", parent, error))?;

    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(RuntimePathError::NotRealDirectory {
                    path: path.to_path_buf(),
                });
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_private_dir(path)?;
            let metadata = fs::symlink_metadata(path)
                .map_err(|error| RuntimePathError::io("inspect", path, error))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(RuntimePathError::NotRealDirectory {
                    path: path.to_path_buf(),
                });
            }
        }
        Err(error) => {
            return Err(RuntimePathError::io("inspect", path, error));
        }
    }

    restrict_private_dir_permissions(path)?;
    Ok(path.to_path_buf())
}

fn create_private_dir(path: &Path) -> RuntimePathResult<()> {
    #[cfg(unix)]
    {
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        if let Err(error) = builder.create(path) {
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(RuntimePathError::io("create", path, error));
            }
        }
    }
    #[cfg(not(unix))]
    {
        if let Err(error) = fs::create_dir(path) {
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(RuntimePathError::io("create", path, error));
            }
        }
    }
    Ok(())
}

fn restrict_private_dir_permissions(path: &Path) -> RuntimePathResult<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| RuntimePathError::io("restrict permissions on", path, error))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

pub fn runtime_dir(state_root: &Path) -> PathBuf {
    state_root.join("data").join("runtime")
}

pub fn ensure_runtime_dir(state_root: &Path) -> RuntimePathResult<PathBuf> {
    ensure_private_dir(&runtime_dir(state_root))
}

pub fn tool_list_cache_dir(state_root: &Path) -> PathBuf {
    runtime_dir(state_root).join("tool-list-cache")
}

pub fn ensure_tool_list_cache_dir(state_root: &Path) -> RuntimePathResult<PathBuf> {
    ensure_private_dir(&tool_list_cache_dir(state_root))
}

pub fn project_registry_path(state_root: &Path) -> PathBuf {
    runtime_dir(state_root).join("project-registry.json")
}

pub fn hub_dir(state_root: &Path) -> PathBuf {
    runtime_dir(state_root).join("hub")
}

pub fn ensure_hub_dir(state_root: &Path) -> RuntimePathResult<PathBuf> {
    ensure_private_dir(&hub_dir(state_root))
}

pub fn hub_state_path(state_root: &Path) -> PathBuf {
    hub_dir(state_root).join("state.json")
}

pub fn hub_health_path(state_root: &Path) -> PathBuf {
    hub_dir(state_root).join("health.json")
}

pub fn hub_log_path(state_root: &Path) -> PathBuf {
    hub_dir(state_root).join("events.log")
}

pub fn hub_stop_path(state_root: &Path) -> PathBuf {
    hub_dir(state_root).join("stop.signal")
}

pub fn hub_lock_path(state_root: &Path) -> PathBuf {
    hub_dir(state_root).join("lock.json")
}

pub fn hub_leases_path(state_root: &Path) -> PathBuf {
    hub_dir(state_root).join("leases.json")
}

pub fn hub_lease_lock_path(state_root: &Path) -> PathBuf {
    hub_dir(state_root).join("leases.lock")
}

pub fn serve_dir(state_root: &Path) -> PathBuf {
    runtime_dir(state_root).join("serve")
}

pub fn ensure_serve_dir(state_root: &Path) -> RuntimePathResult<PathBuf> {
    ensure_private_dir(&serve_dir(state_root))
}

pub fn serve_state_path(state_root: &Path) -> PathBuf {
    serve_dir(state_root).join("state.json")
}

pub fn serve_start_lock_path(state_root: &Path) -> PathBuf {
    serve_dir(state_root).join("start.lock")
}

pub fn serve_restart_guard_path(state_root: &Path) -> PathBuf {
    serve_dir(state_root).join("restart-guard.log")
}

pub fn serve_stdout_log_path(state_root: &Path) -> PathBuf {
    serve_dir(state_root).join("stdout.log")
}

pub fn serve_stderr_log_path(state_root: &Path) -> PathBuf {
    serve_dir(state_root).join("stderr.log")
}

pub fn runtime_bin_dir(state_root: &Path) -> PathBuf {
    runtime_dir(state_root).join("bin")
}

pub fn ensure_runtime_bin_dir(state_root: &Path) -> RuntimePathResult<PathBuf> {
    ensure_private_dir(&runtime_bin_dir(state_root))
}

pub fn serve_runner_path(state_root: &Path) -> PathBuf {
    runtime_bin_dir(state_root).join(if cfg!(windows) {
        "mcpace-serve.exe"
    } else {
        "mcpace-serve"
    })
}

pub fn serve_runner_path_for_start(state_root: &Path) -> PathBuf {
    let suffix = format!(
        "{}-{}-{}",
        std::process::id(),
        unix_time_ms(),
        ATOMIC_WRITE_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    runtime_bin_dir(state_root).join(if cfg!(windows) {
        format!("mcpace-serve-{}.exe", suffix)
    } else {
        format!("mcpace-serve-{}", suffix)
    })
}

fn absolutize(root_path: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root_path.join(path)
    }
}

fn string_at(value: &JsonValue, path: &[&str]) -> Option<String> {
    json_helpers::value_at_path(value, path)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn u16_at(value: &JsonValue, path: &[&str]) -> Option<u16> {
    match json_helpers::value_at_path(value, path) {
        Some(value) => value
            .as_i64()
            .and_then(|number| u16::try_from(number).ok())
            .filter(|value| *value > 0)
            .or_else(|| {
                value
                    .as_str()
                    .and_then(|text| text.trim().parse::<u16>().ok())
                    .filter(|value| *value > 0)
            }),
        None => None,
    }
}

fn normalize_public_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
        || trimmed.contains('#')
    {
        return None;
    }
    let rest = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))?;
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    if !valid_public_url_authority(authority) {
        return None;
    }
    Some(trimmed.trim_end_matches('/').to_string())
}

fn valid_public_url_authority(authority: &str) -> bool {
    if authority.is_empty()
        || authority.contains('/')
        || authority.contains('@')
        || authority
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return false;
    }
    if authority.starts_with('[') {
        let Some(end) = authority.find(']') else {
            return false;
        };
        let host = &authority[1..end];
        return valid_public_url_host(host) && valid_public_port_suffix(&authority[end + 1..]);
    }
    if authority.matches(':').count() > 1 {
        return false;
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => {
            valid_public_url_host(host) && valid_public_port(port)
        }
        Some(_) => false,
        None => valid_public_url_host(authority),
    }
}

fn valid_public_url_host(host: &str) -> bool {
    !host.trim().is_empty()
        && !host
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
}

fn valid_public_port_suffix(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    value
        .strip_prefix(':')
        .map(valid_public_port)
        .unwrap_or(false)
}

fn valid_public_port(port: &str) -> bool {
    port.parse::<u16>()
        .ok()
        .filter(|value| *value > 0)
        .is_some()
}

fn normalize_url_host(host: &str) -> String {
    let trimmed = host.trim();
    if trimmed == "0.0.0.0" || trimmed == "::" || trimmed.is_empty() {
        return DEFAULT_LOCAL_HOST.to_string();
    }
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return trimmed.to_string();
    }
    if trimmed.contains(':') {
        return format!("[{}]", trimmed);
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests;
