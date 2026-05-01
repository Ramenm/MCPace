//! Shared resource and parallelism helpers.
//!
//! MCPace deliberately uses the Rust standard library for local bootstrap paths.
//! These helpers keep that small dependency surface while still making thread
//! counts and HTTP request limits explicit and cgroup-aware through
//! `std::thread::available_parallelism`.

use std::time::Duration;

pub const DEFAULT_HTTP_IO_TIMEOUT_MS: u64 = 30_000;
pub const DEFAULT_MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
pub const DEFAULT_DASHBOARD_OVERVIEW_CACHE_MS: u64 = 1_500;
pub const DEFAULT_DASHBOARD_HEALTH_CACHE_MS: u64 = 1_000;
pub const MAX_HTTP_REQUEST_LINE_BYTES: usize = 8 * 1024;
pub const MAX_HTTP_HEADER_LINE_BYTES: usize = 8 * 1024;
pub const MAX_HTTP_HEADER_BYTES: usize = 64 * 1024;
pub const MAX_HTTP_HEADER_COUNT: usize = 128;

pub fn available_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .max(1)
}

pub fn default_http_connection_limit() -> usize {
    available_parallelism().saturating_mul(4).clamp(4, 64)
}

pub fn default_worker_limit(task_count: usize) -> usize {
    if task_count == 0 {
        return 0;
    }
    let workers = available_parallelism().saturating_mul(2).clamp(1, 16);
    task_count.min(workers).max(1)
}

pub fn default_upstream_session_pool_limit() -> usize {
    available_parallelism().saturating_mul(2).clamp(4, 16)
}

pub fn default_upstream_session_pool_shard_count() -> usize {
    available_parallelism()
        .min(default_upstream_session_pool_limit())
        .clamp(1, 8)
}

pub fn default_http_io_timeout() -> Duration {
    Duration::from_millis(DEFAULT_HTTP_IO_TIMEOUT_MS)
}

pub fn default_dashboard_overview_cache_ttl() -> Duration {
    Duration::from_millis(DEFAULT_DASHBOARD_OVERVIEW_CACHE_MS)
}

pub fn default_dashboard_health_cache_ttl() -> Duration {
    Duration::from_millis(DEFAULT_DASHBOARD_HEALTH_CACHE_MS)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_positive_and_bounded() {
        assert!(available_parallelism() >= 1);
        assert!(default_http_connection_limit() >= 4);
        assert_eq!(default_worker_limit(0), 0);
        assert_eq!(default_worker_limit(1), 1);
        assert!(default_worker_limit(10_000) <= 16);
        assert!(default_upstream_session_pool_limit() >= 4);
        assert!(default_upstream_session_pool_shard_count() >= 1);
        assert!(default_upstream_session_pool_shard_count() <= 8);
        assert!(default_http_io_timeout().as_millis() > 0);
        assert!(default_dashboard_overview_cache_ttl().as_millis() > 0);
        assert!(default_dashboard_health_cache_ttl().as_millis() > 0);
    }

    #[test]
    fn nonnegative_parser_allows_zero_for_cache_knobs() {
        assert_eq!(parse_nonnegative_u64("0", "cache").unwrap(), 0);
        assert_eq!(parse_nonnegative_u64("42", "cache").unwrap(), 42);
        assert!(parse_nonnegative_u64("nope", "cache").is_err());
    }
}
