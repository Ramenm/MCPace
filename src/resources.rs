//! Shared resource and parallelism helpers.
//!
//! MCPace deliberately uses the Rust standard library for local bootstrap paths.
//! These helpers keep that small dependency surface while still making thread
//! counts and HTTP request limits explicit and cgroup-aware through
//! `std::thread::available_parallelism`.

use crate::json::JsonValue;
use std::env;
use std::time::Duration;

pub const DEFAULT_HTTP_IO_TIMEOUT_MS: u64 = 30_000;
pub const DEFAULT_MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
pub const DEFAULT_DASHBOARD_OVERVIEW_CACHE_MS: u64 = 1_500;
pub const DEFAULT_DASHBOARD_HEALTH_CACHE_MS: u64 = 1_000;
pub const MAX_HTTP_REQUEST_LINE_BYTES: usize = 8 * 1024;
pub const MAX_HTTP_HEADER_LINE_BYTES: usize = 8 * 1024;
pub const MAX_HTTP_HEADER_BYTES: usize = 64 * 1024;
pub const MAX_HTTP_HEADER_COUNT: usize = 128;

pub const ENV_HTTP_MAX_CONNECTIONS: &str = "MCPACE_HTTP_MAX_CONNECTIONS";
pub const ENV_HTTP_IO_TIMEOUT_MS: &str = "MCPACE_HTTP_IO_TIMEOUT_MS";
pub const ENV_HTTP_MAX_BODY_BYTES: &str = "MCPACE_HTTP_MAX_BODY_BYTES";
pub const ENV_DASHBOARD_OVERVIEW_CACHE_MS: &str = "MCPACE_DASHBOARD_OVERVIEW_CACHE_MS";
pub const ENV_DASHBOARD_HEALTH_CACHE_MS: &str = "MCPACE_DASHBOARD_HEALTH_CACHE_MS";
pub const ENV_UPSTREAM_WORKERS: &str = "MCPACE_UPSTREAM_WORKERS";
pub const ENV_UPSTREAM_SESSION_POOL_LIMIT: &str = "MCPACE_UPSTREAM_SESSION_POOL_LIMIT";
pub const ENV_UPSTREAM_SESSION_POOL_SHARDS: &str = "MCPACE_UPSTREAM_SESSION_POOL_SHARDS";

const AUTO_HTTP_CONNECTION_MIN: usize = 4;
const AUTO_HTTP_CONNECTION_MAX: usize = 8;
const OVERRIDE_HTTP_CONNECTION_MAX: usize = 256;
const AUTO_UPSTREAM_WORKER_MAX: usize = 8;
const OVERRIDE_UPSTREAM_WORKER_MAX: usize = 64;
const AUTO_UPSTREAM_SESSION_POOL_MIN: usize = 2;
const AUTO_UPSTREAM_SESSION_POOL_MAX: usize = 8;
const OVERRIDE_UPSTREAM_SESSION_POOL_MAX: usize = 128;
const AUTO_UPSTREAM_SESSION_SHARD_MAX: usize = 4;
const OVERRIDE_UPSTREAM_SESSION_SHARD_MAX: usize = 32;

#[allow(dead_code)]
pub fn runtime_resource_env_keys() -> &'static [&'static str] {
    &[
        ENV_HTTP_MAX_CONNECTIONS,
        ENV_HTTP_IO_TIMEOUT_MS,
        ENV_HTTP_MAX_BODY_BYTES,
        ENV_DASHBOARD_OVERVIEW_CACHE_MS,
        ENV_DASHBOARD_HEALTH_CACHE_MS,
        ENV_UPSTREAM_WORKERS,
        ENV_UPSTREAM_SESSION_POOL_LIMIT,
        ENV_UPSTREAM_SESSION_POOL_SHARDS,
    ]
}

pub fn process_resource_snapshot_json(pid: u32) -> JsonValue {
    #[cfg(target_os = "linux")]
    {
        process_resource_snapshot_linux(pid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        JsonValue::object([
            ("pid", JsonValue::number(pid)),
            ("available", JsonValue::bool(false)),
            ("source", JsonValue::string("unsupported-platform")),
            ("platform", JsonValue::string(std::env::consts::OS)),
            (
                "note",
                JsonValue::string("per-process resource snapshots are currently implemented through Linux procfs; this platform needs a native backend"),
            ),
        ])
    }
}

#[cfg(target_os = "linux")]
fn process_resource_snapshot_linux(pid: u32) -> JsonValue {
    use std::fs;

    let proc_dir = std::path::PathBuf::from(format!("/proc/{pid}"));
    let status = fs::read_to_string(proc_dir.join("status")).ok();
    let stat = fs::read_to_string(proc_dir.join("stat")).ok();
    let rss_bytes = status
        .as_deref()
        .and_then(|value| status_kib(value, "VmRSS:"))
        .map(|kib| kib.saturating_mul(1024));
    let virtual_memory_bytes = status
        .as_deref()
        .and_then(|value| status_kib(value, "VmSize:"))
        .map(|kib| kib.saturating_mul(1024));
    let threads = status
        .as_deref()
        .and_then(|value| status_u64(value, "Threads:"));
    let fd_count = fs::read_dir(proc_dir.join("fd"))
        .ok()
        .map(|entries| entries.filter_map(Result::ok).count() as u64);
    let (state, cpu_ticks) = stat
        .as_deref()
        .map(stat_state_and_ticks)
        .unwrap_or((None, None));
    let available = status.is_some() || stat.is_some() || fd_count.is_some();

    JsonValue::object([
        ("pid", JsonValue::number(pid)),
        ("available", JsonValue::bool(available)),
        ("source", JsonValue::string("linux-procfs")),
        ("platform", JsonValue::string(std::env::consts::OS)),
        (
            "rssBytes",
            rss_bytes.map(JsonValue::number).unwrap_or(JsonValue::Null),
        ),
        (
            "virtualMemoryBytes",
            virtual_memory_bytes
                .map(JsonValue::number)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "threads",
            threads.map(JsonValue::number).unwrap_or(JsonValue::Null),
        ),
        (
            "fdCount",
            fd_count.map(JsonValue::number).unwrap_or(JsonValue::Null),
        ),
        (
            "state",
            state.map(JsonValue::string).unwrap_or(JsonValue::Null),
        ),
        (
            "cpuTicks",
            cpu_ticks.map(JsonValue::number).unwrap_or(JsonValue::Null),
        ),
    ])
}

#[cfg(target_os = "linux")]
fn status_kib(status: &str, key: &str) -> Option<u64> {
    status_u64(status, key)
}

#[cfg(target_os = "linux")]
fn status_u64(status: &str, key: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?;
        rest.split_whitespace().next()?.parse::<u64>().ok()
    })
}

#[cfg(target_os = "linux")]
fn stat_state_and_ticks(stat: &str) -> (Option<String>, Option<u64>) {
    let after_name = stat
        .rsplit_once(')')
        .map(|(_, rest)| rest.trim())
        .unwrap_or(stat);
    let fields = after_name.split_whitespace().collect::<Vec<_>>();
    let state = fields.first().map(|value| (*value).to_string());
    let utime = fields.get(11).and_then(|value| value.parse::<u64>().ok());
    let stime = fields.get(12).and_then(|value| value.parse::<u64>().ok());
    let ticks = match (utime, stime) {
        (Some(user), Some(system)) => Some(user.saturating_add(system)),
        (Some(user), None) => Some(user),
        (None, Some(system)) => Some(system),
        (None, None) => None,
    };
    (state, ticks)
}

pub fn available_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .max(1)
}

pub fn default_http_connection_limit() -> usize {
    env_positive_usize(ENV_HTTP_MAX_CONNECTIONS)
        .map(|value| value.clamp(1, OVERRIDE_HTTP_CONNECTION_MAX))
        .unwrap_or_else(|| {
            available_parallelism()
                .saturating_mul(2)
                .clamp(AUTO_HTTP_CONNECTION_MIN, AUTO_HTTP_CONNECTION_MAX)
        })
}

pub fn default_worker_limit(task_count: usize) -> usize {
    if task_count == 0 {
        return 0;
    }
    let workers = env_positive_usize(ENV_UPSTREAM_WORKERS)
        .map(|value| value.clamp(1, OVERRIDE_UPSTREAM_WORKER_MAX))
        .unwrap_or_else(|| {
            available_parallelism()
                .saturating_mul(2)
                .clamp(1, AUTO_UPSTREAM_WORKER_MAX)
        });
    task_count.min(workers).max(1)
}

pub fn default_upstream_session_pool_limit() -> usize {
    env_positive_usize(ENV_UPSTREAM_SESSION_POOL_LIMIT)
        .map(|value| value.clamp(1, OVERRIDE_UPSTREAM_SESSION_POOL_MAX))
        .unwrap_or_else(|| {
            available_parallelism().saturating_mul(2).clamp(
                AUTO_UPSTREAM_SESSION_POOL_MIN,
                AUTO_UPSTREAM_SESSION_POOL_MAX,
            )
        })
}

pub fn default_upstream_session_pool_shard_count() -> usize {
    let pool_limit = default_upstream_session_pool_limit();
    env_positive_usize(ENV_UPSTREAM_SESSION_POOL_SHARDS)
        .map(|value| {
            value
                .min(pool_limit)
                .clamp(1, OVERRIDE_UPSTREAM_SESSION_SHARD_MAX)
        })
        .unwrap_or_else(|| {
            available_parallelism()
                .min(pool_limit)
                .clamp(1, AUTO_UPSTREAM_SESSION_SHARD_MAX)
        })
}

pub fn default_http_io_timeout_ms() -> u64 {
    env_positive_u64(ENV_HTTP_IO_TIMEOUT_MS).unwrap_or(DEFAULT_HTTP_IO_TIMEOUT_MS)
}

pub fn default_max_http_body_bytes() -> usize {
    env_positive_usize(ENV_HTTP_MAX_BODY_BYTES).unwrap_or(DEFAULT_MAX_HTTP_BODY_BYTES)
}

pub fn default_dashboard_overview_cache_ms() -> u64 {
    env_nonnegative_u64(ENV_DASHBOARD_OVERVIEW_CACHE_MS)
        .unwrap_or(DEFAULT_DASHBOARD_OVERVIEW_CACHE_MS)
}

pub fn default_dashboard_health_cache_ms() -> u64 {
    env_nonnegative_u64(ENV_DASHBOARD_HEALTH_CACHE_MS).unwrap_or(DEFAULT_DASHBOARD_HEALTH_CACHE_MS)
}

pub fn default_http_io_timeout() -> Duration {
    Duration::from_millis(default_http_io_timeout_ms())
}

pub fn default_dashboard_overview_cache_ttl() -> Duration {
    Duration::from_millis(default_dashboard_overview_cache_ms())
}

pub fn default_dashboard_health_cache_ttl() -> Duration {
    Duration::from_millis(default_dashboard_health_cache_ms())
}

pub fn parse_positive_usize(value: &str, label: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{} must be a positive integer", label))?;
    if parsed == 0 {
        return Err(format!("{} must be a positive integer", label));
    }
    Ok(parsed)
}

pub fn parse_positive_u64(value: &str, label: &str) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("{} must be a positive integer", label))?;
    if parsed == 0 {
        return Err(format!("{} must be a positive integer", label));
    }
    Ok(parsed)
}

pub fn parse_nonnegative_u64(value: &str, label: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("{} must be a non-negative integer", label))
}

pub fn env_usize(name: &str) -> Option<usize> {
    env::var(name).ok()?.trim().parse::<usize>().ok()
}

pub fn env_u64(name: &str) -> Option<u64> {
    env::var(name).ok()?.trim().parse::<u64>().ok()
}

pub fn env_bool(name: &str) -> Option<bool> {
    match env::var(name).ok()?.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn env_positive_usize(name: &str) -> Option<usize> {
    env::var(name)
        .ok()
        .and_then(|value| parse_positive_usize(value.trim(), name).ok())
}

fn env_positive_u64(name: &str) -> Option<u64> {
    env::var(name)
        .ok()
        .and_then(|value| parse_positive_u64(value.trim(), name).ok())
}

fn env_nonnegative_u64(name: &str) -> Option<u64> {
    env::var(name)
        .ok()
        .and_then(|value| parse_nonnegative_u64(value.trim(), name).ok())
}

pub(crate) fn append_serve_resource_args(
    args: &mut Vec<String>,
    max_connections: Option<usize>,
    io_timeout_ms: Option<u64>,
    max_body_bytes: Option<usize>,
    overview_cache_ms: Option<u64>,
) {
    if let Some(value) = max_connections {
        args.push("--max-connections".to_string());
        args.push(value.to_string());
    }
    if let Some(value) = io_timeout_ms {
        args.push("--io-timeout-ms".to_string());
        args.push(value.to_string());
    }
    if let Some(value) = max_body_bytes {
        args.push("--max-body-bytes".to_string());
        args.push(value.to_string());
    }
    if let Some(value) = overview_cache_ms {
        args.push("--overview-cache-ms".to_string());
        args.push(value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvSnapshot(Vec<(&'static str, Option<String>)>);

    impl EnvSnapshot {
        fn capture_and_clear(keys: &'static [&'static str]) -> Self {
            let saved = keys
                .iter()
                .map(|key| (*key, env::var(key).ok()))
                .collect::<Vec<_>>();
            for key in keys {
                env::remove_var(key);
            }
            Self(saved)
        }
    }

    impl Drop for EnvSnapshot {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }
    }

    #[test]
    fn defaults_are_positive_and_bounded() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvSnapshot::capture_and_clear(runtime_resource_env_keys());

        assert!(available_parallelism() >= 1);
        assert!(default_http_connection_limit() >= 4);
        assert_eq!(default_worker_limit(0), 0);
        assert_eq!(default_worker_limit(1), 1);
        assert!(default_worker_limit(10_000) <= 8);
        assert!(default_upstream_session_pool_limit() >= 2);
        assert!(default_upstream_session_pool_shard_count() >= 1);
        assert!(default_upstream_session_pool_limit() <= 8);
        assert!(default_upstream_session_pool_shard_count() <= 4);
        assert!(
            default_upstream_session_pool_shard_count() <= default_upstream_session_pool_limit()
        );
        assert!(default_http_io_timeout().as_millis() > 0);
        assert!(default_dashboard_overview_cache_ttl().as_millis() > 0);
        assert!(default_dashboard_health_cache_ttl().as_millis() > 0);
        assert!(runtime_resource_env_keys().contains(&ENV_HTTP_MAX_CONNECTIONS));
    }

    #[test]
    fn env_overrides_resource_defaults_without_exceeding_caps() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvSnapshot::capture_and_clear(runtime_resource_env_keys());

        env::set_var(ENV_HTTP_MAX_CONNECTIONS, "9999");
        env::set_var(ENV_UPSTREAM_WORKERS, "3");
        env::set_var(ENV_UPSTREAM_SESSION_POOL_LIMIT, "9");
        env::set_var(ENV_UPSTREAM_SESSION_POOL_SHARDS, "4");
        env::set_var(ENV_HTTP_IO_TIMEOUT_MS, "42");
        env::set_var(ENV_HTTP_MAX_BODY_BYTES, "512");
        env::set_var(ENV_DASHBOARD_OVERVIEW_CACHE_MS, "0");
        env::set_var(ENV_DASHBOARD_HEALTH_CACHE_MS, "7");

        assert_eq!(
            default_http_connection_limit(),
            OVERRIDE_HTTP_CONNECTION_MAX
        );
        assert_eq!(default_worker_limit(99), 3);
        assert_eq!(default_upstream_session_pool_limit(), 9);
        assert_eq!(default_upstream_session_pool_shard_count(), 4);
        assert_eq!(default_http_io_timeout_ms(), 42);
        assert_eq!(default_max_http_body_bytes(), 512);
        assert_eq!(default_dashboard_overview_cache_ms(), 0);
        assert_eq!(default_dashboard_health_cache_ms(), 7);
    }

    #[test]
    fn nonnegative_parser_allows_zero_for_cache_knobs() {
        assert_eq!(parse_nonnegative_u64("0", "cache").unwrap(), 0);
        assert_eq!(parse_nonnegative_u64("42", "cache").unwrap(), 42);
        assert!(parse_nonnegative_u64("nope", "cache").is_err());
    }
}
