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
pub const HTTP_IO_TIMEOUT_MS_MAX: u64 = 300_000;
pub const DEFAULT_MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
pub const HTTP_BODY_BYTES_MAX: usize = 16 * 1024 * 1024;
pub const DEFAULT_DASHBOARD_OVERVIEW_CACHE_MS: u64 = 1_500;
pub const DEFAULT_DASHBOARD_HEALTH_CACHE_MS: u64 = 1_000;
pub const DEFAULT_HTTP_RATE_LIMIT_WINDOW_MS: u64 = 10_000;
pub const DEFAULT_HTTP_RATE_LIMIT_MAX_REQUESTS: usize = 10_000;
pub const HTTP_RATE_LIMIT_WINDOW_MS_MAX: u64 = 300_000;
pub const HTTP_RATE_LIMIT_MAX_REQUESTS_MAX: usize = 100_000;
pub const DEFAULT_HEAVY_ACTION_CONCURRENCY: usize = 2;
pub const HEAVY_ACTION_CONCURRENCY_MAX: usize = 16;
pub const MAX_HTTP_REQUEST_LINE_BYTES: usize = 8 * 1024;
pub const MAX_HTTP_HEADER_LINE_BYTES: usize = 8 * 1024;
pub const MAX_HTTP_HEADER_BYTES: usize = 64 * 1024;
pub const MAX_HTTP_HEADER_COUNT: usize = 128;
#[allow(dead_code)]
pub const UPSTREAM_BATCH_CALLS_MAX: usize = 32;
#[allow(dead_code)]
pub const UPSTREAM_BATCH_ARGUMENT_BYTES_MAX: usize = 256 * 1024;
#[allow(dead_code)]
pub const DEFAULT_UPSTREAM_POOL_LOCK_WAIT_MS: u64 = 250;
#[allow(dead_code)]
pub const UPSTREAM_POOL_LOCK_WAIT_MS_MAX: u64 = 30_000;
pub const GLOBAL_ACTIVE_REQUEST_LIMIT_MAX: usize = 1024;
pub const PROCESS_RSS_SOFT_BYTES_MAX: u64 = 128 * 1024 * 1024 * 1024;
pub const PROCESS_FD_SOFT_LIMIT_MAX: u64 = 1_000_000;
pub const PROCESS_THREAD_SOFT_LIMIT_MAX: u64 = 65_536;

pub const ENV_HTTP_MAX_CONNECTIONS: &str = "MCPACE_HTTP_MAX_CONNECTIONS";
pub const ENV_HTTP_IO_TIMEOUT_MS: &str = "MCPACE_HTTP_IO_TIMEOUT_MS";
pub const ENV_HTTP_MAX_BODY_BYTES: &str = "MCPACE_HTTP_MAX_BODY_BYTES";
pub const ENV_DASHBOARD_OVERVIEW_CACHE_MS: &str = "MCPACE_DASHBOARD_OVERVIEW_CACHE_MS";
pub const ENV_DASHBOARD_HEALTH_CACHE_MS: &str = "MCPACE_DASHBOARD_HEALTH_CACHE_MS";
pub const ENV_HTTP_RATE_LIMIT_WINDOW_MS: &str = "MCPACE_HTTP_RATE_LIMIT_WINDOW_MS";
pub const ENV_HTTP_RATE_LIMIT_MAX_REQUESTS: &str = "MCPACE_HTTP_RATE_LIMIT_MAX_REQUESTS";
pub const ENV_HEAVY_ACTION_CONCURRENCY: &str = "MCPACE_HEAVY_ACTION_CONCURRENCY";
pub const ENV_UPSTREAM_WORKERS: &str = "MCPACE_UPSTREAM_WORKERS";
pub const ENV_UPSTREAM_SESSION_POOL_LIMIT: &str = "MCPACE_UPSTREAM_SESSION_POOL_LIMIT";
pub const ENV_UPSTREAM_SESSION_POOL_SHARDS: &str = "MCPACE_UPSTREAM_SESSION_POOL_SHARDS";
pub const ENV_UPSTREAM_BATCH_MAX_CALLS: &str = "MCPACE_UPSTREAM_BATCH_MAX_CALLS";
pub const ENV_UPSTREAM_BATCH_MAX_ARGUMENT_BYTES: &str = "MCPACE_UPSTREAM_BATCH_MAX_ARGUMENT_BYTES";
pub const ENV_UPSTREAM_POOL_LOCK_WAIT_MS: &str = "MCPACE_UPSTREAM_POOL_LOCK_WAIT_MS";
pub const ENV_GLOBAL_ACTIVE_REQUEST_LIMIT: &str = "MCPACE_GLOBAL_ACTIVE_REQUEST_LIMIT";
pub const ENV_PROCESS_RSS_SOFT_BYTES: &str = "MCPACE_PROCESS_RSS_SOFT_BYTES";
pub const ENV_PROCESS_FD_SOFT_LIMIT: &str = "MCPACE_PROCESS_FD_SOFT_LIMIT";
pub const ENV_PROCESS_THREAD_SOFT_LIMIT: &str = "MCPACE_PROCESS_THREAD_SOFT_LIMIT";

const AUTO_HTTP_CONNECTION_MIN: usize = 4;
const AUTO_HTTP_CONNECTION_MAX: usize = 8;
pub const HTTP_CONNECTION_LIMIT_MAX: usize = 256;
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
        ENV_HTTP_RATE_LIMIT_WINDOW_MS,
        ENV_HTTP_RATE_LIMIT_MAX_REQUESTS,
        ENV_HEAVY_ACTION_CONCURRENCY,
        ENV_UPSTREAM_WORKERS,
        ENV_UPSTREAM_SESSION_POOL_LIMIT,
        ENV_UPSTREAM_SESSION_POOL_SHARDS,
        ENV_UPSTREAM_BATCH_MAX_CALLS,
        ENV_UPSTREAM_BATCH_MAX_ARGUMENT_BYTES,
        ENV_UPSTREAM_POOL_LOCK_WAIT_MS,
        ENV_GLOBAL_ACTIVE_REQUEST_LIMIT,
        ENV_PROCESS_RSS_SOFT_BYTES,
        ENV_PROCESS_FD_SOFT_LIMIT,
        ENV_PROCESS_THREAD_SOFT_LIMIT,
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
        .map(|value| value.clamp(1, HTTP_CONNECTION_LIMIT_MAX))
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
    env_positive_u64(ENV_HTTP_IO_TIMEOUT_MS)
        .map(|value| value.clamp(1, HTTP_IO_TIMEOUT_MS_MAX))
        .unwrap_or(DEFAULT_HTTP_IO_TIMEOUT_MS)
}

pub fn default_max_http_body_bytes() -> usize {
    env_positive_usize(ENV_HTTP_MAX_BODY_BYTES)
        .map(|value| value.clamp(1, HTTP_BODY_BYTES_MAX))
        .unwrap_or(DEFAULT_MAX_HTTP_BODY_BYTES)
}

pub fn default_dashboard_overview_cache_ms() -> u64 {
    env_nonnegative_u64(ENV_DASHBOARD_OVERVIEW_CACHE_MS)
        .unwrap_or(DEFAULT_DASHBOARD_OVERVIEW_CACHE_MS)
}

pub fn default_dashboard_health_cache_ms() -> u64 {
    env_nonnegative_u64(ENV_DASHBOARD_HEALTH_CACHE_MS).unwrap_or(DEFAULT_DASHBOARD_HEALTH_CACHE_MS)
}

pub fn default_http_rate_limit_window_ms() -> u64 {
    env_positive_u64(ENV_HTTP_RATE_LIMIT_WINDOW_MS)
        .map(|value| value.clamp(1, HTTP_RATE_LIMIT_WINDOW_MS_MAX))
        .unwrap_or(DEFAULT_HTTP_RATE_LIMIT_WINDOW_MS)
}

pub fn default_http_rate_limit_max_requests() -> usize {
    env_positive_usize(ENV_HTTP_RATE_LIMIT_MAX_REQUESTS)
        .map(|value| value.clamp(1, HTTP_RATE_LIMIT_MAX_REQUESTS_MAX))
        .unwrap_or(DEFAULT_HTTP_RATE_LIMIT_MAX_REQUESTS)
}

#[allow(dead_code)]
pub fn default_upstream_batch_max_calls() -> usize {
    env_positive_usize(ENV_UPSTREAM_BATCH_MAX_CALLS)
        .map(|value| value.clamp(1, UPSTREAM_BATCH_CALLS_MAX))
        .unwrap_or(UPSTREAM_BATCH_CALLS_MAX)
}

#[allow(dead_code)]
pub fn default_upstream_batch_max_argument_bytes() -> usize {
    env_positive_usize(ENV_UPSTREAM_BATCH_MAX_ARGUMENT_BYTES)
        .map(|value| value.clamp(1, UPSTREAM_BATCH_ARGUMENT_BYTES_MAX))
        .unwrap_or(UPSTREAM_BATCH_ARGUMENT_BYTES_MAX)
}

pub fn default_heavy_action_concurrency() -> usize {
    env_positive_usize(ENV_HEAVY_ACTION_CONCURRENCY)
        .map(|value| value.clamp(1, HEAVY_ACTION_CONCURRENCY_MAX))
        .unwrap_or(DEFAULT_HEAVY_ACTION_CONCURRENCY)
}

#[allow(dead_code)]
pub fn default_upstream_pool_lock_wait_ms() -> u64 {
    env_positive_u64(ENV_UPSTREAM_POOL_LOCK_WAIT_MS)
        .map(|value| value.clamp(1, UPSTREAM_POOL_LOCK_WAIT_MS_MAX))
        .unwrap_or(DEFAULT_UPSTREAM_POOL_LOCK_WAIT_MS)
}

#[allow(dead_code)]
pub fn default_upstream_pool_lock_wait() -> Duration {
    Duration::from_millis(default_upstream_pool_lock_wait_ms())
}

pub fn default_global_active_request_limit() -> usize {
    env_positive_usize(ENV_GLOBAL_ACTIVE_REQUEST_LIMIT)
        .map(|value| value.clamp(1, GLOBAL_ACTIVE_REQUEST_LIMIT_MAX))
        .unwrap_or_else(|| {
            default_http_connection_limit()
                .saturating_mul(2)
                .clamp(AUTO_HTTP_CONNECTION_MIN, GLOBAL_ACTIVE_REQUEST_LIMIT_MAX)
        })
}

pub fn default_process_rss_soft_bytes() -> Option<u64> {
    env_positive_u64(ENV_PROCESS_RSS_SOFT_BYTES)
        .map(|value| value.clamp(1, PROCESS_RSS_SOFT_BYTES_MAX))
}

pub fn default_process_fd_soft_limit() -> Option<u64> {
    env_positive_u64(ENV_PROCESS_FD_SOFT_LIMIT)
        .map(|value| value.clamp(1, PROCESS_FD_SOFT_LIMIT_MAX))
}

pub fn default_process_thread_soft_limit() -> Option<u64> {
    env_positive_u64(ENV_PROCESS_THREAD_SOFT_LIMIT)
        .map(|value| value.clamp(1, PROCESS_THREAD_SOFT_LIMIT_MAX))
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

pub fn parse_http_connection_limit(value: &str, label: &str) -> Result<usize, String> {
    let parsed = parse_positive_usize(value, label)?;
    if parsed > HTTP_CONNECTION_LIMIT_MAX {
        return Err(format!(
            "{} must be <= {} (got {})",
            label, HTTP_CONNECTION_LIMIT_MAX, parsed
        ));
    }
    Ok(parsed)
}

pub fn parse_http_body_limit(value: &str, label: &str) -> Result<usize, String> {
    let parsed = parse_positive_usize(value, label)?;
    if parsed > HTTP_BODY_BYTES_MAX {
        return Err(format!(
            "{} must be <= {} bytes (got {})",
            label, HTTP_BODY_BYTES_MAX, parsed
        ));
    }
    Ok(parsed)
}

#[allow(dead_code)]
pub fn parse_http_rate_limit_window_ms(value: &str, label: &str) -> Result<u64, String> {
    let parsed = parse_positive_u64(value, label)?;
    if parsed > HTTP_RATE_LIMIT_WINDOW_MS_MAX {
        return Err(format!(
            "{} must be <= {} ms (got {})",
            label, HTTP_RATE_LIMIT_WINDOW_MS_MAX, parsed
        ));
    }
    Ok(parsed)
}

#[allow(dead_code)]
pub fn parse_http_rate_limit_max_requests(value: &str, label: &str) -> Result<usize, String> {
    let parsed = parse_positive_usize(value, label)?;
    if parsed > HTTP_RATE_LIMIT_MAX_REQUESTS_MAX {
        return Err(format!(
            "{} must be <= {} (got {})",
            label, HTTP_RATE_LIMIT_MAX_REQUESTS_MAX, parsed
        ));
    }
    Ok(parsed)
}

#[allow(dead_code)]
pub fn parse_upstream_batch_max_calls(value: &str, label: &str) -> Result<usize, String> {
    let parsed = parse_positive_usize(value, label)?;
    if parsed > UPSTREAM_BATCH_CALLS_MAX {
        return Err(format!(
            "{} must be <= {} (got {})",
            label, UPSTREAM_BATCH_CALLS_MAX, parsed
        ));
    }
    Ok(parsed)
}

#[allow(dead_code)]
pub fn parse_upstream_batch_max_argument_bytes(value: &str, label: &str) -> Result<usize, String> {
    let parsed = parse_positive_usize(value, label)?;
    if parsed > UPSTREAM_BATCH_ARGUMENT_BYTES_MAX {
        return Err(format!(
            "{} must be <= {} bytes (got {})",
            label, UPSTREAM_BATCH_ARGUMENT_BYTES_MAX, parsed
        ));
    }
    Ok(parsed)
}

#[allow(dead_code)]
pub fn parse_heavy_action_concurrency(value: &str, label: &str) -> Result<usize, String> {
    let parsed = parse_positive_usize(value, label)?;
    if parsed > HEAVY_ACTION_CONCURRENCY_MAX {
        return Err(format!(
            "{} must be <= {} (got {})",
            label, HEAVY_ACTION_CONCURRENCY_MAX, parsed
        ));
    }
    Ok(parsed)
}

#[allow(dead_code)]
pub fn parse_upstream_pool_lock_wait_ms(value: &str, label: &str) -> Result<u64, String> {
    let parsed = parse_positive_u64(value, label)?;
    if parsed > UPSTREAM_POOL_LOCK_WAIT_MS_MAX {
        return Err(format!(
            "{} must be <= {} ms (got {})",
            label, UPSTREAM_POOL_LOCK_WAIT_MS_MAX, parsed
        ));
    }
    Ok(parsed)
}

pub fn parse_http_io_timeout_ms(value: &str, label: &str) -> Result<u64, String> {
    let parsed = parse_positive_u64(value, label)?;
    if parsed > HTTP_IO_TIMEOUT_MS_MAX {
        return Err(format!(
            "{} must be <= {} ms (got {})",
            label, HTTP_IO_TIMEOUT_MS_MAX, parsed
        ));
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
        assert!(runtime_resource_env_keys().contains(&ENV_HTTP_RATE_LIMIT_WINDOW_MS));
        assert!(runtime_resource_env_keys().contains(&ENV_HTTP_RATE_LIMIT_MAX_REQUESTS));
        assert!(runtime_resource_env_keys().contains(&ENV_HEAVY_ACTION_CONCURRENCY));
        assert!(default_max_http_body_bytes() <= HTTP_BODY_BYTES_MAX);
        assert!(default_http_io_timeout_ms() <= HTTP_IO_TIMEOUT_MS_MAX);
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
        env::set_var(ENV_HTTP_RATE_LIMIT_WINDOW_MS, "1234");
        env::set_var(ENV_HTTP_RATE_LIMIT_MAX_REQUESTS, "321");

        assert_eq!(default_http_connection_limit(), HTTP_CONNECTION_LIMIT_MAX);
        assert_eq!(default_worker_limit(99), 3);
        assert_eq!(default_upstream_session_pool_limit(), 9);
        assert_eq!(default_upstream_session_pool_shard_count(), 4);
        assert_eq!(default_http_io_timeout_ms(), 42);
        assert_eq!(default_max_http_body_bytes(), 512);
        assert_eq!(default_dashboard_overview_cache_ms(), 0);
        assert_eq!(default_dashboard_health_cache_ms(), 7);
        assert_eq!(default_http_rate_limit_window_ms(), 1234);
        assert_eq!(default_http_rate_limit_max_requests(), 321);

        env::set_var(
            ENV_HTTP_MAX_BODY_BYTES,
            HTTP_BODY_BYTES_MAX.saturating_add(1).to_string(),
        );
        assert_eq!(default_max_http_body_bytes(), HTTP_BODY_BYTES_MAX);
        env::set_var(
            ENV_HTTP_IO_TIMEOUT_MS,
            HTTP_IO_TIMEOUT_MS_MAX.saturating_add(1).to_string(),
        );
        assert_eq!(default_http_io_timeout_ms(), HTTP_IO_TIMEOUT_MS_MAX);
        env::set_var(
            ENV_HTTP_RATE_LIMIT_WINDOW_MS,
            HTTP_RATE_LIMIT_WINDOW_MS_MAX.saturating_add(1).to_string(),
        );
        assert_eq!(
            default_http_rate_limit_window_ms(),
            HTTP_RATE_LIMIT_WINDOW_MS_MAX
        );
        env::set_var(
            ENV_HTTP_RATE_LIMIT_MAX_REQUESTS,
            HTTP_RATE_LIMIT_MAX_REQUESTS_MAX
                .saturating_add(1)
                .to_string(),
        );
        assert_eq!(
            default_http_rate_limit_max_requests(),
            HTTP_RATE_LIMIT_MAX_REQUESTS_MAX
        );
    }

    #[test]
    fn http_connection_parser_rejects_unbounded_thread_pools() {
        assert_eq!(
            parse_http_connection_limit("1", "dashboard --max-connections").unwrap(),
            1
        );
        assert_eq!(
            parse_http_connection_limit(
                &HTTP_CONNECTION_LIMIT_MAX.to_string(),
                "dashboard --max-connections"
            )
            .unwrap(),
            HTTP_CONNECTION_LIMIT_MAX
        );
        assert!(parse_http_connection_limit("0", "dashboard --max-connections").is_err());
        let too_large = HTTP_CONNECTION_LIMIT_MAX.saturating_add(1).to_string();
        let error = parse_http_connection_limit(&too_large, "dashboard --max-connections")
            .expect_err("above-cap connection limits must fail closed");
        assert!(error.contains("must be <="));
    }

    #[test]
    fn http_body_parser_rejects_unbounded_buffers() {
        assert_eq!(
            parse_http_body_limit("1", "dashboard --max-body-bytes").unwrap(),
            1
        );
        assert_eq!(
            parse_http_body_limit(
                &HTTP_BODY_BYTES_MAX.to_string(),
                "dashboard --max-body-bytes"
            )
            .unwrap(),
            HTTP_BODY_BYTES_MAX
        );
        assert!(parse_http_body_limit("0", "dashboard --max-body-bytes").is_err());
        let too_large = HTTP_BODY_BYTES_MAX.saturating_add(1).to_string();
        let error = parse_http_body_limit(&too_large, "dashboard --max-body-bytes")
            .expect_err("above-cap body limits must fail closed");
        assert!(error.contains("must be <="));
    }

    #[test]
    fn http_io_timeout_parser_rejects_unbounded_slowloris_windows() {
        assert_eq!(
            parse_http_io_timeout_ms("1", "dashboard --io-timeout-ms").unwrap(),
            1
        );
        assert_eq!(
            parse_http_io_timeout_ms(
                &HTTP_IO_TIMEOUT_MS_MAX.to_string(),
                "dashboard --io-timeout-ms"
            )
            .unwrap(),
            HTTP_IO_TIMEOUT_MS_MAX
        );
        assert!(parse_http_io_timeout_ms("0", "dashboard --io-timeout-ms").is_err());
        let too_large = HTTP_IO_TIMEOUT_MS_MAX.saturating_add(1).to_string();
        let error = parse_http_io_timeout_ms(&too_large, "dashboard --io-timeout-ms")
            .expect_err("above-cap HTTP IO timeouts must fail closed");
        assert!(error.contains("must be <="));
    }

    #[test]
    fn nonnegative_parser_allows_zero_for_cache_knobs() {
        assert_eq!(parse_nonnegative_u64("0", "cache").unwrap(), 0);
        assert_eq!(parse_nonnegative_u64("42", "cache").unwrap(), 42);
        assert!(parse_nonnegative_u64("nope", "cache").is_err());
    }
    #[test]
    fn http_rate_limit_parsers_reject_unbounded_values() {
        assert_eq!(
            parse_http_rate_limit_window_ms("1", "MCPACE_HTTP_RATE_LIMIT_WINDOW_MS").unwrap(),
            1
        );
        assert_eq!(
            parse_http_rate_limit_max_requests("1", "MCPACE_HTTP_RATE_LIMIT_MAX_REQUESTS").unwrap(),
            1
        );
        assert!(parse_http_rate_limit_window_ms("0", "MCPACE_HTTP_RATE_LIMIT_WINDOW_MS").is_err());
        assert!(
            parse_http_rate_limit_max_requests("0", "MCPACE_HTTP_RATE_LIMIT_MAX_REQUESTS").is_err()
        );
        assert!(parse_http_rate_limit_window_ms(
            &HTTP_RATE_LIMIT_WINDOW_MS_MAX.saturating_add(1).to_string(),
            "MCPACE_HTTP_RATE_LIMIT_WINDOW_MS"
        )
        .is_err());
        assert!(parse_http_rate_limit_max_requests(
            &HTTP_RATE_LIMIT_MAX_REQUESTS_MAX
                .saturating_add(1)
                .to_string(),
            "MCPACE_HTTP_RATE_LIMIT_MAX_REQUESTS"
        )
        .is_err());
    }
}
