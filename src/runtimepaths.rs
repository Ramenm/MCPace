use crate::json::JsonValue;
use crate::json_helpers;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static ATOMIC_WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn write_text_atomic(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {}", parent.display(), error))?;
    }
    let temp_path = path.with_extension(format!(
        "tmp-{}-{}-{}",
        std::process::id(),
        unix_time_ms(),
        ATOMIC_WRITE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&temp_path, contents)
        .map_err(|error| format!("failed to write {}: {}", temp_path.display(), error))?;
    replace_file_atomic(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        format!(
            "failed to move {} to {}: {}",
            temp_path.display(),
            path.display(),
            error
        )
    })
}

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

pub fn runtime_dir(state_root: &Path) -> PathBuf {
    state_root.join("data").join("runtime")
}

pub fn ensure_runtime_dir(state_root: &Path) -> Result<PathBuf, String> {
    let path = runtime_dir(state_root);
    std::fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create {}: {}", path.display(), error))?;
    Ok(path)
}

pub fn tool_list_cache_dir(state_root: &Path) -> PathBuf {
    runtime_dir(state_root).join("tool-list-cache")
}

pub fn ensure_tool_list_cache_dir(state_root: &Path) -> Result<PathBuf, String> {
    let path = tool_list_cache_dir(state_root);
    std::fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create {}: {}", path.display(), error))?;
    Ok(path)
}

pub fn project_registry_path(state_root: &Path) -> PathBuf {
    runtime_dir(state_root).join("project-registry.json")
}

pub fn hub_dir(state_root: &Path) -> PathBuf {
    runtime_dir(state_root).join("hub")
}

pub fn ensure_hub_dir(state_root: &Path) -> Result<PathBuf, String> {
    let path = hub_dir(state_root);
    std::fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create {}: {}", path.display(), error))?;
    Ok(path)
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

pub fn ensure_serve_dir(state_root: &Path) -> Result<PathBuf, String> {
    let path = serve_dir(state_root);
    std::fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create {}: {}", path.display(), error))?;
    Ok(path)
}

pub fn serve_state_path(state_root: &Path) -> PathBuf {
    serve_dir(state_root).join("state.json")
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

pub fn ensure_runtime_bin_dir(state_root: &Path) -> Result<PathBuf, String> {
    let path = runtime_bin_dir(state_root);
    std::fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create {}: {}", path.display(), error))?;
    Ok(path)
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
mod tests {
    use super::{normalize_http_path, normalize_public_url, DEFAULT_LOCAL_MCP_PATH};

    #[test]
    fn normalize_http_path_rejects_request_line_injection_primitives() {
        for candidate in [
            "",
            "relative",
            "/mcp?debug=1",
            "/mcp#frag",
            "/mcp with-space",
            "/mcp\twith-tab",
            "/mcp\r\nInjected: bad",
        ] {
            assert_eq!(
                normalize_http_path(candidate, DEFAULT_LOCAL_MCP_PATH),
                DEFAULT_LOCAL_MCP_PATH
            );
        }
        assert_eq!(
            normalize_http_path("/custom/path", DEFAULT_LOCAL_MCP_PATH),
            "/custom/path"
        );
    }

    #[test]
    fn normalize_public_url_rejects_ambiguous_or_unsafe_authorities() {
        assert_eq!(
            normalize_public_url("https://relay.example/mcp"),
            Some("https://relay.example/mcp".to_string())
        );
        assert_eq!(
            normalize_public_url("https://[::1]:39022/mcp"),
            Some("https://[::1]:39022/mcp".to_string())
        );
        for candidate in [
            "https://relay.example/mcp with-space",
            "https://relay.example/mcp\twith-tab",
            "https://relay.example/mcp\r\nInjected: bad",
            "https://user:pass@relay.example/mcp",
            "https://relay.example:0/mcp",
            "https://relay.example:99999/mcp",
            "https://2001:db8::1/mcp",
            "https://[::1]bad/mcp",
            "https://relay.example/mcp#fragment",
            "ftp://relay.example/mcp",
        ] {
            assert_eq!(normalize_public_url(candidate), None);
        }
    }
}
