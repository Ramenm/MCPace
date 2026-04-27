use std::env;
use std::path::{Path, PathBuf};

pub const DEFAULT_LOCAL_HOST: &str = "127.0.0.1";
pub const DEFAULT_LOCAL_MCP_PORT: u16 = 39022;

pub fn default_local_mcp_url() -> String {
    format!(
        "http://{}:{}/mcp",
        DEFAULT_LOCAL_HOST, DEFAULT_LOCAL_MCP_PORT
    )
}

pub fn default_local_health_url() -> String {
    format!(
        "http://{}:{}/healthz",
        DEFAULT_LOCAL_HOST, DEFAULT_LOCAL_MCP_PORT
    )
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
