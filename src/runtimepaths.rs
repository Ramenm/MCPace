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
        unix_time_ms_for_temp_path(),
        ATOMIC_WRITE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&temp_path, contents)
        .map_err(|error| format!("failed to write {}: {}", temp_path.display(), error))?;
    #[cfg(windows)]
    {
        let _ = fs::remove_file(path);
    }
    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        format!(
            "failed to move {} to {}: {}",
            temp_path.display(),
            path.display(),
            error
        )
    })
}

fn unix_time_ms_for_temp_path() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
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
        || trimmed.contains("\r")
        || trimmed.contains("\n")
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
        || trimmed.contains("\r")
        || trimmed.contains("\n")
        || !(trimmed.starts_with("http://") || trimmed.starts_with("https://"))
    {
        None
    } else {
        Some(trimmed.trim_end_matches('/').to_string())
    }
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
