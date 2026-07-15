use super::TEST_ENV_LOCK as ENV_LOCK;
use super::*;

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
    assert_eq!(global_active_request_limit_for_http_connections(32), 64);
    assert_eq!(default_worker_limit(0), 0);
    assert_eq!(default_worker_limit(1), 1);
    assert!(default_worker_limit(10_000) <= 8);
    assert!(default_upstream_session_pool_limit() >= 2);
    assert!(default_upstream_session_pool_limit() <= 8);
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
    env::set_var(ENV_HTTP_IO_TIMEOUT_MS, "42");
    env::set_var(ENV_HTTP_MAX_BODY_BYTES, "512");
    env::set_var(ENV_DASHBOARD_OVERVIEW_CACHE_MS, "0");
    env::set_var(ENV_DASHBOARD_HEALTH_CACHE_MS, "7");
    env::set_var(ENV_HTTP_RATE_LIMIT_WINDOW_MS, "1234");
    env::set_var(ENV_HTTP_RATE_LIMIT_MAX_REQUESTS, "321");
    env::set_var(ENV_GLOBAL_ACTIVE_REQUEST_LIMIT, "17");

    assert_eq!(default_http_connection_limit(), HTTP_CONNECTION_LIMIT_MAX);
    assert_eq!(default_worker_limit(99), 3);
    assert_eq!(default_upstream_session_pool_limit(), 9);
    assert_eq!(default_http_io_timeout_ms(), 42);
    assert_eq!(default_max_http_body_bytes(), 512);
    assert_eq!(default_dashboard_overview_cache_ms(), 0);
    assert_eq!(default_dashboard_health_cache_ms(), 7);
    assert_eq!(default_http_rate_limit_window_ms(), 1234);
    assert_eq!(default_http_rate_limit_max_requests(), 321);
    assert_eq!(global_active_request_limit_for_http_connections(64), 17);

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
