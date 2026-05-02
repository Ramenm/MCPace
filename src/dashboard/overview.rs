use super::response::{now_ms, sanitize_root_path};
use super::{
    run_json_command, run_json_command_vec, CachedHealth, CachedOverview, DashboardConfig,
    ServeSurface,
};
use crate::json::JsonValue;
use crate::json_helpers;
use crate::resources;
use std::collections::BTreeMap;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

pub(super) fn cached_health_json(
    config: &DashboardConfig,
    refresh: bool,
) -> Result<JsonValue, String> {
    if config.health_cache_ttl.is_zero() {
        return build_health_json(config).map(|value| {
            with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed: true,
                    stale: false,
                    ttl: config.health_cache_ttl,
                    age: None,
                    refresh_error: None,
                },
            )
        });
    }

    let mut guard = config
        .health_cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !refresh {
        if let Some(cached) = guard.as_ref() {
            let age = cached.stored_at.elapsed();
            if age <= config.health_cache_ttl {
                return Ok(with_runtime_cache_metadata(
                    cached.value.clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed: false,
                        stale: false,
                        ttl: config.health_cache_ttl,
                        age: Some(age),
                        refresh_error: None,
                    },
                ));
            }
        }
    }

    match build_health_json(config) {
        Ok(value) => {
            *guard = Some(CachedHealth {
                stored_at: Instant::now(),
                value: value.clone(),
            });
            Ok(with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed: refresh,
                    stale: false,
                    ttl: config.health_cache_ttl,
                    age: Some(Duration::from_millis(0)),
                    refresh_error: None,
                },
            ))
        }
        Err(error) => {
            if let Some(cached) = guard.as_ref() {
                return Ok(with_runtime_cache_metadata(
                    cached.value.clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed: refresh,
                        stale: true,
                        ttl: config.health_cache_ttl,
                        age: Some(cached.stored_at.elapsed()),
                        refresh_error: Some(error),
                    },
                ));
            }
            Err(error)
        }
    }
}

pub(super) fn build_health_json(config: &DashboardConfig) -> Result<JsonValue, String> {
    let readiness = run_json_command(&config.root_path, &["verify", "readiness", "--json"])?;
    let ok = json_helpers::bool_at_path(&readiness, &["readyForRuntimeOps"]).unwrap_or(false);
    Ok(JsonValue::object([
        ("ok", JsonValue::bool(ok)),
        ("generatedAtMs", JsonValue::number(now_ms())),
        ("readiness", readiness),
    ]))
}

pub(super) fn cached_overview_json(
    config: &DashboardConfig,
    refresh: bool,
) -> Result<JsonValue, String> {
    if config.overview_cache_ttl.is_zero() {
        return build_overview_json(&config.root_path).map(|value| {
            with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed: true,
                    stale: false,
                    ttl: config.overview_cache_ttl,
                    age: None,
                    refresh_error: None,
                },
            )
        });
    }

    let mut guard = config
        .overview_cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !refresh {
        if let Some(cached) = guard.as_ref() {
            let age = cached.stored_at.elapsed();
            if age <= config.overview_cache_ttl {
                return Ok(with_runtime_cache_metadata(
                    cached.value.clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed: false,
                        stale: false,
                        ttl: config.overview_cache_ttl,
                        age: Some(age),
                        refresh_error: None,
                    },
                ));
            }
        }
    }

    match build_overview_json(&config.root_path) {
        Ok(value) => {
            *guard = Some(CachedOverview {
                stored_at: Instant::now(),
                value: value.clone(),
            });
            Ok(with_runtime_cache_metadata(
                value,
                config,
                CacheMetadata {
                    hit: false,
                    bypassed: refresh,
                    stale: false,
                    ttl: config.overview_cache_ttl,
                    age: Some(Duration::from_millis(0)),
                    refresh_error: None,
                },
            ))
        }
        Err(error) => {
            if let Some(cached) = guard.as_ref() {
                return Ok(with_runtime_cache_metadata(
                    cached.value.clone(),
                    config,
                    CacheMetadata {
                        hit: true,
                        bypassed: refresh,
                        stale: true,
                        ttl: config.overview_cache_ttl,
                        age: Some(cached.stored_at.elapsed()),
                        refresh_error: Some(error),
                    },
                ));
            }
            Err(error)
        }
    }
}

struct CacheMetadata {
    hit: bool,
    bypassed: bool,
    stale: bool,
    ttl: Duration,
    age: Option<Duration>,
    refresh_error: Option<String>,
}

fn with_runtime_cache_metadata(
    mut value: JsonValue,
    config: &DashboardConfig,
    cache: CacheMetadata,
) -> JsonValue {
    if let JsonValue::Object(map) = &mut value {
        map.insert("cache".to_string(), cache_metadata_json(cache));
        map.insert("runtime".to_string(), runtime_status_json(config));
    }
    value
}

fn cache_metadata_json(cache: CacheMetadata) -> JsonValue {
    let mut entries = vec![
        ("hit".to_string(), JsonValue::bool(cache.hit)),
        ("bypassed".to_string(), JsonValue::bool(cache.bypassed)),
        ("stale".to_string(), JsonValue::bool(cache.stale)),
        (
            "ttlMs".to_string(),
            JsonValue::number(cache.ttl.as_millis()),
        ),
    ];
    if let Some(age) = cache.age {
        entries.push(("ageMs".to_string(), JsonValue::number(age.as_millis())));
    }
    if let Some(error) = cache.refresh_error {
        entries.push(("refreshError".to_string(), JsonValue::string(error)));
    }
    JsonValue::object(entries)
}

pub(super) fn runtime_resources_response(config: &DashboardConfig) -> JsonValue {
    JsonValue::object([
        ("ok", JsonValue::bool(true)),
        ("generatedAtMs", JsonValue::number(now_ms())),
        ("runtime", runtime_status_json(config)),
    ])
}

pub(super) fn runtime_status_json(config: &DashboardConfig) -> JsonValue {
    let metrics = config.metrics.snapshot();
    let mut upstream_pool_size = 0usize;
    let mut upstream_pool_max_size = 0usize;
    let mut upstream_pool_idle_ttl_ms = 0u128;
    let mut upstream_pool_locked_shards = 0usize;

    for pool_lock in &config.upstream_session_pools {
        if let Ok(pool) = pool_lock.lock() {
            upstream_pool_size = upstream_pool_size.saturating_add(pool.session_count());
            upstream_pool_max_size =
                upstream_pool_max_size.saturating_add(pool.max_session_count());
            upstream_pool_idle_ttl_ms = pool.idle_ttl_ms();
            upstream_pool_locked_shards = upstream_pool_locked_shards.saturating_add(1);
        }
    }

    JsonValue::object([
        (
            "surface",
            JsonValue::string(match config.surface {
                ServeSurface::Dashboard => "dashboard-http",
                ServeSurface::UnifiedServe => "unified-serve-http",
            }),
        ),
        (
            "availableParallelism",
            JsonValue::number(resources::available_parallelism()),
        ),
        (
            "http",
            JsonValue::object([
                ("maxConnections", JsonValue::number(config.max_connections)),
                (
                    "activeConnections",
                    JsonValue::number(metrics.active_connections),
                ),
                (
                    "acceptedConnections",
                    JsonValue::number(metrics.accepted_connections),
                ),
                (
                    "completedConnections",
                    JsonValue::number(metrics.completed_connections),
                ),
                (
                    "failedConnections",
                    JsonValue::number(metrics.failed_connections),
                ),
                (
                    "maxObservedActiveConnections",
                    JsonValue::number(metrics.max_active_connections),
                ),
                (
                    "ioTimeoutMs",
                    JsonValue::number(config.io_timeout.as_millis()),
                ),
                ("maxBodyBytes", JsonValue::number(config.max_body_bytes)),
                (
                    "maxRequestLineBytes",
                    JsonValue::number(resources::MAX_HTTP_REQUEST_LINE_BYTES),
                ),
                (
                    "maxHeaderLineBytes",
                    JsonValue::number(resources::MAX_HTTP_HEADER_LINE_BYTES),
                ),
                (
                    "maxHeaderBytes",
                    JsonValue::number(resources::MAX_HTTP_HEADER_BYTES),
                ),
                (
                    "maxHeaderCount",
                    JsonValue::number(resources::MAX_HTTP_HEADER_COUNT),
                ),
            ]),
        ),
        (
            "caches",
            JsonValue::object([
                (
                    "overviewTtlMs",
                    JsonValue::number(config.overview_cache_ttl.as_millis()),
                ),
                (
                    "healthTtlMs",
                    JsonValue::number(config.health_cache_ttl.as_millis()),
                ),
            ]),
        ),
        (
            "upstreamSessionPool",
            JsonValue::object([
                ("size", JsonValue::number(upstream_pool_size)),
                ("maxSize", JsonValue::number(upstream_pool_max_size)),
                ("idleTtlMs", JsonValue::number(upstream_pool_idle_ttl_ms)),
                (
                    "shardCount",
                    JsonValue::number(config.upstream_session_pools.len()),
                ),
                (
                    "lockedShardCount",
                    JsonValue::number(upstream_pool_locked_shards),
                ),
            ]),
        ),
    ])
}

pub(super) fn query_bool_flag(query: &str, key: &str) -> bool {
    query.split('&').any(|part| {
        let (name, value) = part.split_once('=').unwrap_or((part, ""));
        if !name.eq_ignore_ascii_case(key) {
            return false;
        }
        value.is_empty()
            || value == "1"
            || value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("yes")
    })
}

pub(super) fn build_overview_json(root_path: &Path) -> Result<JsonValue, String> {
    let mut results = run_json_commands_parallel(
        root_path,
        vec![
            ("doctor", vec!["doctor", "--json"]),
            ("hub", vec!["hub", "status", "--json"]),
            ("readiness", vec!["verify", "readiness", "--json"]),
            ("servers", vec!["server", "list", "--json"]),
            ("clients", vec!["client", "list", "--json"]),
        ],
    )?;

    Ok(JsonValue::object([
        ("generatedAtMs", JsonValue::number(now_ms())),
        (
            "rootPath",
            JsonValue::string(sanitize_root_path(&root_path.display().to_string())),
        ),
        ("doctor", take_parallel_result(&mut results, "doctor")?),
        ("hub", take_parallel_result(&mut results, "hub")?),
        (
            "readiness",
            take_parallel_result(&mut results, "readiness")?,
        ),
        ("servers", take_parallel_result(&mut results, "servers")?),
        ("clients", take_parallel_result(&mut results, "clients")?),
    ]))
}

pub(super) fn run_json_commands_parallel(
    root_path: &Path,
    commands: Vec<(&'static str, Vec<&'static str>)>,
) -> Result<BTreeMap<&'static str, JsonValue>, String> {
    if commands.len() <= 1 {
        let mut results = BTreeMap::new();
        for (name, args) in commands {
            results.insert(
                name,
                run_json_command_vec(root_path, args.into_iter().map(str::to_string).collect())
                    .map_err(|error| format!("{}: {}", name, error))?,
            );
        }
        return Ok(results);
    }

    let handles = commands
        .into_iter()
        .map(|(name, args)| {
            let root_path = root_path.to_path_buf();
            (
                name,
                thread::spawn(move || {
                    run_json_command_vec(&root_path, args.into_iter().map(str::to_string).collect())
                }),
            )
        })
        .collect::<Vec<_>>();

    let mut results = BTreeMap::new();
    for (name, handle) in handles {
        match handle.join() {
            Ok(Ok(value)) => {
                results.insert(name, value);
            }
            Ok(Err(error)) => return Err(format!("{}: {}", name, error)),
            Err(_) => return Err(format!("{}: command worker panicked", name)),
        }
    }
    Ok(results)
}

pub(super) fn take_parallel_result(
    results: &mut BTreeMap<&'static str, JsonValue>,
    name: &'static str,
) -> Result<JsonValue, String> {
    results
        .remove(name)
        .ok_or_else(|| format!("{}: command result missing", name))
}

pub(super) fn action_response(action: &str, result: JsonValue) -> JsonValue {
    JsonValue::object([
        ("action", JsonValue::string(action)),
        ("generatedAtMs", JsonValue::number(now_ms())),
        ("result", result),
    ])
}
